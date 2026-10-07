//! Zafe relay (spec §6): a blind store-and-forward server.
//!
//! It holds mailboxes, member public keys, signed envelopes (sealed ones are opaque), the
//! encrypted vault log and push tokens. It checks signatures, membership, sequence numbers
//! and log chaining, and never has keys that decrypt anything.
//!
//! State is stored in SQLite (a file, or in memory for tests). That keeps the relay a
//! single self-contained binary, which suits self-hosting; the hosted tier can move the
//! same queries to Postgres.
//!
//! Versions: request bodies are tagged (`zafe_proto::version`); a body in a version this
//! relay doesn't speak gets HTTP 426 with `zafe-supported-version`. The database schema
//! version is `PRAGMA user_version` ([`version::RELAY_DB`]); a newer database is refused.
//!
//! Limits: request rates per key and IP ([`limits`]) and storage per mailbox ([`quota`]),
//! both off in [`Relay::new`] and on in the `zafe-relay` binary. Long polls
//! (`/v1/wait`, [`wait`]) are capped per key and in total, always.

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Bytes,
    extract::{ConnectInfo, DefaultBodyLimit, Extension, Request, State},
    http::StatusCode,
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rusqlite::{params, Connection, OptionalExtension};
pub mod fcm;
pub mod limits;
pub mod quota;
pub mod wait;

use limits::{Buckets, Limits};
use quota::{Quota, Quotas, Usage};
use wait::Waiters;

use zafe_proto::{
    log::GENESIS_PREV_HASH,
    relay::{
        encode_body, join_token_hash, AppendResult, CreateMailbox, InboxAck, InboxAckResponse,
        InboxRead, InboxResponse, Join, LogRead, LogResponse, MailboxesRead, MailboxesResponse,
        MembersRead, MembersResponse, PushPlatform, RegisterPush, Remove, ReplaceMember, Reseed,
        ReseedResponse, Seal, SetThreshold, Signed, WaitRequest, WaitResponse, MAX_ACK_CURSORS,
        MAX_REQUEST_SKEW_SECS, MAX_WAIT_SECS, UNSUPPORTED_VERSION_HEADER,
    },
    version::{self, UnsupportedVersion},
    Envelope, IdentityPublic, LogEntry, MailboxId, ProtoError, Recipient,
};

/// Largest number of items returned by one read.
const PAGE: i64 = 500;

/// Largest request body. The biggest real bodies are log entries carrying a proposal's
/// PCZT (tens of KB for a few payments; a 50-recipient batch stays well under this) and
/// commitment batches (256 x ~70 bytes).
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Undelivered envelopes older than this are pruned (spec §6.1).
pub const DELIVERY_RETENTION_SECS: u64 = 30 * 24 * 3600;

type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Sends content-free "vault activity" pushes. Real APNs/FCM senders plug in here (M1).
pub trait Notifier: Send + Sync {
    fn notify(&self, platform: PushPlatform, token: &str);
}

/// Default notifier: logs that a push would be sent.
pub struct LogNotifier;

impl Notifier for LogNotifier {
    fn notify(&self, platform: PushPlatform, _token: &str) {
        tracing::debug!(platform = platform.as_str(), "push: vault activity");
    }
}

#[derive(Clone)]
pub struct Relay {
    db: Arc<Mutex<Connection>>,
    clock: Clock,
    notifier: Arc<dyn Notifier>,
    per_key: Option<Arc<Buckets<[u8; 32]>>>,
    per_ip: Option<Arc<Buckets<std::net::IpAddr>>>,
    creates_per_ip: Option<Arc<Buckets<std::net::IpAddr>>>,
    client_ip_header: Option<String>,
    quotas: Quotas,
    waiters: Arc<Waiters>,
}

/// The client address `limit_ip` found for a request (None: unknown, e.g. in tests).
#[derive(Clone, Copy)]
struct ClientIp(Option<std::net::IpAddr>);

