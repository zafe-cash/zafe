use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
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
        join_token_hash, CreateMailbox, Join, LabelledAppend, PostEnvelope, Reseed, Seal,
        SetThreshold, Signed, READ_BUDGET_BYTES,
    },
    Envelope, Identity, Kind, LogEntry, LogKey, MailboxId,
};
use zafe_relay::{quota::Quotas, Relay, MAX_BODY_BYTES};

const VAULT: MailboxId = [1; 16];
const NOW: u64 = 1_790_000_000;
const TOKEN: [u8; 32] = [4; 32];

async fn signed<T: serde::Serialize + serde::de::DeserializeOwned>(
    app: &Router,
    path: &str,
    who: &Identity,
    payload: T,
) -> StatusCode {
    let body = Signed::new(who, payload).unwrap().to_bytes().unwrap();
    let response = app
        .clone()
        .oneshot(Request::post(path).body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let _ = response.into_body().collect().await;
    status
}

fn relay(quotas: Quotas) -> Relay {
    Relay::with_clock(Arc::new(|| NOW)).with_quotas(quotas)
}

fn ids(n: usize, seed: u64) -> Vec<Identity> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..n).map(|_| Identity::generate(&mut rng)).collect()
}

async fn open_mailbox(app: &Router, ids: &[Identity]) {
    open_mailbox_at(app, VAULT, ids).await
}

async fn open_mailbox_at(app: &Router, mailbox: MailboxId, ids: &[Identity]) {
    let create = CreateMailbox {
        mailbox,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: ids.len() as u16,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/create", &ids[0], create).await,
        StatusCode::OK
    );
    let threshold = SetThreshold {
        mailbox,
        threshold: 2,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/threshold", &ids[0], threshold).await,
        StatusCode::OK
    );
    for id in &ids[1..] {
        let join = Join {
            mailbox,
            join_token: TOKEN,
        };
        assert_eq!(
            signed(app, "/v1/mailbox/join", id, join).await,
            StatusCode::OK
        );
    }
    let seal = Seal {
        mailbox,
        members: ids.iter().map(|i| i.public().sig_pk).collect(),
    };
    assert_eq!(
        signed(app, "/v1/mailbox/seal", &ids[0], seal).await,
        StatusCode::OK
    );
}

async fn append(app: &Router, who: &Identity, entry: &LogEntry) -> StatusCode {
    let req = LabelledAppend::new(VAULT, entry, NOW).unwrap();
    signed(app, "/v1/log/append-labelled", who, req).await
}

async fn send(
    app: &Router,
    from: &Identity,
    to: &Identity,
    seq: u64,
    payload: &[u8],
) -> StatusCode {
    send_at(app, VAULT, from, to, seq, payload).await
}

async fn send_at(
    app: &Router,
    mailbox: MailboxId,
    from: &Identity,
    to: &Identity,
    seq: u64,
    payload: &[u8],
) -> StatusCode {
    let mut rng = StdRng::seed_from_u64(seq);
    let envelope = Envelope::sealed(
        from,
        to.public(),
        mailbox,
        seq,
        Kind::DkgEcho,
        payload,
        &mut rng,
    )
    .unwrap()
    .to_bytes()
    .unwrap();
    signed(app, "/v1/envelope/send", from, PostEnvelope { envelope }).await
}

#[tokio::test]
async fn one_member_cant_fill_the_log_for_everyone() {
    let app = relay(Quotas {
        log_bytes: Some(256 * 1024),
        ..Quotas::none()
    })
    .router();
    let ids = ids(3, 1);
    open_mailbox(&app, &ids).await;
    let mut rng = StdRng::seed_from_u64(1);
    let key = LogKey::generate(0, &mut rng);
    let first = LogEntry::create(&ids[0], &key, VAULT, 0, [0; 32], b"created", &mut rng).unwrap();
    assert_eq!(append(&app, &ids[0], &first).await, StatusCode::OK);
    let mut head = first.hash().unwrap();
    let mut index = 1;

    let mut size = 16 * 1024;
    loop {
        let e = LogEntry::create(
            &ids[1],
            &key,
            VAULT,
            index,
            head,
            &vec![0u8; size],
            &mut rng,
        )
        .unwrap();
        let status = append(&app, &ids[1], &e).await;
        if status == StatusCode::OK {
            head = e.hash().unwrap();
            index += 1;
            continue;
        }
        assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
        if size == 0 {
            break;
        }
        size /= 2;
    }
    let vote = LogEntry::create(&ids[2], &key, VAULT, index, head, b"vote", &mut rng).unwrap();
    assert_eq!(
        append(&app, &ids[2], &vote).await,
        StatusCode::OK,
        "one member filled the log quota for every member"
    );
}

