//! Security review 2026-11, batch 4 (`docs/security-review-2026-11.md`, rule 7: "The relay
//! verifies and bounds before it locks, and charges before it works"). Each test shows one
//! finding and fails on the relay as it is; they are the target for the fixes.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use rand::{rngs::StdRng, SeedableRng};
use tower::ServiceExt;
use zafe_proto::{
    relay::{
        decode_body, encode_body, join_token_hash, AppendResult, CreateMailbox, InboxRead, Join,
        LabelledAppend, LabelledReplace, LogRead, MembersRead, PostEnvelope, PushPlatform,
        RegisterPush, Reseed, ReseedResponse, Seal, SeatCancel, SeatCancelPost, SeatMoveApproval,
        SetThreshold, Signed,
    },
    Envelope, Identity, Kind, LogEntry, LogKey, MailboxId,
};
use zafe_relay::{
    limits::{Limits, Rate},
    Notifier, Relay,
};

/// Vault id and label (the same here: a legacy-style mailbox).
const VAULT: MailboxId = [1; 16];
const NOW: u64 = 1_790_000_000;
const TOKEN: [u8; 32] = [4; 32];
/// The client reads at most this much per response (`zafe_core::relay_client::
/// MAX_RESPONSE_BYTES`, not a dependency of this crate).
const CLIENT_MAX_RESPONSE: usize = 64 * 1024 * 1024;

async fn call(app: &Router, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
    let response = app
        .clone()
        .oneshot(Request::post(path).body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, body.to_vec())
}

async fn signed<T: serde::Serialize + serde::de::DeserializeOwned>(
    app: &Router,
    path: &str,
    who: &Identity,
    payload: T,
) -> (StatusCode, Vec<u8>) {
    call(
        app,
        path,
        Signed::new(who, payload).unwrap().to_bytes().unwrap(),
    )
    .await
}

fn relay() -> Relay {
    Relay::with_clock(Arc::new(|| NOW))
}

fn ids(n: usize, seed: u64) -> Vec<Identity> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n).map(|_| Identity::generate(&mut rng)).collect()
}

/// A sealed mailbox `VAULT` of `ids` (ids[0] creates it), threshold 2.
async fn open_mailbox(app: &Router, ids: &[Identity]) {
    let create = CreateMailbox {
        mailbox: VAULT,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: ids.len() as u16,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/create", &ids[0], create).await.0,
        StatusCode::OK
    );
    let threshold = SetThreshold {
        mailbox: VAULT,
        threshold: 2,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/threshold", &ids[0], threshold)
            .await
            .0,
        StatusCode::OK
    );
    for id in &ids[1..] {
        let join = Join {
            mailbox: VAULT,
            join_token: TOKEN,
        };
        assert_eq!(
            signed(app, "/v1/mailbox/join", id, join).await.0,
            StatusCode::OK
        );
    }
    let seal = Seal {
        mailbox: VAULT,
        members: ids.iter().map(|i| i.public().sig_pk).collect(),
    };
    assert_eq!(
        signed(app, "/v1/mailbox/seal", &ids[0], seal).await.0,
        StatusCode::OK
    );
}

/// `n` chained entries of `VAULT` from index `from` on top of `head`, by `ids` in turn.
fn chain(
    ids: &[Identity],
    from: u64,
    mut head: [u8; 32],
    n: u64,
    event: &[u8],
    rng: &mut StdRng,
) -> Vec<LogEntry> {
    let key = LogKey::generate(0, rng);
    (from..from + n)
        .map(|i| {
            let author = &ids[i as usize % ids.len()];
            let e = LogEntry::create(author, &key, VAULT, i, head, event, rng).unwrap();
            head = e.hash().unwrap();
            e
        })
        .collect()
}

async fn append(app: &Router, who: &Identity, entry: &LogEntry) -> (StatusCode, Vec<u8>) {
    let req = LabelledAppend::new(VAULT, entry, NOW).unwrap();
    signed(app, "/v1/log/append-labelled", who, req).await
}

