//! Labelled mailboxes (`docs/relay-move.md` §4.6), `/v1/push/forget`, `GET /v1/info` and
//! the create token.

use std::sync::{Arc, Mutex};

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
        decode_body, join_token_hash, AppendResult, CreateMailbox, ForgetPush, InboxAck,
        InboxAckResponse, InboxRead, InboxResponse, Join, LabelledAppend, LogRead, LogResponse,
        MailboxInfo, MembersRead, PushPlatform, RegisterPush, RelayInfo, Reseed, ReseedResponse,
        Seal, Signed, WaitRequest, WaitResponse, CREATE_TOKEN_HEADER, UNKNOWN_MAILBOX,
    },
    Envelope, Identity, Kind, LogEntry, LogKey, MailboxId,
};
use zafe_relay::{Notifier, Relay};

/// The vault id the entries carry.
const VAULT: MailboxId = [1; 16];
/// This relay's label for the vault.
const LABEL: MailboxId = [2; 16];
/// Another vault's id.
const OTHER_VAULT: MailboxId = [3; 16];
const NOW: u64 = 1_790_000_000;
const TOKEN: [u8; 32] = [4; 32];

async fn send(app: &Router, request: Request<Body>) -> (StatusCode, Vec<u8>) {
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, body.to_vec())
}

async fn call(app: &Router, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
    send(app, Request::post(path).body(Body::from(body)).unwrap()).await
}

fn signed_body<T: serde::Serialize + serde::de::DeserializeOwned>(
    who: &Identity,
    payload: T,
) -> Vec<u8> {
    Signed::new(who, payload).unwrap().to_bytes().unwrap()
}

async fn signed<T: serde::Serialize + serde::de::DeserializeOwned>(
    app: &Router,
    path: &str,
    who: &Identity,
    payload: T,
) -> (StatusCode, Vec<u8>) {
    call(app, path, signed_body(who, payload)).await
}

fn relay() -> Relay {
    Relay::with_clock(Arc::new(|| NOW))
}

fn ids(n: usize, seed: u64) -> Vec<Identity> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n).map(|_| Identity::generate(&mut rng)).collect()
}

/// A sealed mailbox `label` with every `ids` as a member, set up through the existing
/// routes (create, join, seal), which work with any label.
async fn open_mailbox(app: &Router, label: MailboxId, ids: &[Identity]) {
    let create = CreateMailbox {
        mailbox: label,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: ids.len() as u16,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/create", &ids[0], create).await.0,
        StatusCode::OK
    );
    for id in &ids[1..] {
        let join = Join {
            mailbox: label,
            join_token: TOKEN,
        };
        assert_eq!(
            signed(app, "/v1/mailbox/join", id, join).await.0,
            StatusCode::OK
        );
    }
    let seal = Seal {
        mailbox: label,
        members: ids.iter().map(|i| i.public().sig_pk).collect(),
    };
    assert_eq!(
        signed(app, "/v1/mailbox/seal", &ids[0], seal).await.0,
        StatusCode::OK
    );
}

/// `n` chained entries of vault `vault`, authored in turn by `ids`.
fn entries(vault: MailboxId, ids: &[Identity], n: u64, rng: &mut StdRng) -> Vec<LogEntry> {
    let key = LogKey::generate(0, rng);
    let mut head = [0; 32];
    (0..n)
        .map(|i| {
            let author = &ids[i as usize % ids.len()];
            let e = LogEntry::create(author, &key, vault, i, head, b"event", rng).unwrap();
            head = e.hash().unwrap();
            e
        })
        .collect()
}

async fn append_to(
    app: &Router,
    who: &Identity,
    label: MailboxId,
    entry: &LogEntry,
) -> (StatusCode, Option<AppendResult>) {
    let req = LabelledAppend::new(label, entry, NOW).unwrap();
    let (status, body) = signed(app, "/v1/log/append-labelled", who, req).await;
    (status, decode_body::<AppendResult>(&body).ok())
}