#[derive(Debug, thiserror::Error)]
pub enum RelayError {
    #[error("malformed request")]
    BadRequest,
    #[error("invalid signature")]
    Unauthenticated,
    #[error("not allowed")]
    Forbidden,
    #[error("unknown mailbox")]
    NotFound,
    #[error("mailbox already exists")]
    Exists,
    #[error("request timestamp outside the allowed window")]
    Stale,
    #[error("replayed or reordered envelope")]
    Replay,
    #[error("storage error")]
    Storage,
    /// Too many requests from one key or address; retry after this many seconds.
    #[error("too many requests; retry in {0} s")]
    RateLimited(u64),
    /// The write would take the mailbox over a storage quota (HTTP 507 Insufficient
    /// Storage; see [`quota`]). Nothing was stored.
    #[error("storage quota exceeded: {0}")]
    QuotaExceeded(Quota),
    /// A request in a version this relay doesn't speak (HTTP 426), or a database written
    /// by a newer relay.
    #[error(transparent)]
    UnsupportedVersion(#[from] UnsupportedVersion),
}

/// Keeps version errors apart from malformed requests.
fn bad_request(e: ProtoError) -> RelayError {
    match e {
        ProtoError::UnsupportedVersion(v) => RelayError::UnsupportedVersion(v),
        _ => RelayError::BadRequest,
    }
}

impl From<rusqlite::Error> for RelayError {
    fn from(e: rusqlite::Error) -> Self {
        tracing::error!("storage: {e}");
        RelayError::Storage
    }
}

impl IntoResponse for RelayError {
    fn into_response(self) -> Response {
        if let RelayError::RateLimited(secs) = self {
            return (
                StatusCode::TOO_MANY_REQUESTS,
                [(axum::http::header::RETRY_AFTER, secs.to_string())],
                self.to_string(),
            )
                .into_response();
        }
        if let RelayError::UnsupportedVersion(v) = self {
            return (
                StatusCode::UPGRADE_REQUIRED,
                [(UNSUPPORTED_VERSION_HEADER, v.supported.to_string())],
                self.to_string(),
            )
                .into_response();
        }
        let status = match self {
            RelayError::BadRequest | RelayError::Stale => StatusCode::BAD_REQUEST,
            RelayError::Unauthenticated => StatusCode::UNAUTHORIZED,
            RelayError::Forbidden => StatusCode::FORBIDDEN,
            RelayError::NotFound => StatusCode::NOT_FOUND,
            RelayError::Exists | RelayError::Replay => StatusCode::CONFLICT,
            RelayError::Storage | RelayError::UnsupportedVersion(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            RelayError::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            RelayError::QuotaExceeded(_) => StatusCode::INSUFFICIENT_STORAGE,
        };
        if let RelayError::QuotaExceeded(quota) = self {
            return (
                status,
                [(zafe_proto::relay::QUOTA_HEADER, quota.token())],
                self.to_string(),
            )
                .into_response();
        }
        (status, self.to_string()).into_response()
    }
}

type RelayResult = Result<Vec<u8>, RelayError>;

// `mailboxes.delivery_bytes` / `log_bytes`: running totals for the storage quotas (bytes of
// this mailbox's `deliveries.envelope` / `log_entries.entry`), kept in step by every insert
// and by pruning (see `quota`). No SQL comments inside the schema: SQLite keeps the CREATE
// text and re-parses it on ALTER TABLE.
const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS mailboxes (
    id BLOB PRIMARY KEY,
    creator BLOB NOT NULL,
    join_token_hash BLOB NOT NULL,
    max_members INTEGER NOT NULL,
    sealed INTEGER NOT NULL DEFAULT 0,
    next_cursor INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    delivery_bytes INTEGER NOT NULL DEFAULT 0,
    log_bytes INTEGER NOT NULL DEFAULT 0,
    threshold INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS members (
    mailbox BLOB NOT NULL,
    sig_pk BLOB NOT NULL,
    enc_pk BLOB NOT NULL,
    PRIMARY KEY (mailbox, sig_pk)
);
CREATE INDEX IF NOT EXISTS members_by_key ON members (sig_pk);
CREATE TABLE IF NOT EXISTS deliveries (
    mailbox BLOB NOT NULL,
    cursor INTEGER NOT NULL,
    recipient BLOB NOT NULL,
    envelope BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (mailbox, cursor)
);
CREATE INDEX IF NOT EXISTS deliveries_by_recipient ON deliveries (mailbox, recipient, cursor);
CREATE INDEX IF NOT EXISTS mailboxes_by_creator ON mailboxes (creator);
CREATE TABLE IF NOT EXISTS sender_seqs (
    mailbox BLOB NOT NULL,
    sender BLOB NOT NULL,
    last_seq INTEGER NOT NULL,
    PRIMARY KEY (mailbox, sender)
);
CREATE TABLE IF NOT EXISTS log_entries (
    mailbox BLOB NOT NULL,
    idx INTEGER NOT NULL,
    entry BLOB NOT NULL,
    hash BLOB NOT NULL,
    PRIMARY KEY (mailbox, idx)
);
CREATE TABLE IF NOT EXISTS push_tokens (
    mailbox BLOB NOT NULL,
    member BLOB NOT NULL,
    platform TEXT NOT NULL,
    token TEXT NOT NULL,
    PRIMARY KEY (mailbox, member)
);
";

fn system_clock() -> Clock {
    Arc::new(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    })
}

impl Default for Relay {
    fn default() -> Self {
        Self::new()
    }
}

impl Relay {
    /// An in-memory relay (tests, development).
    pub fn new() -> Self {
        Self::with_clock(system_clock())
    }

    /// An in-memory relay with a controllable clock (seconds since the Unix epoch).
    pub fn with_clock(clock: Clock) -> Self {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        Self::from_connection(conn, clock).expect("schema")
    }

    /// A relay persisted in the SQLite file at `path`.
    pub fn open(path: &Path) -> Result<Self, RelayError> {
        Self::open_with_clock(path, system_clock())
    }

    /// A persisted relay with a controllable clock (tests).
    pub fn open_with_clock(path: &Path, clock: Clock) -> Result<Self, RelayError> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        Self::from_connection(conn, clock)
    }

    fn from_connection(conn: Connection, clock: Clock) -> Result<Self, RelayError> {
        let found: u16 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        let has_tables: bool = conn.query_row(
            "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table')",
            [],
            |r| r.get(0),
        )?;
        // Version 0 with tables: written before versioning (unversioned envelopes and log
        // entries), refused; delete it. Older schemas migrate from `found` here.
        if found == 1 {
            migrate_from_v1(&conn)?;
            migrate_from_v2(&conn)?;
        } else if found == 2 {
            migrate_from_v2(&conn)?;
        } else if found != 0 || has_tables {
            version::check(version::Format::RelayDb, found)?;
        }
        conn.execute_batch(SCHEMA)?;
        conn.pragma_update(None, "user_version", version::RELAY_DB)?;
        Ok(Self {
            db: Arc::new(Mutex::new(conn)),
            clock,
            notifier: Arc::new(LogNotifier),
            per_key: None,
            per_ip: None,
            creates_per_ip: None,
            client_ip_header: None,
            quotas: Quotas::none(),
            waiters: Arc::new(Waiters::new(
                wait::DEFAULT_WAITS_PER_KEY,
                wait::DEFAULT_WAITS_TOTAL,
            )),
        })
    }

    /// Caps concurrent long polls per signing key and in total (defaults:
    /// [`wait::DEFAULT_WAITS_PER_KEY`], [`wait::DEFAULT_WAITS_TOTAL`]).
    pub fn with_wait_caps(mut self, per_key: usize, total: usize) -> Self {
        self.waiters = Arc::new(Waiters::new(per_key, total));
        self
    }

    /// Sets storage quotas (none by default; `zafe-relay` uses [`Quotas::hosted`]).
    pub fn with_quotas(mut self, quotas: Quotas) -> Self {
        self.quotas = quotas;
        self
    }