/// A 2-of-3 mailbox with its first entry (by the creator).
async fn seat_world(app: &Router, seed: u64) -> Vec<Identity> {
    let ids = ids(3, seed);
    open_mailbox(app, &ids).await;
    let mut rng = StdRng::seed_from_u64(seed);
    let first = chain(&ids, 0, [0; 32], 1, b"created", &mut rng);
    assert_eq!(append(app, &ids[0], &first[0]).await.0, StatusCode::OK);
    ids
}

fn reseed(ids: &[Identity], from: u64, entries: &[LogEntry], finish: bool) -> Reseed {
    Reseed {
        mailbox: VAULT,
        members: ids.iter().map(|i| *i.public()).collect(),
        threshold: 2,
        from,
        entries: entries.iter().map(|e| e.to_bytes().unwrap()).collect(),
        finish,
        timestamp: NOW,
    }
}

// --- 1. Seat-move approvals: deduplicated and capped before any signature is checked ------

/// A member posts one approval 64 times (or more approvals than the vault has members): the
/// relay verifies every copy inside its database lock today (`replace_in`). Honest clients
/// send exactly t distinct approvals (replay refuses a repeated author), so the relay can
/// refuse such a request before it checks anything.
#[tokio::test]
async fn seat_approvals_that_repeat_a_key_or_outnumber_the_members_are_refused() {
    let app = relay().router();
    let ids = seat_world(&app, 1).await;
    let new = self::ids(1, 2).remove(0);
    let a = SeatMoveApproval {
        vault_id: VAULT,
        old: ids[1].public().sig_pk,
        new: *new.public(),
        round: 0,
    };
    let by = |i: usize| (ids[i].public().sig_pk, a.sign(&ids[i]).unwrap());
    // Two copies of one approval (fewer than the members) are refused as malformed.
    let twice = LabelledReplace {
        mailbox: VAULT,
        approval: a,
        approvals: vec![by(0); 2],
    };
    let (status, _) = signed(&app, "/v1/mailbox/replace-labelled", &ids[0], twice).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "a repeated approval key");
    let repeated = LabelledReplace {
        mailbox: VAULT,
        approval: a,
        approvals: vec![by(0); 64],
    };
    let (status, _) = signed(&app, "/v1/mailbox/replace-labelled", &ids[0], repeated).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "64 copies of one approval are refused unread (today: each is verified under the lock)"
    );

    // Two real approvals padded with strangers' (valid) signatures: more than the mailbox's
    // three members.
    let strangers = self::ids(4, 3);
    let mut approvals = vec![by(0), by(2)];
    approvals.extend(
        strangers
            .iter()
            .map(|s| (s.public().sig_pk, a.sign(s).unwrap())),
    );
    let padded = LabelledReplace {
        mailbox: VAULT,
        approval: a,
        approvals,
    };
    let (status, _) = signed(&app, "/v1/mailbox/replace-labelled", &ids[0], padded).await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "more approvals than members are refused"
    );
}

// --- 1b. Reseed: a per-request cap on the work done under the lock -------------------------

/// What one reseed request may append (the proposed cap; clients continue from the `len`
/// the relay answers, so a partial take is compatible).
const RESEED_CAP: u64 = 512;

/// A stranger restores a made-up vault in one request of thousands of tiny entries: the
/// relay verifies, hashes and inserts every one while it holds the database lock.
#[tokio::test]
async fn a_reseed_takes_at_most_a_capped_batch_per_request() {
    let app = relay().router();
    let ids = ids(2, 4);
    let mut rng = StdRng::seed_from_u64(4);
    let entries = chain(&ids, 0, [0; 32], 3000, b"", &mut rng);
    let req = reseed(&ids, 0, &entries, true);
    let (status, body) = signed(&app, "/v1/mailbox/reseed", &ids[0], req).await;
    assert_eq!(status, StatusCode::OK);
    let answer: ReseedResponse = decode_body(&body).unwrap();
    assert!(
        answer.len <= RESEED_CAP && !answer.open,
        "one request appended {} entries under the lock (cap {RESEED_CAP})",
        answer.len
    );
}

// --- 1c. Appends count the whole log under the lock --------------------------------------

