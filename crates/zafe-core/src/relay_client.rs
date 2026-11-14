//! Client for the Zafe relay (spec §6). Signs every request with the member identity.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{de::DeserializeOwned, Serialize};
use zafe_proto::{
    relay::{
        decode_body, encode_body, join_token_hash, AppendResult, CancelSetup, CreateMailbox,
        ForgetPush, InboxAck, InboxAckResponse, InboxRead, InboxResponse, Join, LabelledAppend,
        LabelledReplace, LogRead, LogResponse, MailboxInfo, MailboxesRead, MailboxesResponse,
        MembersRead, MembersResponse, PostEnvelope, PushPlatform, RecoveryPut, RecoveryRead,
        RecoveryValueKind, RecoveryValues, RegisterPush, RelayInfo, Remove, RenewInvite, Reseed,
        ReseedResponse, Seal, SeatCancel, SeatCancelPost, SeatFinish, SeatFinishPost,
        SeatMoveApproval, SetThreshold, Signed, WaitRequest, WaitResponse, CREATE_TOKEN_HEADER,
        MAX_ACK_CURSORS, MAX_WAIT_SECS, UNKNOWN_MAILBOX, UNSUPPORTED_VERSION_HEADER,
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
    /// The response was over [`MAX_RESPONSE_BYTES`] (a misbehaving relay; audit L6).
    #[error("the relay's response is over {MAX_RESPONSE_BYTES} bytes")]
    ResponseTooLarge,
    /// The relay doesn't have `route` (HTTP 404 that isn't "unknown mailbox"): it is older
    /// than this app's feature and needs updating.
    #[error("the relay is too old for {route}")]
    RelayTooOld { route: &'static str },
    /// The relay is private: creating or restoring a mailbox there needs its create token,
    /// and this client sent none or the wrong one (HTTP 401 `www-authenticate:
    /// zafe-create-token`; see [`RelayClient::with_create_token`]).
    #[error("this relay needs a create token")]
    CreateTokenRefused,
    /// The relay already holds its cap of pending moves for this seat (HTTP 409).
    #[error("the server holds too many open moves for this seat")]
    TooManySeatMoves,
}

/// Development only: connect to `to` whenever a relay move names `from`. One relay can have
/// different addresses for different members (the emulator reaches the host's
/// `127.0.0.1:18900` as `10.0.2.2:18900`), while every member signs the same URL; the
/// CLI's `--relay-alias` sets this. The app never does.
pub fn set_relay_alias(from: &str, to: &str) {
    let mut map = ALIASES.lock().expect("lock");
    map.insert(
        from.trim().trim_end_matches('/').to_ascii_lowercase(),
        to.trim().to_owned(),
    );
}

static ALIASES: std::sync::Mutex<std::collections::BTreeMap<String, String>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

fn relay_alias(url: &str) -> String {
    let key = url.trim().trim_end_matches('/').to_ascii_lowercase();
    ALIASES
        .lock()
        .expect("lock")
        .get(&key)
        .cloned()
        .unwrap_or_else(|| url.to_owned())
}

/// Largest response body read from a relay. A page is at most 500 envelopes or log
/// entries, almost all a few KB, so this is far above anything real while bounding the
/// memory a hostile relay can make the app allocate.
pub const MAX_RESPONSE_BYTES: usize = 64 * 1024 * 1024;

impl RelayClientError {
    /// The relay couldn't be reached or didn't answer itself: no connection, a timeout, Tor
    /// not connected, or a gateway error (502-504) from a proxy in front of it. Anything
    /// else came from the relay.
    pub fn is_unreachable(&self) -> bool {
        match self {
            RelayClientError::Transport { .. } => true,
            RelayClientError::Status { status, .. } => (502..=504).contains(status),
            _ => false,
        }
    }

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

/// A 404 from a relay without `route` (its own 404s for a missing mailbox say so) becomes
/// [`RelayClientError::RelayTooOld`].
fn route_missing(route: &'static str) -> impl FnOnce(RelayClientError) -> RelayClientError {
    move |e| match e {
        RelayClientError::Status { status: 404, body } if body != UNKNOWN_MAILBOX => {
            RelayClientError::RelayTooOld { route }
        }
        e => e,
    }
}

fn decoding(e: ProtoError) -> RelayClientError {
    match e {
        ProtoError::UnsupportedVersion(v) => RelayClientError::UnsupportedVersion(v),
        _ => RelayClientError::Encoding,
    }
}

/// The format a request body to `path` is tagged with (every route since relay API 2).
fn body_format(_path: &str) -> Format {
    Format::RelayApi
}

#[derive(Clone)]
pub struct RelayClient {
    base: String,
    http: reqwest::Client,
    /// This device's copy of the vault logs (see [`crate::log_cache`]); `None` = the
    /// relay is trusted to remember the log (tests, tools).
    log_cache: Option<crate::log_cache::LogCache>,
    /// A private relay's create token, sent with mailbox creations and restores only.
    create_token: Option<String>,
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

/// Marks an oversized body inside the Tor client's error type.
const TOO_LARGE: &str = "relay response too large";

/// What the client needs from a relay response, whichever way it travelled.
struct RawResponse {
    status: u16,
    retry_after: Option<String>,
    supported_version: Option<String>,
    www_authenticate: Option<String>,
    body: Vec<u8>,
}

/// One extra request header (name, value).
type ExtraHeader = Option<(&'static str, String)>;

/// One request through Tor (`zcash_client_backend`'s HTTP-over-Tor: a fresh Tor stream and
/// rustls session with the bundled roots per request, no retries). A private root added
/// with [`RelayClient::with_extra_root`] is not trusted on this path.
async fn over_tor(
    client: &zcash_client_backend::tor::Client,
    url: String,
    body: Option<Vec<u8>>,
    header: ExtraHeader,
) -> Result<RawResponse, RelayClientError> {
    use http_body_util::BodyExt;
    use zcash_client_backend::tor::{self, http::HttpError};

    let uri: tonic::transport::Uri = url.parse().map_err(transport)?;
    // Read frame by frame so an oversized body is cut off, not collected.
    let read_body = |mut incoming: hyper::body::Incoming| async move {
        let mut out = Vec::new();
        while let Some(frame) = incoming.frame().await {
            if let Ok(data) = frame.map_err(HttpError::from)?.into_data() {
                if out.len() + data.len() > MAX_RESPONSE_BYTES {
                    return Err(tor::Error::Io(std::io::Error::other(TOO_LARGE)));
                }
                out.extend_from_slice(&data);
            }
        }
        Ok::<_, tor::Error>(bytes::Bytes::from(out))
    };
    let response = match body {
        None => client.http_get(uri, |b| b, read_body, 0, |_| None).await,
        Some(body) => {
            client
                .http_post(
                    uri,
                    |b| {
                        let b = b.header("content-type", "application/octet-stream");
                        match &header {
                            Some((name, value)) => b.header(*name, value.as_str()),
                            None => b,
                        }
                    },
                    http_body_util::Full::new(bytes::Bytes::from(body)),
                    read_body,
                    0,
                    |_| None,
                )
                .await
        }
    }
    .map_err(|e| match &e {
        tor::Error::Io(io) if io.to_string() == TOO_LARGE => RelayClientError::ResponseTooLarge,
        _ => transport(e),
    })?;
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
        www_authenticate: header("www-authenticate"),
        body: response.body().to_vec(),
    })
}

/// Every relay client (pooled or not) comes from here. Redirects are never followed:
/// reqwest's default follows up to 10 (`reqwest-0.12.28/src/redirect.rs:161-163`), resends
/// the body on 307/308 (`tower-http-0.6.11/src/follow_redirect/mod.rs:291`) and strips only
/// the standard credential headers across hosts (`redirect.rs:239-250`), so a relay could
/// send a phone's signed requests and the create token anywhere. A redirect answers as its
/// status. The Tor path follows none either (`zcash_client_backend` `tor::http`).
fn http_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
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
            create_token: None,
        }
    }

    /// Like [`RelayClient::new`], but every client made this way shares one connection
    /// pool, so a call reuses the TLS connection the previous one opened instead of a new
    /// handshake (~0.7 s to the hosted relay from a phone: most of an approval's time).
    /// Only for a process whose relay calls all run on **one long-lived runtime** (the
    /// app's bridge): pooled connections belong to the runtime that opened them, so tests
    /// and tools that start a runtime per call use [`RelayClient::new`]. Tor is unaffected:
    /// with Tor on no request takes the direct path, so pooled connections sit unused.
    pub fn pooled(base: impl Into<String>) -> Self {
        static HTTP: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            http: HTTP
                .get_or_init(|| http_builder().build().unwrap_or_default())
                .clone(),
            log_cache: crate::log_cache::configured(),
            create_token: None,
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

    /// A client for another relay (`url`) like this one: same connection pool, log copy
    /// and network path; no create token. A relay move talks to the new relay this way.
    /// The URL goes through [`set_relay_alias`] (development: one relay, two addresses).
    pub fn at(&self, url: &str) -> Self {
        Self {
            base: relay_alias(url).trim_end_matches('/').to_owned(),
            http: self.http.clone(),
            log_cache: self.log_cache.clone(),
            create_token: None,
        }
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
            create_token: None,
        })
    }

    /// Sends `token` (a private relay's `ZAFE_RELAY_CREATE_TOKEN`) with mailbox creations
    /// and restores (`/v1/mailbox/create`, `/v1/mailbox/reseed`), never with anything else.
    /// `None` or blank: no token.
    pub fn with_create_token(mut self, token: Option<&str>) -> Self {
        self.create_token = token
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(str::to_owned);
        self
    }

    /// The create token this client sends with creations and restores, if any.
    pub fn create_token(&self) -> Option<&str> {
        self.create_token.as_deref()
    }

    /// `GET /health`: whether a Zafe relay answers at this URL (200 `ok`).
    pub async fn health(&self) -> Result<(), RelayClientError> {
        let response = self.exchange("/health", None, None, None).await?;
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
        header: ExtraHeader,
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
                    .guard(self.direct(url, body, timeout, header), || {
                        RelayClientError::Transport {
                            failure: crate::net::NetFailure::TorConnecting,
                            message: tor::revoked_error().to_string(),
                        }
                    })
                    .await
            }
            Route::Tor(client) => {
                let limit = timeout.unwrap_or(REQUEST_TIMEOUT) + TOR_EXTRA_TIME;
                tokio::time::timeout(limit, over_tor(&client, url, body, header))
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
        header: ExtraHeader,
    ) -> Result<RawResponse, RelayClientError> {
        let mut request = match body {
            Some(body) => self
                .http
                .post(url)
                .header("content-type", "application/octet-stream")
                .body(body),
            None => self.http.get(url),
        };
        if let Some((name, value)) = header {
            request = request.header(name, value);
        }
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let mut response = request.send().await.map_err(transport)?;
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned)
        };
        let (retry_after, supported_version, www_authenticate) = (
            header("retry-after"),
            header(UNSUPPORTED_VERSION_HEADER),
            header("www-authenticate"),
        );
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
        {
            return Err(RelayClientError::ResponseTooLarge);
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport)? {
            if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
                return Err(RelayClientError::ResponseTooLarge);
            }
            body.extend_from_slice(&chunk);
        }
        Ok(RawResponse {
            status,
            retry_after,
            supported_version,
            www_authenticate,
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
        // The create token goes only where the relay checks it.
        let header = self
            .create_token
            .clone()
            .filter(|_| matches!(path, "/v1/mailbox/create" | "/v1/mailbox/reseed"))
            .map(|t| (CREATE_TOKEN_HEADER, t));
        let response = self.exchange(path, Some(body), timeout, header).await?;
        let status = response.status;
        if status == 401 && response.www_authenticate.as_deref() == Some(CREATE_TOKEN_HEADER) {
            return Err(RelayClientError::CreateTokenRefused);
        }
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
        if status == 507 {
            return Err(RelayClientError::StorageFull {
                detail: String::from_utf8_lossy(&bytes).into_owned(),
            });
        }
        if status == 409 && bytes == zafe_proto::relay::TOO_MANY_SEAT_MOVES.as_bytes() {
            return Err(RelayClientError::TooManySeatMoves);
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

    /// Creator only, before sealing: deletes the mailbox, so the invite stops working and
    /// members waiting in it learn the setup is gone. `Ok(false)` when there was nothing to
    /// delete there: a relay without the route, or a mailbox that is already gone (both
    /// answer HTTP 404).
    pub async fn cancel_setup(
        &self,
        creator: &Identity,
        mailbox: MailboxId,
    ) -> Result<bool, RelayClientError> {
        match self
            .signed::<_, ()>("/v1/mailbox/cancel", creator, CancelSetup { mailbox })
            .await
        {
            Ok(()) => Ok(true),
            Err(RelayClientError::Status { status: 404, .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Restarts the invite's clock (creator only, before the setup locks). `Ok(false)`:
    /// a relay without the route (its invites never expire).
    pub async fn renew_invite(
        &self,
        creator: &Identity,
        mailbox: MailboxId,
    ) -> Result<bool, RelayClientError> {
        match self
            .signed::<_, ()>("/v1/mailbox/renew", creator, RenewInvite { mailbox })
            .await
        {
            Ok(()) => Ok(true),
            Err(RelayClientError::Status { status: 404, .. }) => Ok(false),
            Err(e) => Err(e),
        }
    }

    /// Approves a seat move on the relay that files the vault under `mailbox` (its label
    /// there), with the approvals of at least the vault's threshold of other members: the
    /// new key becomes a pending member (see [`LabelledReplace`]). Succeeds if the relay
    /// already holds the move.
    pub async fn replace_member_at(
        &self,
        who: &Identity,
        mailbox: MailboxId,
        approval: SeatMoveApproval,
        approvals: Vec<([u8; 32], Vec<u8>)>,
    ) -> Result<(), RelayClientError> {
        const ROUTE: &str = "/v1/mailbox/replace-labelled";
        self.signed(
            ROUTE,
            who,
            LabelledReplace {
                mailbox,
                approval,
                approvals,
            },
        )
        .await
        .map_err(route_missing(ROUTE))
    }

    /// Posts `who`'s finish of a pending seat move (see [`SeatFinish`]).
    pub async fn finish_seat_move(
        &self,
        who: &Identity,
        finish: SeatFinish,
    ) -> Result<(), RelayClientError> {
        const ROUTE: &str = "/v1/seat/finish";
        let post = SeatFinishPost {
            finish,
            by: *who.public(),
            signature: finish.sign(who).map_err(|_| RelayClientError::Encoding)?,
        };
        let body = encode_body(&post).map_err(|_| RelayClientError::Encoding)?;
        decode_body(&self.post(ROUTE, body).await.map_err(route_missing(ROUTE))?).map_err(decoding)
    }

    /// Posts the old key's signed cancel of a pending first-round move of its seat (see
    /// [`SeatCancel`]; it names the mailbox). Anyone may post it.
    pub async fn cancel_seat_move(
        &self,
        cancel: SeatCancel,
        signature: Vec<u8>,
    ) -> Result<(), RelayClientError> {
        const ROUTE: &str = "/v1/seat/cancel";
        let post = SeatCancelPost { cancel, signature };
        let body = encode_body(&post).map_err(|_| RelayClientError::Encoding)?;
        decode_body(&self.post(ROUTE, body).await.map_err(route_missing(ROUTE))?).map_err(decoding)
    }

    /// Leaves one value of the safety check between `subject`'s new phone and the
    /// co-signer `cosigner` (see [`RecoveryPut`]). A relay without the route answers 404.
    pub async fn recovery_put(
        &self,
        who: &Identity,
        subject: [u8; 32],
        cosigner: [u8; 32],
        kind: RecoveryValueKind,
        value: [u8; 32],
    ) -> Result<(), RelayClientError> {
        let payload = RecoveryPut {
            subject,
            cosigner,
            kind,
            value,
            timestamp: now(),
        };
        self.signed::<_, ()>("/v1/seat-check/put", who, payload)
            .await
    }

    /// The values left for `subject`'s recovery whose signature verifies, that are about
    /// `subject` and signed by the side whose value they are (the relay is not trusted
    /// with them).
    pub async fn recovery_read(
        &self,
        who: &Identity,
        subject: [u8; 32],
    ) -> Result<Vec<Signed<RecoveryPut>>, RelayClientError> {
        let read = RecoveryRead {
            subject,
            timestamp: now(),
        };
        let response: RecoveryValues = self.signed("/v1/seat-check/read", who, read).await?;
        Ok(response
            .values
            .iter()
            .filter_map(|b| Signed::<RecoveryPut>::from_bytes(b).ok())
            .filter(|s| {
                s.verify().is_ok_and(|p| {
                    let by = match p.kind {
                        RecoveryValueKind::Announce | RecoveryValueKind::Challenge => p.cosigner,
                        RecoveryValueKind::Commit | RecoveryValueKind::Reveal => p.subject,
                    };
                    p.subject == subject && s.signer.sig_pk == by
                })
            })
            .collect())
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

    /// Delivers `envelope`, whose sender is `who` (the request is signed by it).
    pub async fn send(&self, who: &Identity, envelope: &Envelope) -> Result<(), RelayClientError> {
        let envelope = envelope
            .to_bytes()
            .map_err(|_| RelayClientError::Encoding)?;
        self.signed("/v1/envelope/send", who, PostEnvelope { envelope })
            .await
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

    /// Appends `entry` to the mailbox this relay files the vault under (`mailbox`, its label
    /// there, which may differ from the vault id the entry carries). `who` is a member of
    /// that mailbox, not necessarily the entry's author.
    pub async fn append_log_to(
        &self,
        who: &Identity,
        mailbox: MailboxId,
        entry: &LogEntry,
    ) -> Result<AppendResult, RelayClientError> {
        const ROUTE: &str = "/v1/log/append-labelled";
        let request =
            LabelledAppend::new(mailbox, entry, now()).map_err(|_| RelayClientError::Encoding)?;
        self.signed(ROUTE, who, request)
            .await
            .map_err(route_missing(ROUTE))
    }

    /// Deletes `who`'s push token for `mailbox` on this relay (its vault left it). A relay
    /// older than the route fails with [`RelayClientError::RelayTooOld`].
    pub async fn forget_push(
        &self,
        who: &Identity,
        mailbox: MailboxId,
    ) -> Result<(), RelayClientError> {
        const ROUTE: &str = "/v1/push/forget";
        let request = ForgetPush {
            mailbox,
            timestamp: now(),
        };
        self.signed(ROUTE, who, request)
            .await
            .map_err(route_missing(ROUTE))
    }

    /// `GET /v1/info`: the relay's versions, limits, whether it sends push notifications
    /// and needs a create token. Unauthenticated. A relay older than the route fails with
    /// [`RelayClientError::RelayTooOld`].
    pub async fn info(&self) -> Result<RelayInfo, RelayClientError> {
        const ROUTE: &str = "/v1/info";
        let response = self.exchange(ROUTE, None, None, None).await?;
        match response.status {
            200..=299 => {
                serde_json::from_slice(&response.body).map_err(|_| RelayClientError::Encoding)
            }
            429 => Err(RelayClientError::RateLimited {
                retry_after_secs: response
                    .retry_after
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(60),
            }),
            404 => Err(RelayClientError::RelayTooOld { route: ROUTE }),
            status => Err(RelayClientError::Status {
                status,
                body: String::from_utf8_lossy(&response.body)
                    .chars()
                    .take(200)
                    .collect(),
            }),
        }
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
        Ok(self.read_log_page(who, mailbox, from).await?.0)
    }

    /// [`RelayClient::read_log`], with the page's size as the relay measures it
    /// ([`zafe_proto::relay::page_bytes`] of the stored entries), for
    /// [`zafe_proto::relay::page_may_continue`].
    pub async fn read_log_page(
        &self,
        who: &Identity,
        mailbox: MailboxId,
        from: u64,
    ) -> Result<(Vec<LogEntry>, usize), RelayClientError> {
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
        let bytes = zafe_proto::relay::page_bytes(response.entries.iter().map(Vec::as_slice));
        Ok((response.decoded().map_err(decoding)?, bytes))
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