    /// What `mailbox` stores now.
    pub fn usage(&self, mailbox: &MailboxId) -> Result<Usage, RelayError> {
        let db = self.db.lock().expect("lock");
        let (delivery_bytes, log_bytes) = db
            .query_row(
                "SELECT delivery_bytes, log_bytes FROM mailboxes WHERE id = ?1",
                params![&mailbox[..]],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?
            .ok_or(RelayError::NotFound)?;
        let deliveries: i64 = db.query_row(
            "SELECT COUNT(*) FROM deliveries WHERE mailbox = ?1",
            params![&mailbox[..]],
            |r| r.get(0),
        )?;
        Ok(Usage {
            deliveries: deliveries as u64,
            delivery_bytes: delivery_bytes as u64,
            log_bytes: log_bytes as u64,
        })
    }

    /// Sets request limits (none by default; `zafe-relay` uses [`Limits::hosted`]).
    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.per_key = limits.per_key.map(|r| Arc::new(Buckets::new(r)));
        self.per_ip = limits.per_ip.map(|r| Arc::new(Buckets::new(r)));
        self.creates_per_ip = limits.creates_per_ip.map(|r| Arc::new(Buckets::daily(r)));
        self.client_ip_header = limits.client_ip_header;
        self
    }

    /// Deletes a push token everywhere it is registered (FCM reported it gone). Returns
    /// how many registrations were removed.
    pub fn forget_push_token(&self, token: &str) -> Result<usize, RelayError> {
        let db = self.db.lock().expect("lock");
        Ok(db.execute("DELETE FROM push_tokens WHERE token = ?1", params![token])?)
    }

    /// Charges one request to a signing key whose signature already verified.
    fn limit_key(&self, key: &[u8; 32]) -> Result<(), RelayError> {
        match &self.per_key {
            Some(b) => b.take(*key, self.now()).map_err(RelayError::RateLimited),
            None => Ok(()),
        }
    }

    /// Replaces the push notifier.
    pub fn with_notifier(mut self, notifier: Arc<dyn Notifier>) -> Self {
        self.notifier = notifier;
        self
    }

    pub fn router(self) -> Router {
        Router::new()
            .route("/health", get(health))
            .route("/v1/mailbox/create", post(create))
            .route("/v1/mailbox/join", post(join))
            .route("/v1/mailbox/seal", post(seal))
            .route("/v1/mailbox/remove", post(remove))
            .route("/v1/mailbox/members", post(members))
            .route("/v1/mailbox/threshold", post(set_threshold))
            .route("/v1/mailbox/replace", post(replace_member))
            .route("/v1/mailbox/reseed", post(reseed))
            .route("/v1/mailboxes", post(mailboxes))
            .route("/v1/push/register", post(register_push))
            .route("/v1/envelope", post(post_envelope))
            .route("/v1/inbox", post(inbox))
            .route("/v1/inbox/ack", post(inbox_ack))
            .route("/v1/log/append", post(log_append))
            .route("/v1/log/read", post(log_read))
            .route("/v1/wait", post(wait))
            .layer(middleware::from_fn_with_state(self.clone(), limit_ip))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .with_state(self)
    }

    /// Deletes deliveries older than [`DELIVERY_RETENTION_SECS`] (and forgets idle rate
    /// limit buckets). Returns how many deliveries.
    pub fn prune(&self) -> Result<usize, RelayError> {
        if let Some(b) = &self.per_key {
            b.prune(self.now());
        }
        if let Some(b) = &self.per_ip {
            b.prune(self.now());
        }
        if let Some(b) = &self.creates_per_ip {
            b.prune(self.now());
        }
        let cutoff = self.now().saturating_sub(DELIVERY_RETENTION_SECS) as i64;
        let mut db = self.db.lock().expect("lock");
        let tx = db.transaction()?;
        // Give the expired bytes back to each mailbox's counter, then delete. This scans
        // the expired rows once an hour, never on a request.
        tx.execute(
            "UPDATE mailboxes SET delivery_bytes = MAX(0, delivery_bytes - (
                 SELECT COALESCE(SUM(length(envelope)), 0) FROM deliveries
                 WHERE mailbox = mailboxes.id AND created_at < ?1))
             WHERE id IN (SELECT DISTINCT mailbox FROM deliveries WHERE created_at < ?1)",
            params![cutoff],
        )?;
        let deleted = tx.execute(
            "DELETE FROM deliveries WHERE created_at < ?1",
            params![cutoff],
        )?;
        tx.commit()?;
        Ok(deleted)
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    fn check_fresh(&self, timestamp: u64) -> Result<(), RelayError> {
        if timestamp.abs_diff(self.now()) > MAX_REQUEST_SKEW_SECS {
            return Err(RelayError::Stale);
        }
        Ok(())
    }
}

// --- Storage helpers ---------------------------------------------------------------------

/// Schema 1 → 2: adds the quota counters to `mailboxes` and fills them from the stored
/// rows (one scan, at startup).
fn migrate_from_v1(conn: &Connection) -> Result<(), RelayError> {
    conn.execute_batch(
        "BEGIN;
         ALTER TABLE mailboxes ADD COLUMN delivery_bytes INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE mailboxes ADD COLUMN log_bytes INTEGER NOT NULL DEFAULT 0;
         UPDATE mailboxes SET
             delivery_bytes = (SELECT COALESCE(SUM(length(envelope)), 0) FROM deliveries
                               WHERE mailbox = mailboxes.id),
             log_bytes = (SELECT COALESCE(SUM(length(entry)), 0) FROM log_entries
                          WHERE mailbox = mailboxes.id);
         PRAGMA user_version = 2;
         COMMIT;",
    )?;
    tracing::info!("relay database migrated from schema 1 to 2 (storage counters)");
    Ok(())
}

/// Schema 2 → 3: `mailboxes.threshold` (0 = unknown: the vault can't move seats).
fn migrate_from_v2(conn: &Connection) -> Result<(), RelayError> {
    conn.execute_batch(
        "BEGIN;
         ALTER TABLE mailboxes ADD COLUMN threshold INTEGER NOT NULL DEFAULT 0;
         PRAGMA user_version = 3;
         COMMIT;",
    )?;
    tracing::info!("relay database migrated from schema 2 to 3 (thresholds)");
    Ok(())
}

