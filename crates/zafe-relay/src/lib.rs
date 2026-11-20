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
use rusqlite::{params, Connection, ErrorCode, OptionalExtension};
pub mod expiry;
pub mod fcm;
pub mod limits;
pub mod push;
pub mod quota;
pub mod wait;

pub use push::PUSH_WINDOW_SECS;

use expiry::{day, Expiry};
use limits::{Buckets, Limits};
use quota::{Quota, Quotas, Usage};
use wait::Waiters;

use zafe_proto::{
    log::GENESIS_PREV_HASH,
    relay::{
        encode_body, join_token_hash, AppendResult, CancelSetup, CancelledSeat, CreateMailbox,
        ForgetPush, InboxAck, InboxAckResponse, InboxRead, InboxResponse, Join, LabelledAppend,
        LabelledReplace, LogRead, LogResponse, MailboxInfo, MailboxesRead, MailboxesResponse,
        MembersRead, MembersResponse, PendingSeat, PostEnvelope, PqKeyPut, PushPlatform,
        RecoveryPut, RecoveryRead, RecoveryValueKind, RecoveryValues, RegisterPush, RelayExpiry,
        RelayInfo, RelayLimits, RelayVersions, Remove, RenewInvite, Reseed, ReseedResponse, Seal,
        SeatCancel, SeatCancelPost, SeatFinishPost, SeatMoveApproval, SetThreshold, Signed,
        WaitRequest, WaitResponse, CREATE_TOKEN_HEADER, MAX_ACK_CURSORS, MAX_REQUEST_SKEW_SECS,
        MAX_WAIT_SECS, UNKNOWN_MAILBOX, UNSUPPORTED_VERSION_HEADER,
    },
    version::{self, UnsupportedVersion},
    Envelope, IdentityPublic, LogEntry, MailboxId, ProtoError, Recipient,
};

pub const RECOVERY_RETENTION_SECS: u64 = 7 * 24 * 3600;
pub const MAX_PQ_KEYS_PER_SUBJECT: i64 = 64;

const PAGE: i64 = zafe_proto::relay::READ_PAGE as i64;

pub const MAX_BODY_BYTES: usize = zafe_proto::relay::MAX_BODY_BYTES;

pub const RESEED_MAX_ENTRIES: usize = 512;

const PRUNE_BATCH: i64 = 1000;

pub const NEARLY_FULL_PERCENT: u64 = 90;

pub const YOUNG_MAILBOX_DAYS: i64 = 30;

pub const DELIVERY_RETENTION_SECS: u64 = 30 * 24 * 3600;

pub const MAX_MEMBERS: u16 = 64;

type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

pub trait Notifier: Send + Sync {
    fn notify(&self, platform: PushPlatform, token: &str);

    fn delivers(&self) -> bool {
        true
    }
}

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
    limits: Limits,
    quotas: Quotas,
    expiry: Expiry,
    create_token: Option<[u8; 32]>,
    waiters: Arc<Waiters>,
    pushes: Arc<push::Pushes>,
}

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
    #[error("this mailbox holds another vault")]
    OtherVault,
    #[error("this relay needs a create token")]
    CreateTokenRequired,
    #[error("this invite expired")]
    InviteExpired,
    #[error("mailbox already exists")]
    Exists,
    #[error("{}", zafe_proto::relay::TOO_MANY_SEAT_MOVES)]
    TooManySeatMoves,
    #[error("request timestamp outside the allowed window")]
    Stale,
    #[error("replayed or reordered envelope")]
    Replay,
    #[error("storage error")]
    Storage,
    #[error("too many requests; retry in {0} s")]
    RateLimited(u64),
    #[error("storage quota exceeded: {0}")]
    QuotaExceeded(Quota),
    #[error(transparent)]
    UnsupportedVersion(#[from] UnsupportedVersion),
}

fn bad_request(e: ProtoError) -> RelayError {
    match e {
        ProtoError::UnsupportedVersion(v) => RelayError::UnsupportedVersion(v),
        _ => RelayError::BadRequest,
    }
}

impl From<rusqlite::Error> for RelayError {
    fn from(e: rusqlite::Error) -> Self {
        if e.sqlite_error_code() == Some(ErrorCode::DiskFull) {
            tracing::warn!("storage: the database is full");
            return RelayError::QuotaExceeded(Quota::Relay);
        }
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
            RelayError::Exists | RelayError::Replay | RelayError::TooManySeatMoves => {
                StatusCode::CONFLICT
            }
            RelayError::Storage | RelayError::UnsupportedVersion(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
            RelayError::RateLimited(_) => StatusCode::TOO_MANY_REQUESTS,
            RelayError::QuotaExceeded(_) => StatusCode::INSUFFICIENT_STORAGE,
        };
        if let RelayError::QuotaExceeded(q) = self {
            return (status, q.as_str()).into_response();
        }
        (status, self.to_string()).into_response()
    }
}

