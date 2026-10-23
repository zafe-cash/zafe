//! SQLite persistence, retention pruning and push hooks.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use tower::ServiceExt;
use zafe_proto::{
    relay::{
        decode_body, join_token_hash, AppendResult, CreateMailbox, InboxRead, InboxResponse, Join,
        LogRead, LogResponse, MembersRead, MembersResponse, PushPlatform, RegisterPush, Seal,
        Signed,
    },
    Envelope, Identity, Kind, LogEntry, LogKey,
};
use zafe_relay::{Notifier, Relay, DELIVERY_RETENTION_SECS};

const MAILBOX: [u8; 16] = [8; 16];
const T0: u64 = 1_790_000_000;

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
) -> (StatusCode, Vec<u8>) {
    call(
        app,
        path,
        Signed::new(who, payload).unwrap().to_bytes().unwrap(),
    )
    .await
}

fn clock(now: &Arc<AtomicU64>) -> Arc<dyn Fn() -> u64 + Send + Sync> {
    let now = now.clone();
    Arc::new(move || now.load(Ordering::SeqCst))
}

/// Creates a sealed 2-member mailbox with one envelope and one log entry.
async fn populate(app: &Router, ids: &[Identity], rng: &mut StdRng) -> LogKey {
    populate_with_head(app, ids, rng).await.0
}

/// Creates the mailbox with all `ids` as members, sends one envelope and appends the
/// first log entry. Returns the log key and the first entry's hash (the log head).
async fn populate_with_head(
    app: &Router,
    ids: &[Identity],
    rng: &mut StdRng,
) -> (LogKey, [u8; 32]) {
    let mut token = [0u8; 32];
    rng.fill_bytes(&mut token);
    let create = CreateMailbox {
        mailbox: MAILBOX,
        join_token_hash: join_token_hash(&token),
        max_members: ids.len() as u16,
    };
    assert_eq!(
        signed(app, "/v1/mailbox/create", &ids[0], create).await.0,
        StatusCode::OK
    );
    for id in &ids[1..] {
        assert_eq!(
            signed(
                app,
                "/v1/mailbox/join",
                id,
                Join {
                    mailbox: MAILBOX,
                    join_token: token
                }
            )
            .await
            .0,
            StatusCode::OK
        );
    }
    let members = ids.iter().map(|i| i.public().sig_pk).collect();
    assert_eq!(
        signed(
            app,
            "/v1/mailbox/seal",
            &ids[0],
            Seal {
                mailbox: MAILBOX,
                members
            }
        )
        .await
        .0,
        StatusCode::OK
    );
    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        MAILBOX,
        1,
        Kind::Approval,
        b"hello",
        rng,
    )
    .unwrap();
    assert_eq!(
        call(app, "/v1/envelope", env.to_bytes().unwrap()).await.0,
        StatusCode::OK
    );
    let key = LogKey::generate(0, rng);
    let entry = LogEntry::create(&ids[0], &key, MAILBOX, 0, [0; 32], b"created", rng).unwrap();
    let (_, body) = call(app, "/v1/log/append", entry.to_bytes().unwrap()).await;
    assert_eq!(
        decode_body::<AppendResult>(&body).unwrap(),
        AppendResult::Appended { index: 0 }
    );
    (key, entry.hash().unwrap())
}