/// Refuses a delivery of `len` bytes to each of `recipients` that would go over a quota.
fn check_delivery_quotas(
    tx: &Connection,
    quotas: &Quotas,
    mailbox: &MailboxId,
    recipients: &[[u8; 32]],
    len: usize,
) -> Result<(), RelayError> {
    if let Some(cap) = quotas.delivery_bytes {
        let used: i64 = tx.query_row(
            "SELECT delivery_bytes FROM mailboxes WHERE id = ?1",
            params![&mailbox[..]],
            |r| r.get(0),
        )?;
        let adding = (len as u64).saturating_mul(recipients.len() as u64);
        if (used as u64).saturating_add(adding) > cap {
            return Err(RelayError::QuotaExceeded(Quota::Deliveries));
        }
    }
    if let Some(cap) = quotas.envelopes_per_recipient {
        // Walks at most `cap` entries of the (mailbox, recipient, cursor) index.
        let mut stmt = tx.prepare_cached(
            "SELECT COUNT(*) FROM (SELECT 1 FROM deliveries
             WHERE mailbox = ?1 AND recipient = ?2 LIMIT ?3)",
        )?;
        let limit = i64::try_from(cap).unwrap_or(i64::MAX);
        for recipient in recipients {
            let n: i64 =
                stmt.query_row(params![&mailbox[..], &recipient[..], limit], |r| r.get(0))?;
            if n as u64 >= cap {
                return Err(RelayError::QuotaExceeded(Quota::Inbox));
            }
        }
    }
    Ok(())
}

struct MailboxRow {
    creator: [u8; 32],
    join_token_hash: [u8; 32],
    max_members: u16,
    sealed: bool,
    threshold: u16,
}

fn arr32(v: Vec<u8>) -> Result<[u8; 32], RelayError> {
    v.try_into().map_err(|_| RelayError::Storage)
}

fn mailbox(db: &Connection, id: &MailboxId) -> Result<MailboxRow, RelayError> {
    let row = db
        .query_row(
            "SELECT creator, join_token_hash, max_members, sealed, threshold
             FROM mailboxes WHERE id = ?1",
            params![&id[..]],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or(RelayError::NotFound)?;
    Ok(MailboxRow {
        creator: arr32(row.0)?,
        join_token_hash: arr32(row.1)?,
        max_members: u16::try_from(row.2).map_err(|_| RelayError::Storage)?,
        sealed: row.3 != 0,
        threshold: u16::try_from(row.4).map_err(|_| RelayError::Storage)?,
    })
}

fn members_of(db: &Connection, id: &MailboxId) -> Result<Vec<IdentityPublic>, RelayError> {
    let mut stmt =
        db.prepare("SELECT sig_pk, enc_pk FROM members WHERE mailbox = ?1 ORDER BY sig_pk")?;
    let rows = stmt.query_map(params![&id[..]], |r| {
        Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    rows.map(|r| {
        let (sig, enc) = r?;
        Ok(IdentityPublic {
            sig_pk: arr32(sig)?,
            enc_pk: arr32(enc)?,
        })
    })
    .collect()
}

fn member(
    db: &Connection,
    id: &MailboxId,
    pk: &[u8; 32],
) -> Result<Option<IdentityPublic>, RelayError> {
    Ok(members_of(db, id)?.into_iter().find(|m| &m.sig_pk == pk))
}

/// Decodes and verifies a signed request, then charges it to the signer's rate limit.
fn verified<T>(relay: &Relay, body: &Bytes) -> Result<Signed<T>, RelayError>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let signed = Signed::<T>::from_bytes(body).map_err(bad_request)?;
    signed.verify().map_err(|_| RelayError::Unauthenticated)?;
    relay.limit_key(&signed.signer.sig_pk)?;
    Ok(signed)
}

/// Per-IP limit on every route (health checks included). Also hands the address to the
/// handlers ([`ClientIp`]) for the mailbox creation limit.
async fn limit_ip(State(relay): State<Relay>, mut request: Request, next: Next) -> Response {
    let peer = request
        .extensions()
        .get::<ConnectInfo<std::net::SocketAddr>>()
        .map(|c| c.0.ip());
    let ip = limits::client_ip(request.headers(), relay.client_ip_header.as_deref(), peer);
    if let (Some(buckets), Some(ip)) = (&relay.per_ip, ip) {
        if let Err(secs) = buckets.take(ip, relay.now()) {
            return RelayError::RateLimited(secs).into_response();
        }
    }
    request.extensions_mut().insert(ClientIp(ip));
    next.run(request).await
}

fn ok<T: serde::Serialize>(value: &T) -> RelayResult {
    encode_body(value).map_err(|_| RelayError::BadRequest)
}

// --- Handlers ----------------------------------------------------------------------------

async fn create(
    State(relay): State<Relay>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    body: Bytes,
) -> RelayResult {
    let req = verified::<CreateMailbox>(&relay, &body)?;
    let p = &req.payload;
    if p.max_members < 2 {
        return Err(RelayError::BadRequest);
    }
    if let (Some(buckets), Some(ip)) = (&relay.creates_per_ip, ip) {
        buckets
            .take(ip, relay.now())
            .map_err(RelayError::RateLimited)?;
    }
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO mailboxes (id, creator, join_token_hash, max_members, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            &p.mailbox[..],
            &req.signer.sig_pk[..],
            &p.join_token_hash[..],
            p.max_members,
            relay.now() as i64
        ],
    )?;
    if inserted == 0 {
        return Err(RelayError::Exists);
    }
    // Counted after the insert, so retrying an existing mailbox still answers Exists;
    // over the cap, dropping the transaction undoes the insert.
    if let Some(max) = relay.quotas.mailboxes_per_key {
        let created: i64 = tx.query_row(
            "SELECT COUNT(*) FROM mailboxes WHERE creator = ?1",
            params![&req.signer.sig_pk[..]],
            |r| r.get(0),
        )?;
        if created as u64 > max {
            return Err(RelayError::QuotaExceeded(Quota::Mailboxes));
        }
    }
    check_capacity(&relay, &tx)?;
    tx.execute(
        "INSERT INTO members (mailbox, sig_pk, enc_pk) VALUES (?1, ?2, ?3)",
        params![
            &p.mailbox[..],
            &req.signer.sig_pk[..],
            &req.signer.enc_pk[..]
        ],
    )?;
    tx.commit()?;
    ok(&())
}

/// Refuses a new mailbox (already inserted in `tx`) when the relay is over its total.
fn check_capacity(relay: &Relay, tx: &rusqlite::Transaction) -> Result<(), RelayError> {
    if let Some(max) = relay.quotas.mailboxes_total {
        let all: i64 = tx.query_row("SELECT COUNT(*) FROM mailboxes", [], |r| r.get(0))?;
        if all as u64 > max {
            return Err(RelayError::QuotaExceeded(Quota::Capacity));
        }
    }
    Ok(())
}

async fn join(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<Join>(&relay, &body)?;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &req.payload.mailbox)?;
    if mb.sealed || join_token_hash(&req.payload.join_token) != mb.join_token_hash {
        return Err(RelayError::Forbidden);
    }
    let current = members_of(&tx, &req.payload.mailbox)?;
    if !current.iter().any(|m| m.sig_pk == req.signer.sig_pk)
        && current.len() >= usize::from(mb.max_members)
    {
        return Err(RelayError::Forbidden);
    }
    tx.execute(
        "INSERT OR REPLACE INTO members (mailbox, sig_pk, enc_pk) VALUES (?1, ?2, ?3)",
        params![
            &req.payload.mailbox[..],
            &req.signer.sig_pk[..],
            &req.signer.enc_pk[..]
        ],
    )?;
    tx.commit()?;
    ok(&())
}