type RelayResult = Result<Vec<u8>, RelayError>;

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
    sender BLOB NOT NULL DEFAULT x'',
    PRIMARY KEY (mailbox, cursor)
);
CREATE INDEX IF NOT EXISTS deliveries_by_recipient ON deliveries (mailbox, recipient, cursor);
CREATE INDEX IF NOT EXISTS deliveries_by_age ON deliveries (created_at);
CREATE TABLE IF NOT EXISTS inbox_counts (
    mailbox BLOB NOT NULL,
    recipient BLOB NOT NULL,
    sender BLOB NOT NULL,
    n INTEGER NOT NULL,
    bytes INTEGER NOT NULL,
    PRIMARY KEY (mailbox, recipient, sender)
);
CREATE INDEX IF NOT EXISTS inbox_counts_by_sender ON inbox_counts (mailbox, sender);
CREATE TABLE IF NOT EXISTS log_counts (
    mailbox BLOB NOT NULL,
    author BLOB NOT NULL,
    bytes INTEGER NOT NULL,
    PRIMARY KEY (mailbox, author)
);
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
CREATE TABLE IF NOT EXISTS seat_moves (
    mailbox BLOB NOT NULL,
    move_id BLOB NOT NULL,
    old BLOB NOT NULL,
    new_sig BLOB NOT NULL,
    new_enc BLOB NOT NULL,
    round INTEGER NOT NULL,
    status INTEGER NOT NULL,
    cancel_at INTEGER,
    cancel_sig BLOB,
    PRIMARY KEY (mailbox, move_id)
);
CREATE TABLE IF NOT EXISTS seat_finishes (
    mailbox BLOB NOT NULL,
    move_id BLOB NOT NULL,
    signer BLOB NOT NULL,
    PRIMARY KEY (mailbox, move_id, signer)
);
CREATE TABLE IF NOT EXISTS recovery_values (
    subject BLOB NOT NULL,
    cosigner BLOB NOT NULL,
    kind INTEGER NOT NULL,
    signed BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (subject, cosigner, kind)
);
CREATE TABLE IF NOT EXISTS recovery_pq_keys (
    subject BLOB NOT NULL,
    owner BLOB NOT NULL,
    signed BLOB NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (subject, owner)
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
    pub fn new() -> Self {
        Self::with_clock(system_clock())
    }

    pub fn with_clock(clock: Clock) -> Self {
        let conn = Connection::open_in_memory().expect("in-memory sqlite");
        Self::from_connection(conn, clock).expect("schema")
    }

    pub fn open(path: &Path) -> Result<Self, RelayError> {
        Self::open_with_clock(path, system_clock())
    }

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
        match found {
            1..=7 => {
                if found == 1 {
                    migrate_from_v1(&conn)?;
                }
                if found <= 2 {
                    migrate_from_v2(&conn)?;
                }
                if found <= 3 {
                    migrate_from_v3(&conn, day(clock()))?;
                }
                if found <= 4 {
                    migrate_from_v4(&conn)?;
                }
                if found <= 6 {
                    migrate_from_v6(&conn)?;
                }
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
            pushes: Arc::new(push::Pushes::default()),
        })
    }

    pub fn with_wait_caps(mut self, per_key: usize, total: usize) -> Self {
        self.waiters = Arc::new(Waiters::new(per_key, total));
        self
    }

    pub fn with_quotas(mut self, quotas: Quotas) -> Self {
        if let Some(cap) = quotas.db_bytes {
            let db = self.db.lock().expect("lock");
            let page: u64 = db
                .pragma_query_value(None, "page_size", |r| r.get::<_, i64>(0))
                .map(|p| p.max(512) as u64)
                .unwrap_or(4096);
            let pages = (cap / page).max(1);
            if let Err(e) = db.pragma_update(None, "max_page_count", pages as i64) {
                tracing::error!("max_page_count: {e}");
            }
        }
        self.quotas = quotas;
        self
    }

    fn nearly_full(&self, db: &Connection) -> Result<bool, RelayError> {
        let Some(cap) = self.quotas.db_bytes else {
            return Ok(false);
        };
        let pages: i64 = db.pragma_query_value(None, "page_count", |r| r.get(0))?;
        let free: i64 = db.pragma_query_value(None, "freelist_count", |r| r.get(0))?;
        let page: i64 = db.pragma_query_value(None, "page_size", |r| r.get(0))?;
        let used = (pages - free).max(0) as u64 * page.max(0) as u64;
        Ok(used.saturating_mul(100) >= cap.saturating_mul(NEARLY_FULL_PERCENT))
    }

    fn check_room_for_creation(&self) -> Result<(), RelayError> {
        let db = self.db.lock().expect("lock");
        if self.nearly_full(&db)? {
            return Err(RelayError::QuotaExceeded(Quota::Relay));
        }
        Ok(())
    }

    fn check_room_for_write(&self, db: &Connection, mb: &MailboxRow) -> Result<(), RelayError> {
        if self.today() - mb.created_day < YOUNG_MAILBOX_DAYS && self.nearly_full(db)? {
            return Err(RelayError::QuotaExceeded(Quota::Relay));
        }
        Ok(())
    }

    pub fn with_expiry(mut self, expiry: Expiry) -> Self {
        self.expiry = expiry;
        self
    }

    pub fn with_create_token(mut self, token: Option<&str>) -> Self {
        self.create_token = token
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(|t| create_token_hash(t.as_bytes()));
        self
    }

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

    pub fn with_limits(mut self, limits: Limits) -> Self {
        self.per_key = limits.per_key.map(|r| Arc::new(Buckets::new(r)));
        self.per_ip = limits.per_ip.map(|r| Arc::new(Buckets::new(r)));
        self.creates_per_ip = limits.creates_per_ip.map(|r| Arc::new(Buckets::daily(r)));
        self.client_ip_header = limits.client_ip_header.clone();
        self.limits = limits;
        self
    }

    pub fn forget_push_token(&self, token: &str) -> Result<usize, RelayError> {
        self.pushes.forget(token);
        let db = self.db.lock().expect("lock");
        Ok(db.execute("DELETE FROM push_tokens WHERE token = ?1", params![token])?)
    }

    fn push(&self, tokens: Vec<(PushPlatform, String)>) {
        let now = self.now();
        for (platform, token) in tokens {
            if self.pushes.admit(platform, &token, now) {
                self.notifier.notify(platform, &token);
            }
        }
    }

    pub fn flush_pushes(&self) -> usize {
        let due = self.pushes.due(self.now());
        for (platform, token) in &due {
            self.notifier.notify(*platform, token);
        }
        due.len()
    }

    fn limit_key(&self, key: &[u8; 32]) -> Result<(), RelayError> {
        match &self.per_key {
            Some(b) => b.take(*key, self.now()).map_err(RelayError::RateLimited),
            None => Ok(()),
        }
    }

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
            .route("/v1/mailbox/replace", post(retired))
            .route(
                "/v1/mailbox/replace-labelled",
                post(replace_member_labelled),
            )
            .route("/v1/seat/finish", post(seat_finish))
            .route("/v1/seat/cancel", post(seat_cancel))
            .route("/v1/mailbox/reseed", post(reseed))
            .route("/v1/mailboxes", post(mailboxes))
            .route("/v1/seat-check/put", post(recovery_put))
            .route("/v1/seat-check/read", post(recovery_read))
            .route("/v1/seat-check/pq", post(recovery_pq_put))
            .route("/v1/seat-check/pq-read", post(recovery_pq_read))
            .route("/v1/push/register", post(register_push))
            .route("/v1/push/forget", post(forget_push))
            .route("/v1/envelope", post(retired))
            .route("/v1/envelope/send", post(post_envelope))
            .route("/v1/inbox", post(inbox))
            .route("/v1/inbox/ack", post(inbox_ack))
            .route("/v1/log/append", post(retired))
            .route("/v1/log/append-labelled", post(log_append_labelled))
            .route("/v1/log/read", post(log_read))
            .route("/v1/wait", post(wait))
            .layer(middleware::from_fn_with_state(self.clone(), limit_ip))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .with_state(self)
    }

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
        let mut deleted = 0;
        loop {
            let mut db = self.db.lock().expect("lock");
            let tx = db.transaction()?;
            let rows = {
                let mut stmt = tx.prepare_cached(
                    "DELETE FROM deliveries WHERE rowid IN
                         (SELECT rowid FROM deliveries WHERE created_at < ?1 LIMIT ?2)
                     RETURNING mailbox, recipient, sender, length(envelope)",
                )?;
                let rows = stmt.query_map(params![cutoff, PRUNE_BATCH], |r| {
                    Ok(Gone {
                        mailbox: r.get(0)?,
                        recipient: r.get(1)?,
                        sender: r.get(2)?,
                        bytes: r.get(3)?,
                    })
                })?;
                rows.collect::<Result<Vec<_>, _>>()?
            };
            uncount(&tx, &rows)?;
            tx.commit()?;
            deleted += rows.len();
            if (rows.len() as i64) < PRUNE_BATCH {
                break;
            }
        }
        let recovery_cutoff = self.now().saturating_sub(RECOVERY_RETENTION_SECS) as i64;
        let db = self.db.lock().expect("lock");
        db.execute(
            "DELETE FROM recovery_values WHERE created_at < ?1",
            params![recovery_cutoff],
        )?;
        db.execute(
            "DELETE FROM recovery_pq_keys WHERE created_at < ?1",
            params![recovery_cutoff],
        )?;
        Ok(deleted)
    }

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

    pub fn info(&self) -> RelayInfo {
        let q = &self.quotas;
        RelayInfo {
            versions: RelayVersions::current(),
            limits: RelayLimits {
                max_body_bytes: MAX_BODY_BYTES as u64,
                max_entry_bytes: zafe_proto::relay::MAX_ENTRY_BYTES as u64,
                max_members: MAX_MEMBERS,
                log_bytes: q.log_bytes,
                delivery_bytes: q.delivery_bytes,
                envelopes_per_pair: q.envelopes_per_pair,
                pair_bytes: q.pair_bytes,
                author_log_bytes: q.log_bytes.map(quota::author_log_cap),
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

    fn charge_creation(
        &self,
        ip: Option<std::net::IpAddr>,
        headers: &HeaderMap,
    ) -> Result<(), RelayError> {
        self.charge_daily(ip)?;
        self.check_create_token(headers)
    }

    fn charge_daily(&self, ip: Option<std::net::IpAddr>) -> Result<(), RelayError> {
        if let (Some(buckets), Some(ip)) = (&self.creates_per_ip, ip) {
            buckets
                .take(ip, self.now())
                .map_err(RelayError::RateLimited)?;
        }
        Ok(())
    }

    fn check_fresh(&self, timestamp: u64) -> Result<(), RelayError> {
        if timestamp.abs_diff(self.now()) > MAX_REQUEST_SKEW_SECS {
            return Err(RelayError::Stale);
        }
        Ok(())
    }
}

fn create_token_hash(token: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"Zafe_CreateToken")
        .hash(token)
        .as_bytes()
        .try_into()
        .expect("32 bytes")
}

fn delete_mailbox(tx: &Connection, id: &MailboxId) -> Result<(), RelayError> {
    for table in [
        "members",
        "deliveries",
        "sender_seqs",
        "push_tokens",
        "log_entries",
        "seat_moves",
        "seat_finishes",
        "inbox_counts",
        "log_counts",
    ] {
        tx.execute(
            &format!("DELETE FROM {table} WHERE mailbox = ?1"),
            params![&id[..]],
        )?;
    }
    tx.execute("DELETE FROM mailboxes WHERE id = ?1", params![&id[..]])?;
    Ok(())
}

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

fn migrate_from_v4(conn: &Connection) -> Result<(), RelayError> {
    conn.execute_batch(
        "BEGIN;
         DROP TABLE IF EXISTS recovery_values;
         PRAGMA user_version = 5;
         COMMIT;",
    )?;
    tracing::info!("relay database migrated from schema 4 to 5 (per co-signer recovery)");
    Ok(())
}

fn migrate_from_v6(conn: &Connection) -> Result<(), RelayError> {
    let tx = conn.unchecked_transaction()?;
    let has_sender: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM pragma_table_info('deliveries') WHERE name = 'sender')",
        [],
        |r| r.get(0),
    )?;
    if !has_sender {
        tx.execute_batch("ALTER TABLE deliveries ADD COLUMN sender BLOB NOT NULL DEFAULT x''")?;
    }
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS inbox_counts (
             mailbox BLOB NOT NULL,
             recipient BLOB NOT NULL,
             sender BLOB NOT NULL,
             n INTEGER NOT NULL,
             bytes INTEGER NOT NULL,
             PRIMARY KEY (mailbox, recipient, sender)
         );
         DELETE FROM inbox_counts;",
    )?;
    let mut after = 0i64;
    loop {
        let batch: Vec<(i64, Vec<u8>)> = {
            let mut stmt = tx.prepare(
                "SELECT rowid, envelope FROM deliveries WHERE rowid > ?1 ORDER BY rowid LIMIT 64",
            )?;
            let rows = stmt.query_map(params![after], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        let Some(&(last, _)) = batch.last() else {
            break;
        };
        for (rowid, envelope) in &batch {
            let sender = Envelope::from_bytes(envelope)
                .map(|e| e.header.from.to_vec())
                .unwrap_or_default();
            tx.execute(
                "UPDATE deliveries SET sender = ?2 WHERE rowid = ?1",
                params![rowid, sender],
            )?;
        }
        after = last;
    }
    tx.execute_batch(
        "INSERT INTO inbox_counts (mailbox, recipient, sender, n, bytes)
             SELECT mailbox, recipient, sender, COUNT(*), SUM(length(envelope))
             FROM deliveries GROUP BY mailbox, recipient, sender;
         CREATE TABLE IF NOT EXISTS log_counts (
             mailbox BLOB NOT NULL,
             author BLOB NOT NULL,
             bytes INTEGER NOT NULL,
             PRIMARY KEY (mailbox, author)
         );
         DELETE FROM log_counts;",
    )?;
    let mut after = 0i64;
    loop {
        let batch: Vec<(i64, Vec<u8>, Vec<u8>)> = {
            let mut stmt = tx.prepare(
                "SELECT rowid, mailbox, entry FROM log_entries WHERE rowid > ?1
                 ORDER BY rowid LIMIT 64",
            )?;
            let rows = stmt.query_map(params![after], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
            rows.collect::<Result<_, _>>()?
        };
        let Some(&(last, _, _)) = batch.last() else {
            break;
        };
        for (_, mailbox, entry) in &batch {
            let author = LogEntry::from_bytes(entry)
                .map(|e| e.header.author.to_vec())
                .unwrap_or_default();
            count_log(&tx, mailbox, &author, entry.len())?;
        }
        after = last;
    }
    tx.execute_batch("PRAGMA user_version = 7;")?;
    tx.commit()?;
    tracing::info!("relay database migrated from schema 6 to 7 (inbox counters per sender, log counters per author)");
    Ok(())
}

fn count_log(tx: &Connection, mailbox: &[u8], author: &[u8], len: usize) -> Result<(), RelayError> {
    tx.execute(
        "INSERT INTO log_counts (mailbox, author, bytes) VALUES (?1, ?2, ?3)
         ON CONFLICT (mailbox, author) DO UPDATE SET bytes = bytes + excluded.bytes",
        params![mailbox, author, len as i64],
    )?;
    Ok(())
}

struct Gone {
    mailbox: Vec<u8>,
    recipient: Vec<u8>,
    sender: Vec<u8>,
    bytes: i64,
}

fn uncount(tx: &Connection, gone: &[Gone]) -> Result<(), RelayError> {
    use std::collections::BTreeMap;
    type Pair<'a> = (&'a [u8], &'a [u8], &'a [u8]);
    let mut pairs: BTreeMap<Pair, (i64, i64)> = BTreeMap::new();
    let mut mailboxes: BTreeMap<&[u8], i64> = BTreeMap::new();
    for g in gone {
        let pair = pairs
            .entry((&g.mailbox, &g.recipient, &g.sender))
            .or_default();
        pair.0 += 1;
        pair.1 += g.bytes;
        *mailboxes.entry(&g.mailbox).or_default() += g.bytes;
    }
    for ((mailbox, recipient, sender), (n, bytes)) in pairs {
        tx.execute(
            "UPDATE inbox_counts SET n = n - ?4, bytes = bytes - ?5
             WHERE mailbox = ?1 AND recipient = ?2 AND sender = ?3",
            params![mailbox, recipient, sender, n, bytes],
        )?;
        tx.execute(
            "DELETE FROM inbox_counts
             WHERE mailbox = ?1 AND recipient = ?2 AND sender = ?3 AND n <= 0",
            params![mailbox, recipient, sender],
        )?;
    }
    for (mailbox, bytes) in mailboxes {
        tx.execute(
            "UPDATE mailboxes SET delivery_bytes = MAX(0, delivery_bytes - ?2) WHERE id = ?1",
            params![mailbox, bytes],
        )?;
    }
    Ok(())
}

fn check_delivery_quotas(
    tx: &Connection,
    quotas: &Quotas,
    mailbox: &MailboxId,
    sender: &[u8; 32],
    recipients: &[[u8; 32]],
    len: usize,
) -> Result<(), RelayError> {
    let len = len as u64;
    if quotas.envelopes_per_pair.is_some() || quotas.pair_bytes.is_some() {
        let mut stmt = tx.prepare_cached(
            "SELECT n, bytes FROM inbox_counts
             WHERE mailbox = ?1 AND recipient = ?2 AND sender = ?3",
        )?;
        for recipient in recipients {
            let (count, bytes): (i64, i64) = stmt
                .query_row(params![&mailbox[..], &recipient[..], &sender[..]], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?
                .unwrap_or((0, 0));
            let (count, bytes) = (count.max(0) as u64, bytes.max(0) as u64);
            if quotas.envelopes_per_pair.is_some_and(|cap| count >= cap)
                || quotas
                    .pair_bytes
                    .is_some_and(|cap| bytes.saturating_add(len) > cap)
            {
                return Err(RelayError::QuotaExceeded(Quota::Recipient));
            }
        }
    }
    if let Some(cap) = quotas.delivery_bytes {
        let adding = len.saturating_mul(recipients.len() as u64);
        let (total, mine): (i64, i64) = tx.query_row(
            "SELECT (SELECT delivery_bytes FROM mailboxes WHERE id = ?1),
                    (SELECT COALESCE(SUM(bytes), 0) FROM inbox_counts
                     WHERE mailbox = ?1 AND sender = ?2)",
            params![&mailbox[..], &sender[..]],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let sender_cap = quota::sender_delivery_cap(cap, quotas.pair_bytes);
        if (total.max(0) as u64).saturating_add(adding) > cap
            || (mine.max(0) as u64).saturating_add(adding) > sender_cap
        {
            return Err(RelayError::QuotaExceeded(Quota::Deliveries));
        }
    }
    Ok(())
}

fn log_len(db: &Connection, id: &MailboxId) -> Result<u64, RelayError> {
    let len: i64 = db.query_row(
        "SELECT COALESCE(MAX(idx) + 1, 0) FROM log_entries WHERE mailbox = ?1",
        params![&id[..]],
        |r| r.get(0),
    )?;
    Ok(len as u64)
}

struct MailboxRow {
    creator: [u8; 32],
    join_token_hash: [u8; 32],
    max_members: u16,
    sealed: bool,
    threshold: u16,
    issued_at: u64,
    vault_id: Option<MailboxId>,
    last_seen_day: i64,
    created_day: i64,
    shared: bool,
}

fn arr32(v: Vec<u8>) -> Result<[u8; 32], RelayError> {
    v.try_into().map_err(|_| RelayError::Storage)
}

fn mailbox(db: &Connection, id: &MailboxId) -> Result<MailboxRow, RelayError> {
    let row = db
        .query_row(
            "SELECT creator, join_token_hash, max_members, sealed, threshold, created_at,
                    vault_id, last_seen_day, shared, created_day
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
                        r.get::<_, i64>(9)?,
                    ),
                ))
            },
        )
        .optional()?
        .ok_or(RelayError::NotFound)?;
    let (row, (vault_id, last_seen_day, shared, created_day)) = row;
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
        created_day,
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