async fn read_log(app: &Router, who: &Identity, label: MailboxId) -> Vec<LogEntry> {
    let read = LogRead {
        mailbox: label,
        from: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(app, "/v1/log/read", who, read).await;
    assert_eq!(status, StatusCode::OK);
    decode_body::<LogResponse>(&body)
        .unwrap()
        .decoded()
        .unwrap()
}

fn reseed_req(
    label: MailboxId,
    ids: &[Identity],
    from: u64,
    entries: &[LogEntry],
    finish: bool,
) -> Reseed {
    Reseed {
        mailbox: label,
        members: ids.iter().map(|i| *i.public()).collect(),
        threshold: 2,
        from,
        entries: entries.iter().map(|e| e.to_bytes().unwrap()).collect(),
        finish,
        timestamp: NOW,
    }
}

// --- Labels --------------------------------------------------------------------------------

#[tokio::test]
async fn a_labelled_mailbox_holds_one_vault_whatever_its_label() {
    let app = relay().router();
    let ids = ids(4, 1);
    let members = &ids[..3];
    open_mailbox(&app, LABEL, members).await;
    let mut rng = StdRng::seed_from_u64(2);
    let log = entries(VAULT, members, 3, &mut rng);

    // The label isn't the vault id, so the old route (which files by vault id) can't
    // reach it.
    let (status, body) = call(&app, "/v1/log/append", log[0].to_bytes().unwrap()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, UNKNOWN_MAILBOX.as_bytes());

    // Any member may post any member's entry (catching up), once, in order.
    assert_eq!(
        append_to(&app, &ids[1], LABEL, &log[0]).await,
        (StatusCode::OK, Some(AppendResult::Appended { index: 0 }))
    );
    assert_eq!(
        append_to(&app, &ids[2], LABEL, &log[0]).await,
        (StatusCode::OK, Some(AppendResult::Conflict { len: 1 }))
    );
    // An outsider can't post a member's entry, nor learn the log length.
    let (status, result) = append_to(&app, &ids[3], LABEL, &log[1]).await;
    assert_eq!((status, result), (StatusCode::FORBIDDEN, None));
    // An entry of another vault is refused, even correctly chained and signed by a member.
    let key = LogKey::generate(0, &mut rng);
    let head = log[0].hash().unwrap();
    let foreign =
        LogEntry::create(&ids[1], &key, OTHER_VAULT, 1, head, b"other", &mut rng).unwrap();
    let (status, result) = append_to(&app, &ids[1], LABEL, &foreign).await;
    assert_eq!((status, result), (StatusCode::BAD_REQUEST, None));
    // A request signed for another label doesn't verify for this one.
    let mut moved =
        Signed::new(&ids[1], LabelledAppend::new([9; 16], &log[1], NOW).unwrap()).unwrap();
    moved.payload.mailbox = LABEL;
    let (status, _) = call(&app, "/v1/log/append-labelled", moved.to_bytes().unwrap()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // A stale request is refused.
    let old = LabelledAppend::new(LABEL, &log[1], NOW - 3600).unwrap();
    assert_eq!(
        signed(&app, "/v1/log/append-labelled", &ids[1], old)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    for e in &log[1..] {
        assert_eq!(append_to(&app, &ids[0], LABEL, e).await.0, StatusCode::OK);
    }
    assert_eq!(read_log(&app, &ids[2], LABEL).await, log);
}

/// A mailbox made with the old routes (its id is the vault id) takes its vault id from its
/// first entry too, so entries of two vaults can't mix there either.
#[tokio::test]
async fn a_mailbox_whose_entries_carry_two_vault_ids_is_refused() {
    let app = relay().router();
    let ids = ids(3, 3);
    open_mailbox(&app, VAULT, &ids).await;
    let mut rng = StdRng::seed_from_u64(4);
    // The first entry, posted through the labelled route, is of another vault.
    let foreign = entries(OTHER_VAULT, &ids, 2, &mut rng);
    assert_eq!(
        append_to(&app, &ids[0], VAULT, &foreign[0]).await.0,
        StatusCode::OK
    );
    // From then on the mailbox holds that vault: entries of the vault its id names are
    // refused on both routes, wherever they would go in the chain.
    let own = entries(VAULT, &ids, 1, &mut rng);
    let next = entry_after(VAULT, &ids[1], 1, &foreign[0], &mut rng);
    assert_eq!(
        call(&app, "/v1/log/append", next.to_bytes().unwrap())
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    for entry in [&next, &own[0]] {
        assert_eq!(
            append_to(&app, &ids[1], VAULT, entry).await.0,
            StatusCode::BAD_REQUEST
        );
    }
    assert_eq!(
        append_to(&app, &ids[1], VAULT, &foreign[1]).await.0,
        StatusCode::OK
    );

    // The old route keeps working for a mailbox filed under its vault id.
    let app = relay().router();
    open_mailbox(&app, VAULT, &ids).await;
    let log = entries(VAULT, &ids, 2, &mut rng);
    for (i, e) in log.iter().enumerate() {
        let (status, body) = call(&app, "/v1/log/append", e.to_bytes().unwrap()).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            decode_body::<AppendResult>(&body).unwrap(),
            AppendResult::Appended { index: i as u64 }
        );
    }
}

/// An entry of `vault` at `index` after `prev`, correctly chained and signed by `author`.
fn entry_after(
    vault: MailboxId,
    author: &Identity,
    index: u64,
    prev: &LogEntry,
    rng: &mut StdRng,
) -> LogEntry {
    let key = LogKey::generate(0, rng);
    LogEntry::create(
        author,
        &key,
        vault,
        index,
        prev.hash().unwrap(),
        b"other",
        rng,
    )
    .unwrap()
}

#[tokio::test]
async fn a_reseed_takes_any_label_but_one_vault() {
    let app = relay().router();
    let ids = ids(3, 5);
    let mut rng = StdRng::seed_from_u64(6);
    let log = entries(VAULT, &ids, 4, &mut rng);
    let reseed = |from, entries: &[LogEntry], finish| {
        signed(
            &app,
            "/v1/mailbox/reseed",
            &ids[0],
            reseed_req(LABEL, &ids, from, entries, finish),
        )
    };

    // One request mixing two vaults' entries is refused whole.
    let mixed = [
        log[0].clone(),
        entry_after(OTHER_VAULT, &ids[1], 1, &log[0], &mut rng),
    ];
    let (status, body) = reseed(0, &mixed, false).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, b"this mailbox holds another vault");

    // Restored in two steps under a label: the second step's vault must be the first's.
    let (status, body) = reseed(0, &log[..2], false).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode_body::<ReseedResponse>(&body).unwrap().len, 2);
    let other = entry_after(OTHER_VAULT, &ids[1], 2, &log[1], &mut rng);
    let (status, body) = reseed(2, &[other], true).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body, b"this mailbox holds another vault");
    let (status, body) = reseed(2, &log[2..3], true).await;
    assert_eq!(status, StatusCode::OK);
    assert!(decode_body::<ReseedResponse>(&body).unwrap().open);

    // Open: members append through the labelled route, and the vault is still enforced.
    let other = entry_after(OTHER_VAULT, &ids[1], 3, &log[2], &mut rng);
    assert_eq!(
        append_to(&app, &ids[1], LABEL, &other).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        append_to(&app, &ids[1], LABEL, &log[3]).await,
        (StatusCode::OK, Some(AppendResult::Appended { index: 3 }))
    );
    assert_eq!(read_log(&app, &ids[2], LABEL).await, log);
}