async fn seal(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<Seal>(&relay, &body)?;
    let db = relay.db.lock().expect("lock");
    let mb = mailbox(&db, &req.payload.mailbox)?;
    if req.signer.sig_pk != mb.creator || mb.sealed {
        return Err(RelayError::Forbidden);
    }
    let mut wanted = req.payload.members.clone();
    wanted.sort();
    wanted.dedup();
    let current: Vec<[u8; 32]> = members_of(&db, &req.payload.mailbox)?
        .iter()
        .map(|m| m.sig_pk)
        .collect();
    if wanted != current {
        return Err(RelayError::Forbidden);
    }
    db.execute(
        "UPDATE mailboxes SET sealed = 1 WHERE id = ?1",
        params![&req.payload.mailbox[..]],
    )?;
    ok(&())
}

async fn remove(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<Remove>(&relay, &body)?;
    let db = relay.db.lock().expect("lock");
    let mb = mailbox(&db, &req.payload.mailbox)?;
    if req.signer.sig_pk != mb.creator || mb.sealed || req.payload.member == mb.creator {
        return Err(RelayError::Forbidden);
    }
    db.execute(
        "DELETE FROM members WHERE mailbox = ?1 AND sig_pk = ?2",
        params![&req.payload.mailbox[..], &req.payload.member[..]],
    )?;
    ok(&())
}

async fn members(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<MembersRead>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    let mb = mailbox(&db, &req.payload.mailbox)?;
    let members = members_of(&db, &req.payload.mailbox)?;
    if !members.iter().any(|m| m.sig_pk == req.signer.sig_pk) {
        return Err(RelayError::Forbidden);
    }
    ok(&MembersResponse {
        members,
        sealed: mb.sealed,
    })
}

async fn set_threshold(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<SetThreshold>(&relay, &body)?;
    let p = &req.payload;
    let db = relay.db.lock().expect("lock");
    let mb = mailbox(&db, &p.mailbox)?;
    if req.signer.sig_pk != mb.creator || mb.sealed {
        return Err(RelayError::Forbidden);
    }
    if p.threshold == 0 || p.threshold > mb.max_members {
        return Err(RelayError::BadRequest);
    }
    db.execute(
        "UPDATE mailboxes SET threshold = ?2 WHERE id = ?1",
        params![&p.mailbox[..], p.threshold],
    )?;
    ok(&())
}

/// Moves a lost member's seat to a new key, given approvals from the vault's threshold of
/// other current members (spec §10.1). The relay can't read the vault log, so it checks
/// the approvals itself; members check the same signatures in the log.
async fn replace_member(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<ReplaceMember>(&relay, &body)?;
    let a = req.payload.approval;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &a.mailbox)?;
    let current = members_of(&tx, &a.mailbox)?;
    let is_member = |pk: &[u8; 32]| current.iter().any(|m| &m.sig_pk == pk);
    if req.signer.sig_pk != a.new.sig_pk && !is_member(&req.signer.sig_pk) {
        return Err(RelayError::Forbidden);
    }
    if !is_member(&a.old) && current.contains(&a.new) {
        return ok(&()); // already moved
    }
    if !mb.sealed || mb.threshold == 0 || !is_member(&a.old) || is_member(&a.new.sig_pk) {
        return Err(RelayError::Forbidden);
    }
    let mut approvers = std::collections::BTreeSet::new();
    for (pk, signature) in &req.payload.approvals {
        let Some(by) = current
            .iter()
            .find(|m| &m.sig_pk == pk && m.sig_pk != a.old)
        else {
            continue;
        };
        if a.verify(by, signature).is_ok() {
            approvers.insert(*pk);
        }
    }
    if approvers.len() < usize::from(mb.threshold) {
        return Err(RelayError::Forbidden);
    }
    tx.execute(
        "DELETE FROM members WHERE mailbox = ?1 AND sig_pk = ?2",
        params![&a.mailbox[..], &a.old[..]],
    )?;
    tx.execute(
        "DELETE FROM push_tokens WHERE mailbox = ?1 AND member = ?2",
        params![&a.mailbox[..], &a.old[..]],
    )?;
    tx.execute(
        "INSERT INTO members (mailbox, sig_pk, enc_pk) VALUES (?1, ?2, ?3)",
        params![&a.mailbox[..], &a.new.sig_pk[..], &a.new.enc_pk[..]],
    )?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(&a.mailbox);
    ok(&())
}

/// Most members a reseeded mailbox may list (the same order as any real vault).
const MAX_RESEED_MEMBERS: usize = 64;