/// `append_entry` (and reseed) find the log length with `SELECT COUNT(*)`, which walks every
/// index entry of the mailbox, inside the lock, on every append, `Conflict` included. A
/// stranger reseeds its own mailbox with ~2.7 M tiny entries (the 512 MiB log quota), then
/// every request it sends holds the lock for that walk. The cost must not grow with the log.
#[tokio::test]
async fn an_append_does_not_cost_more_on_a_long_log() {
    async fn conflict_time(n: u64, seed: u64) -> Duration {
        let app = relay().router();
        let ids = ids(2, seed);
        let mut rng = StdRng::seed_from_u64(seed);
        let entries = chain(&ids, 0, [0; 32], n, b"", &mut rng);
        let mut from = 0u64;
        while from < n {
            let end = (from + 2000).min(n);
            let req = reseed(&ids, from, &entries[from as usize..end as usize], end == n);
            let (status, body) = signed(&app, "/v1/mailbox/reseed", &ids[0], req).await;
            assert_eq!(status, StatusCode::OK);
            from = decode_body::<ReseedResponse>(&body).unwrap().len;
        }
        // A stale entry: the relay counts the log, then answers Conflict.
        let stale = &entries[0];
        let body = Signed::new(&ids[0], LabelledAppend::new(VAULT, stale, NOW).unwrap())
            .unwrap()
            .to_bytes()
            .unwrap();
        let start = Instant::now();
        for _ in 0..50 {
            let (status, _) = call(&app, "/v1/log/append-labelled", body.clone()).await;
            assert_eq!(status, StatusCode::OK);
        }
        start.elapsed()
    }
    let short = conflict_time(10, 5).await;
    let long = conflict_time(200_000, 6).await;
    assert!(
        long < short * 4,
        "50 appends: {short:?} on a 10-entry log, {long:?} on a 200,000-entry log"
    );
}

// --- 2. Responses fit the client's read limit -------------------------------------------

/// A dishonest member appends 70 entries of ~0.9 MB (well inside the 512 MiB log quota and
/// the 1 MiB body limit): the relay answers one page of up to 500 of them, ~63 MB here, so
/// every member that reads from before them gets `ResponseTooLarge` for good, and the relay
/// builds the whole page in memory first.
#[tokio::test]
async fn a_log_page_fits_the_clients_read_limit() {
    let app = relay().router();
    let ids = seat_world(&app, 7).await;
    let mut rng = StdRng::seed_from_u64(7);
    let mut head = {
        let read = LogRead {
            mailbox: VAULT,
            from: 0,
            timestamp: NOW,
        };
        let (_, body) = signed(&app, "/v1/log/read", &ids[0], read).await;
        let page = decode_body::<zafe_proto::relay::LogResponse>(&body)
            .unwrap()
            .decoded()
            .unwrap();
        page[0].hash().unwrap()
    };
    let big = vec![0u8; 900_000];
    let key = LogKey::generate(0, &mut rng);
    for i in 1..=70u64 {
        let e = LogEntry::create(&ids[1], &key, VAULT, i, head, &big, &mut rng).unwrap();
        head = e.hash().unwrap();
        let (status, body) = append(&app, &ids[1], &e).await;
        assert_eq!(status, StatusCode::OK);
        assert!(matches!(
            decode_body::<AppendResult>(&body).unwrap(),
            AppendResult::Appended { .. }
        ));
    }
    let read = LogRead {
        mailbox: VAULT,
        from: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/log/read", &ids[2], read).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.len() <= CLIENT_MAX_RESPONSE,
        "a log page of {} bytes; clients read at most {CLIENT_MAX_RESPONSE}",
        body.len()
    );
}