/// Every route that takes the mailbox from the request or the envelope header works with a
/// label that isn't the vault id.
#[tokio::test]
async fn other_routes_work_with_labels_unchanged() {
    let app = relay().router();
    let ids = ids(3, 7);
    open_mailbox(&app, LABEL, &ids).await;
    let mut rng = StdRng::seed_from_u64(8);
    let log = entries(VAULT, &ids, 1, &mut rng);
    assert_eq!(
        append_to(&app, &ids[0], LABEL, &log[0]).await.0,
        StatusCode::OK
    );

    let read = MembersRead {
        mailbox: LABEL,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/mailbox/info", &ids[1], read).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode_body::<MailboxInfo>(&body).unwrap().members.len(), 3);

    let push = RegisterPush {
        mailbox: LABEL,
        platform: PushPlatform::Fcm,
        token: "t".into(),
    };
    assert_eq!(
        signed(&app, "/v1/push/register", &ids[1], push).await.0,
        StatusCode::OK
    );

    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        LABEL,
        1,
        Kind::Approval,
        b"hi",
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        call(&app, "/v1/envelope", env.to_bytes().unwrap()).await.0,
        StatusCode::OK
    );
    let read = InboxRead {
        mailbox: LABEL,
        after: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/inbox", &ids[1], read).await;
    assert_eq!(status, StatusCode::OK);
    let inbox = decode_body::<InboxResponse>(&body).unwrap().decoded();
    assert_eq!(inbox.len(), 1);
    assert_eq!(inbox[0].1, env);

    let wait = WaitRequest {
        mailbox: LABEL,
        log_len: 0,
        inbox_after: 0,
        max_wait_secs: 0,
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/wait", &ids[1], wait).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        decode_body::<WaitResponse>(&body).unwrap(),
        WaitResponse {
            log_len: 1,
            inbox_cursor: inbox[0].0
        }
    );

    let ack = InboxAck {
        mailbox: LABEL,
        cursors: vec![inbox[0].0],
        timestamp: NOW,
    };
    let (status, body) = signed(&app, "/v1/inbox/ack", &ids[1], ack).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(decode_body::<InboxAckResponse>(&body).unwrap().deleted, 1);
    assert_eq!(read_log(&app, &ids[1], LABEL).await, log);
}

/// Envelopes name the label in their signed header, so one sent on the old relay can't be
/// replayed into the vault's mailbox on another relay (`docs/relay-move.md` §8, T7).
#[tokio::test]
async fn an_envelope_with_an_old_label_fails_on_the_new_mailbox() {
    const OLD: MailboxId = [5; 16];
    const NEW: MailboxId = [6; 16];
    let old_relay = relay().router();
    let new_relay = relay().router();
    let ids = ids(3, 9);
    open_mailbox(&old_relay, OLD, &ids).await;
    open_mailbox(&new_relay, NEW, &ids).await;
    let mut rng = StdRng::seed_from_u64(10);
    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        OLD,
        1,
        Kind::Approval,
        b"approve",
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        call(&old_relay, "/v1/envelope", env.to_bytes().unwrap())
            .await
            .0,
        StatusCode::OK
    );
    // As sent: the new relay has no mailbox under the old label.
    let (status, body) = call(&new_relay, "/v1/envelope", env.to_bytes().unwrap()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body, UNKNOWN_MAILBOX.as_bytes());
    // Relabelled: the sender's signature covers the header.
    let mut relabelled = env.clone();
    relabelled.header.mailbox = NEW;
    assert_eq!(
        call(&new_relay, "/v1/envelope", relabelled.to_bytes().unwrap())
            .await
            .0,
        StatusCode::UNAUTHORIZED
    );
    let read = InboxRead {
        mailbox: NEW,
        after: 0,
        timestamp: NOW,
    };
    let (_, body) = signed(&new_relay, "/v1/inbox", &ids[1], read).await;
    assert!(decode_body::<InboxResponse>(&body)
        .unwrap()
        .envelopes
        .is_empty());
}