/// Restores a vault from a member's saved copy of its log (see [`Reseed`]). The relay is
/// blind: it can check chaining and signatures, not what the entries say, and it doesn't
/// require their authors to be current members (seats move). Members verify the log
/// themselves, and their own saved copies must agree with whatever is restored.
async fn reseed(
    State(relay): State<Relay>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    body: Bytes,
) -> RelayResult {
    let req = verified::<Reseed>(&relay, &body)?;
    let p = &req.payload;
    relay.check_fresh(p.timestamp)?;
    let signer = req.signer.sig_pk;
    let mut seen = std::collections::BTreeSet::new();
    if p.members.len() < 2
        || p.members.len() > MAX_RESEED_MEMBERS
        || !p.members.iter().any(|m| m.sig_pk == signer)
        || !p.members.iter().all(|m| seen.insert(m.sig_pk))
        || p.threshold == 0
        || usize::from(p.threshold) > p.members.len()
    {
        return Err(RelayError::BadRequest);
    }
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let count = |tx: &rusqlite::Transaction| -> Result<u64, RelayError> {
        Ok(tx.query_row(
            "SELECT COUNT(*) FROM log_entries WHERE mailbox = ?1",
            params![&p.mailbox[..]],
            |r| r.get::<_, i64>(0),
        )? as u64)
    };
    match mailbox(&tx, &p.mailbox) {
        Err(RelayError::NotFound) => {
            if let (Some(buckets), Some(ip)) = (&relay.creates_per_ip, ip) {
                buckets
                    .take(ip, relay.now())
                    .map_err(RelayError::RateLimited)?;
            }
            // No join token (its hash is all zeros: nothing hashes to it), so nobody can
            // join; the members are listed by the signer, who alone may continue.
            tx.execute(
                "INSERT INTO mailboxes (id, creator, join_token_hash, max_members, created_at, threshold)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    &p.mailbox[..],
                    &signer[..],
                    &[0u8; 32][..],
                    p.members.len() as i64,
                    relay.now() as i64,
                    p.threshold
                ],
            )?;
            if let Some(max) = relay.quotas.mailboxes_per_key {
                let created: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM mailboxes WHERE creator = ?1",
                    params![&signer[..]],
                    |r| r.get(0),
                )?;
                if created as u64 > max {
                    return Err(RelayError::QuotaExceeded(Quota::Mailboxes));
                }
            }
            check_capacity(&relay, &tx)?;
            for m in &p.members {
                tx.execute(
                    "INSERT INTO members (mailbox, sig_pk, enc_pk) VALUES (?1, ?2, ?3)",
                    params![&p.mailbox[..], &m.sig_pk[..], &m.enc_pk[..]],
                )?;
            }
        }
        Err(e) => return Err(e),
        Ok(mb) if mb.sealed => {
            // Already open: a member learns where its log is, nothing changes.
            member(&tx, &p.mailbox, &signer)?.ok_or(RelayError::Forbidden)?;
            let len = count(&tx)?;
            return ok(&ReseedResponse {
                len,
                open: true,
                restored: false,
            });
        }
        Ok(mb) => {
            // A restore in progress: only its maker continues it. A mailbox that is still
            // being set up by a keygen (it has a join token) is not ours to fill.
            if mb.creator != signer || mb.join_token_hash != [0u8; 32] {
                return Err(RelayError::Forbidden);
            }
        }
    }

    let mut len = count(&tx)?;
    if p.from != len {
        // Out of step (a request was lost or repeated): say where to continue.
        tx.commit()?;
        return ok(&ReseedResponse {
            len,
            open: false,
            restored: true,
        });
    }
    let mut head: [u8; 32] = match tx
        .query_row(
            "SELECT hash FROM log_entries WHERE mailbox = ?1 ORDER BY idx DESC LIMIT 1",
            params![&p.mailbox[..]],
            |r| r.get::<_, Vec<u8>>(0),
        )
        .optional()?
    {
        Some(h) => arr32(h)?,
        None => GENESIS_PREV_HASH,
    };
    let mut used: i64 = tx.query_row(
        "SELECT log_bytes FROM mailboxes WHERE id = ?1",
        params![&p.mailbox[..]],
        |r| r.get(0),
    )?;
    for bytes in &p.entries {
        let entry = LogEntry::from_bytes(bytes).map_err(bad_request)?;
        let h = &entry.header;
        if h.mailbox != p.mailbox || h.index != len || h.prev_hash != head {
            return Err(RelayError::BadRequest);
        }
        // Only the author's key is needed to check a signature.
        entry
            .verify_signature(&IdentityPublic {
                sig_pk: h.author,
                enc_pk: [0; 32],
            })
            .map_err(|_| RelayError::Unauthenticated)?;
        if let Some(cap) = relay.quotas.log_bytes {
            if (used as u64).saturating_add(bytes.len() as u64) > cap {
                return Err(RelayError::QuotaExceeded(Quota::Log));
            }
        }
        let hash = entry.hash().map_err(|_| RelayError::BadRequest)?;
        tx.execute(
            "INSERT INTO log_entries (mailbox, idx, entry, hash) VALUES (?1, ?2, ?3, ?4)",
            params![&p.mailbox[..], len as i64, &bytes[..], &hash[..]],
        )?;
        used += bytes.len() as i64;
        len += 1;
        head = hash;
    }
    tx.execute(
        "UPDATE mailboxes SET log_bytes = ?2 WHERE id = ?1",
        params![&p.mailbox[..], used],
    )?;
    let open = p.finish && len > 0;
    if open {
        tx.execute(
            "UPDATE mailboxes SET sealed = 1 WHERE id = ?1",
            params![&p.mailbox[..]],
        )?;
    }
    tx.commit()?;
    drop(db);
    relay.waiters.signal(&p.mailbox);
    ok(&ReseedResponse {
        len,
        open,
        restored: true,
    })
}

async fn mailboxes(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<MailboxesRead>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    let mut stmt = db.prepare(
        "SELECT mailbox FROM members WHERE sig_pk = ?1 AND enc_pk = ?2 ORDER BY mailbox",
    )?;
    let rows = stmt.query_map(
        params![&req.signer.sig_pk[..], &req.signer.enc_pk[..]],
        |r| r.get::<_, Vec<u8>>(0),
    )?;
    let mailboxes = rows
        .map(|r| {
            r.map_err(RelayError::from)?
                .try_into()
                .map_err(|_| RelayError::Storage)
        })
        .collect::<Result<Vec<MailboxId>, RelayError>>()?;
    ok(&MailboxesResponse { mailboxes })
}

