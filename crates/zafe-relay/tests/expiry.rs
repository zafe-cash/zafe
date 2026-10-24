//! Relays own their data (`docs/relay-move.md` §6): mailboxes nobody uses expire.

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
        join_token_hash, CreateMailbox, Join, LogRead, PushPlatform, RegisterPush, RenewInvite,
        Reseed, Seal, Signed,
    },
    Envelope, Identity, Kind, LogEntry, LogKey, MailboxId,
};
use zafe_relay::{expiry::Expiry, quota::Quotas, Relay};

const DAY: u64 = 86_400;
/// Midnight, so `T0 + n * DAY` is day n.
const T0: u64 = 20_700 * DAY;
const LABEL: MailboxId = [2; 16];
const VAULT: MailboxId = [1; 16];
const TOKEN: [u8; 32] = [4; 32];

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
) -> StatusCode {
    call(
        app,
        path,
        Signed::new(who, payload).unwrap().to_bytes().unwrap(),
    )
    .await
    .0
}

struct World {
    now: Arc<AtomicU64>,
    relay: Relay,
    app: Router,
    ids: Vec<Identity>,
    rng: StdRng,
    dir: std::path::PathBuf,
    path: std::path::PathBuf,
}

impl World {
    /// A relay on a file (so tests can look inside), with the hosted expiry rules.
    fn new(name: &str, quotas: Quotas) -> Self {
        let dir = std::env::temp_dir().join(format!("zafe-expiry-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("relay.sqlite");
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
        let now = Arc::new(AtomicU64::new(T0));
        let clock = {
            let now = now.clone();
            Arc::new(move || now.load(Ordering::SeqCst))
        };
        let relay = Relay::open_with_clock(&path, clock)
            .unwrap()
            .with_expiry(Expiry::hosted())
            .with_quotas(quotas);
        let mut rng = StdRng::seed_from_u64(1);
        let ids = (0..3).map(|_| Identity::generate(&mut rng)).collect();
        Self {
            now,
            app: relay.clone().router(),
            relay,
            ids,
            rng,
            dir,
            path,
        }
    }

    fn day(&self, n: u64) {
        self.now.store(T0 + n * DAY + 3600, Ordering::SeqCst);
    }

    fn time(&self) -> u64 {
        self.now.load(Ordering::SeqCst)
    }

    /// Member `who` reads the log of `mailbox`.
    async fn read(&self, who: usize, mailbox: MailboxId) -> StatusCode {
        let read = LogRead {
            mailbox,
            from: 0,
            timestamp: self.time(),
        };
        signed(&self.app, "/v1/log/read", &self.ids[who], read).await
    }

    /// Member 0 restores a two-entry vault log under `label` (every member listed).
    async fn reseed(&mut self, label: MailboxId) -> StatusCode {
        let key = LogKey::generate(0, &mut self.rng);
        let first =
            LogEntry::create(&self.ids[0], &key, VAULT, 0, [0; 32], b"a", &mut self.rng).unwrap();
        let head = first.hash().unwrap();
        let second =
            LogEntry::create(&self.ids[1], &key, VAULT, 1, head, b"b", &mut self.rng).unwrap();
        let reseed = Reseed {
            mailbox: label,
            members: self.ids.iter().map(|i| *i.public()).collect(),
            threshold: 2,
            from: 0,
            entries: vec![first.to_bytes().unwrap(), second.to_bytes().unwrap()],
            finish: true,
            timestamp: self.time(),
        };
        signed(&self.app, "/v1/mailbox/reseed", &self.ids[0], reseed).await
    }

    /// A vault set up the usual way under `mailbox`: created, joined, sealed, one log
    /// entry, one envelope, a push token. Every table has rows for it.
    async fn vault(&mut self, mailbox: MailboxId) {
        let create = CreateMailbox {
            mailbox,
            join_token_hash: join_token_hash(&TOKEN),
            max_members: 3,
        };
        assert_eq!(
            signed(&self.app, "/v1/mailbox/create", &self.ids[0], create).await,
            StatusCode::OK
        );
        for id in &self.ids[1..] {
            let join = Join {
                mailbox,
                join_token: TOKEN,
            };
            assert_eq!(
                signed(&self.app, "/v1/mailbox/join", id, join).await,
                StatusCode::OK
            );
        }
        let seal = Seal {
            mailbox,
            members: self.ids.iter().map(|i| i.public().sig_pk).collect(),
        };
        assert_eq!(
            signed(&self.app, "/v1/mailbox/seal", &self.ids[0], seal).await,
            StatusCode::OK
        );
        let key = LogKey::generate(0, &mut self.rng);
        let entry =
            LogEntry::create(&self.ids[0], &key, mailbox, 0, [0; 32], b"c", &mut self.rng).unwrap();
        assert_eq!(
            call(&self.app, "/v1/log/append", entry.to_bytes().unwrap())
                .await
                .0,
            StatusCode::OK
        );
        let env = Envelope::public(&self.ids[1], mailbox, 1, Kind::Approval, b"hi").unwrap();
        assert_eq!(
            call(&self.app, "/v1/envelope", env.to_bytes().unwrap())
                .await
                .0,
            StatusCode::OK
        );
        let push = RegisterPush {
            mailbox,
            platform: PushPlatform::Fcm,
            token: "t".into(),
        };
        assert_eq!(
            signed(&self.app, "/v1/push/register", &self.ids[2], push).await,
            StatusCode::OK
        );
    }

    /// Rows stored for `mailbox`, per table.
    fn rows(&self, mailbox: MailboxId) -> Vec<(&'static str, i64)> {
        let conn = rusqlite::Connection::open(&self.path).unwrap();
        let mut out = vec![(
            "mailboxes",
            conn.query_row(
                "SELECT COUNT(*) FROM mailboxes WHERE id = ?1",
                [&mailbox[..]],
                |r| r.get(0),
            )
            .unwrap(),
        )];
        for table in [
            "members",
            "deliveries",
            "sender_seqs",
            "push_tokens",
            "log_entries",
        ] {
            let n = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE mailbox = ?1"),
                    [&mailbox[..]],
                    |r| r.get(0),
                )
                .unwrap();
            out.push((table, n));
        }
        out
    }
}

impl Drop for World {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Members who only read keep a restored vault: expiry counts requests, not appends.
#[tokio::test]
async fn a_quiet_restored_vault_survives() {
    let mut w = World::new("quiet", Quotas::none());
    assert_eq!(w.reseed(LABEL).await, StatusCode::OK);
    w.day(5);
    assert_eq!(w.read(1, LABEL).await, StatusCode::OK);
    w.day(40);
    assert_eq!(w.relay.expire().unwrap(), 0);
    // A year after the last member request it is idle.
    w.day(5 + 365);
    assert_eq!(w.relay.expire().unwrap(), 0);
    w.day(5 + 366);
    assert_eq!(w.relay.expire().unwrap(), 1);
    assert!(
        w.rows(LABEL).iter().all(|(_, n)| *n == 0),
        "{:?}",
        w.rows(LABEL)
    );
    assert_eq!(w.read(1, LABEL).await, StatusCode::NOT_FOUND);
}

/// A restore (or a move) that no other member followed is deleted after 30 days, however
/// often its maker looks at it, and stops counting against the maker's quota.
#[tokio::test]
async fn an_unused_reseed_expires() {
    let mut w = World::new(
        "unused",
        Quotas {
            mailboxes_per_key: Some(1),
            ..Quotas::none()
        },
    );
    assert_eq!(w.reseed(LABEL).await, StatusCode::OK);
    assert_eq!(
        w.reseed([3; 16]).await,
        StatusCode::INSUFFICIENT_STORAGE,
        "one mailbox per key"
    );
    for day in [1, 10, 29, 30] {
        w.day(day);
        assert_eq!(w.read(0, LABEL).await, StatusCode::OK);
        assert_eq!(w.relay.expire().unwrap(), 0, "day {day}");
    }
    w.day(31);
    assert_eq!(w.relay.expire().unwrap(), 1);
    assert!(
        w.rows(LABEL).iter().all(|(_, n)| *n == 0),
        "{:?}",
        w.rows(LABEL)
    );
    assert_eq!(w.reseed([3; 16]).await, StatusCode::OK);
}

/// A setup nobody joined goes the same way, even if its creator keeps renewing the invite.
#[tokio::test]
async fn a_setup_nobody_joined_expires() {
    let w = World::new("setup", Quotas::none());
    let create = CreateMailbox {
        mailbox: LABEL,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: 3,
    };
    assert_eq!(
        signed(&w.app, "/v1/mailbox/create", &w.ids[0], create).await,
        StatusCode::OK
    );
    for day in [2, 20, 30] {
        w.day(day);
        let renew = RenewInvite { mailbox: LABEL };
        assert_eq!(
            signed(&w.app, "/v1/mailbox/renew", &w.ids[0], renew).await,
            StatusCode::OK
        );
    }
    w.day(31);
    assert_eq!(w.relay.expire().unwrap(), 1);
}

/// An idle vault is deleted with every row stored for it; another vault in use stays.
#[tokio::test]
async fn an_idle_vault_expires_from_every_table() {
    let mut w = World::new("idle", Quotas::none());
    const OTHER: MailboxId = [6; 16];
    w.vault(LABEL).await;
    w.vault(OTHER).await;
    assert!(
        w.rows(LABEL).iter().all(|(_, n)| *n > 0),
        "{:?}",
        w.rows(LABEL)
    );
    for day in [100, 300, 365] {
        w.day(day);
        assert_eq!(w.read(2, OTHER).await, StatusCode::OK);
        assert_eq!(w.relay.expire().unwrap(), 0, "day {day}");
    }
    w.day(366);
    assert_eq!(w.relay.expire().unwrap(), 1);
    assert!(
        w.rows(LABEL).iter().all(|(_, n)| *n == 0),
        "{:?}",
        w.rows(LABEL)
    );
    assert!(
        w.rows(OTHER).iter().all(|(_, n)| *n > 0),
        "{:?}",
        w.rows(OTHER)
    );
}

/// Last-seen is written at most once per mailbox per day, so reads don't each add a write.
#[tokio::test]
async fn last_seen_is_written_once_a_day() {
    let mut w = World::new("once", Quotas::none());
    w.vault(LABEL).await;
    // Another connection holds the write lock: a request that tried to write would wait for
    // it and fail, one that only reads goes through.
    let observer = rusqlite::Connection::open(&w.path).unwrap();
    let last_seen = || -> i64 {
        observer
            .query_row(
                "SELECT last_seen_day FROM mailboxes WHERE id = ?1",
                [&LABEL[..]],
                |r| r.get(0),
            )
            .unwrap()
    };
    observer.execute_batch("BEGIN IMMEDIATE").unwrap();
    for _ in 0..3 {
        assert_eq!(w.read(1, LABEL).await, StatusCode::OK);
        assert_eq!(w.read(2, LABEL).await, StatusCode::OK);
    }
    observer.execute_batch("COMMIT").unwrap();
    assert_eq!(last_seen(), (T0 / DAY) as i64);

    // The first request of a new day writes, the rest of the day doesn't.
    w.day(1);
    assert_eq!(w.read(1, LABEL).await, StatusCode::OK);
    assert_eq!(last_seen(), (T0 / DAY + 1) as i64);
    observer.execute_batch("BEGIN IMMEDIATE").unwrap();
    for who in [0, 1, 2] {
        assert_eq!(w.read(who, LABEL).await, StatusCode::OK);
    }
    observer.execute_batch("COMMIT").unwrap();
}

/// Forgetting a push token is a phone leaving, not use: it doesn't keep a mailbox alive.
#[tokio::test]
async fn forgetting_a_push_token_does_not_count_as_use() {
    let mut w = World::new("forget", Quotas::none());
    w.vault(LABEL).await;
    w.day(200);
    let forget = zafe_proto::relay::ForgetPush {
        mailbox: LABEL,
        timestamp: w.time(),
    };
    assert_eq!(
        signed(&w.app, "/v1/push/forget", &w.ids[2], forget).await,
        StatusCode::OK
    );
    w.day(366);
    assert_eq!(w.relay.expire().unwrap(), 1);
}

#[tokio::test]
async fn expiry_is_off_unless_configured() {
    let now = Arc::new(AtomicU64::new(T0));
    let clock = {
        let now = now.clone();
        Arc::new(move || now.load(Ordering::SeqCst))
    };
    let relay = Relay::with_clock(clock);
    let app = relay.clone().router();
    let mut rng = StdRng::seed_from_u64(2);
    let creator = Identity::generate(&mut rng);
    let create = CreateMailbox {
        mailbox: LABEL,
        join_token_hash: join_token_hash(&TOKEN),
        max_members: 3,
    };
    assert_eq!(
        signed(&app, "/v1/mailbox/create", &creator, create).await,
        StatusCode::OK
    );
    now.fetch_add(5000 * DAY, Ordering::SeqCst);
    assert_eq!(relay.expire().unwrap(), 0);
    let relay = relay.with_expiry(Expiry {
        idle_days: Some(1000),
        unused_days: None,
    });
    assert_eq!(relay.expire().unwrap(), 1);
}