// --- Push forget -----------------------------------------------------------------------------

#[derive(Default)]
struct Recorder(Mutex<Vec<String>>);

impl Notifier for Recorder {
    fn notify(&self, _platform: PushPlatform, token: &str) {
        self.0.lock().unwrap().push(token.to_owned());
    }
}

#[tokio::test]
async fn a_member_forgets_its_own_push_token() {
    let recorder = Arc::new(Recorder::default());
    let app = relay().with_notifier(recorder.clone()).router();
    let ids = ids(3, 11);
    open_mailbox(&app, LABEL, &ids).await;
    for (i, token) in [(1, "token-B"), (2, "token-C")] {
        let push = RegisterPush {
            mailbox: LABEL,
            platform: PushPlatform::Fcm,
            token: token.into(),
        };
        assert_eq!(
            signed(&app, "/v1/push/register", &ids[i], push).await.0,
            StatusCode::OK
        );
    }
    let forget = |timestamp| ForgetPush {
        mailbox: LABEL,
        timestamp,
    };
    // Stale requests are refused; a fresh one deletes only the signer's own token.
    assert_eq!(
        signed(&app, "/v1/push/forget", &ids[1], forget(NOW - 3600))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        signed(&app, "/v1/push/forget", &ids[1], forget(NOW))
            .await
            .0,
        StatusCode::OK
    );
    // Again, from an outsider, or for a mailbox this relay doesn't have: nothing to do,
    // never 404 (that means a relay without the route).
    assert_eq!(
        signed(&app, "/v1/push/forget", &ids[1], forget(NOW))
            .await
            .0,
        StatusCode::OK
    );
    let outsider = Identity::generate(&mut StdRng::seed_from_u64(12));
    assert_eq!(
        signed(&app, "/v1/push/forget", &outsider, forget(NOW))
            .await
            .0,
        StatusCode::OK
    );
    let unknown = ForgetPush {
        mailbox: [8; 16],
        timestamp: NOW,
    };
    assert_eq!(
        signed(&app, "/v1/push/forget", &ids[1], unknown).await.0,
        StatusCode::OK
    );
    let env = Envelope::public(&ids[0], LABEL, 1, Kind::Approval, b"news").unwrap();
    assert_eq!(
        call(&app, "/v1/envelope", env.to_bytes().unwrap()).await.0,
        StatusCode::OK
    );
    assert_eq!(*recorder.0.lock().unwrap(), vec!["token-C".to_owned()]);
}