async fn register_push(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<RegisterPush>(&relay, &body)?;
    if req.payload.token.is_empty() || req.payload.token.len() > 4096 {
        return Err(RelayError::BadRequest);
    }
    let db = relay.db.lock().expect("lock");
    mailbox(&db, &req.payload.mailbox)?;
    if member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    db.execute(
        "INSERT OR REPLACE INTO push_tokens (mailbox, member, platform, token) VALUES (?1, ?2, ?3, ?4)",
        params![&req.payload.mailbox[..], &req.signer.sig_pk[..], req.payload.platform.as_str(), &req.payload.token],
    )?;
    ok(&())
}

async fn post_envelope(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let envelope = Envelope::from_bytes(&body).map_err(bad_request)?;
    let h = &envelope.header;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    mailbox(&tx, &h.mailbox)?;
    let members = members_of(&tx, &h.mailbox)?;
    let sender = members
        .iter()
        .find(|m| m.sig_pk == h.from)
        .ok_or(RelayError::Forbidden)?;
    envelope
        .verify(sender)
        .map_err(|_| RelayError::Unauthenticated)?;
    relay.limit_key(&sender.sig_pk)?;

    let recipients: Vec<[u8; 32]> = match h.to {
        Recipient::One(pk) if pk != sender.sig_pk && members.iter().any(|m| m.sig_pk == pk) => {
            vec![pk]
        }
        Recipient::One(_) => return Err(RelayError::Forbidden),
        Recipient::All => members
            .iter()
            .map(|m| m.sig_pk)
            .filter(|pk| *pk != sender.sig_pk)
            .collect(),
    };

    // Replay guard: sequence numbers strictly increase per (mailbox, sender).
    let last: Option<i64> = tx
        .query_row(
            "SELECT last_seq FROM sender_seqs WHERE mailbox = ?1 AND sender = ?2",
            params![&h.mailbox[..], &h.from[..]],
            |r| r.get(0),
        )
        .optional()?;
    let seq = i64::try_from(h.seq).map_err(|_| RelayError::BadRequest)?;
    if last.is_some_and(|l| seq <= l) {
        return Err(RelayError::Replay);
    }
    tx.execute(
        "INSERT OR REPLACE INTO sender_seqs (mailbox, sender, last_seq) VALUES (?1, ?2, ?3)",
        params![&h.mailbox[..], &h.from[..], seq],
    )?;

    check_delivery_quotas(&tx, &relay.quotas, &h.mailbox, &recipients, body.len())?;
    let now = relay.now() as i64;
    for recipient in &recipients {
        let cursor: i64 = tx.query_row(
            "SELECT next_cursor FROM mailboxes WHERE id = ?1",
            params![&h.mailbox[..]],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO deliveries (mailbox, cursor, recipient, envelope, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![&h.mailbox[..], cursor, &recipient[..], &body[..], now],
        )?;
        tx.execute(
            "UPDATE mailboxes SET next_cursor = next_cursor + 1,
                 delivery_bytes = delivery_bytes + ?2 WHERE id = ?1",
            params![&h.mailbox[..], body.len() as i64],
        )?;
    }

    // Collect push tokens for the recipients before releasing the database.
    let pushes = push_tokens(&tx, &h.mailbox, recipients.iter())?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(&h.mailbox);
    for (platform, token) in pushes {
        relay.notifier.notify(platform, &token);
    }
    ok(&())
}

/// Registered push tokens of `members` in `mailbox`.
fn push_tokens<'a>(
    tx: &rusqlite::Transaction<'_>,
    mailbox: &MailboxId,
    members: impl Iterator<Item = &'a [u8; 32]>,
) -> Result<Vec<(PushPlatform, String)>, RelayError> {
    let mut stmt =
        tx.prepare("SELECT platform, token FROM push_tokens WHERE mailbox = ?1 AND member = ?2")?;
    let mut out = Vec::new();
    for member in members {
        if let Some((platform, token)) = stmt
            .query_row(params![&mailbox[..], &member[..]], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })
            .optional()?
        {
            let platform = if platform == "apns" {
                PushPlatform::Apns
            } else {
                PushPlatform::Fcm
            };
            out.push((platform, token));
        }
    }
    Ok(out)
}

async fn inbox(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<InboxRead>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    mailbox(&db, &req.payload.mailbox)?;
    if member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    let after = i64::try_from(req.payload.after).unwrap_or(i64::MAX);
    let mut stmt = db.prepare(
        "SELECT cursor, envelope FROM deliveries
         WHERE mailbox = ?1 AND recipient = ?2 AND cursor > ?3 ORDER BY cursor LIMIT ?4",
    )?;
    let rows = stmt.query_map(
        params![
            &req.payload.mailbox[..],
            &req.signer.sig_pk[..],
            after,
            PAGE
        ],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)),
    )?;
    // Stored as received (`Envelope::to_bytes`); clients decode each one.
    let envelopes = rows
        .map(|row| row.map(|(cursor, bytes)| (cursor as u64, bytes)))
        .collect::<Result<Vec<_>, _>>()?;
    ok(&InboxResponse { envelopes })
}