#[tokio::test]
async fn every_stored_entry_can_be_reseeded() {
    let app = relay(Quotas::none()).router();
    let ids = ids(3, 2);
    open_mailbox(&app, &ids).await;
    let mut rng = StdRng::seed_from_u64(2);
    let key = LogKey::generate(0, &mut rng);
    let first = LogEntry::create(&ids[0], &key, VAULT, 0, [0; 32], b"created", &mut rng).unwrap();
    assert_eq!(append(&app, &ids[0], &first).await, StatusCode::OK);
    let head = first.hash().unwrap();

    let mut size = MAX_BODY_BYTES;
    let big = loop {
        let e =
            LogEntry::create(&ids[1], &key, VAULT, 1, head, &vec![0u8; size], &mut rng).unwrap();
        if append(&app, &ids[1], &e).await == StatusCode::OK {
            break e;
        }
        size -= 256;
    };

    let new = relay(Quotas::none()).router();
    let req = |from: u64, entry: &LogEntry, finish: bool| Reseed {
        mailbox: VAULT,
        members: ids.iter().map(|i| *i.public()).collect(),
        threshold: 2,
        from,
        entries: vec![entry.to_bytes().unwrap()],
        finish,
        timestamp: NOW,
    };
    assert_eq!(
        signed(&new, "/v1/mailbox/reseed", &ids[2], req(0, &first, false)).await,
        StatusCode::OK
    );
    assert_eq!(
        signed(&new, "/v1/mailbox/reseed", &ids[2], req(1, &big, true)).await,
        StatusCode::OK,
        "an entry the old relay stored can't be copied to a new one"
    );
}

#[tokio::test]
async fn one_sender_cant_make_another_members_inbox_read_more_than_a_page() {
    let app = relay(Quotas::hosted()).router();
    let ids = ids(3, 3);
    open_mailbox(&app, &ids).await;
    let chunk = vec![0u8; 512 * 1024];
    let mut parked = 0usize;
    for seq in 1..=200u64 {
        if send(&app, &ids[0], &ids[1], seq, &chunk).await != StatusCode::OK {
            break;
        }
        parked += chunk.len();
    }
    assert!(
        parked <= READ_BUDGET_BYTES,
        "one member parked {} MiB in another member's inbox",
        parked >> 20
    );
}

#[tokio::test]
async fn a_large_vaults_pair_share_holds_one_full_envelope() {
    let app = relay(Quotas::hosted()).router();
    let ids = ids(64, 4);
    open_mailbox(&app, &ids).await;
    let request = vec![0u8; 70 * 1024];
    assert_eq!(
        send(&app, &ids[0], &ids[1], 1, &request).await,
        StatusCode::OK,
        "the leader's signing request doesn't fit an empty inbox share"
    );
}