fn verified<T>(relay: &Relay, body: &Bytes) -> Result<Signed<T>, RelayError>
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let signed = Signed::<T>::from_bytes(body).map_err(bad_request)?;
    signed.verify().map_err(|_| RelayError::Unauthenticated)?;
    relay.limit_key(&signed.signer.sig_pk)?;
    Ok(signed)
}

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
    relay.check_room_for_creation()?;
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
    if req.signer.sig_pk != mb.creator {
        return Err(RelayError::Forbidden);
    }
    let logged: bool = db.query_row(
        "SELECT EXISTS (SELECT 1 FROM log_entries WHERE mailbox = ?1)",
        params![&id[..]],
        |r| r.get(0),
    )?;
    if logged {
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
    if !members.iter().any(|m| m.sig_pk == req.signer.sig_pk)
        && pending_member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none()
    {
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
    if !members.iter().any(|m| m.sig_pk == req.signer.sig_pk)
        && pending_member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none()
    {
        return Err(RelayError::Forbidden);
    }
    relay.touch(&db, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
    let pending = pending_seats(&db, &req.payload.mailbox)?;
    let vault_id = mb.vault_id.unwrap_or(req.payload.mailbox);
    let cancelled = cancelled_seats(&db, &vault_id, &req.payload.mailbox)?;
    ok(&MailboxInfo {
        members,
        sealed: mb.sealed,
        threshold: mb.threshold,
        pending,
        cancelled,
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

async fn retired() -> RelayResult {
    Err(RelayError::UnsupportedVersion(UnsupportedVersion {
        format: version::Format::RelayApi,
        found: version::RELAY_API - 1,
        supported: version::RELAY_API,
    }))
}

const SEAT_PENDING: i64 = 0;
const SEAT_CANCELLED: i64 = 1;
const SEAT_FINISHED: i64 = 2;

struct SeatRow {
    old: [u8; 32],
    new: IdentityPublic,
    round: u32,
    status: i64,
}

fn seat_row(
    db: &Connection,
    id: &MailboxId,
    move_id: &[u8; 32],
) -> Result<Option<SeatRow>, RelayError> {
    let row = db
        .query_row(
            "SELECT old, new_sig, new_enc, round, status FROM seat_moves
             WHERE mailbox = ?1 AND move_id = ?2",
            params![&id[..], &move_id[..]],
            |r| {
                Ok((
                    r.get::<_, Vec<u8>>(0)?,
                    r.get::<_, Vec<u8>>(1)?,
                    r.get::<_, Vec<u8>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?;
    row.map(|(old, sig, enc, round, status)| {
        Ok(SeatRow {
            old: arr32(old)?,
            new: IdentityPublic {
                sig_pk: arr32(sig)?,
                enc_pk: arr32(enc)?,
            },
            round: u32::try_from(round).map_err(|_| RelayError::Storage)?,
            status,
        })
    })
    .transpose()
}

fn pending_seats(db: &Connection, id: &MailboxId) -> Result<Vec<PendingSeat>, RelayError> {
    let mut stmt = db.prepare(
        "SELECT move_id, old, new_sig, new_enc, round FROM seat_moves
         WHERE mailbox = ?1 AND status = ?2 ORDER BY move_id",
    )?;
    let rows = stmt.query_map(params![&id[..], SEAT_PENDING], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
            r.get::<_, i64>(4)?,
        ))
    })?;
    rows.map(|r| {
        let (move_id, old, sig, enc, round) = r?;
        Ok(PendingSeat {
            move_id: arr32(move_id)?,
            old: arr32(old)?,
            new: IdentityPublic {
                sig_pk: arr32(sig)?,
                enc_pk: arr32(enc)?,
            },
            round: u32::try_from(round).map_err(|_| RelayError::Storage)?,
        })
    })
    .collect()
}

fn pending_member(
    db: &Connection,
    id: &MailboxId,
    pk: &[u8; 32],
) -> Result<Option<IdentityPublic>, RelayError> {
    Ok(pending_seats(db, id)?
        .into_iter()
        .find(|p| &p.new.sig_pk == pk)
        .map(|p| p.new))
}

fn drop_finishes(tx: &Connection, id: &MailboxId, moves: &[[u8; 32]]) -> Result<(), RelayError> {
    for move_id in moves {
        tx.execute(
            "DELETE FROM seat_finishes WHERE mailbox = ?1 AND move_id = ?2",
            params![&id[..], &move_id[..]],
        )?;
    }
    Ok(())
}

async fn replace_member_labelled(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<LabelledReplace>(&relay, &body)?;
    let p = req.payload;
    if p.approvals.len() > usize::from(MAX_MEMBERS) {
        return Err(RelayError::BadRequest);
    }
    let mut keys = std::collections::BTreeSet::new();
    if !p.approvals.iter().all(|(pk, _)| keys.insert(*pk)) {
        return Err(RelayError::BadRequest);
    }
    let signed_by: std::collections::BTreeSet<[u8; 32]> = p
        .approvals
        .iter()
        .filter(|(pk, signature)| {
            let by = IdentityPublic {
                sig_pk: *pk,
                enc_pk: [0; 32],
            };
            p.approval.verify(&by, signature).is_ok()
        })
        .map(|(pk, _)| *pk)
        .collect();
    replace_in(
        &relay,
        &req.signer.sig_pk,
        &p.mailbox,
        p.approval,
        p.approvals.len(),
        &signed_by,
    )
}

fn replace_in(
    relay: &Relay,
    signer: &[u8; 32],
    label: &MailboxId,
    a: SeatMoveApproval,
    approvals: usize,
    signed_by: &std::collections::BTreeSet<[u8; 32]>,
) -> RelayResult {
    let move_id = a.move_id();
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, label)?;
    if mb.vault_id.unwrap_or(*label) != a.vault_id {
        return Err(RelayError::OtherVault);
    }
    let current = members_of(&tx, label)?;
    let is_member = |pk: &[u8; 32]| current.iter().any(|m| &m.sig_pk == pk);
    if !is_member(signer) {
        return Err(RelayError::Forbidden);
    }
    if approvals > current.len() {
        return Err(RelayError::BadRequest);
    }
    match seat_row(&tx, label, &move_id)? {
        Some(row) if row.status != SEAT_CANCELLED => return ok(&()),
        Some(_) => return Err(RelayError::Forbidden),
        None => {}
    }
    if !mb.sealed
        || mb.threshold == 0
        || !is_member(&a.old)
        || is_member(&a.new.sig_pk)
        || a.new.sig_pk == a.old
    {
        return Err(RelayError::Forbidden);
    }
    let pending: i64 = tx.query_row(
        "SELECT COUNT(*) FROM seat_moves WHERE mailbox = ?1 AND old = ?2 AND status = ?3",
        params![&label[..], &a.old[..], SEAT_PENDING],
        |r| r.get(0),
    )?;
    if pending as usize >= zafe_proto::relay::MAX_PENDING_SEAT_MOVES {
        return Err(RelayError::TooManySeatMoves);
    }
    let approvers = signed_by
        .iter()
        .filter(|pk| **pk != a.old && is_member(pk))
        .count();
    if approvers < usize::from(mb.threshold) {
        return Err(RelayError::Forbidden);
    }
    tx.execute(
        "INSERT INTO seat_moves (mailbox, move_id, old, new_sig, new_enc, round, status)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            &label[..],
            &move_id[..],
            &a.old[..],
            &a.new.sig_pk[..],
            &a.new.enc_pk[..],
            i64::from(a.round),
            SEAT_PENDING
        ],
    )?;
    relay.touch(&tx, label, &mb, signer)?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(label);
    ok(&())
}

async fn seat_finish(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let post: SeatFinishPost = zafe_proto::relay::decode_body(&body).map_err(bad_request)?;
    let f = post.finish;
    f.verify(&post.by, &post.signature)
        .map_err(|_| RelayError::Unauthenticated)?;
    relay.limit_key(&post.by.sig_pk)?;
    let by = post.by.sig_pk;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &f.mailbox)?;
    if mb.vault_id.unwrap_or(f.mailbox) != f.vault_id {
        return Err(RelayError::OtherVault);
    }
    let current = members_of(&tx, &f.mailbox)?;
    let is_member = |pk: &[u8; 32]| current.iter().any(|m| &m.sig_pk == pk);
    if !is_member(&by) || by == f.old {
        return Err(RelayError::Forbidden);
    }
    let row = seat_row(&tx, &f.mailbox, &f.move_id)?.ok_or(RelayError::Forbidden)?;
    if row.old != f.old || row.new.sig_pk != f.new {
        return Err(RelayError::Forbidden);
    }
    if row.status == SEAT_FINISHED {
        return ok(&());
    }
    tx.execute(
        "INSERT OR IGNORE INTO seat_finishes (mailbox, move_id, signer) VALUES (?1, ?2, ?3)",
        params![&f.mailbox[..], &f.move_id[..], &by[..]],
    )?;
    let finishers: Vec<[u8; 32]> = {
        let mut stmt =
            tx.prepare("SELECT signer FROM seat_finishes WHERE mailbox = ?1 AND move_id = ?2")?;
        let rows = stmt.query_map(params![&f.mailbox[..], &f.move_id[..]], |r| {
            r.get::<_, Vec<u8>>(0)
        })?;
        rows.map(|r| arr32(r?)).collect::<Result<_, _>>()?
    };
    let counted = finishers
        .iter()
        .filter(|pk| is_member(pk) && **pk != f.old)
        .count();
    if counted >= usize::from(mb.threshold) && mb.threshold != 0 && is_member(&f.old) {
        tx.execute(
            "DELETE FROM members WHERE mailbox = ?1 AND sig_pk = ?2",
            params![&f.mailbox[..], &f.old[..]],
        )?;
        tx.execute(
            "DELETE FROM push_tokens WHERE mailbox = ?1 AND member = ?2",
            params![&f.mailbox[..], &f.old[..]],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO members (mailbox, sig_pk, enc_pk) VALUES (?1, ?2, ?3)",
            params![&f.mailbox[..], &row.new.sig_pk[..], &row.new.enc_pk[..]],
        )?;
        tx.execute(
            "UPDATE seat_moves SET status = ?3 WHERE mailbox = ?1 AND move_id = ?2",
            params![&f.mailbox[..], &f.move_id[..], SEAT_FINISHED],
        )?;
        let moot: Vec<[u8; 32]> = pending_seats(&tx, &f.mailbox)?
            .into_iter()
            .filter(|p| p.old == f.old || p.new.sig_pk == f.new)
            .map(|p| p.move_id)
            .collect();
        for move_id in &moot {
            tx.execute(
                "DELETE FROM seat_moves WHERE mailbox = ?1 AND move_id = ?2",
                params![&f.mailbox[..], &move_id[..]],
            )?;
        }
        drop_finishes(&tx, &f.mailbox, &moot)?;
        drop_finishes(&tx, &f.mailbox, &[f.move_id])?;
    }
    relay.touch(&tx, &f.mailbox, &mb, &by)?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(&f.mailbox);
    ok(&())
}

async fn seat_cancel(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let post: SeatCancelPost = zafe_proto::relay::decode_body(&body).map_err(bad_request)?;
    let (c, label) = (post.cancel, post.cancel.mailbox);
    if post.signature.len() > 128 {
        return Err(RelayError::BadRequest);
    }
    let by = IdentityPublic {
        sig_pk: c.old,
        enc_pk: [0; 32],
    };
    c.verify(&by, &post.signature)
        .map_err(|_| RelayError::Unauthenticated)?;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &label)?;
    if mb.vault_id.unwrap_or(label) != c.vault_id {
        return Err(RelayError::OtherVault);
    }
    let old = member(&tx, &label, &c.old)?.ok_or(RelayError::Forbidden)?;
    let row = seat_row(&tx, &label, &c.move_id)?.ok_or(RelayError::Forbidden)?;
    if row.old != c.old || row.new.sig_pk != c.new || row.round != 0 {
        return Err(RelayError::Forbidden);
    }
    match row.status {
        SEAT_CANCELLED => return ok(&()),
        SEAT_PENDING => {}
        _ => return Err(RelayError::Forbidden),
    }
    relay.limit_key(&old.sig_pk)?;
    tx.execute(
        "UPDATE seat_moves SET status = ?3, cancel_at = ?4, cancel_sig = ?5
         WHERE mailbox = ?1 AND move_id = ?2",
        params![
            &label[..],
            &c.move_id[..],
            SEAT_CANCELLED,
            i64::try_from(c.at).unwrap_or(i64::MAX),
            &post.signature[..]
        ],
    )?;
    relay.touch(&tx, &label, &mb, &c.old)?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(&label);
    ok(&())
}

fn cancelled_seats(
    db: &Connection,
    vault_id: &MailboxId,
    id: &MailboxId,
) -> Result<Vec<CancelledSeat>, RelayError> {
    let mut stmt = db.prepare(
        "SELECT move_id, old, new_sig, cancel_at, cancel_sig FROM seat_moves
         WHERE mailbox = ?1 AND status = ?2 AND cancel_sig IS NOT NULL ORDER BY move_id",
    )?;
    let rows = stmt.query_map(params![&id[..], SEAT_CANCELLED], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, Vec<u8>>(1)?,
            r.get::<_, Vec<u8>>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, Vec<u8>>(4)?,
        ))
    })?;
    rows.map(|r| {
        let (move_id, old, new, at, signature) = r?;
        Ok(CancelledSeat {
            cancel: SeatCancel {
                mailbox: *id,
                vault_id: *vault_id,
                move_id: arr32(move_id)?,
                old: arr32(old)?,
                new: arr32(new)?,
                at: u64::try_from(at).unwrap_or(0),
            },
            signature,
        })
    })
    .collect()
}

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
    let take = p.entries.len().min(RESEED_MAX_ENTRIES);
    let batch = &p.entries[..take];
    if zafe_proto::relay::page_bytes(batch.iter().map(Vec::as_slice)) > MAX_BODY_BYTES {
        return Err(RelayError::BadRequest);
    }
    let mut checked = Vec::with_capacity(take);
    for bytes in batch {
        let entry = LogEntry::from_bytes(bytes).map_err(bad_request)?;
        entry
            .verify_signature(&IdentityPublic {
                sig_pk: entry.header.author,
                enc_pk: [0; 32],
            })
            .map_err(|_| RelayError::Unauthenticated)?;
        let hash = entry.hash().map_err(|_| RelayError::BadRequest)?;
        checked.push((entry.header, hash));
    }
    let all = take == p.entries.len();
    let exists = {
        let db = relay.db.lock().expect("lock");
        match mailbox(&db, &p.mailbox) {
            Ok(_) => true,
            Err(RelayError::NotFound) => false,
            Err(e) => return Err(e),
        }
    };
    if !exists {
        relay.check_room_for_creation()?;
        relay.charge_creation(ip, &headers)?;
    }

    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let stored_vault = match mailbox(&tx, &p.mailbox) {
        Err(RelayError::NotFound) if exists => return Err(RelayError::NotFound),
        Err(RelayError::NotFound) => {
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
            member(&tx, &p.mailbox, &signer)?.ok_or(RelayError::Forbidden)?;
            relay.touch(&tx, &p.mailbox, &mb, &signer)?;
            let len = log_len(&tx, &p.mailbox)?;
            tx.commit()?;
            return ok(&ReseedResponse {
                len,
                open: true,
                restored: false,
            });
        }
        Ok(mb) => {
            if mb.creator != signer || mb.join_token_hash != [0u8; 32] {
                return Err(RelayError::Forbidden);
            }
            relay.check_room_for_write(&tx, &mb)?;
            relay.touch(&tx, &p.mailbox, &mb, &signer)?;
            mb.vault_id
        }
    };

    let mut len = log_len(&tx, &p.mailbox)?;
    if p.from != len {
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
    for ((h, hash), bytes) in checked.iter().zip(batch) {
        if h.index != len || h.prev_hash != head {
            return Err(RelayError::BadRequest);
        }
        match vault {
            Some(v) if v != h.mailbox => return Err(RelayError::OtherVault),
            Some(_) => {}
            None => vault = Some(h.mailbox),
        }
        if let Some(cap) = relay.quotas.log_bytes {
            if (used as u64).saturating_add(bytes.len() as u64) > cap {
                return Err(RelayError::QuotaExceeded(Quota::Log));
            }
        }
        tx.execute(
            "INSERT INTO log_entries (mailbox, idx, entry, hash) VALUES (?1, ?2, ?3, ?4)",
            params![&p.mailbox[..], len as i64, &bytes[..], &hash[..]],
        )?;
        count_log(&tx, &p.mailbox[..], &h.author[..], bytes.len())?;
        used += bytes.len() as i64;
        len += 1;
        head = *hash;
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
    let open = p.finish && all && len > 0;
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
        "SELECT mailbox FROM members WHERE sig_pk = ?1 AND enc_pk = ?2
         UNION SELECT mailbox FROM seat_moves WHERE new_sig = ?1 AND new_enc = ?2 AND status = ?3
         ORDER BY mailbox",
    )?;
    let rows = stmt.query_map(
        params![&req.signer.sig_pk[..], &req.signer.enc_pk[..], SEAT_PENDING],
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

async fn recovery_put(
    State(relay): State<Relay>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    body: Bytes,
) -> RelayResult {
    let req = verified::<RecoveryPut>(&relay, &body)?;
    let p = &req.payload;
    relay.check_fresh(p.timestamp)?;
    let signer = req.signer.sig_pk;
    let by_cosigner = matches!(
        p.kind,
        RecoveryValueKind::Announce | RecoveryValueKind::Challenge
    );
    let allowed = p.subject != p.cosigner
        && if by_cosigner {
            signer == p.cosigner
        } else {
            signer == p.subject
        };
    if !allowed {
        return Err(RelayError::Forbidden);
    }
    let signed = req.to_bytes().map_err(|_| RelayError::BadRequest)?;
    if by_cosigner {
        relay.charge_daily(ip)?;
    }
    let db = relay.db.lock().expect("lock");
    if !by_cosigner {
        let announced: bool = db.query_row(
            "SELECT EXISTS (SELECT 1 FROM recovery_values
                            WHERE subject = ?1 AND cosigner = ?2 AND kind = ?3)",
            params![
                &p.subject[..],
                &p.cosigner[..],
                RecoveryValueKind::Announce as i64
            ],
            |r| r.get(0),
        )?;
        if !announced {
            return Err(RelayError::Forbidden);
        }
    }
    db.execute(
        "INSERT OR IGNORE INTO recovery_values (subject, cosigner, kind, signed, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            &p.subject[..],
            &p.cosigner[..],
            p.kind as i64,
            &signed[..],
            relay.now() as i64
        ],
    )?;
    ok(&())
}

