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
//! (`/v1/wait`, [`wait`]) are capped per key and in total, always. Mailboxes nobody uses
//! expire ([`expiry`]), also off in [`Relay::new`].
//!
//! Labels: a mailbox id is this relay's label for a vault. It may differ from the vault id
//! the log entries carry (`EntryHeader::mailbox`), so a vault can move to a mailbox nobody
//! could create first (`docs/relay-move.md` §4.6). The relay keeps the vault id of a
//! mailbox's first entry (`mailboxes.vault_id`) and refuses entries of any other vault.
//! `/v1/log/append` finds the mailbox from the entry, so it only reaches mailboxes whose
//! label is the vault id; `/v1/log/append-labelled` takes the label. Seat moves are signed
//! over the vault id: `/v1/mailbox/replace` finds the mailbox by it, and
//! `/v1/mailbox/replace-labelled` takes the label and checks the vault id it holds. Every
//! other route takes the mailbox from the request or the envelope header.

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Bytes,
    extract::{ConnectInfo, DefaultBodyLimit, Extension, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use rusqlite::{params, Connection, OptionalExtension};
pub mod expiry;
pub mod fcm;
pub mod limits;
pub mod quota;
pub mod wait;

use expiry::{day, Expiry};
use limits::{Buckets, Limits};
use quota::{Quota, Quotas, Usage};
use wait::Waiters;

use zafe_proto::{
    log::GENESIS_PREV_HASH,
    relay::{
        encode_body, join_token_hash, AppendResult, CancelSetup, CreateMailbox, ForgetPush,
        InboxAck, InboxAckResponse, InboxRead, InboxResponse, Join, LabelledAppend,
        LabelledReplace, LogRead, LogResponse, MailboxInfo, MailboxesRead, MailboxesResponse,
        MembersRead, MembersResponse, PushPlatform, RecoveryPut, RecoveryRead, RecoveryValueKind,
        RecoveryValues, RegisterPush, RelayExpiry, RelayInfo, RelayLimits, RelayVersions, Remove,
        RenewInvite, ReplaceApproval, ReplaceMember, Reseed, ReseedResponse, Seal, SetThreshold,
        Signed, WaitRequest, WaitResponse, CREATE_TOKEN_HEADER, MAX_ACK_CURSORS,
        MAX_REQUEST_SKEW_SECS, MAX_WAIT_SECS, UNKNOWN_MAILBOX, UNSUPPORTED_VERSION_HEADER,
    },
    version::{self, UnsupportedVersion},
    Envelope, IdentityPublic, LogEntry, MailboxId, ProtoError, Recipient,
};

/// How long a seat recovery's safety-check values stay (a recovery takes minutes to days).
pub const RECOVERY_RETENTION_SECS: u64 = 7 * 24 * 3600;

/// Most co-signers' challenges kept for one recovering phone: more than any vault has
/// approvers, few enough that a stranger can't bury the real ones.
pub const MAX_RECOVERY_CHALLENGES: i64 = 16;

/// Largest number of items returned by one read (clients rely on it: a shorter log read
/// is the end of the log).
const PAGE: i64 = zafe_proto::relay::READ_PAGE as i64;

/// Largest request body. The biggest real bodies are log entries carrying a proposal's
/// PCZT (tens of KB for a few payments; a 50-recipient batch stays well under this) and
/// commitment batches (256 x ~70 bytes).
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// Undelivered envelopes older than this are pruned (spec §6.1).
pub const DELIVERY_RETENTION_SECS: u64 = 30 * 24 * 3600;

/// Most members one mailbox may have (created or restored). Vaults have at most 15.
pub const MAX_MEMBERS: u16 = 64;

type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

/// Sends content-free "vault activity" pushes. Real APNs/FCM senders plug in here (M1).
pub trait Notifier: Send + Sync {
    fn notify(&self, platform: PushPlatform, token: &str);

    /// Whether pushes really go out (`GET /v1/info` says so to clients).
    fn delivers(&self) -> bool {
        true
    }
}

/// Default notifier: logs that a push would be sent.
pub struct LogNotifier;

impl Notifier for LogNotifier {
    fn notify(&self, platform: PushPlatform, _token: &str) {
        tracing::debug!(platform = platform.as_str(), "push: vault activity");
    }

    fn delivers(&self) -> bool {
        false
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
    /// The configured limits, as `GET /v1/info` reports them.
    limits: Limits,
    quotas: Quotas,
    expiry: Expiry,
    /// [`create_token_hash`] of `ZAFE_RELAY_CREATE_TOKEN` (never the token itself).
    create_token: Option<[u8; 32]>,
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
    #[error("{}", UNKNOWN_MAILBOX)]
    NotFound,
    /// A log entry of another vault than the one the mailbox holds (the vault id of its
    /// first entry).
    #[error("this mailbox holds another vault")]
    OtherVault,
    /// This relay needs a create token to create or restore a mailbox, and the request
    /// didn't carry the right one (HTTP 401, `www-authenticate: zafe-create-token`).
    #[error("this relay needs a create token")]
    CreateTokenRequired,
    /// The invite is older than `INVITE_TTL_SECS` and wasn't renewed (HTTP 410 Gone).
    #[error("this invite expired")]
    InviteExpired,
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
        if let RelayError::CreateTokenRequired = self {
            return (
                StatusCode::UNAUTHORIZED,
                [(axum::http::header::WWW_AUTHENTICATE, CREATE_TOKEN_HEADER)],
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
            RelayError::BadRequest | RelayError::Stale | RelayError::OtherVault => {
                StatusCode::BAD_REQUEST
            }
            RelayError::Unauthenticated | RelayError::CreateTokenRequired => {
                StatusCode::UNAUTHORIZED
            }
            RelayError::Forbidden => StatusCode::FORBIDDEN,
            RelayError::NotFound => StatusCode::NOT_FOUND,
            RelayError::InviteExpired => StatusCode::GONE,
            RelayError::Exists | RelayError::Replay => StatusCode::CONFLICT,
            RelayError::Storage | RelayError::UnsupportedVersion(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            RelayError::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            RelayError::QuotaExceeded(_) => StatusCode::INSUFFICIENT_STORAGE,
        };
        (status, self.to_string()).into_response()
    }
}

type RelayResult = Result<Vec<u8>, RelayError>;

// `mailboxes.delivery_bytes` / `log_bytes`: running totals for the storage quotas (bytes of
// this mailbox's `deliveries.envelope` / `log_entries.entry`), kept in step by every insert
// and by pruning (see `quota`). `vault_id`: the vault id of the first log entry (NULL until
// there is one; the id is only a label). `last_seen_day` / `created_day`: day numbers for
// expiry; `shared`: a member other than the creator has made a request (see `expiry`).
// No SQL comments inside the schema: SQLite keeps the CREATE text and re-parses it on
// ALTER TABLE.
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
    threshold INTEGER NOT NULL DEFAULT 0,
    vault_id BLOB,
    last_seen_day INTEGER NOT NULL DEFAULT 0,
    created_day INTEGER NOT NULL DEFAULT 0,
    shared INTEGER NOT NULL DEFAULT 0
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
CREATE TABLE IF NOT EXISTS recovery_values (
    subject BLOB NOT NULL,
    signer BLOB NOT NULL,
    kind INTEGER NOT NULL,
    signed BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (subject, signer, kind)
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
        match found {
            1..=3 => {
                if found == 1 {
                    migrate_from_v1(&conn)?;
                }
                if found <= 2 {
                    migrate_from_v2(&conn)?;
                }
                migrate_from_v3(&conn, day(clock()))?;
            }
            0 if !has_tables => {}
            _ => version::check(version::Format::RelayDb, found)?,
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
            limits: Limits::none(),
            quotas: Quotas::none(),
            expiry: Expiry::none(),
            create_token: None,
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

    /// Sets when [`Relay::expire`] deletes mailboxes (never by default; `zafe-relay` uses
    /// [`Expiry::hosted`]).
    pub fn with_expiry(mut self, expiry: Expiry) -> Self {
        self.expiry = expiry;
        self
    }

    /// Makes `/v1/mailbox/create` and the first `/v1/mailbox/reseed` of a mailbox need
    /// `token` in the [`CREATE_TOKEN_HEADER`] header (a private relay). `None` or blank:
    /// anyone may create. Only a hash of it is kept.
    pub fn with_create_token(mut self, token: Option<&str>) -> Self {
        self.create_token = token
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| create_token_hash(t.as_bytes()));
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
        self.client_ip_header = limits.client_ip_header.clone();
        self.limits = limits;
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
            .route("/v1/info", get(info))
            .route("/v1/mailbox/create", post(create))
            .route("/v1/mailbox/join", post(join))
            .route("/v1/mailbox/seal", post(seal))
            .route("/v1/mailbox/remove", post(remove))
            .route("/v1/mailbox/cancel", post(cancel_setup))
            .route("/v1/mailbox/renew", post(renew_invite))
            .route("/v1/mailbox/members", post(members))
            .route("/v1/mailbox/info", post(mailbox_info))
            .route("/v1/mailbox/threshold", post(set_threshold))
            .route("/v1/mailbox/replace", post(replace_member))
            .route(
                "/v1/mailbox/replace-labelled",
                post(replace_member_labelled),
            )
            .route("/v1/mailbox/reseed", post(reseed))
            .route("/v1/mailboxes", post(mailboxes))
            .route("/v1/recovery/put", post(recovery_put))
            .route("/v1/recovery/read", post(recovery_read))
            .route("/v1/push/register", post(register_push))
            .route("/v1/push/forget", post(forget_push))
            .route("/v1/envelope", post(post_envelope))
            .route("/v1/inbox", post(inbox))
            .route("/v1/inbox/ack", post(inbox_ack))
            .route("/v1/log/append", post(log_append))
            .route("/v1/log/append-labelled", post(log_append_labelled))
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
        let recovery_cutoff = self.now().saturating_sub(RECOVERY_RETENTION_SECS) as i64;
        tx.execute(
            "DELETE FROM recovery_values WHERE created_at < ?1",
            params![recovery_cutoff],
        )?;
        tx.commit()?;
        Ok(deleted)
    }

    /// Deletes mailboxes nobody uses any more ([`expiry`]) with everything stored for them.
    /// Returns how many. Does nothing unless [`Relay::with_expiry`] set a rule.
    pub fn expire(&self) -> Result<usize, RelayError> {
        let today = day(self.now());
        let before = |days: Option<u32>| days.map(|d| today.saturating_sub(i64::from(d)));
        let (idle, unused) = (
            before(self.expiry.idle_days),
            before(self.expiry.unused_days),
        );
        if idle.is_none() && unused.is_none() {
            return Ok(0);
        }
        let mut db = self.db.lock().expect("lock");
        let tx = db.transaction()?;
        let ids = {
            // A scan of `mailboxes` once an hour, never on a request.
            let mut stmt = tx.prepare(
                "SELECT id FROM mailboxes
                 WHERE (?1 IS NOT NULL AND last_seen_day < ?1)
                    OR (?2 IS NOT NULL AND shared = 0 AND created_day < ?2)",
            )?;
            let rows = stmt.query_map(params![idle, unused], |r| r.get::<_, Vec<u8>>(0))?;
            rows.map(|r| {
                MailboxId::try_from(r.map_err(RelayError::from)?).map_err(|_| RelayError::Storage)
            })
            .collect::<Result<Vec<MailboxId>, RelayError>>()?
        };
        for id in &ids {
            delete_mailbox(&tx, id)?;
        }
        tx.commit()?;
        drop(db);
        for id in &ids {
            self.waiters.signal(id);
        }
        Ok(ids.len())
    }

    /// What `GET /v1/info` answers.
    pub fn info(&self) -> RelayInfo {
        let q = &self.quotas;
        RelayInfo {
            versions: RelayVersions::current(),
            limits: RelayLimits {
                max_body_bytes: MAX_BODY_BYTES as u64,
                max_members: MAX_MEMBERS,
                log_bytes: q.log_bytes,
                delivery_bytes: q.delivery_bytes,
                envelopes_per_recipient: q.envelopes_per_recipient,
                mailboxes_per_key: q.mailboxes_per_key,
                requests_per_minute: self.limits.per_key.map(|r| r.per_minute),
                delivery_retention_days: (DELIVERY_RETENTION_SECS / expiry::DAY_SECS) as u32,
            },
            push: self.notifier.delivers(),
            create_token: self.create_token.is_some(),
            expiry: RelayExpiry {
                idle_days: self.expiry.idle_days,
                unused_days: self.expiry.unused_days,
            },
        }
    }

    fn now(&self) -> u64 {
        (self.clock)()
    }

    fn today(&self) -> i64 {
        day(self.now())
    }

    /// Records that member `who` made a request to mailbox `id` (row `mb`) today, for
    /// [`expiry`]. Writes only when the day or the shared flag changes: at most once per
    /// mailbox per day, plus once when a member other than the creator first shows up.
    fn touch(
        &self,
        db: &Connection,
        id: &MailboxId,
        mb: &MailboxRow,
        who: &[u8; 32],
    ) -> Result<(), RelayError> {
        let today = self.today();
        let other = *who != mb.creator;
        if mb.last_seen_day >= today && (mb.shared || !other) {
            return Ok(());
        }
        db.execute(
            "UPDATE mailboxes SET last_seen_day = MAX(last_seen_day, ?2), shared = shared OR ?3
             WHERE id = ?1",
            params![&id[..], today, other],
        )?;
        Ok(())
    }

    /// Refuses a creation without this relay's create token (if it has one). Compared as
    /// hashes in constant time; the token is never logged.
    fn check_create_token(&self, headers: &HeaderMap) -> Result<(), RelayError> {
        use subtle::ConstantTimeEq;
        let Some(want) = &self.create_token else {
            return Ok(());
        };
        let given = headers
            .get(CREATE_TOKEN_HEADER)
            .map(|v| v.as_bytes())
            .unwrap_or_default();
        let given = String::from_utf8_lossy(given);
        if bool::from(create_token_hash(given.trim().as_bytes()).ct_eq(want)) {
            Ok(())
        } else {
            Err(RelayError::CreateTokenRequired)
        }
    }

    /// Takes one creation from the client address's daily bucket, then checks the create
    /// token, so wrong tokens count against the bucket too.
    fn charge_creation(
        &self,
        ip: Option<std::net::IpAddr>,
        headers: &HeaderMap,
    ) -> Result<(), RelayError> {
        if let (Some(buckets), Some(ip)) = (&self.creates_per_ip, ip) {
            buckets
                .take(ip, self.now())
                .map_err(RelayError::RateLimited)?;
        }
        self.check_create_token(headers)
    }

    fn check_fresh(&self, timestamp: u64) -> Result<(), RelayError> {
        if timestamp.abs_diff(self.now()) > MAX_REQUEST_SKEW_SECS {
            return Err(RelayError::Stale);
        }
        Ok(())
    }
}

// --- Storage helpers ---------------------------------------------------------------------

/// `BLAKE2b-256("Zafe_CreateToken", token)`.
fn create_token_hash(token: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"Zafe_CreateToken")
        .hash(token)
        .as_bytes()
        .try_into()
        .expect("32 bytes")
}

/// Deletes mailbox `id` and every row stored for it. The storage counters live on the
/// mailbox row, so they go with it; the per-key mailbox count is counted from the table.
fn delete_mailbox(tx: &Connection, id: &MailboxId) -> Result<(), RelayError> {
    for table in [
        "members",
        "deliveries",
        "sender_seqs",
        "push_tokens",
        "log_entries",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE mailbox = ?1"),
            params![&id[..]],
        )?;
    }
    tx.execute("DELETE FROM mailboxes WHERE id = ?1", params![&id[..]])?;
    Ok(())
}

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

/// Schema 3 → 4: labels and expiry. Every existing mailbox was filed under its vault id
/// (`vault_id = id`), counts as seen `today` (so nothing expires at the first prune after
/// the upgrade) and as shared (only the idle rule applies to it).
fn migrate_from_v3(conn: &Connection, today: i64) -> Result<(), RelayError> {
    conn.execute_batch(&format!(
        "BEGIN;
         ALTER TABLE mailboxes ADD COLUMN vault_id BLOB;
         ALTER TABLE mailboxes ADD COLUMN last_seen_day INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE mailboxes ADD COLUMN created_day INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE mailboxes ADD COLUMN shared INTEGER NOT NULL DEFAULT 0;
         UPDATE mailboxes SET vault_id = id, last_seen_day = {today}, created_day = {today},
             shared = 1;
         PRAGMA user_version = 4;
         COMMIT;"
    ))?;
    tracing::info!("relay database migrated from schema 3 to 4 (labels, expiry)");
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
    /// When the invite was issued or last renewed (`created_at`, seconds).
    issued_at: u64,
    /// The vault id of the first log entry (`None` before there is one).
    vault_id: Option<MailboxId>,
    last_seen_day: i64,
    shared: bool,
}

fn arr32(v: Vec<u8>) -> Result<[u8; 32], RelayError> {
    v.try_into().map_err(|_| RelayError::Storage)
}

fn mailbox(db: &Connection, id: &MailboxId) -> Result<MailboxRow, RelayError> {
    let row = db
        .query_row(
            "SELECT creator, join_token_hash, max_members, sealed, threshold, created_at,
                    vault_id, last_seen_day, shared
             FROM mailboxes WHERE id = ?1",
            params![&id[..]],
            |r| {
                Ok((
                    (
                        r.get::<_, Vec<u8>>(0)?,
                        r.get::<_, Vec<u8>>(1)?,
                        r.get::<_, i64>(2)?,
                        r.get::<_, i64>(3)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                    ),
                    (
                        r.get::<_, Option<Vec<u8>>>(6)?,
                        r.get::<_, i64>(7)?,
                        r.get::<_, i64>(8)?,
                    ),
                ))
            },
        )
        .optional()?
        .ok_or(RelayError::NotFound)?;
    let (row, (vault_id, last_seen_day, shared)) = row;
    Ok(MailboxRow {
        creator: arr32(row.0)?,
        join_token_hash: arr32(row.1)?,
        max_members: u16::try_from(row.2).map_err(|_| RelayError::Storage)?,
        sealed: row.3 != 0,
        threshold: u16::try_from(row.4).map_err(|_| RelayError::Storage)?,
        issued_at: u64::try_from(row.5).unwrap_or(0),
        vault_id: vault_id
            .map(|v| MailboxId::try_from(v).map_err(|_| RelayError::Storage))
            .transpose()?,
        last_seen_day,
        shared: shared != 0,
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
    headers: HeaderMap,
    body: Bytes,
) -> RelayResult {
    let req = verified::<CreateMailbox>(&relay, &body)?;
    let p = &req.payload;
    if p.max_members < 2 || p.max_members > MAX_MEMBERS {
        return Err(RelayError::BadRequest);
    }
    relay.charge_creation(ip, &headers)?;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO mailboxes
             (id, creator, join_token_hash, max_members, created_at, last_seen_day, created_day)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
        params![
            &p.mailbox[..],
            &req.signer.sig_pk[..],
            &p.join_token_hash[..],
            p.max_members,
            relay.now() as i64,
            relay.today()
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

async fn join(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<Join>(&relay, &body)?;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &req.payload.mailbox)?;
    if mb.sealed || join_token_hash(&req.payload.join_token) != mb.join_token_hash {
        return Err(RelayError::Forbidden);
    }
    let current = members_of(&tx, &req.payload.mailbox)?;
    let already = current.iter().any(|m| m.sig_pk == req.signer.sig_pk);
    // A new member needs a live invite; one already in can always rejoin.
    if !already
        && relay.now()
            > mb.issued_at
                .saturating_add(zafe_proto::relay::INVITE_TTL_SECS)
    {
        return Err(RelayError::InviteExpired);
    }
    if !already && current.len() >= usize::from(mb.max_members) {
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
    relay.touch(&tx, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
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
    relay.touch(&db, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
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
    relay.touch(&db, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
    ok(&())
}

/// Deletes a mailbox whose setup the creator abandoned (wrong details, changed their
/// mind), so its invite stops working and the members waiting in it learn the setup is
/// gone (their next read answers 404). Only before sealing and only while the log is
/// empty: a vault that exists is never deleted by this. Byte counters live on the
/// mailbox row, so they go with it.
/// The creator restarts the invite's clock (`INVITE_TTL_SECS`), until the setup locks.
async fn renew_invite(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<RenewInvite>(&relay, &body)?;
    let id = &req.payload.mailbox;
    let db = relay.db.lock().expect("lock");
    let mb = mailbox(&db, id)?;
    if req.signer.sig_pk != mb.creator || mb.sealed {
        return Err(RelayError::Forbidden);
    }
    let now = i64::try_from(relay.now()).unwrap_or(i64::MAX);
    db.execute(
        "UPDATE mailboxes SET created_at = ?1 WHERE id = ?2",
        params![now, &id[..]],
    )?;
    relay.touch(&db, id, &mb, &req.signer.sig_pk)?;
    ok(&())
}

async fn cancel_setup(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<CancelSetup>(&relay, &body)?;
    let id = &req.payload.mailbox;
    let mut db = relay.db.lock().expect("lock");
    let mb = mailbox(&db, id)?;
    // Sealed is fine: until the vault's first log entry no keys exist, and a setup whose
    // member never came back must be cancellable.
    if req.signer.sig_pk != mb.creator {
        return Err(RelayError::Forbidden);
    }
    let logged: i64 = db.query_row(
        "SELECT COUNT(*) FROM log_entries WHERE mailbox = ?1",
        params![&id[..]],
        |r| r.get(0),
    )?;
    if logged > 0 {
        return Err(RelayError::Forbidden);
    }
    let tx = db.transaction()?;
    delete_mailbox(&tx, id)?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(id);
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
    relay.touch(&db, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
    ok(&MembersResponse {
        members,
        sealed: mb.sealed,
    })
}

async fn mailbox_info(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<MembersRead>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    let mb = mailbox(&db, &req.payload.mailbox)?;
    let members = members_of(&db, &req.payload.mailbox)?;
    if !members.iter().any(|m| m.sig_pk == req.signer.sig_pk) {
        return Err(RelayError::Forbidden);
    }
    relay.touch(&db, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
    ok(&MailboxInfo {
        members,
        sealed: mb.sealed,
        threshold: mb.threshold,
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
    relay.touch(&db, &p.mailbox, &mb, &req.signer.sig_pk)?;
    ok(&())
}

/// Moves a lost member's seat to a new key, given approvals from the vault's threshold of
/// other current members (spec §10.1). The relay can't read the vault log, so it checks
/// the approvals itself; members check the same signatures in the log.
async fn replace_member(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<ReplaceMember>(&relay, &body)?;
    let a = req.payload.approval;
    replace_in(
        &relay,
        &req.signer.sig_pk,
        &a.mailbox,
        a,
        &req.payload.approvals,
    )
}

/// [`replace_member`] on a labelled mailbox (see [`LabelledReplace`]): the approvals are
/// signed over the vault id, which must be the one this mailbox holds.
async fn replace_member_labelled(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<LabelledReplace>(&relay, &body)?;
    let p = req.payload;
    replace_in(
        &relay,
        &req.signer.sig_pk,
        &p.mailbox,
        p.approval,
        &p.approvals,
    )
}

/// Moves `approval.old`'s seat in mailbox `label` to `approval.new`.
fn replace_in(
    relay: &Relay,
    signer: &[u8; 32],
    label: &MailboxId,
    a: ReplaceApproval,
    approvals: &[([u8; 32], Vec<u8>)],
) -> RelayResult {
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, label)?;
    // The approvals name the vault: they must be for the one this mailbox holds (before
    // its first entry, a mailbox holds the vault its id names).
    if mb.vault_id.unwrap_or(*label) != a.mailbox {
        return Err(RelayError::OtherVault);
    }
    let current = members_of(&tx, label)?;
    let is_member = |pk: &[u8; 32]| current.iter().any(|m| &m.sig_pk == pk);
    if *signer != a.new.sig_pk && !is_member(signer) {
        return Err(RelayError::Forbidden);
    }
    if !is_member(&a.old) && current.contains(&a.new) {
        return ok(&()); // already moved
    }
    if !mb.sealed || mb.threshold == 0 || !is_member(&a.old) || is_member(&a.new.sig_pk) {
        return Err(RelayError::Forbidden);
    }
    let mut approvers = std::collections::BTreeSet::new();
    for (pk, signature) in approvals {
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
        params![&label[..], &a.old[..]],
    )?;
    tx.execute(
        "DELETE FROM push_tokens WHERE mailbox = ?1 AND member = ?2",
        params![&label[..], &a.old[..]],
    )?;
    tx.execute(
        "INSERT INTO members (mailbox, sig_pk, enc_pk) VALUES (?1, ?2, ?3)",
        params![&label[..], &a.new.sig_pk[..], &a.new.enc_pk[..]],
    )?;
    relay.touch(&tx, label, &mb, signer)?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(label);
    ok(&())
}

/// Restores a vault from a member's saved copy of its log (see [`Reseed`]). The relay is
/// blind: it can check chaining and signatures, not what the entries say, and it doesn't
/// require their authors to be current members (seats move). Members verify the log
/// themselves, and their own saved copies must agree with whatever is restored. The
/// mailbox id is a label: the entries' vault id is kept from the first one and must not
/// change.
async fn reseed(
    State(relay): State<Relay>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    headers: HeaderMap,
    body: Bytes,
) -> RelayResult {
    let req = verified::<Reseed>(&relay, &body)?;
    let p = &req.payload;
    relay.check_fresh(p.timestamp)?;
    let signer = req.signer.sig_pk;
    let mut seen = std::collections::BTreeSet::new();
    if p.members.len() < 2
        || p.members.len() > usize::from(MAX_MEMBERS)
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
    // The vault id the mailbox holds so far (`None`: no entry yet).
    let stored_vault = match mailbox(&tx, &p.mailbox) {
        Err(RelayError::NotFound) => {
            relay.charge_creation(ip, &headers)?;
            // No join token (its hash is all zeros: nothing hashes to it), so nobody can
            // join; the members are listed by the signer, who alone may continue.
            tx.execute(
                "INSERT INTO mailboxes (id, creator, join_token_hash, max_members, created_at,
                     threshold, last_seen_day, created_day)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)",
                params![
                    &p.mailbox[..],
                    &signer[..],
                    &[0u8; 32][..],
                    p.members.len() as i64,
                    relay.now() as i64,
                    p.threshold,
                    relay.today()
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
            for m in &p.members {
                tx.execute(
                    "INSERT INTO members (mailbox, sig_pk, enc_pk) VALUES (?1, ?2, ?3)",
                    params![&p.mailbox[..], &m.sig_pk[..], &m.enc_pk[..]],
                )?;
            }
            None
        }
        Err(e) => return Err(e),
        Ok(mb) if mb.sealed => {
            // Already open: a member learns where its log is, nothing changes.
            member(&tx, &p.mailbox, &signer)?.ok_or(RelayError::Forbidden)?;
            relay.touch(&tx, &p.mailbox, &mb, &signer)?;
            let len = count(&tx)?;
            tx.commit()?;
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
            relay.touch(&tx, &p.mailbox, &mb, &signer)?;
            mb.vault_id
        }
    };

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
    let mut vault = stored_vault;
    for bytes in &p.entries {
        let entry = LogEntry::from_bytes(bytes).map_err(bad_request)?;
        let h = &entry.header;
        if h.index != len || h.prev_hash != head {
            return Err(RelayError::BadRequest);
        }
        match vault {
            Some(v) if v != h.mailbox => return Err(RelayError::OtherVault),
            Some(_) => {}
            None => vault = Some(h.mailbox),
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
    if let (None, Some(v)) = (stored_vault, vault) {
        tx.execute(
            "UPDATE mailboxes SET vault_id = ?2 WHERE id = ?1",
            params![&p.mailbox[..], &v[..]],
        )?;
    }
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

/// Keeps one value of a seat recovery's safety check (see [`RecoveryPut`]). The new phone
/// (the subject) leaves its commitment and later its secret; anyone else may leave one
/// challenge. The first of each (subject, signer, kind) stays: a value can't be changed
/// after the other side has seen it.
async fn recovery_put(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<RecoveryPut>(&relay, &body)?;
    let p = &req.payload;
    relay.check_fresh(p.timestamp)?;
    let by_subject = req.signer.sig_pk == p.subject;
    let allowed = match p.kind {
        RecoveryValueKind::Commit | RecoveryValueKind::Reveal => by_subject,
        RecoveryValueKind::Challenge => !by_subject,
    };
    if !allowed {
        return Err(RelayError::Forbidden);
    }
    // The canonical bytes, not the body: nothing extra rides along.
    let signed = req.to_bytes().map_err(|_| RelayError::BadRequest)?;
    let db = relay.db.lock().expect("lock");
    if p.kind == RecoveryValueKind::Challenge {
        let challenges: i64 = db.query_row(
            "SELECT COUNT(*) FROM recovery_values WHERE subject = ?1 AND kind = ?2",
            params![&p.subject[..], p.kind as i64],
            |r| r.get(0),
        )?;
        if challenges >= MAX_RECOVERY_CHALLENGES {
            return Err(RelayError::Forbidden);
        }
    }
    db.execute(
        "INSERT OR IGNORE INTO recovery_values (subject, signer, kind, signed, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            &p.subject[..],
            &req.signer.sig_pk[..],
            p.kind as i64,
            &signed[..],
            relay.now() as i64
        ],
    )?;
    ok(&())
}

/// Every value left for a recovering phone, as posted (clients verify the signatures).
async fn recovery_read(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<RecoveryRead>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    let mut stmt = db.prepare(
        "SELECT signed FROM recovery_values WHERE subject = ?1 ORDER BY kind, created_at",
    )?;
    let values = stmt
        .query_map(params![&req.payload.subject[..]], |r| {
            r.get::<_, Vec<u8>>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    ok(&RecoveryValues { values })
}

async fn register_push(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<RegisterPush>(&relay, &body)?;
    if req.payload.token.is_empty() || req.payload.token.len() > 4096 {
        return Err(RelayError::BadRequest);
    }
    let db = relay.db.lock().expect("lock");
    let mb = mailbox(&db, &req.payload.mailbox)?;
    if member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    relay.touch(&db, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
    db.execute(
        "INSERT OR REPLACE INTO push_tokens (mailbox, member, platform, token) VALUES (?1, ?2, ?3, ?4)",
        params![&req.payload.mailbox[..], &req.signer.sig_pk[..], req.payload.platform.as_str(), &req.payload.token],
    )?;
    ok(&())
}

/// Deletes the signer's push token for a mailbox (see [`ForgetPush`]). Not a use of the
/// mailbox for [`expiry`], and never 404: nothing registered is nothing to forget.
async fn forget_push(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<ForgetPush>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    db.execute(
        "DELETE FROM push_tokens WHERE mailbox = ?1 AND member = ?2",
        params![&req.payload.mailbox[..], &req.signer.sig_pk[..]],
    )?;
    ok(&())
}

async fn post_envelope(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let envelope = Envelope::from_bytes(&body).map_err(bad_request)?;
    let h = &envelope.header;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &h.mailbox)?;
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
    relay.touch(&tx, &h.mailbox, &mb, &sender.sig_pk)?;
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
            let platform = match platform.as_str() {
                "apns" => PushPlatform::Apns,
                "fcm-fid" => PushPlatform::FcmFid,
                _ => PushPlatform::Fcm,
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
    let mb = mailbox(&db, &req.payload.mailbox)?;
    if member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    relay.touch(&db, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
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
    let mb = mailbox(&tx, &req.payload.mailbox)?;
    if member(&tx, &req.payload.mailbox, &req.signer.sig_pk)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    relay.touch(&tx, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
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

/// `/v1/log/append`: the body is the entry, and the mailbox is the one named by the entry's
/// vault id (a mailbox whose label is its vault id). The entry's author is the requester.
async fn log_append(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let entry = LogEntry::from_bytes(&body).map_err(bad_request)?;
    let label = entry.header.mailbox;
    append_entry(&relay, &label, &entry, &body, None)
}

/// `/v1/log/append-labelled` (see [`LabelledAppend`]): the signed request names the mailbox,
/// and its signer, a member, is the requester.
async fn log_append_labelled(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<LabelledAppend>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let entry = LogEntry::from_bytes(&req.payload.entry).map_err(bad_request)?;
    append_entry(
        &relay,
        &req.payload.mailbox,
        &entry,
        &req.payload.entry,
        Some(&req.signer.sig_pk),
    )
}

/// Appends `entry` (`bytes` as received) to the log of mailbox `label`. `poster`: the member
/// who signed a labelled request (already charged to its rate limit); `None` charges the
/// entry's author instead.
fn append_entry(
    relay: &Relay,
    label: &MailboxId,
    entry: &LogEntry,
    bytes: &[u8],
    poster: Option<&[u8; 32]>,
) -> RelayResult {
    let h = &entry.header;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, label)?;
    if let Some(poster) = poster {
        member(&tx, label, poster)?.ok_or(RelayError::Forbidden)?;
    }
    if !mb.sealed {
        return Err(RelayError::Forbidden);
    }
    let author = member(&tx, label, &h.author)?.ok_or(RelayError::Forbidden)?;
    // Verify before answering anything about the log, `Conflict { len }` included: only
    // a member's signed entry learns the log length (audit L7).
    entry
        .verify_signature(&author)
        .map_err(|_| RelayError::Unauthenticated)?;
    if poster.is_none() {
        relay.limit_key(&author.sig_pk)?;
    }
    if mb.vault_id.is_some_and(|v| v != h.mailbox) {
        return Err(RelayError::OtherVault);
    }
    relay.touch(&tx, label, &mb, poster.unwrap_or(&author.sig_pk))?;

    let len: i64 = tx.query_row(
        "SELECT COUNT(*) FROM log_entries WHERE mailbox = ?1",
        params![&label[..]],
        |r| r.get(0),
    )?;
    let head: Option<Vec<u8>> = tx
        .query_row(
            "SELECT hash FROM log_entries WHERE mailbox = ?1 ORDER BY idx DESC LIMIT 1",
            params![&label[..]],
            |r| r.get(0),
        )
        .optional()?;
    let head = match head {
        Some(bytes) => arr32(bytes)?,
        None => GENESIS_PREV_HASH,
    };
    if h.index != len as u64 || h.prev_hash != head {
        tx.commit()?; // keeps the touch
        return ok(&AppendResult::Conflict { len: len as u64 });
    }
    let hash = entry.hash().map_err(|_| RelayError::BadRequest)?;
    if let Some(cap) = relay.quotas.log_bytes {
        let used: i64 = tx.query_row(
            "SELECT log_bytes FROM mailboxes WHERE id = ?1",
            params![&label[..]],
            |r| r.get(0),
        )?;
        if (used as u64).saturating_add(bytes.len() as u64) > cap {
            return Err(RelayError::QuotaExceeded(Quota::Log));
        }
    }
    tx.execute(
        "INSERT INTO log_entries (mailbox, idx, entry, hash) VALUES (?1, ?2, ?3, ?4)",
        params![&label[..], len, bytes, &hash[..]],
    )?;
    tx.execute(
        "UPDATE mailboxes SET log_bytes = log_bytes + ?2 WHERE id = ?1",
        params![&label[..], bytes.len() as i64],
    )?;
    if mb.vault_id.is_none() {
        tx.execute(
            "UPDATE mailboxes SET vault_id = ?2 WHERE id = ?1",
            params![&label[..], &h.mailbox[..]],
        )?;
    }
    // Every other member learns there is vault activity (a proposal, a vote, a send...);
    // the push carries nothing else, the app reads the encrypted log itself.
    let others: Vec<[u8; 32]> = members_of(&tx, label)?
        .iter()
        .map(|m| m.sig_pk)
        .filter(|pk| *pk != h.author)
        .collect();
    let pushes = push_tokens(&tx, label, others.iter())?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(label);
    for (platform, token) in pushes {
        relay.notifier.notify(platform, &token);
    }
    ok(&AppendResult::Appended { index: len as u64 })
}

/// What this relay is and allows (see [`RelayInfo`]): JSON, unauthenticated, nothing about
/// any mailbox.
async fn info(State(relay): State<Relay>) -> Response {
    axum::Json(relay.info()).into_response()
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
    let mb = mailbox(&db, &req.payload.mailbox)?;
    if member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    relay.touch(&db, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
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
    let mb = match self::mailbox(&db, mailbox) {
        Err(RelayError::NotFound) => return Err(RelayError::Forbidden),
        r => r?,
    };
    if member(&db, mailbox, who)?.is_none() {
        return Err(RelayError::Forbidden);
    }
    relay.touch(&db, mailbox, &mb, who)?;
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