#[tokio::test]
async fn a_move_keeps_a_members_used_log_share() {
    let quotas = Quotas {
        log_bytes: Some(128 * 1024),
        ..Quotas::none()
    };
    let app = relay(quotas).router();
    let ids = ids(3, 5);
    open_mailbox(&app, &ids).await;
    let mut rng = StdRng::seed_from_u64(5);
    let key = LogKey::generate(0, &mut rng);
    let first = LogEntry::create(&ids[0], &key, VAULT, 0, [0; 32], b"created", &mut rng).unwrap();
    assert_eq!(append(&app, &ids[0], &first).await, StatusCode::OK);
    let mut log = vec![first];
    let mut size = 8 * 1024;
    loop {
        let head = log.last().unwrap().hash().unwrap();
        let e = LogEntry::create(
            &ids[1],
            &key,
            VAULT,
            log.len() as u64,
            head,
            &vec![0u8; size],
            &mut rng,
        )
        .unwrap();
        if append(&app, &ids[1], &e).await == StatusCode::OK {
            log.push(e);
            continue;
        }
        if size == 0 {
            break;
        }
        size /= 2;
    }
    assert!(log.len() > 2);

    let new = relay(quotas).router();
    let reseed = Reseed {
        mailbox: VAULT,
        members: ids.iter().map(|i| *i.public()).collect(),
        threshold: 2,
        from: 0,
        entries: log.iter().map(|e| e.to_bytes().unwrap()).collect(),
        finish: true,
        timestamp: NOW,
    };
    assert_eq!(
        signed(&new, "/v1/mailbox/reseed", &ids[2], reseed).await,
        StatusCode::OK
    );
    let head = log.last().unwrap().hash().unwrap();
    let index = log.len() as u64;
    let more = LogEntry::create(&ids[1], &key, VAULT, index, head, b"more", &mut rng).unwrap();
    assert_eq!(
        append(&new, &ids[1], &more).await,
        StatusCode::INSUFFICIENT_STORAGE,
        "a move gave member 1 a fresh half"
    );
    let vote = LogEntry::create(&ids[2], &key, VAULT, index, head, b"vote", &mut rng).unwrap();
    assert_eq!(append(&new, &ids[2], &vote).await, StatusCode::OK);
}

fn db_used(path: &std::path::Path) -> u64 {
    let conn = rusqlite::Connection::open(path).unwrap();
    let get = |p: &str| -> u64 {
        conn.pragma_query_value(None, p, |r| r.get::<_, i64>(0))
            .unwrap() as u64
    };
    (get("page_count") - get("freelist_count")) * get("page_size")
}

#[tokio::test]
async fn a_nearly_full_relay_keeps_room_for_established_vaults() {
    const OLD: MailboxId = [2; 16];
    const YOUNG: MailboxId = [3; 16];
    let dir = std::env::temp_dir().join(format!("zafe-relay-nearly-full-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("relay.sqlite");
    let now = Arc::new(AtomicU64::new(NOW));
    let clock = {
        let now = now.clone();
        Arc::new(move || now.load(Ordering::SeqCst))
    };
    let relay = Relay::open_with_clock(&path, clock).unwrap();
    let app = relay.clone().router();
    let old = ids(2, 6);
    open_mailbox_at(&app, OLD, &old).await;
    now.store(NOW + 31 * 86_400, Ordering::SeqCst);
    let young = ids(2, 7);
    open_mailbox_at(&app, YOUNG, &young).await;

    let used = db_used(&path);
    let full = relay
        .with_quotas(Quotas {
            db_bytes: Some(used * 100 / 95),
            ..Quotas::none()
        })
        .router();
    assert_eq!(
        send_at(&full, YOUNG, &young[0], &young[1], 1, b"x").await,
        StatusCode::INSUFFICIENT_STORAGE,
        "a young mailbox still writes on a nearly full relay"
    );
    assert_eq!(
        send_at(&full, OLD, &old[0], &old[1], 1, b"x").await,
        StatusCode::OK,
        "an established vault lost its room"
    );
    let create = CreateMailbox {
        mailbox: [4; 16],
        join_token_hash: join_token_hash(&TOKEN),
        max_members: 2,
    };
    assert_eq!(
        signed(&full, "/v1/mailbox/create", &ids(1, 8)[0], create).await,
        StatusCode::INSUFFICIENT_STORAGE
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_large_vaults_leader_can_send_one_round() {
    let app = relay(Quotas::hosted()).router();
    let ids = ids(64, 5);
    open_mailbox(&app, &ids).await;
    let mut request = vec![0u8; 70 * 1024];
    rand::RngCore::fill_bytes(&mut StdRng::seed_from_u64(9), &mut request);
    for (seq, signer) in ids[1..33].iter().enumerate() {
        assert_eq!(
            send(&app, &ids[0], signer, seq as u64 + 1, &request).await,
            StatusCode::OK,
            "the leader's request to signer {} of 32 was refused",
            seq + 1
        );
    }
}