// --- Info ----------------------------------------------------------------------------------

async fn get(app: &Router, path: &str) -> (StatusCode, Vec<u8>) {
    send(app, Request::get(path).body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn info_says_what_the_relay_allows_and_health_stays_plain() {
    use zafe_relay::{expiry::Expiry, limits::Limits, quota::Quotas};
    let (status, body) = get(&relay().router(), "/v1/info").await;
    assert_eq!(status, StatusCode::OK);
    let info: RelayInfo = serde_json::from_slice(&body).unwrap();
    assert_eq!(info.versions.relay_api, zafe_proto::version::RELAY_API);
    assert_eq!(info.versions.envelope, zafe_proto::version::ENVELOPE);
    assert_eq!(info.versions.log_entry, zafe_proto::version::LOG_ENTRY);
    assert_eq!(
        info.limits.max_body_bytes,
        zafe_relay::MAX_BODY_BYTES as u64
    );
    assert_eq!(info.limits.max_members, zafe_relay::MAX_MEMBERS);
    assert_eq!(info.limits.log_bytes, None);
    assert!(!info.push, "pushes are only logged");
    assert!(!info.create_token);
    assert_eq!(info.expiry.idle_days, None);

    let app = relay()
        .with_limits(Limits::hosted())
        .with_quotas(Quotas::hosted())
        .with_expiry(Expiry::hosted())
        .with_create_token(Some("secret"))
        .with_notifier(Arc::new(Recorder::default()))
        .router();
    let (_, body) = get(&app, "/v1/info").await;
    let text = String::from_utf8(body.clone()).unwrap();
    assert!(!text.contains("secret"), "{text}");
    let info: RelayInfo = serde_json::from_slice(&body).unwrap();
    assert!(info.push && info.create_token);
    assert_eq!(info.limits.log_bytes, Quotas::hosted().log_bytes);
    assert_eq!(info.limits.mailboxes_per_key, Some(8));
    assert_eq!(info.limits.requests_per_minute, Some(300));
    assert_eq!(info.limits.delivery_retention_days, 30);
    assert_eq!(info.expiry.idle_days, Some(365));
    assert_eq!(info.expiry.unused_days, Some(30));
    // Readers ignore fields they don't know and default the ones they miss.
    let partial: RelayInfo = serde_json::from_str(r#"{"push":true,"later":1}"#).unwrap();
    assert!(partial.push && !partial.create_token);

    assert_eq!(get(&app, "/health").await, (StatusCode::OK, b"ok".to_vec()));
}

// --- Create token --------------------------------------------------------------------------

fn create_body(who: &Identity, mailbox: MailboxId) -> Vec<u8> {
    signed_body(
        who,
        CreateMailbox {
            mailbox,
            join_token_hash: join_token_hash(&TOKEN),
            max_members: 3,
        },
    )
}

async fn post_with_token(
    app: &Router,
    path: &str,
    body: Vec<u8>,
    token: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::post(path).header("fly-client-ip", "203.0.113.7");
    if let Some(token) = token {
        request = request.header(CREATE_TOKEN_HEADER, token);
    }
    app.clone()
        .oneshot(request.body(Body::from(body)).unwrap())
        .await
        .unwrap()
}

#[tokio::test]
async fn a_private_relay_needs_its_create_token() {
    let app = relay().with_create_token(Some(" s3cret ")).router();
    let ids = ids(4, 13);
    for token in [None, Some("wrong"), Some("s3cre"), Some("")] {
        let refused = post_with_token(
            &app,
            "/v1/mailbox/create",
            create_body(&ids[0], LABEL),
            token,
        )
        .await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED, "{token:?}");
        assert_eq!(
            refused.headers()["www-authenticate"],
            CREATE_TOKEN_HEADER,
            "{token:?}"
        );
    }
    let created = post_with_token(
        &app,
        "/v1/mailbox/create",
        create_body(&ids[0], LABEL),
        Some("s3cret"),
    )
    .await;
    assert_eq!(created.status(), StatusCode::OK);
    // Joining and everything after creation needs no token.
    let join = Join {
        mailbox: LABEL,
        join_token: TOKEN,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/join", &ids[1], join).await.0,
        StatusCode::OK
    );

    // Restoring creates a mailbox too: the first request needs the token, the rest don't.
    let mut rng = StdRng::seed_from_u64(14);
    let log = entries(VAULT, &ids[..3], 2, &mut rng);
    let reseed = |from, entries: &[LogEntry], finish| {
        signed_body(
            &ids[0],
            reseed_req([7; 16], &ids[..3], from, entries, finish),
        )
    };
    let refused = post_with_token(
        &app,
        "/v1/mailbox/reseed",
        reseed(0, &log[..1], false),
        None,
    )
    .await;
    assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    let started = post_with_token(
        &app,
        "/v1/mailbox/reseed",
        reseed(0, &log[..1], false),
        Some("s3cret"),
    )
    .await;
    assert_eq!(started.status(), StatusCode::OK);
    let finished =
        post_with_token(&app, "/v1/mailbox/reseed", reseed(1, &log[1..], true), None).await;
    assert_eq!(finished.status(), StatusCode::OK);
    // Another member learning where the open log is needs none either.
    let lookup = signed_body(&ids[1], reseed_req([7; 16], &ids[..3], 0, &[], true));
    assert_eq!(
        post_with_token(&app, "/v1/mailbox/reseed", lookup, None)
            .await
            .status(),
        StatusCode::OK
    );
}

#[tokio::test]
async fn wrong_create_tokens_count_against_the_creation_limit() {
    use zafe_relay::limits::{DailyRate, Limits};
    let app = relay()
        .with_limits(Limits {
            creates_per_ip: Some(DailyRate::new(24, 2)),
            client_ip_header: Some("fly-client-ip".into()),
            ..Limits::none()
        })
        .with_create_token(Some("s3cret"))
        .router();
    let ids = ids(3, 15);
    for (i, token) in ["guess1", "guess2"].into_iter().enumerate() {
        let refused = post_with_token(
            &app,
            "/v1/mailbox/create",
            create_body(&ids[i], [i as u8 + 20; 16]),
            Some(token),
        )
        .await;
        assert_eq!(refused.status(), StatusCode::UNAUTHORIZED);
    }
    // The address used its allowance on wrong guesses: even the right token waits now.
    let limited = post_with_token(
        &app,
        "/v1/mailbox/create",
        create_body(&ids[2], [30; 16]),
        Some("s3cret"),
    )
    .await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn a_relay_without_a_token_ignores_one() {
    let app = relay().with_create_token(Some("  ")).router();
    let ids = ids(1, 16);
    for (n, token) in [None, Some("anything")].into_iter().enumerate() {
        let created = post_with_token(
            &app,
            "/v1/mailbox/create",
            create_body(&ids[0], [n as u8 + 40; 16]),
            token,
        )
        .await;
        assert_eq!(created.status(), StatusCode::OK);
    }
    let (status, body) = get(&app, "/v1/info").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !serde_json::from_slice::<RelayInfo>(&body)
            .unwrap()
            .create_token
    );
}

#[tokio::test]
async fn mailboxes_have_at_most_64_members() {
    let app = relay().router();
    let ids = ids(1, 17);
    let create = |max_members| CreateMailbox {
        mailbox: LABEL,
        join_token_hash: join_token_hash(&TOKEN),
        max_members,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/create", &ids[0], create(65))
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        signed(&app, "/v1/mailbox/create", &ids[0], create(64))
            .await
            .0,
        StatusCode::OK
    );
}
