//! Storage quotas per mailbox: inbox count, delivery bytes, log bytes (HTTP 507).

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
        decode_body, join_token_hash, AppendResult, CreateMailbox, InboxAck, InboxAckResponse,
        InboxRead, InboxResponse, Join, Seal, Signed, MAX_ACK_CURSORS,
    },
    Envelope, Identity, Kind, LogEntry, LogKey,
};
use zafe_relay::{
    quota::{Quotas, Usage},
    Relay, DELIVERY_RETENTION_SECS,
};

const MAILBOX: [u8; 16] = [9; 16];
const T0: u64 = 1_790_000_000;
const TOKEN: [u8; 32] = [3; 32];

async fn call(app: &Router, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
    let r = app
        .clone()
        .oneshot(Request::post(path).body(Body::from(body)).unwrap())
        .await
        .unwrap();
    let status = r.status();
    (
        status,
        r.into_body().collect().await.unwrap().to_bytes().to_vec(),
    )
}

async fn signed<T: serde::Serialize + serde::de::DeserializeOwned>(
    app: &Router,
    path: &str,
    who: &Identity,
    payload: T,
) -> StatusCode {
    call(
        app,
        path,
        Signed::new(who, payload).unwrap().to_bytes().unwrap(),
    )
    .await
    .0
}

fn clock(now: &Arc<AtomicU64>) -> Arc<dyn Fn() -> u64 + Send + Sync> {
    let now = now.clone();
    Arc::new(move || now.load(Ordering::SeqCst))
}

/// A sealed mailbox with every `ids` as a member.
async fn mailbox(app: &Router, ids: &[Identity]) {
    let create = CreateMailbox {
        mailbox: MAILBOX,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: ids.len() as u16,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/create", &ids[0], create).await,
        StatusCode::OK
    );
    for id in &ids[1..] {
        let join = Join {
            mailbox: MAILBOX,
            join_token: TOKEN,
        };
        assert_eq!(
            signed(app, "/v1/mailbox/join", id, join).await,
            StatusCode::OK
        );
    }
    let members = ids.iter().map(|i| i.public().sig_pk).collect();
    let seal = Seal {
        mailbox: MAILBOX,
        members,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/seal", &ids[0], seal).await,
        StatusCode::OK
    );
}

fn to_one(from: &Identity, to: &Identity, seq: u64, rng: &mut StdRng) -> Vec<u8> {
    Envelope::sealed(from, to.public(), MAILBOX, seq, Kind::Approval, b"hi", rng)
        .unwrap()
        .to_bytes()
        .unwrap()
}

fn to_all(from: &Identity, seq: u64, payload: &[u8]) -> Vec<u8> {
    Envelope::public(from, MAILBOX, seq, Kind::Approval, payload)
        .unwrap()
        .to_bytes()
        .unwrap()
}

async fn post(app: &Router, envelope: Vec<u8>) -> (StatusCode, String) {
    let (status, body) = call(app, "/v1/envelope", envelope).await;
    (status, String::from_utf8_lossy(&body).into_owned())
}

#[tokio::test]
async fn a_full_inbox_refuses_more_envelopes_for_that_recipient() {
    let relay = Relay::new().with_quotas(Quotas {
        envelopes_per_recipient: Some(2),
        ..Quotas::none()
    });
    let app = relay.clone().router();
    let mut rng = StdRng::seed_from_u64(1);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    mailbox(&app, &ids).await;

    for seq in 1..=2 {
        let env = to_one(&ids[0], &ids[1], seq, &mut rng);
        assert_eq!(post(&app, env).await.0, StatusCode::OK);
    }
    let (status, body) = post(&app, to_one(&ids[0], &ids[1], 3, &mut rng)).await;
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    assert!(body.contains("inbox is full"), "{body}");
    // A broadcast reaching the full inbox is refused whole.
    assert_eq!(
        post(&app, to_all(&ids[0], 3, b"all")).await.0,
        StatusCode::INSUFFICIENT_STORAGE
    );
    assert_eq!(relay.usage(&MAILBOX).unwrap().deliveries, 2);

    // Other recipients still receive, and the refused sends did not use up seq 3.
    let env = to_one(&ids[0], &ids[2], 3, &mut rng);
    assert_eq!(post(&app, env).await.0, StatusCode::OK);
    // Another sender filling the same inbox is refused too (the cap is per recipient).
    let env = to_one(&ids[2], &ids[1], 1, &mut rng);
    assert_eq!(post(&app, env).await.0, StatusCode::INSUFFICIENT_STORAGE);
}