async fn recovery_read(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<RecoveryRead>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    let mut stmt = db
        .prepare("SELECT signed FROM recovery_values WHERE subject = ?1 ORDER BY cosigner, kind")?;
    let values = stmt
        .query_map(params![&req.payload.subject[..]], |r| {
            r.get::<_, Vec<u8>>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    ok(&RecoveryValues { values })
}

async fn recovery_pq_put(
    State(relay): State<Relay>,
    Extension(ClientIp(ip)): Extension<ClientIp>,
    body: Bytes,
) -> RelayResult {
    let req = verified::<PqKeyPut>(&relay, &body)?;
    let p = &req.payload;
    relay.check_fresh(p.timestamp)?;
    if req.signer.sig_pk != p.owner {
        return Err(RelayError::Forbidden);
    }
    if p.key.len() != zafe_proto::relay::PQ_KEY_LEN {
        return Err(RelayError::BadRequest);
    }
    let signed = req.to_bytes().map_err(|_| RelayError::BadRequest)?;
    relay.charge_daily(ip)?;
    let db = relay.db.lock().expect("lock");
    let (count, present): (i64, bool) = db.query_row(
        "SELECT COUNT(*), COALESCE(MAX(owner = ?2), 0) FROM recovery_pq_keys WHERE subject = ?1",
        params![&p.subject[..], &p.owner[..]],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if !present && count >= MAX_PQ_KEYS_PER_SUBJECT {
        return Err(RelayError::Forbidden);
    }
    db.execute(
        "INSERT OR IGNORE INTO recovery_pq_keys (subject, owner, signed, created_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            &p.subject[..],
            &p.owner[..],
            &signed[..],
            relay.now() as i64
        ],
    )?;
    ok(&())
}

async fn recovery_pq_read(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<RecoveryRead>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let db = relay.db.lock().expect("lock");
    let mut stmt =
        db.prepare("SELECT signed FROM recovery_pq_keys WHERE subject = ?1 ORDER BY owner")?;
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
    let req = verified::<PostEnvelope>(&relay, &body)?;
    let body = &req.payload.envelope;
    if body.len() > zafe_proto::relay::MAX_ENTRY_BYTES {
        return Err(RelayError::BadRequest);
    }
    let envelope = Envelope::from_bytes(body).map_err(bad_request)?;
    let h = &envelope.header;
    if h.from != req.signer.sig_pk {
        return Err(RelayError::Forbidden);
    }
    envelope
        .verify(&req.signer)
        .map_err(|_| RelayError::Unauthenticated)?;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &h.mailbox)?;
    let members = members_of(&tx, &h.mailbox)?;
    let sender = members
        .iter()
        .find(|m| m.sig_pk == h.from)
        .ok_or(RelayError::Forbidden)?;

    let recipients: Vec<[u8; 32]> = match h.to {
        Recipient::One(pk) if pk != sender.sig_pk && members.iter().any(|m| m.sig_pk == pk) => {
            vec![pk]
        }
        Recipient::One(pk)
            if pk != sender.sig_pk && pending_member(&tx, &h.mailbox, &pk)?.is_some() =>
        {
            vec![pk]
        }
        Recipient::One(_) => return Err(RelayError::Forbidden),
        Recipient::All => members
            .iter()
            .map(|m| m.sig_pk)
            .filter(|pk| *pk != sender.sig_pk)
            .collect(),
    };

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

    relay.check_room_for_write(&tx, &mb)?;
    check_delivery_quotas(
        &tx,
        &relay.quotas,
        &h.mailbox,
        &sender.sig_pk,
        &recipients,
        body.len(),
    )?;
    relay.touch(&tx, &h.mailbox, &mb, &sender.sig_pk)?;
    let now = relay.now() as i64;
    for recipient in &recipients {
        let cursor: i64 = tx.query_row(
            "SELECT next_cursor FROM mailboxes WHERE id = ?1",
            params![&h.mailbox[..]],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO deliveries (mailbox, cursor, recipient, envelope, created_at, sender)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                &h.mailbox[..],
                cursor,
                &recipient[..],
                &body[..],
                now,
                &h.from[..]
            ],
        )?;
        tx.execute(
            "UPDATE mailboxes SET next_cursor = next_cursor + 1,
                 delivery_bytes = delivery_bytes + ?2 WHERE id = ?1",
            params![&h.mailbox[..], body.len() as i64],
        )?;
        tx.execute(
            "INSERT INTO inbox_counts (mailbox, recipient, sender, n, bytes)
             VALUES (?1, ?2, ?3, 1, ?4)
             ON CONFLICT (mailbox, recipient, sender) DO UPDATE
                 SET n = n + 1, bytes = bytes + excluded.bytes",
            params![
                &h.mailbox[..],
                &recipient[..],
                &h.from[..],
                body.len() as i64
            ],
        )?;
    }

    let pushes = push_tokens(&tx, &h.mailbox, recipients.iter())?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(&h.mailbox);
    relay.push(pushes);
    ok(&())
}

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
    let who = &req.signer.sig_pk;
    if member(&db, &req.payload.mailbox, who)?.is_none()
        && pending_member(&db, &req.payload.mailbox, who)?.is_none()
    {
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
    let (mut envelopes, mut bytes) = (Vec::new(), 0usize);
    for row in rows {
        let (cursor, envelope) = row?;
        if !zafe_proto::relay::page_has_room(envelopes.len(), bytes, envelope.len()) {
            break;
        }
        bytes += envelope.len();
        envelopes.push((cursor as u64, envelope));
    }
    ok(&InboxResponse { envelopes })
}