/// The same through the inbox: 70 envelopes of ~0.9 MB to one member (inside the 256 MiB
/// delivery quota) make its inbox unreadable for the 30 days they are kept; clients always
/// read from cursor 0, so it can't ack them away either.
#[tokio::test]
async fn an_inbox_page_fits_the_clients_read_limit() {
    let app = relay().router();
    let ids = seat_world(&app, 8).await;
    let mut rng = StdRng::seed_from_u64(8);
    let big = vec![0u8; 900_000];
    for seq in 1..=70u64 {
        let env = Envelope::sealed(
            &ids[1],
            ids[2].public(),
            VAULT,
            seq,
            Kind::SigningRequest,
            &big,
            &mut rng,
        )
        .unwrap();
        let envelope = env.to_bytes().unwrap();
        let (status, _) = signed(
            &app,
            "/v1/envelope/send",
            &ids[1],
            PostEnvelope { envelope },
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let read = InboxRead {
        mailbox: VAULT,
        after: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/inbox", &ids[2], read).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.len() <= CLIENT_MAX_RESPONSE,
        "an inbox page of {} bytes; clients read at most {CLIENT_MAX_RESPONSE}",
        body.len()
    );
}

// --- 3. Pushes: a per-mailbox rate ------------------------------------------------------

struct Recorder(Mutex<Vec<String>>);

impl Notifier for Recorder {
    fn notify(&self, _platform: PushPlatform, token: &str) {
        self.0.lock().unwrap().push(token.to_owned());
    }
}

/// One member's burst (20 envelopes and 20 log entries in the same second) sent 40 pushes
/// to each other member's phone. FCM limits sends per target (`QUOTA_EXCEEDED`, "Sending
/// limit exceeded for the message target"), so a member could spend another member's push
/// allowance, e.g. while that phone's seat is being moved. Pushes share one collapse key and
/// carry nothing, so the relay sends each token (one phone, all its vaults) at most one per
/// window, plus one trailing push at the window's end when it held any back.
#[tokio::test]
async fn a_burst_of_activity_pushes_each_phone_once_per_window() {
    use std::sync::atomic::{AtomicU64, Ordering};
    let now = Arc::new(AtomicU64::new(NOW));
    let clock = now.clone();
    let recorder = Arc::new(Recorder(Mutex::new(Vec::new())));
    let relay = Relay::with_clock(Arc::new(move || clock.load(Ordering::SeqCst)))
        .with_notifier(recorder.clone());
    let app = relay.clone().router();
    let ids = seat_world(&app, 9).await;
    let register = RegisterPush {
        mailbox: VAULT,
        platform: PushPlatform::Fcm,
        token: "phone-2".into(),
    };
    assert_eq!(
        signed(&app, "/v1/push/register", &ids[2], register).await.0,
        StatusCode::OK
    );
    let pushes = || {
        recorder
            .0
            .lock()
            .unwrap()
            .iter()
            .filter(|t| *t == "phone-2")
            .count()
    };
    let mut rng = StdRng::seed_from_u64(9);
    for seq in 1..=20u64 {
        let env = Envelope::sealed(
            &ids[1],
            ids[2].public(),
            VAULT,
            seq,
            Kind::SigningRequest,
            b"x",
            &mut rng,
        )
        .unwrap();
        let envelope = env.to_bytes().unwrap();
        let (status, _) = signed(
            &app,
            "/v1/envelope/send",
            &ids[1],
            PostEnvelope { envelope },
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let read = LogRead {
        mailbox: VAULT,
        from: 0,
        timestamp: NOW,
    };
    let (_, body) = signed(&app, "/v1/log/read", &ids[1], read).await;
    let head = decode_body::<zafe_proto::relay::LogResponse>(&body)
        .unwrap()
        .decoded()
        .unwrap()[0]
        .hash()
        .unwrap();
    for e in chain(&ids[1..2], 1, head, 20, b"vote", &mut rng) {
        assert_eq!(append(&app, &ids[1], &e).await.0, StatusCode::OK);
    }
    assert_eq!(pushes(), 1, "one push at once for the whole burst");
    relay.flush_pushes();
    assert_eq!(pushes(), 1, "nothing more inside the window");
    now.store(NOW + zafe_relay::PUSH_WINDOW_SECS, Ordering::SeqCst);
    relay.flush_pushes();
    assert_eq!(pushes(), 2, "one trailing push at the window's end");
    now.store(NOW + 10 * zafe_relay::PUSH_WINDOW_SECS, Ordering::SeqCst);
    relay.flush_pushes();
    assert_eq!(pushes(), 2, "nothing held back: no more pushes");
}

// --- 4. Charges: the requester, never the key that signed a relayed object --------------

/// `/v1/seat/cancel` takes no request signature: anyone may post the old key's signed
/// cancel, and every post charges the *old key's* rate (`limit_key(&old.sig_pk)`, before the
/// "already cancelled" answer). The cancel is shown to every member and the pending key
/// (`/v1/mailbox/info`), so the thief whose move was kept can replay it and lock the seat's
/// owner out of the relay (429 on every request) while it tries again.
#[tokio::test]
async fn a_replayed_cancel_doesnt_spend_the_old_keys_rate() {
    let limits = Limits {
        per_key: Some(Rate::new(60, 10)),
        ..Limits::none()
    };
    let app = relay().with_limits(limits).router();
    let ids = seat_world(&app, 10).await;
    let thief = self::ids(1, 11).remove(0);
    let a = SeatMoveApproval {
        vault_id: VAULT,
        old: ids[1].public().sig_pk,
        new: *thief.public(),
        round: 0,
    };
    let approve = LabelledReplace {
        mailbox: VAULT,
        approval: a,
        approvals: [0, 2]
            .iter()
            .map(|&i| (ids[i].public().sig_pk, a.sign(&ids[i]).unwrap()))
            .collect(),
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/replace-labelled", &ids[0], approve)
            .await
            .0,
        StatusCode::OK
    );
    let cancel = SeatCancel {
        mailbox: VAULT,
        vault_id: VAULT,
        move_id: a.move_id(),
        old: a.old,
        new: a.new.sig_pk,
        at: NOW,
    };
    let post = encode_body(&SeatCancelPost {
        cancel,
        signature: cancel.sign(&ids[1]).unwrap(),
    })
    .unwrap();
    // The owner keeps its seat; then someone else replays the same bytes.
    assert_eq!(
        call(&app, "/v1/seat/cancel", post.clone()).await.0,
        StatusCode::OK
    );
    for _ in 0..20 {
        call(&app, "/v1/seat/cancel", post.clone()).await;
    }
    let read = MembersRead {
        mailbox: VAULT,
        timestamp: NOW,
    };
    let (status, _) = signed(&app, "/v1/mailbox/info", &ids[1], read).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "the old key is rate limited by someone else's replays"
    );
}

// --- 5. Creator-only entry 0 ----------------------------------------------------------

/// After the creator seals the setup, any member can append entry 0 first: its entry
/// fixes the mailbox's vault id and takes index 0, so the creator's `Created` gets
/// `OtherVault`/`Conflict`, the setup can't finish, and `/v1/mailbox/cancel` refuses
/// (the log isn't empty): one member strands the setup and the creator's mailbox quota.
#[tokio::test]
async fn only_the_creator_appends_the_first_entry() {
    let app = relay().router();
    let ids = ids(3, 14);
    open_mailbox(&app, &ids).await;
    let mut rng = StdRng::seed_from_u64(14);
    let first = chain(&ids[1..2], 0, [0; 32], 1, b"junk", &mut rng);
    let (status, _) = append(&app, &ids[1], &first[0]).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a member other than the creator wrote entry 0"
    );
}

// --- Unbounded work in the hourly prune ---------------------------------------------------

/// `prune` filters `deliveries` on `created_at`, which has no index and is stored after
/// the envelope blob, so every hour it reads every stored envelope (overflow pages and all)
/// while holding the database lock. Here 200 MB of fresh deliveries (nothing to prune):
/// pruning must not cost like reading them all.
#[tokio::test]
async fn pruning_nothing_doesnt_read_every_delivery() {
    let dir = std::env::temp_dir().join(format!("zafe-relay-rule7-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let relay = Relay::open_with_clock(&dir.join("relay.sqlite"), Arc::new(|| NOW)).unwrap();
    let app = relay.clone().router();
    let ids = seat_world(&app, 15).await;
    let start = Instant::now();
    relay.prune().unwrap();
    let empty = start.elapsed();
    let mut rng = StdRng::seed_from_u64(15);
    let big = vec![0u8; 900_000];
    for seq in 1..=200u64 {
        let env = Envelope::sealed(
            &ids[1],
            ids[(seq % 2) as usize * 2].public(),
            VAULT,
            seq,
            Kind::SigningRequest,
            &big,
            &mut rng,
        )
        .unwrap();
        let envelope = env.to_bytes().unwrap();
        let (status, _) = signed(
            &app,
            "/v1/envelope/send",
            &ids[1],
            PostEnvelope { envelope },
        )
        .await;
        assert_eq!(status, StatusCode::OK);
    }
    let start = Instant::now();
    relay.prune().unwrap();
    let full = start.elapsed();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        full < empty * 4 + Duration::from_millis(5),
        "prune with nothing to prune: {empty:?} empty, {full:?} over 200 MB of deliveries"
    );
}

// --- Unbounded work in the per-recipient inbox cap ----------------------------------------

/// `check_delivery_quotas` counted each recipient's undelivered envelopes by walking up to
/// the per-recipient cap (10,000 hosted) index entries per recipient, inside the lock,
/// so every broadcast into deep inboxes walked ~63 x depth entries (a refused one, nothing
/// stored, could be sent again at once). It reads one counter row per recipient now: a
/// broadcast into 64 inboxes holding ~6,200 envelopes each costs what one into empty ones
/// does.
#[tokio::test]
async fn a_post_to_deep_inboxes_costs_no_more_than_one_to_empty_ones() {
    use zafe_relay::quota::Quotas;
    /// Each of 63 senders' pair with an inbox: 100.
    const CAP: u64 = 100;
    async fn world(seed: u64) -> (Router, Vec<Identity>) {
        let quotas = Quotas {
            envelopes_per_pair: Some(CAP),
            ..Quotas::none()
        };
        let app = relay().with_quotas(quotas).router();
        let ids = ids(64, seed);
        open_mailbox(&app, &ids).await;
        (app, ids)
    }
    async fn broadcast(app: &Router, who: &Identity, seq: u64) -> StatusCode {
        let env = Envelope::public(who, VAULT, seq, Kind::SigningRequest, b"x").unwrap();
        let envelope = env.to_bytes().unwrap();
        signed(app, "/v1/envelope/send", who, PostEnvelope { envelope })
            .await
            .0
    }
    let (app, ids) = world(16).await;
    let start = Instant::now();
    for seq in 1..=20 {
        assert_eq!(broadcast(&app, &ids[0], seq).await, StatusCode::OK);
    }
    let empty = start.elapsed();

    let (app, ids) = world(17).await;
    for sender in &ids[1..] {
        for seq in 1..=CAP {
            assert_eq!(broadcast(&app, sender, seq).await, StatusCode::OK);
        }
    }
    let start = Instant::now();
    for seq in 1..=20 {
        assert_eq!(broadcast(&app, &ids[0], seq).await, StatusCode::OK);
    }
    let deep = start.elapsed();
    assert!(
        deep < empty * 3,
        "20 broadcasts: {empty:?} into empty inboxes, {deep:?} into deep ones"
    );
}

// --- 3b. FCM: bounded concurrency, failing tokens pruned ---------------------------------

const TEST_KEY: &str = include_str!("fixtures/test-only-rsa.pem");

/// A mock of Google's token endpoint and FCM that answers each send after `delay` with
/// `answer`, and counts sends in flight (now and at most).
async fn mock_fcm(
    delay: Duration,
    answer: (u16, serde_json::Value),
) -> (String, Arc<Mutex<(usize, usize, usize)>>) {
    use axum::{routing::post, Json};
    let flight = Arc::new(Mutex::new((0usize, 0usize, 0usize)));
    let seen = flight.clone();
    let app = Router::new()
        .route(
            "/token",
            post(|| async { Json(serde_json::json!({"access_token": "tok", "expires_in": 3600})) }),
        )
        .route(
            "/v1/projects/zafe-test/messages:send",
            post(move || {
                let (seen, answer) = (seen.clone(), answer.clone());
                async move {
                    {
                        let mut f = seen.lock().unwrap();
                        f.0 += 1;
                        f.1 = f.1.max(f.0);
                    }
                    tokio::time::sleep(delay).await;
                    let mut f = seen.lock().unwrap();
                    f.0 -= 1;
                    f.2 += 1;
                    (
                        StatusCode::from_u16(answer.0).unwrap(),
                        Json(answer.1.clone()),
                    )
                }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (base, flight)
}

fn account(base: &str) -> zafe_relay::fcm::ServiceAccount {
    zafe_relay::fcm::ServiceAccount {
        project_id: "zafe-test".into(),
        client_email: "relay@zafe-test.iam.gserviceaccount.com".into(),
        private_key: TEST_KEY.into(),
        token_uri: format!("{base}/token"),
    }
}

/// Every push is its own spawned task with no bound (`FcmNotifier::notify`), so a burst of
/// activity across vaults opens as many connections to FCM at once.
#[tokio::test]
async fn pushes_in_flight_are_bounded() {
    /// The proposed bound.
    const IN_FLIGHT: usize = 32;
    let ok = (
        200,
        serde_json::json!({"name": "projects/zafe-test/messages/1"}),
    );
    let (base, flight) = mock_fcm(Duration::from_millis(300), ok).await;
    let notifier = zafe_relay::fcm::FcmNotifier::with_endpoint(account(&base), &base);
    for i in 0..300 {
        notifier.notify(PushPlatform::Fcm, &format!("device-{i}"));
    }
    for _ in 0..200 {
        if flight.lock().unwrap().2 == 300 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let (_, most, done) = *flight.lock().unwrap();
    assert_eq!(done, 300, "every push is still sent");
    assert!(most <= IN_FLIGHT, "{most} pushes in flight at once");
}

/// A token FCM says belongs to another sender (403 `SENDER_ID_MISMATCH`) never works for
/// this relay, yet only `UNREGISTERED` is forgotten: every later activity sends to it again.
#[tokio::test]
async fn a_token_of_another_sender_is_forgotten() {
    let mismatch = (
        403,
        serde_json::json!({"error": {
            "code": 403,
            "status": "PERMISSION_DENIED",
            "details": [{
                "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError",
                "errorCode": "SENDER_ID_MISMATCH"
            }]
        }}),
    );
    let (base, _) = mock_fcm(Duration::ZERO, mismatch).await;
    let gone = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = gone.clone();
    let notifier = zafe_relay::fcm::FcmNotifier::with_endpoint(account(&base), &base)
        .on_unregistered(move |t| sink.lock().unwrap().push(t.to_owned()));
    notifier.notify(PushPlatform::Fcm, "other-project");
    for _ in 0..50 {
        if !gone.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert_eq!(*gone.lock().unwrap(), vec!["other-project".to_owned()]);
}

/// FCM's `INVALID_ARGUMENT` is a gone token only when its `google.rpc.BadRequest` details
/// name the token field ("An extension of type google.rpc.BadRequest is returned to specify
/// which field was invalid", FCM v1 ErrorCode reference; "INVALID_ARGUMENT can also be
/// returned in cases of issues in the message payload", FCM "Manage tokens").
#[tokio::test]
async fn an_invalid_token_is_forgotten_but_not_a_bad_payload() {
    let bad = |field: &str| {
        (
            400,
            serde_json::json!({"error": {
                "code": 400,
                "status": "INVALID_ARGUMENT",
                "details": [{
                    "@type": "type.googleapis.com/google.rpc.BadRequest",
                    "fieldViolations": [{
                        "field": field,
                        "description": "Invalid registration token"
                    }]
                }]
            }}),
        )
    };
    for (field, forgotten) in [
        ("message.token", true),
        ("message.fid", true),
        ("message.android.priority", false),
    ] {
        let (base, _) = mock_fcm(Duration::ZERO, bad(field)).await;
        let gone = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = gone.clone();
        let notifier = zafe_relay::fcm::FcmNotifier::with_endpoint(account(&base), &base)
            .on_unregistered(move |t| sink.lock().unwrap().push(t.to_owned()));
        notifier.notify(PushPlatform::Fcm, "t");
        for _ in 0..25 {
            if !gone.lock().unwrap().is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(!gone.lock().unwrap().is_empty(), forgotten, "{field}");
    }
}

// --- NEW 6 (bytes): one sender can't fill the mailbox's delivery bytes for everyone ------

/// The byte quota was per mailbox: one member's large envelopes (kept 30 days when the
/// recipient never acks their kind) made every send in the vault 507. Each sender may use
/// half of the bytes, so the others still deliver.
#[tokio::test]
async fn one_member_cant_fill_the_vaults_delivery_bytes() {
    use zafe_relay::quota::Quotas;
    let ids = self::ids(3, 18);
    let mut rng = StdRng::seed_from_u64(18);
    let big = vec![0u8; 900_000];
    let len = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        VAULT,
        1,
        Kind::DkgEcho,
        &big,
        &mut rng,
    )
    .unwrap()
    .to_bytes()
    .unwrap()
    .len() as u64;
    // Exactly 30 such envelopes fill the vault's bytes.
    let relay = relay().with_quotas(Quotas {
        delivery_bytes: Some(30 * len),
        ..Quotas::none()
    });
    let app = relay.router();
    open_mailbox(&app, &ids).await;
    let mut stored = 0;
    for seq in 1..=40u64 {
        let env = Envelope::sealed(
            &ids[0],
            ids[1].public(),
            VAULT,
            seq,
            Kind::DkgEcho,
            &big,
            &mut rng,
        )
        .unwrap();
        let envelope = env.to_bytes().unwrap();
        if signed(
            &app,
            "/v1/envelope/send",
            &ids[0],
            PostEnvelope { envelope },
        )
        .await
        .0 == StatusCode::OK
        {
            stored += 1;
        }
    }
    assert!(stored > 0 && stored < 40, "{stored} stored");
    for (seq, (from, to)) in [(1, (2, 1)), (1, (1, 2)), (2, (2, 0))] {
        let env = Envelope::sealed(
            &ids[from],
            ids[to].public(),
            VAULT,
            seq,
            Kind::SigningRequest,
            b"sign this",
            &mut rng,
        )
        .unwrap();
        let envelope = env.to_bytes().unwrap();
        assert_eq!(
            signed(
                &app,
                "/v1/envelope/send",
                &ids[from],
                PostEnvelope { envelope }
            )
            .await
            .0,
            StatusCode::OK,
            "{from} -> {to} still delivers"
        );
    }
}

// --- 2b. A global storage cap -------------------------------------------------------------

/// Quotas were per mailbox only and mailboxes are cheap (20 a day per IP), so one address
/// could fill the disk the testnet and mainnet relays share. `Quotas::db_bytes` caps the
/// database (`PRAGMA max_page_count`): a write past it is 507, and creations stop at 90% so
/// existing vaults keep room.
#[tokio::test]
async fn the_database_has_a_global_cap() {
    use zafe_relay::quota::Quotas;
    let relay = relay().with_quotas(Quotas {
        db_bytes: Some(8 * 1024 * 1024),
        ..Quotas::none()
    });
    let app = relay.clone().router();
    let ids = seat_world(&app, 19).await;
    let mut rng = StdRng::seed_from_u64(19);
    let big = vec![0u8; 500_000];
    let mut full = None;
    for seq in 1..=40u64 {
        let env = Envelope::sealed(
            &ids[0],
            ids[1].public(),
            VAULT,
            seq,
            Kind::DkgEcho,
            &big,
            &mut rng,
        )
        .unwrap();
        let envelope = env.to_bytes().unwrap();
        let (status, body) = signed(
            &app,
            "/v1/envelope/send",
            &ids[0],
            PostEnvelope { envelope },
        )
        .await;
        if status != StatusCode::OK {
            full = Some((status, String::from_utf8_lossy(&body).into_owned()));
            break;
        }
    }
    let (status, body) = full.expect("8 MiB never filled");
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    assert!(body.contains("relay is full"), "{body}");
    // Reads still work.
    let read = InboxRead {
        mailbox: VAULT,
        after: 0,
        timestamp: NOW,
    };
    assert_eq!(
        signed(&app, "/v1/inbox", &ids[1], read).await.0,
        StatusCode::OK
    );
    // No new vault on a relay this full.
    let create = CreateMailbox {
        mailbox: [5; 16],
        join_token_hash: join_token_hash(&TOKEN),
        max_members: 2,
    };
    let stranger = self::ids(1, 20).remove(0);
    assert_eq!(
        signed(&app, "/v1/mailbox/create", &stranger, create)
            .await
            .0,
        StatusCode::INSUFFICIENT_STORAGE
    );
}