#[tokio::test]
async fn delivery_bytes_are_capped_and_pruning_frees_them() {
    let now = Arc::new(AtomicU64::new(T0));
    let mut rng = StdRng::seed_from_u64(2);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let payload = vec![0u8; 1000];
    let len = to_all(&ids[0], 1, &payload).len() as u64;
    // Room for two broadcasts to two recipients (4 stored copies), not a third.
    let relay = Relay::with_clock(clock(&now)).with_quotas(Quotas {
        delivery_bytes: Some(4 * len + len / 2),
        ..Quotas::none()
    });
    let app = relay.clone().router();
    mailbox(&app, &ids).await;

    for seq in 1..=2 {
        assert_eq!(
            post(&app, to_all(&ids[0], seq, &payload)).await.0,
            StatusCode::OK
        );
    }
    assert_eq!(
        relay.usage(&MAILBOX).unwrap(),
        Usage {
            deliveries: 4,
            delivery_bytes: 4 * len,
            log_bytes: 0
        },
        "a broadcast counts once per recipient"
    );
    let (status, body) = post(&app, to_all(&ids[0], 3, &payload)).await;
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    assert!(body.contains("undelivered"), "{body}");
    assert_eq!(relay.usage(&MAILBOX).unwrap().delivery_bytes, 4 * len);

    // A day later one more small envelope still fits; then retention frees the old ones.
    now.store(T0 + 86_400, Ordering::SeqCst);
    let small = to_one(&ids[0], &ids[1], 3, &mut rng);
    let small_len = small.len() as u64;
    assert_eq!(post(&app, small).await.0, StatusCode::OK);
    now.store(T0 + DELIVERY_RETENTION_SECS + 1, Ordering::SeqCst);
    assert_eq!(relay.prune().unwrap(), 4);
    assert_eq!(
        relay.usage(&MAILBOX).unwrap(),
        Usage {
            deliveries: 1,
            delivery_bytes: small_len,
            log_bytes: 0
        }
    );
    assert_eq!(
        post(&app, to_all(&ids[0], 4, &payload)).await.0,
        StatusCode::OK
    );
}

async fn append(app: &Router, entry: &LogEntry) -> (StatusCode, Vec<u8>) {
    call(app, "/v1/log/append", entry.to_bytes().unwrap()).await
}

#[tokio::test]
async fn the_log_is_capped() {
    let mut rng = StdRng::seed_from_u64(3);
    let ids: Vec<Identity> = (0..2).map(|_| Identity::generate(&mut rng)).collect();
    let key = LogKey::generate(0, &mut rng);
    let event = vec![1u8; 2000];
    let first = LogEntry::create(&ids[0], &key, MAILBOX, 0, [0; 32], &event, &mut rng).unwrap();
    let len = first.to_bytes().unwrap().len() as u64;
    let relay = Relay::new().with_quotas(Quotas {
        log_bytes: Some(2 * len),
        ..Quotas::none()
    });
    let app = relay.clone().router();
    mailbox(&app, &ids).await;

    let (status, _) = append(&app, &first).await;
    assert_eq!(status, StatusCode::OK);
    let second = LogEntry::create(
        &ids[1],
        &key,
        MAILBOX,
        1,
        first.hash().unwrap(),
        &event,
        &mut rng,
    )
    .unwrap();
    let (_, body) = append(&app, &second).await;
    assert_eq!(
        decode_body::<AppendResult>(&body).unwrap(),
        AppendResult::Appended { index: 1 }
    );
    assert_eq!(relay.usage(&MAILBOX).unwrap().log_bytes, 2 * len);

    let third = LogEntry::create(
        &ids[0],
        &key,
        MAILBOX,
        2,
        second.hash().unwrap(),
        b"x",
        &mut rng,
    )
    .unwrap();
    let (status, body) = append(&app, &third).await;
    assert_eq!(status, StatusCode::INSUFFICIENT_STORAGE);
    assert!(String::from_utf8_lossy(&body).contains("vault log"));
    assert_eq!(relay.usage(&MAILBOX).unwrap().log_bytes, 2 * len);
    // Pruning never touches the log.
    relay.prune().unwrap();
    assert_eq!(relay.usage(&MAILBOX).unwrap().log_bytes, 2 * len);
}

