//! Client for the Zafe relay (spec §6). Signs every request with the member identity.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{de::DeserializeOwned, Serialize};
use zafe_proto::{
    relay::{
        decode_body, join_token_hash, AppendResult, CreateMailbox, InboxAck, InboxAckResponse,
        InboxRead, InboxResponse, Join, LogRead, LogResponse, MailboxInfo, MailboxesRead,
        MailboxesResponse, MembersRead, MembersResponse, PushPlatform, RegisterPush, Remove,
        ReplaceApproval, ReplaceMember, Reseed, ReseedResponse, Seal, SetThreshold, Signed,
        WaitRequest, WaitResponse, MAX_ACK_CURSORS, MAX_WAIT_SECS, QUOTA_CAPACITY, QUOTA_HEADER,
        UNSUPPORTED_VERSION_HEADER,
    },
    version::{Format, UnsupportedVersion},
    Envelope, Identity, LogEntry, MailboxId, ProtoError,
};

#[derive(Debug, thiserror::Error)]
pub enum RelayClientError {
    #[error("relay returned {status}: {body}")]
    Status { status: u16, body: String },
    /// The relay could not be reached (or the connection failed part way).
    #[error("transport: {message}")]
    Transport {
        failure: crate::net::NetFailure,
        message: String,
    },
    /// The relay is limiting this key or address (HTTP 429).
    #[error("the relay is busy; try again in {retry_after_secs} s")]
    RateLimited { retry_after_secs: u64 },
    /// The relay refused to store more for this vault (HTTP 507): a member's inbox, the
    /// vault's undelivered messages or its log is over the relay's storage quota.
    /// `detail` is the relay's explanation.
    #[error("the relay's storage for this vault is full: {detail}")]
    StorageFull { detail: String },
    /// The relay takes no more vaults for now (a capped beta, HTTP 507 with
    /// `zafe-quota: capacity`). Existing vaults are not affected.
    #[error("the relay is full: it takes no new vaults for now")]
    AtCapacity,
    #[error("encoding")]
    Encoding,
    /// The relay refused this client's version of `format` (HTTP 426). `ours` newer than
    /// `relay_supports` means the relay needs updating; older means the app does.
    #[error("the relay speaks {format} version {relay_supports}, this app {ours}")]
    VersionRejected {
        format: Format,
        ours: u16,
        relay_supports: u16,
    },
    /// The relay sent data in a version this build can't read.
    #[error(transparent)]
    UnsupportedVersion(#[from] UnsupportedVersion),
}

impl RelayClientError {
    /// This app is older than what it talked to: updating the app fixes it.
    pub fn app_outdated(&self) -> bool {
        match self {
            RelayClientError::VersionRejected {
                ours,
                relay_supports,
                ..
            } => ours < relay_supports,
            RelayClientError::UnsupportedVersion(v) => v.is_newer(),
            _ => false,
        }
    }
}

fn decoding(e: ProtoError) -> RelayClientError {
    match e {
        ProtoError::UnsupportedVersion(v) => RelayClientError::UnsupportedVersion(v),
        _ => RelayClientError::Encoding,
    }
}

/// The format a request body to `path` is tagged with.
fn body_format(path: &str) -> Format {
    match path {
        "/v1/envelope" => Format::Envelope,
        "/v1/log/append" => Format::LogEntry,
        _ => Format::RelayApi,
    }
}

#[derive(Clone)]
pub struct RelayClient {
    base: String,
    http: reqwest::Client,
    /// This device's copy of the vault logs (see [`crate::log_cache`]); `None` = the
    /// relay is trusted to remember the log (tests, tools).
    log_cache: Option<crate::log_cache::LogCache>,
}

/// A transport error with its causes, so "invalid peer certificate: UnknownIssuer" isn't
/// hidden behind reqwest's "error sending request".
fn transport(e: impl std::error::Error + 'static) -> RelayClientError {
    RelayClientError::Transport {
        failure: crate::net::NetFailure::of(&e),
        message: crate::net::error_chain(&e),
    }
}

/// Time to establish a connection to the relay.
pub const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Whole request, response included (relay answers are small; only `/v1/wait` is held
/// open, for at most [`MAX_WAIT_SECS`]).
pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// How much longer than the relay's wait a long poll may take before the client gives up
/// (network latency, a slow proxy).
pub const WAIT_GRACE: std::time::Duration = std::time::Duration::from_secs(15);

/// Extra time a request through Tor may take on top of the direct timeouts: every request
/// opens a new Tor stream and TLS session (several round trips through the circuit).
pub const TOR_EXTRA_TIME: std::time::Duration = std::time::Duration::from_secs(30);

/// What the client needs from a relay response, whichever way it travelled.
struct RawResponse {
    status: u16,
    retry_after: Option<String>,
    supported_version: Option<String>,
    quota: Option<String>,
    body: Vec<u8>,
}

/// One request through Tor (`zcash_client_backend`'s HTTP-over-Tor: a fresh Tor stream and
/// rustls session with the bundled roots per request, no retries). A private root added
/// with [`RelayClient::with_extra_root`] is not trusted on this path.
async fn over_tor(
    client: &zcash_client_backend::tor::Client,
    url: String,
    body: Option<Vec<u8>>,
) -> Result<RawResponse, RelayClientError> {
    use http_body_util::BodyExt;
    use zcash_client_backend::tor::{self, http::HttpError};

    let uri: tonic::transport::Uri = url.parse().map_err(transport)?;
    let read_body = |incoming: hyper::body::Incoming| async move {
        Ok::<_, tor::Error>(
            incoming
                .collect()
                .await
                .map_err(HttpError::from)?
                .to_bytes(),
        )
    };
    let response = match body {
        None => client.http_get(uri, |b| b, read_body, 0, |_| None).await,
        Some(body) => {
            client
                .http_post(
                    uri,
                    |b| b.header("content-type", "application/octet-stream"),
                    http_body_util::Full::new(bytes::Bytes::from(body)),
                    read_body,
                    0,
                    |_| None,
                )
                .await
        }
    }
    .map_err(transport)?;
    let header = |name: &str| {
        response
            .headers()
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned)
    };
    Ok(RawResponse {
        status: response.status().as_u16(),
        retry_after: header("retry-after"),
        supported_version: header(UNSUPPORTED_VERSION_HEADER),
        quota: header(QUOTA_HEADER),
        body: response.body().to_vec(),
    })
}

fn http_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl RelayClient {
    /// `base` is the relay URL, e.g. `http://127.0.0.1:8787` or `https://relay.example`.
    /// `https` uses rustls with the bundled Mozilla root store (webpki-roots), so it works
    /// the same on Android, iOS and desktop without the platform's certificate store.
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            // Only fails if the TLS backend can't initialise; fall back to the defaults.
            http: http_builder().build().unwrap_or_default(),
            log_cache: crate::log_cache::configured(),
        }
    }

    /// Replaces the log copy this client keeps and checks the relay against (the default
    /// is the one set with [`crate::log_cache::configure`]).
    pub fn with_log_cache(mut self, cache: Option<crate::log_cache::LogCache>) -> Self {
        self.log_cache = cache;
        self
    }

    /// The relay's base URL.
    pub fn base_url(&self) -> &str {
        &self.base
    }

    pub fn log_cache(&self) -> Option<&crate::log_cache::LogCache> {
        self.log_cache.as_ref()
    }

    /// Like [`RelayClient::new`], but also trusts `root_der` (a DER CA certificate) on top
    /// of the bundled roots: for a self-hosted relay behind a private CA, and for tests.
    /// Certificate and hostname verification stay fully on.
    pub fn with_extra_root(
        base: impl Into<String>,
        root_der: &[u8],
    ) -> Result<Self, RelayClientError> {
        let root = reqwest::Certificate::from_der(root_der).map_err(transport)?;
        let http = http_builder()
            .add_root_certificate(root)
            .build()
            .map_err(transport)?;
        Ok(Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http,
            log_cache: crate::log_cache::configured(),
        })
    }

    /// `GET /health`: whether a Zafe relay answers at this URL (200 `ok`).
    pub async fn health(&self) -> Result<(), RelayClientError> {
        let response = self.exchange("/health", None, None).await?;
        if (200..300).contains(&response.status) && response.body == b"ok" {
            Ok(())
        } else {
            Err(RelayClientError::Status {
                status: response.status,
                body: String::from_utf8_lossy(&response.body)
                    .chars()
                    .take(200)
                    .collect(),
            })
        }
    }

    /// One request, through Tor or directly as the route policy says ([`crate::tor`]):
    /// `body` makes it a POST. While Tor is on but unusable this fails with
    /// `NetFailure::TorConnecting`/`TorFailed` and sends nothing.
    async fn exchange(
        &self,
        path: &str,
        body: Option<Vec<u8>>,
        timeout: Option<std::time::Duration>,
    ) -> Result<RawResponse, RelayClientError> {
        use crate::tor::{self, Purpose, Route};
        let url = format!("{}{path}", self.base);
        match tor::route(Purpose::Relay, tor::ROUTE_WAIT)
            .await
            .map_err(|b| RelayClientError::Transport {
                failure: b.into(),
                message: b.to_string(),
            })? {
            Route::Direct(lease) => {
                lease
                    .guard(self.direct(url, body, timeout), || {
                        RelayClientError::Transport {
                            failure: crate::net::NetFailure::TorConnecting,
                            message: tor::revoked_error().to_string(),
                        }
                    })
                    .await
            }
            Route::Tor(client) => {
                let limit = timeout.unwrap_or(REQUEST_TIMEOUT) + TOR_EXTRA_TIME;
                tokio::time::timeout(limit, over_tor(&client, url, body))
                    .await
                    .map_err(|_| RelayClientError::Transport {
                        failure: crate::net::NetFailure::Timeout,
                        message: "the relay didn't answer in time (through Tor)".into(),
                    })?
            }
        }
    }

    async fn direct(
        &self,
        url: String,
        body: Option<Vec<u8>>,
        timeout: Option<std::time::Duration>,
    ) -> Result<RawResponse, RelayClientError> {
        let mut request = match body {
            Some(body) => self
                .http
                .post(url)
                .header("content-type", "application/octet-stream")
                .body(body),
            None => self.http.get(url),
        };
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().await.map_err(transport)?;
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let (retry_after, supported_version, quota) = (
            header("retry-after"),
            header(UNSUPPORTED_VERSION_HEADER),
            header(QUOTA_HEADER),
        );
        let body = response.bytes().await.map_err(transport)?.to_vec();
        Ok(RawResponse {
            status,
            retry_after,
            supported_version,
            quota,
            body,
        })
    }

    async fn post(&self, path: &str, body: Vec<u8>) -> Result<Vec<u8>, RelayClientError> {
        self.post_with_timeout(path, body, None).await
    }

    async fn post_with_timeout(
        &self,
        path: &str,
        body: Vec<u8>,
        timeout: Option<std::time::Duration>,
    ) -> Result<Vec<u8>, RelayClientError> {
        let response = self.exchange(path, Some(body), timeout).await?;
        let status = response.status;
        if status == 426 {
            let format = body_format(path);
            let relay_supports = response
                .supported_version
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            return Err(RelayClientError::VersionRejected {
                format,
                ours: format.current(),
                relay_supports,
            });
        }
        if status == 429 {
            let retry_after_secs = response
                .retry_after
                .and_then(|v| v.parse().ok())
                .unwrap_or(60);
            return Err(RelayClientError::RateLimited { retry_after_secs });
        }
        let bytes = response.body;
        if status == 507 && response.quota.as_deref() == Some(QUOTA_CAPACITY) {
            return Err(RelayClientError::AtCapacity);
        }
        if status == 507 {
            return Err(RelayClientError::StorageFull {
                detail: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        if !(200..300).contains(&status) {
            return Err(RelayClientError::Status {
                status,
                body: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        Ok(bytes)
    }

    async fn signed<T, R>(
        &self,
        path: &str,
        who: &Identity,
        payload: T,
    ) -> Result<R, RelayClientError>
    where
        T: Serialize + DeserializeOwned,
        R: DeserializeOwned,
    {
        let body = Signed::new(who, payload)
            .and_then(|s| s.to_bytes())
            .map_err(|_| RelayClientError::Encoding)?;
        decode_body(&self.post(path, body).await?).map_err(decoding)
    }

    pub async fn create_mailbox(
        &self,
        creator: &Identity,
        mailbox: MailboxId,
        join_token: &[u8; 32],
        max_members: u16,
    ) -> Result<(), RelayClientError> {
        let payload = CreateMailbox {
            max_members,
            mailbox,
            join_token_hash: join_token_hash(join_token),
        };
        self.signed("/v1/mailbox/create", creator, payload).await
    }

    pub async fn join(
        &self,
        member: &Identity,
        mailbox: MailboxId,
        join_token: [u8; 32],
    ) -> Result<(), RelayClientError> {
        self.signed(
            "/v1/mailbox/join",
            member,
            Join {
                mailbox,
                join_token,
            },
        )
        .await
    }

    pub async fn seal(
        &self,
        creator: &Identity,
        mailbox: MailboxId,
        members: Vec<[u8; 32]>,
    ) -> Result<(), RelayClientError> {
        self.signed("/v1/mailbox/seal", creator, Seal { mailbox, members })
            .await
    }

    /// Creator only, before sealing: records the vault's threshold, which moving a lost
    /// member's seat later needs. `Ok(false)` from a relay without the route (HTTP 404):
    /// such a vault can't move seats.
    pub async fn set_threshold(
        &self,
        creator: &Identity,
        mailbox: MailboxId,
        threshold: u16,
    ) -> Result<bool, RelayClientError> {
        match self
            .signed::<_, ()>(
                "/v1/mailbox/threshold",
                creator,
                SetThreshold { mailbox, threshold },
            )
            .await
        {
            Ok(()) => Ok(true),
            Err(RelayClientError::Status { status: 404, .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Moves a lost member's seat to a new key on the relay, with the approvals of at least
    /// the vault's threshold of other members. Succeeds if it already moved.
    pub async fn replace_member(
        &self,
        who: &Identity,
        approval: ReplaceApproval,
        approvals: Vec<([u8; 32], Vec<u8>)>,
    ) -> Result<(), RelayClientError> {
        self.signed(
            "/v1/mailbox/replace",
            who,
            ReplaceMember {
                approval,
                approvals,
            },
        )
        .await
    }

    /// The mailboxes `who` is a member of. Empty from a relay without the route (404).
    pub async fn mailboxes(&self, who: &Identity) -> Result<Vec<MailboxId>, RelayClientError> {
        match self
            .signed::<_, MailboxesResponse>(
                "/v1/mailboxes",
                who,
                MailboxesRead { timestamp: now() },
            )
            .await
        {
            Ok(r) => Ok(r.mailboxes),
            Err(RelayClientError::Status { status: 404, .. }) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    /// Creator only, before sealing: removes a joined member.
    pub async fn remove(
        &self,
        creator: &Identity,
        mailbox: MailboxId,
        member: [u8; 32],
    ) -> Result<(), RelayClientError> {
        self.signed("/v1/mailbox/remove", creator, Remove { mailbox, member })
            .await
    }

    /// Registers this member's push token for the mailbox (content-free notifications).
    pub async fn register_push(
        &self,
        member: &Identity,
        mailbox: MailboxId,
        platform: PushPlatform,
        token: String,
    ) -> Result<(), RelayClientError> {
        self.signed(
            "/v1/push/register",
            member,
            RegisterPush {
                mailbox,
                platform,
                token,
            },
        )
        .await
    }

    pub async fn members(
        &self,
        who: &Identity,
        mailbox: MailboxId,
    ) -> Result<MembersResponse, RelayClientError> {
        self.signed(
            "/v1/mailbox/members",
            who,
            MembersRead {
                mailbox,
                timestamp: now(),
            },
        )
        .await
    }

    /// Members, sealed flag and approval threshold the relay holds. A relay older than
    /// the route answers 404 (`Status { status: 404 }`).
    pub async fn mailbox_info(
        &self,
        who: &Identity,
        mailbox: MailboxId,
    ) -> Result<MailboxInfo, RelayClientError> {
        self.signed(
            "/v1/mailbox/info",
            who,
            MembersRead {
                mailbox,
                timestamp: now(),
            },
        )
        .await
    }

    pub async fn send(&self, envelope: &Envelope) -> Result<(), RelayClientError> {
        let body = envelope
            .to_bytes()
            .map_err(|_| RelayClientError::Encoding)?;
        decode_body(&self.post("/v1/envelope", body).await?).map_err(decoding)
    }

    /// Envelopes delivered to `who` after cursor `after`, oldest first. Envelopes in a
    /// version this build can't read are dropped.
    pub async fn inbox(
        &self,
        who: &Identity,
        mailbox: MailboxId,
        after: u64,
    ) -> Result<Vec<(u64, Envelope)>, RelayClientError> {
        let response: InboxResponse = self
            .signed(
                "/v1/inbox",
                who,
                InboxRead {
                    mailbox,
                    after,
                    timestamp: now(),
                },
            )
            .await?;
        Ok(response.decoded())
    }

    /// Deletes envelopes `who` has handled, by delivery cursor. Returns how many the relay
    /// deleted; a relay without the endpoint (404) deletes none (they expire there).
    pub async fn ack_inbox(
        &self,
        who: &Identity,
        mailbox: MailboxId,
        cursors: &[u64],
    ) -> Result<u64, RelayClientError> {
        let mut deleted = 0;
        for chunk in cursors.chunks(MAX_ACK_CURSORS) {
            let body = Signed::new(
                who,
                InboxAck {
                    mailbox,
                    cursors: chunk.to_vec(),
                    timestamp: now(),
                },
            )
            .and_then(|s| s.to_bytes())
            .map_err(|_| RelayClientError::Encoding)?;
            match self.post("/v1/inbox/ack", body).await {
                Ok(bytes) => {
                    deleted += decode_body::<InboxAckResponse>(&bytes)
                        .map_err(decoding)?
                        .deleted;
                }
                Err(RelayClientError::Status { status: 404, .. }) => return Ok(deleted),
                Err(e) => return Err(e),
            }
        }
        Ok(deleted)
    }

    pub async fn append_log(&self, entry: &LogEntry) -> Result<AppendResult, RelayClientError> {
        let body = entry.to_bytes().map_err(|_| RelayClientError::Encoding)?;
        decode_body(&self.post("/v1/log/append", body).await?).map_err(decoding)
    }

    /// Restores a vault on this relay from a saved copy of its log (see [`Reseed`]).
    pub async fn reseed(
        &self,
        who: &Identity,
        mut request: Reseed,
    ) -> Result<ReseedResponse, RelayClientError> {
        request.timestamp = now();
        self.signed("/v1/mailbox/reseed", who, request).await
    }

    pub async fn read_log(
        &self,
        who: &Identity,
        mailbox: MailboxId,
        from: u64,
    ) -> Result<Vec<LogEntry>, RelayClientError> {
        let response: LogResponse = self
            .signed(
                "/v1/log/read",
                who,
                LogRead {
                    mailbox,
                    from,
                    timestamp: now(),
                },
            )
            .await?;
        response.decoded().map_err(decoding)
    }

    /// Long poll: returns as soon as the vault log has more than `log_len` entries or an
    /// envelope for `who` is delivered past cursor `inbox_after`, or after `max_wait`
    /// (the relay caps it at [`MAX_WAIT_SECS`]) with where things are. `Ok(None)`: this
    /// relay has no long polls (older than the endpoint, HTTP 404); poll instead.
    pub async fn wait_for_activity(
        &self,
        who: &Identity,
        mailbox: MailboxId,
        log_len: u64,
        inbox_after: u64,
        max_wait: std::time::Duration,
    ) -> Result<Option<WaitResponse>, RelayClientError> {
        let max_wait_secs = u32::try_from(max_wait.as_secs())
            .unwrap_or(u32::MAX)
            .min(MAX_WAIT_SECS);
        let payload = WaitRequest {
            mailbox,
            log_len,
            inbox_after,
            max_wait_secs,
            timestamp: now(),
        };
        let body = Signed::new(who, payload)
            .and_then(|s| s.to_bytes())
            .map_err(|_| RelayClientError::Encoding)?;
        let timeout = std::time::Duration::from_secs(max_wait_secs.into()) + WAIT_GRACE;
        match self
            .post_with_timeout("/v1/wait", body, Some(timeout))
            .await
        {
            Ok(bytes) => decode_body(&bytes).map(Some).map_err(decoding),
            // The relay answers 403 (never 404) for an unknown mailbox here, so a 404 is a
            // relay without the route.
            Err(RelayClientError::Status { status: 404, .. }) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