async fn inbox_ack(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<InboxAck>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    if req.payload.cursors.len() > MAX_ACK_CURSORS {
        return Err(RelayError::BadRequest);
    }
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, &req.payload.mailbox)?;
    let who = &req.signer.sig_pk;
    if member(&tx, &req.payload.mailbox, who)?.is_none()
        && pending_member(&tx, &req.payload.mailbox, who)?.is_none()
    {
        return Err(RelayError::Forbidden);
    }
    relay.touch(&tx, &req.payload.mailbox, &mb, &req.signer.sig_pk)?;
    let mut gone = Vec::new();
    for &cursor in &req.payload.cursors {
        let Ok(cursor) = i64::try_from(cursor) else {
            continue;
        };
        let row = tx
            .query_row(
                "DELETE FROM deliveries WHERE mailbox = ?1 AND recipient = ?2 AND cursor = ?3
                 RETURNING sender, length(envelope)",
                params![&req.payload.mailbox[..], &req.signer.sig_pk[..], cursor],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, i64>(1)?)),
            )
            .optional()?;
        if let Some((sender, bytes)) = row {
            gone.push(Gone {
                mailbox: req.payload.mailbox.to_vec(),
                recipient: req.signer.sig_pk.to_vec(),
                sender,
                bytes,
            });
        }
    }
    let deleted = gone.len() as u64;
    uncount(&tx, &gone)?;
    tx.commit()?;
    ok(&InboxAckResponse { deleted })
}