/// Fills a persisted relay with one broadcast and one log entry; returns the usage.
async fn fill(relay: &Relay, rng: &mut StdRng) -> Usage {
    let app = relay.clone().router();
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(rng)).collect();
    mailbox(&app, &ids).await;
    assert_eq!(
        post(&app, to_all(&ids[0], 1, &[5; 300])).await.0,
        StatusCode::OK
    );
    let key = LogKey::generate(0, rng);
    let entry = LogEntry::create(&ids[0], &key, MAILBOX, 0, [0; 32], b"created", rng).unwrap();
    assert_eq!(append(&app, &entry).await.0, StatusCode::OK);
    let usage = relay.usage(&MAILBOX).unwrap();
    assert_eq!(usage.deliveries, 2);
    assert!(
        usage.delivery_bytes > 600 && usage.log_bytes > 0,
        "{usage:?}"
    );
    usage
}

fn temp_db(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("zafe-relay-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("relay.sqlite");
    let _ = std::fs::remove_file(&path);
    (dir, path)
}

#[tokio::test]
async fn counters_survive_a_restart() {
    let (dir, path) = temp_db("quota-restart");
    let mut rng = StdRng::seed_from_u64(4);
    let before = fill(&Relay::open(&path).unwrap(), &mut rng).await;
    assert_eq!(Relay::open(&path).unwrap().usage(&MAILBOX).unwrap(), before);
    std::fs::remove_dir_all(&dir).ok();
}

/// Schema 1 had no counters (nor thresholds): opening it adds them and fills the counters
/// from the stored rows.
#[tokio::test]
async fn a_schema_1_database_is_migrated() {
    let (dir, path) = temp_db("quota-migrate");
    let mut rng = StdRng::seed_from_u64(5);
    let before = fill(&Relay::open(&path).unwrap(), &mut rng).await;

    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "ALTER TABLE mailboxes DROP COLUMN delivery_bytes;
         ALTER TABLE mailboxes DROP COLUMN log_bytes;
         ALTER TABLE mailboxes DROP COLUMN threshold;
         PRAGMA user_version = 1;",
    )
    .unwrap();
    drop(conn);

    let relay = Relay::open(&path).unwrap();
    assert_eq!(relay.usage(&MAILBOX).unwrap(), before);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let version: u16 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, zafe_proto::version::RELAY_DB);
    std::fs::remove_dir_all(&dir).ok();
}

/// Schema 2 had no thresholds: opening it adds the column (0: the vault can't move seats).
#[tokio::test]
async fn a_schema_2_database_is_migrated() {
    let (dir, path) = temp_db("threshold-migrate");
    let mut rng = StdRng::seed_from_u64(6);
    let before = fill(&Relay::open(&path).unwrap(), &mut rng).await;

    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        "ALTER TABLE mailboxes DROP COLUMN threshold;
         PRAGMA user_version = 2;",
    )
    .unwrap();
    drop(conn);

    let relay = Relay::open(&path).unwrap();
    assert_eq!(relay.usage(&MAILBOX).unwrap(), before);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let threshold: i64 = conn
        .query_row("SELECT threshold FROM mailboxes", [], |r| r.get(0))
        .unwrap();
    assert_eq!(threshold, 0);
    let version: u16 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(version, zafe_proto::version::RELAY_DB);
    std::fs::remove_dir_all(&dir).ok();
}