async fn inbox_len(app: &Router, who: &Identity, now: u64) -> usize {
    let (status, body) = signed(
        app,
        "/v1/inbox",
        who,
        InboxRead {
            mailbox: MAILBOX,
            after: 0,
            timestamp: now,
        },
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    decode_body::<InboxResponse>(&body).unwrap().envelopes.len()
}

#[tokio::test]
async fn state_survives_a_restart() {
    let dir = std::env::temp_dir().join(format!("zafe-relay-persist-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("relay.sqlite");
    let _ = std::fs::remove_file(&path);
    let now = Arc::new(AtomicU64::new(T0));
    let mut rng = StdRng::seed_from_u64(1);
    let ids: Vec<Identity> = (0..2).map(|_| Identity::generate(&mut rng)).collect();

    let key = {
        let app = Relay::open_with_clock(&path, clock(&now)).unwrap().router();
        populate(&app, &ids, &mut rng).await
    }; // relay dropped: connection closed

    let app = Relay::open_with_clock(&path, clock(&now)).unwrap().router();
    let (_, body) = signed(
        &app,
        "/v1/mailbox/members",
        &ids[1],
        MembersRead {
            mailbox: MAILBOX,
            timestamp: T0,
        },
    )
    .await;
    let members = decode_body::<MembersResponse>(&body).unwrap();
    assert!(members.sealed);
    assert_eq!(members.members.len(), 2);
    assert_eq!(inbox_len(&app, &ids[1], T0).await, 1);

    let (_, body) = signed(
        &app,
        "/v1/log/read",
        &ids[1],
        LogRead {
            mailbox: MAILBOX,
            from: 0,
            timestamp: T0,
        },
    )
    .await;
    let entries = decode_body::<LogResponse>(&body)
        .unwrap()
        .decoded()
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].decrypt(&key).unwrap(), b"created");

    // Replay protection survives too: the same envelope seq is still refused.
    let replay = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        MAILBOX,
        1,
        Kind::Approval,
        b"again",
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        call(&app, "/v1/envelope", replay.to_bytes().unwrap())
            .await
            .0,
        StatusCode::CONFLICT
    );
    // And the log head: appending at index 0 again conflicts.
    let dup = LogEntry::create(&ids[1], &key, MAILBOX, 0, [0; 32], b"x", &mut rng).unwrap();
    let (_, body) = call(&app, "/v1/log/append", dup.to_bytes().unwrap()).await;
    assert_eq!(
        decode_body::<AppendResult>(&body).unwrap(),
        AppendResult::Conflict { len: 1 }
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn old_deliveries_are_pruned_but_the_log_is_kept() {
    let now = Arc::new(AtomicU64::new(T0));
    let relay = Relay::with_clock(clock(&now));
    let app = relay.clone().router();
    let mut rng = StdRng::seed_from_u64(2);
    let ids: Vec<Identity> = (0..2).map(|_| Identity::generate(&mut rng)).collect();
    populate(&app, &ids, &mut rng).await;

    assert_eq!(relay.prune().unwrap(), 0, "fresh deliveries stay");
    now.store(T0 + DELIVERY_RETENTION_SECS + 1, Ordering::SeqCst);
    assert_eq!(relay.prune().unwrap(), 1);
    let later = T0 + DELIVERY_RETENTION_SECS + 1;
    assert_eq!(inbox_len(&app, &ids[1], later).await, 0);
    let (_, body) = signed(
        &app,
        "/v1/log/read",
        &ids[1],
        LogRead {
            mailbox: MAILBOX,
            from: 0,
            timestamp: later,
        },
    )
    .await;
    assert_eq!(decode_body::<LogResponse>(&body).unwrap().entries.len(), 1);
}

#[derive(Default)]
struct Recorder(Mutex<Vec<(PushPlatform, String)>>);

impl Notifier for Recorder {
    fn notify(&self, platform: PushPlatform, token: &str) {
        self.0.lock().unwrap().push((platform, token.to_owned()));
    }
}

#[tokio::test]
async fn deliveries_push_to_registered_recipients_only() {
    let now = Arc::new(AtomicU64::new(T0));
    let recorder = Arc::new(Recorder::default());
    let app = Relay::with_clock(clock(&now))
        .with_notifier(recorder.clone())
        .router();
    let mut rng = StdRng::seed_from_u64(3);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    populate(&app, &ids[..2], &mut rng).await;
    assert!(
        recorder.0.lock().unwrap().is_empty(),
        "no tokens registered yet"
    );

    // Only members can register; an outsider is refused.
    let outsider = RegisterPush {
        mailbox: MAILBOX,
        platform: PushPlatform::Fcm,
        token: "t".into(),
    };
    assert_eq!(
        signed(&app, "/v1/push/register", &ids[2], outsider).await.0,
        StatusCode::FORBIDDEN
    );
    for (i, token) in [(0, "apns-token-A"), (1, "fcm-token-B")] {
        let platform = if i == 0 {
            PushPlatform::Apns
        } else {
            PushPlatform::Fcm
        };
        let req = RegisterPush {
            mailbox: MAILBOX,
            platform,
            token: token.into(),
        };
        assert_eq!(
            signed(&app, "/v1/push/register", &ids[i], req).await.0,
            StatusCode::OK
        );
    }

    // A sealed envelope to member 1 pushes to member 1 only, with no content.
    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        MAILBOX,
        2,
        Kind::SigningRequest,
        b"secret",
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        call(&app, "/v1/envelope", env.to_bytes().unwrap()).await.0,
        StatusCode::OK
    );
    assert_eq!(
        *recorder.0.lock().unwrap(),
        vec![(PushPlatform::Fcm, "fcm-token-B".to_owned())]
    );
}

/// An app moving to FCM installation IDs re-registers: the FID replaces its old token
/// (one target per member), so it is pushed once, as `FcmFid` (sent in FCM's `fid` field).
#[tokio::test]
async fn an_installation_id_replaces_the_old_token() {
    let now = Arc::new(AtomicU64::new(T0));
    let recorder = Arc::new(Recorder::default());
    let app = Relay::with_clock(clock(&now))
        .with_notifier(recorder.clone())
        .router();
    let mut rng = StdRng::seed_from_u64(4);
    let ids: Vec<Identity> = (0..2).map(|_| Identity::generate(&mut rng)).collect();
    populate(&app, &ids, &mut rng).await;
    for (platform, token) in [
        (PushPlatform::Fcm, "old-token"),
        (PushPlatform::FcmFid, "fid-1"),
    ] {
        let req = RegisterPush {
            mailbox: MAILBOX,
            platform,
            token: token.into(),
        };
        assert_eq!(
            signed(&app, "/v1/push/register", &ids[1], req).await.0,
            StatusCode::OK
        );
    }
    let env = Envelope::sealed(
        &ids[0],
        ids[1].public(),
        MAILBOX,
        2,
        Kind::SigningRequest,
        b"secret",
        &mut rng,
    )
    .unwrap();
    assert_eq!(
        call(&app, "/v1/envelope", env.to_bytes().unwrap()).await.0,
        StatusCode::OK
    );
    assert_eq!(
        *recorder.0.lock().unwrap(),
        vec![(PushPlatform::FcmFid, "fid-1".to_owned())]
    );
}

#[tokio::test]
async fn log_appends_push_every_other_member() {
    let now = Arc::new(AtomicU64::new(T0));
    let recorder = Arc::new(Recorder::default());
    let relay = Relay::with_clock(clock(&now)).with_notifier(recorder.clone());
    let app = relay.clone().router();
    let mut rng = StdRng::seed_from_u64(4);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let (key, head) = populate_with_head(&app, &ids[..3], &mut rng).await;
    recorder.0.lock().unwrap().clear();
    for (i, token) in ["tok-0", "tok-1", "tok-2"].iter().enumerate() {
        let req = RegisterPush {
            mailbox: MAILBOX,
            platform: PushPlatform::Fcm,
            token: (*token).into(),
        };
        assert_eq!(
            signed(&app, "/v1/push/register", &ids[i], req).await.0,
            StatusCode::OK
        );
    }
    let entry = LogEntry::create(&ids[1], &key, MAILBOX, 1, head, b"vote", &mut rng).unwrap();
    let (_, body) = call(&app, "/v1/log/append", entry.to_bytes().unwrap()).await;
    assert_eq!(
        decode_body::<AppendResult>(&body).unwrap(),
        AppendResult::Appended { index: 1 }
    );
    let mut pushed: Vec<String> = recorder
        .0
        .lock()
        .unwrap()
        .iter()
        .map(|(_, t)| t.clone())
        .collect();
    pushed.sort();
    assert_eq!(
        pushed,
        vec!["tok-0".to_owned(), "tok-2".to_owned()],
        "author is not pushed"
    );

    // A token FCM reported as unregistered is forgotten: no more pushes to it.
    assert_eq!(relay.forget_push_token("tok-0").unwrap(), 1);
    recorder.0.lock().unwrap().clear();
    let next = LogEntry::create(
        &ids[1],
        &key,
        MAILBOX,
        2,
        entry.hash().unwrap(),
        b"vote",
        &mut rng,
    )
    .unwrap();
    call(&app, "/v1/log/append", next.to_bytes().unwrap()).await;
    assert_eq!(
        *recorder.0.lock().unwrap(),
        vec![(PushPlatform::Fcm, "tok-2".to_owned())]
    );
}

/// The database records its schema version; one written by a newer relay is refused
/// instead of being misread.
#[test]
fn database_schema_is_versioned() {
    use zafe_proto::version;
    let dir = std::env::temp_dir().join(format!("zafe-relay-schema-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("relay.sqlite");
    let _ = std::fs::remove_file(&path);

    drop(Relay::open(&path).unwrap());
    let conn = rusqlite::Connection::open(&path).unwrap();
    let stored: u16 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .unwrap();
    assert_eq!(stored, version::RELAY_DB);
    assert!(Relay::open(&path).is_ok(), "reopens its own version");

    conn.pragma_update(None, "user_version", version::RELAY_DB + 1)
        .unwrap();
    drop(conn);
    assert!(matches!(
        Relay::open(&path),
        Err(zafe_relay::RelayError::UnsupportedVersion(v)) if v.is_newer()
    ));

    // A database from before versioning (tables, no version) is refused too.
    let old = dir.join("old.sqlite");
    let _ = std::fs::remove_file(&old);
    rusqlite::Connection::open(&old)
        .unwrap()
        .execute_batch("CREATE TABLE mailboxes (id BLOB PRIMARY KEY);")
        .unwrap();
    assert!(matches!(
        Relay::open(&old),
        Err(zafe_relay::RelayError::UnsupportedVersion(v)) if v.found == 0 && !v.is_newer()
    ));
    let _ = std::fs::remove_dir_all(&dir);
}