async fn log_append_labelled(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<LabelledAppend>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    if req.payload.entry.len() > zafe_proto::relay::MAX_ENTRY_BYTES {
        return Err(RelayError::BadRequest);
    }
    let entry = LogEntry::from_bytes(&req.payload.entry).map_err(bad_request)?;
    entry
        .verify_signature(&IdentityPublic {
            sig_pk: entry.header.author,
            enc_pk: [0; 32],
        })
        .map_err(|_| RelayError::Unauthenticated)?;
    let hash = entry.hash().map_err(|_| RelayError::BadRequest)?;
    append_entry(
        &relay,
        &req.payload.mailbox,
        &entry,
        hash,
        &req.payload.entry,
        &req.signer.sig_pk,
    )
}

fn append_entry(
    relay: &Relay,
    label: &MailboxId,
    entry: &LogEntry,
    hash: [u8; 32],
    bytes: &[u8],
    poster: &[u8; 32],
) -> RelayResult {
    let h = &entry.header;
    let mut db = relay.db.lock().expect("lock");
    let tx = db.transaction()?;
    let mb = mailbox(&tx, label)?;
    member(&tx, label, poster)?.ok_or(RelayError::Forbidden)?;
    if !mb.sealed {
        return Err(RelayError::Forbidden);
    }
    member(&tx, label, &h.author)?.ok_or(RelayError::Forbidden)?;
    if mb.vault_id.is_some_and(|v| v != h.mailbox) {
        return Err(RelayError::OtherVault);
    }
    relay.touch(&tx, label, &mb, poster)?;

    let len = log_len(&tx, label)? as i64;
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
        tx.commit()?;
        return ok(&AppendResult::Conflict { len: len as u64 });
    }
    if len == 0 && (h.author != mb.creator || *poster != mb.creator) {
        return Err(RelayError::Forbidden);
    }
    relay.check_room_for_write(&tx, &mb)?;
    if let Some(cap) = relay.quotas.log_bytes {
        let (used, mine): (i64, i64) = tx.query_row(
            "SELECT (SELECT log_bytes FROM mailboxes WHERE id = ?1),
                    (SELECT COALESCE(SUM(bytes), 0) FROM log_counts
                     WHERE mailbox = ?1 AND author = ?2)",
            params![&label[..], &h.author[..]],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let adding = bytes.len() as u64;
        if (used.max(0) as u64).saturating_add(adding) > cap
            || (mine.max(0) as u64).saturating_add(adding) > quota::author_log_cap(cap)
        {
            return Err(RelayError::QuotaExceeded(Quota::Log));
        }
    }
    tx.execute(
        "INSERT INTO log_entries (mailbox, idx, entry, hash) VALUES (?1, ?2, ?3, ?4)",
        params![&label[..], len, bytes, &hash[..]],
    )?;
    count_log(&tx, &label[..], &h.author[..], bytes.len())?;
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
    let others: Vec<[u8; 32]> = members_of(&tx, label)?
        .iter()
        .map(|m| m.sig_pk)
        .filter(|pk| *pk != h.author)
        .collect();
    let pushes = push_tokens(&tx, label, others.iter())?;
    tx.commit()?;
    drop(db);
    relay.waiters.signal(label);
    relay.push(pushes);
    ok(&AppendResult::Appended { index: len as u64 })
}

async fn info(State(relay): State<Relay>) -> Response {
    axum::Json(relay.info()).into_response()
}

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
    if member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none()
        && pending_member(&db, &req.payload.mailbox, &req.signer.sig_pk)?.is_none()
    {
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
    let (mut entries, mut bytes) = (Vec::new(), 0usize);
    for row in rows {
        let entry = row?;
        if !zafe_proto::relay::page_has_room(entries.len(), bytes, entry.len()) {
            break;
        }
        bytes += entry.len();
        entries.push(entry);
    }
    ok(&LogResponse { entries })
}

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

async fn wait(State(relay): State<Relay>, body: Bytes) -> RelayResult {
    let req = verified::<WaitRequest>(&relay, &body)?;
    relay.check_fresh(req.payload.timestamp)?;
    let p = &req.payload;
    let who = req.signer.sig_pk;
    let deadline = tokio::time::Instant::now()
        + std::time::Duration::from_secs(u64::from(p.max_wait_secs.min(MAX_WAIT_SECS)));
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
        now = activity(&relay, &p.mailbox, &who)?;
        if !woken || now.is_news(p.log_len, p.inbox_after) {
            return ok(&now);
        }
    }
}