async fn inbox_cursors(app: &Router, who: &Identity) -> Vec<u64> {
    let read = InboxRead {
        mailbox: MAILBOX,
        after: 0,
        timestamp: T0,
    };
    let (status, body) = call(
        app,
        "/v1/inbox",
        Signed::new(who, read).unwrap().to_bytes().unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let inbox: InboxResponse = decode_body(&body).unwrap();
    inbox.envelopes.iter().map(|(c, _)| *c).collect()
}

async fn ack(app: &Router, who: &Identity, cursors: Vec<u64>) -> (StatusCode, u64) {
    let body = InboxAck {
        mailbox: MAILBOX,
        cursors,
        timestamp: T0,
    };
    let (status, bytes) = call(
        app,
        "/v1/inbox/ack",
        Signed::new(who, body).unwrap().to_bytes().unwrap(),
    )
    .await;
    let deleted = if status == StatusCode::OK {
        decode_body::<InboxAckResponse>(&bytes).unwrap().deleted
    } else {
        0
    };
    (status, deleted)
}

#[tokio::test]
async fn acknowledged_deliveries_are_deleted_and_free_their_bytes() {
    let now = Arc::new(AtomicU64::new(T0));
    let relay = Relay::with_clock(clock(&now)).with_quotas(Quotas::hosted());
    let app = relay.clone().router();
    let mut rng = StdRng::seed_from_u64(3);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    mailbox(&app, &ids).await;
    for seq in 1..=3 {
        let env = to_one(&ids[0], &ids[1], seq, &mut rng);
        assert_eq!(post(&app, env).await.0, StatusCode::OK);
    }
    let env = to_one(&ids[0], &ids[2], 4, &mut rng);
    assert_eq!(post(&app, env).await.0, StatusCode::OK);
    let before = relay.usage(&MAILBOX).unwrap();
    assert_eq!(before.deliveries, 4);

    let mine = inbox_cursors(&app, &ids[1]).await;
    let theirs = inbox_cursors(&app, &ids[2]).await;
    assert_eq!((mine.len(), theirs.len()), (3, 1));

    // Another member can't delete deliveries that aren't theirs; unknown cursors are ignored.
    assert_eq!(
        ack(&app, &ids[2], vec![mine[0], 999]).await,
        (StatusCode::OK, 0)
    );
    // The recipient deletes exactly the ones it names.
    assert_eq!(
        ack(&app, &ids[1], vec![mine[0], mine[2]]).await,
        (StatusCode::OK, 2)
    );
    assert_eq!(inbox_cursors(&app, &ids[1]).await, vec![mine[1]]);
    assert_eq!(inbox_cursors(&app, &ids[2]).await, theirs);
    let after = relay.usage(&MAILBOX).unwrap();
    assert_eq!(after.deliveries, 2);
    assert_eq!(
        after.delivery_bytes,
        before.delivery_bytes / 4 * 2,
        "the byte counter drops with the deleted envelopes (same size here)"
    );

    // Outsiders are refused; too many cursors in one request is a bad request.
    let outsider = Identity::generate(&mut rng);
    assert_eq!(
        ack(&app, &outsider, vec![mine[1]]).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        ack(&app, &ids[1], vec![1; MAX_ACK_CURSORS + 1]).await.0,
        StatusCode::BAD_REQUEST
    );
}

/// A recorded restore request can't be replayed later to give a mailbox an old member list.
#[tokio::test]
async fn a_stale_reseed_is_refused() {
    let app = Relay::new().router();
    let mut rng = StdRng::seed_from_u64(8);
    let ids: Vec<Identity> = (0..2).map(|_| Identity::generate(&mut rng)).collect();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let request = |timestamp| zafe_proto::relay::Reseed {
        mailbox: [5; 16],
        members: vec![*ids[0].public(), *ids[1].public()],
        threshold: 2,
        from: 0,
        entries: vec![],
        finish: false,
        timestamp,
    };
    assert_ne!(
        signed(&app, "/v1/mailbox/reseed", &ids[0], request(now - 3600)).await,
        StatusCode::OK
    );
    assert_ne!(
        signed(&app, "/v1/mailbox/reseed", &ids[0], request(now + 3600)).await,
        StatusCode::OK
    );
    assert_eq!(
        signed(&app, "/v1/mailbox/reseed", &ids[0], request(now)).await,
        StatusCode::OK
    );
}