/// Deletes the signer's handled deliveries, keeping the quota counter in step.
async fn inbox_ack(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<InboxAck>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    if req.payload.cursors.len() > MAX_ACK_CURSORS {
        return Err(RelayError::BadRequest);
    }
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    mailbox(&tx, &req.payload.mailbox)?;
    if member(&tx, &req.payload.mailbox, &req.signer.sig_pk)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    let mut deleted = 0u64;
    let mut bytes = 0i64;
    for &cursor in &req.payload.cursors {
        let Ok(cursor) = i64::try_from(cursor) else {
            continue;
        };
        let size: Option<i64> = tx
            .query_row(
                "DELETE FROM deliveries WHERE mailbox = ?1 AND recipient = ?2 AND cursor = ?3
                 RETURNING length(envelope)",
                params![&req.payload.mailbox[..], &req.signer.sig_pk[..], cursor],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(size) = size {
            deleted += 1;
            bytes += size;
        }
    }
    if bytes > 0 {
        tx.execute(
            "UPDATE mailboxes SET delivery_bytes = MAX(0, delivery_bytes - ?2) WHERE id = ?1",
            params![&req.payload.mailbox[..], bytes],
        )?;
    }
    tx.commit()?;
    ok(&InboxAckResponse { deleted })
}

async fn log_append(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let entry = LogEntry::from_bytes(&body).map_err(bad_request)?;
    let h = &entry.header;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &h.mailbox)?;
    if !mb.sealed {
        return Err(RelayError::Forbidden);
    }
    let author = member(&tx, &h.mailbox, &h.author)?.ok_or(RelayError::Forbidden)?;

    let len: i64 = tx.query_row(
        "SELECT COUNT(*) FROM log_entries WHERE mailbox = ?1",
        params![&h.mailbox[..]],
        |r| r.get(0),
    )?;
    let head: Option<Vec<u8>> = tx
        .query_row(
            "SELECT hash FROM log_entries WHERE mailbox = ?1 ORDER BY idx DESC LIMIT 1",
            params![&h.mailbox[..]],
            |r| r.get(0),
        )
        .optional()?;
    let head = match head {
        Some(bytes) => arr32(bytes)?,
        None => GENESIS_PREV_HASH,
    };
    if h.index != len as u64 || h.prev_hash != head {
        return ok(&AppendResult::Conflict { len: len as u64 });
    }
    entry
        .verify_signature(&author)
        .map_err(|_| RelayError::Unauthenticated)?;
    relay.limit_key(&author.sig_pk)?;
    let hash = entry.hash().map_err(|_| RelayError::BadRequest)?;
    if let Some(cap) = relay.quotas.log_bytes {
        let used: i64 = tx.query_row(
            "SELECT log_bytes FROM mailboxes WHERE id = ?1",
            params![&h.mailbox[..]],
            |r| r.get(0),
        )?;
        if (used as u64).saturating_add(body.len() as u64) > cap {
            return Err(RelayError::QuotaExceeded(Quota::Log));
        }
    }
    tx.execute(
        "INSERT INTO log_entries (mailbox, idx, entry, hash) VALUES (?1, ?2, ?3, ?4)",
        params![&h.mailbox[..], len, &body[..], &hash[..]],
    )?;
    tx.execute(
        "UPDATE mailboxes SET log_bytes = log_bytes + ?2 WHERE id = ?1",
        params![&h.mailbox[..], body.len() as i64],
    )?;
    // Every other member learns there is vault activity (a proposal, a vote, a send...);
    // the push carries nothing else, the app reads the encrypted log itself.
    let others: Vec<[u8; 32]> = members_of(&tx, &h.mailbox)?
        .iter()
        .map(|m| m.sig_pk)
        .filter(|pk| *pk != h.author)
        .collect();
    let pushes = push_tokens(&tx, &h.mailbox, others.iter())?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(&h.mailbox);
    for (platform, token) in pushes {
        relay.notifier.notify(platform, &token);
    }
    ok(&AppendResult::Appended { index: len as u64 })
}

/// Liveness and readiness for load balancers and uptime checks: 200 `ok` when the
/// database answers a trivial query, 503 otherwise. Reveals nothing about mailboxes.
async fn health(State(relay): State<Relay>) -> Response {
    let alive = match relay.db.lock() {
        Ok(db) => db.query_row("SELECT 1", [], |r| r.get::<_, i64>(0)).is_ok(),
        Err(_) => false,
    };
    if alive {
        (StatusCode::OK, "ok").into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, "unavailable").into_response()
    }
}

async fn log_read(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<LogRead>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    mailbox(&db, &req.payload.mailbox)?;
    if member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    let from = i64::try_from(req.payload.from).unwrap_or(i64::MAX);
    let mut stmt = db.prepare(
        "SELECT entry FROM log_entries WHERE mailbox = ?1 AND idx >= ?2 ORDER BY idx LIMIT ?3",
    )?;
    let rows = stmt.query_map(params![&req.payload.mailbox[..], from, PAGE], |r| {
        r.get::<_, Vec<u8>>(0)
    })?;
    // Stored as appended (`LogEntry::to_bytes`); clients decode and verify the chain.
    let entries = rows.collect::<Result<Vec<_>, _>>()?;
    ok(&LogResponse { entries })
}

/// Where `mailbox` is for member `who` (log length, newest delivery cursor). Unknown
/// mailboxes and non-members are both 403, so a 404 on `/v1/wait` only ever means a relay
/// without the endpoint.
fn activity(
    relay: &Relay,
    mailbox: &MailboxId,
    who: &[u8; 32],
) -> Result<WaitResponse, RelayError> {
    let db = relay.db.lock().expect("lock");
    let exists: bool = db.query_row(
        "SELECT EXISTS (SELECT 1 FROM mailboxes WHERE id = ?1)",
        params![&mailbox[..]],
        |r| r.get(0),
    )?;
    if !exists || member(&db, mailbox, who)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    let log_len: i64 = db.query_row(
        "SELECT COALESCE(MAX(idx) + 1, 0) FROM log_entries WHERE mailbox = ?1",
        params![&mailbox[..]],
        |r| r.get(0),
    )?;
    let inbox_cursor: i64 = db.query_row(
        "SELECT COALESCE(MAX(cursor), 0) FROM deliveries WHERE mailbox = ?1 AND recipient = ?2",
        params![&mailbox[..], &who[..]],
        |r| r.get(0),
    )?;
    Ok(WaitResponse {
        log_len: log_len as u64,
        inbox_cursor: inbox_cursor as u64,
    })
}

/// Long poll (see [`WaitRequest`] and the [`wait`] module).
async fn wait(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<WaitRequest>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let p = &req.payload;
    let who = req.signer.sig_pk;
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(u64::from(p.max_wait_secs.min(MAX_WAIT_SECS)));
    // Subscribe before reading, so a write right after the read still wakes this wait.
    let mut signals = relay.waiters.subscribe(p.mailbox);
    let mut now = activity(&relay, &p.mailbox, &who)?;
    if now.is_news(p.log_len, p.inbox_after) || p.max_wait_secs == 0 {
        return ok(&now);
    }
    let _slot = relay
        .waiters
        .acquire(who)
        .ok_or(RelayError::RateLimited(wait::WAIT_RETRY_SECS))?;
    loop {
        let woken = tokio::time::timeout_at(deadline, signals.changed())
            .await
            .is_ok();
        // A signal may be for another member's inbox: check what changed for this one.
        now = activity(&relay, &p.mailbox, &who)?;
        if !woken || now.is_news(p.log_len, p.inbox_after) {
            return ok(&now);
        }
    }
}
