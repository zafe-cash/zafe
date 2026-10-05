//! Optional Tor routing for every connection Zafe makes (the relay and lightwalletd).
//!
//! The route is a process-wide policy. Turning Tor on flips it **before** Tor has
//! bootstrapped, so from that moment every relay request and every lightwalletd
//! connection either goes through Tor or fails with [`Blocked`]; nothing ever falls back
//! to a direct connection. Direct connections opened before the switch are cut: each one
//! holds a [`DirectLease`] whose token the switch cancels, and its I/O then fails.
//!
//! Tor is embedded (arti, through `zcash_client_backend`'s `tor` feature); there is no
//! separate tor process. The relay and lightwalletd use separate circuits (isolated
//! clients), so an exit relay never sees both from one circuit.
//!
//! Bootstrapping is bounded ([`BOOTSTRAP_TIMEOUT`], or the caller's budget): arti itself
//! retries forever on a network that blocks Tor. A failed or timed-out bootstrap leaves
//! the route on Tor with status [`TorStatus::Failed`], so requests keep failing until Tor
//! connects or the user turns it off.

use std::{
    future::Future,
    io,
    path::Path,
    pin::Pin,
    sync::{LazyLock, Mutex, MutexGuard},
    task::{Context, Poll},
    time::Duration,
};

use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_util::sync::{CancellationToken, WaitForCancellationFutureOwned};
use zcash_client_backend::tor::{Client, DormantMode, Timeouts};

/// Longest a foreground bootstrap may take. A first bootstrap on a slow link takes tens
/// of seconds (it downloads the consensus); later ones reuse the cached directory.
pub const BOOTSTRAP_TIMEOUT: Duration = Duration::from_secs(3 * 60);

/// How long a request waits for a bootstrap in progress before failing with
/// [`Blocked::Connecting`]. Short: the app polls again, and a sync waiting here holds the
/// wallet lock.
pub const ROUTE_WAIT: Duration = Duration::from_secs(20);

/// How often a waiting bootstrap checks whether Tor was turned off meanwhile.
const ABANDON_POLL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TorStatus {
    /// Tor is off: connections are direct.
    Off,
    /// Tor is on and bootstrapping; connections wait or fail.
    Connecting,
    /// Tor is on and connected; connections go through it.
    Ready,
    /// Tor is on but could not connect; connections fail until it does.
    Failed,
}

/// Why a connection was refused: Tor is on but has no circuit to offer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Blocked {
    #[error("Tor is still connecting")]
    Connecting,
    #[error("Tor could not connect")]
    Failed,
}

#[derive(Debug, thiserror::Error)]
pub enum TorError {
    /// Bootstrapping (or waiting for another bootstrap) took longer than allowed.
    #[error("Tor did not connect in time")]
    Timeout,
    /// Tor was turned off while it was bootstrapping.
    #[error("Tor was turned off while it was connecting")]
    Abandoned,
    #[error("Tor data directory: {0}")]
    Directory(io::Error),
    #[error("Tor: {0}")]
    Bootstrap(String),
}

/// What one kind of traffic uses. Each gets its own isolated circuits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Purpose {
    Relay,
    Lightwalletd,
    /// The ZEC/USD price (`crate::price`).
    Price,
}

#[derive(Clone)]
struct Clients {
    base: Client,
    relay: Client,
    lightwalletd: Client,
    price: Client,
}

impl Clients {
    fn new(base: Client) -> Self {
        Self {
            relay: base.isolated_client(),
            lightwalletd: base.isolated_client(),
            price: base.isolated_client(),
            base,
        }
    }

    fn for_purpose(&self, purpose: Purpose) -> Client {
        match purpose {
            Purpose::Relay => self.relay.clone(),
            Purpose::Lightwalletd => self.lightwalletd.clone(),
            Purpose::Price => self.price.clone(),
        }
    }
}

struct Policy {
    /// The route the user chose: Tor or direct. Set before bootstrapping.
    tor: bool,
    status: TorStatus,
    clients: Option<Clients>,
    /// Handed to every direct connection; cancelled (and replaced) when Tor is turned on.
    direct: CancellationToken,
    /// The app is in the background: keep Tor dormant (less padding traffic, battery).
    dormant: bool,
}

static POLICY: LazyLock<Mutex<Policy>> = LazyLock::new(|| {
    Mutex::new(Policy {
        tor: false,
        status: TorStatus::Off,
        clients: None,
        direct: CancellationToken::new(),
        dormant: false,
    })
});
/// Signalled after every status change, so waiting requests re-check at once.
static CHANGED: LazyLock<tokio::sync::Notify> = LazyLock::new(tokio::sync::Notify::new);
/// One bootstrap at a time (a foreground switch and a background check can race).
static BOOTSTRAP: LazyLock<tokio::sync::Mutex<()>> = LazyLock::new(|| tokio::sync::Mutex::new(()));

fn policy() -> MutexGuard<'static, Policy> {
    POLICY.lock().unwrap_or_else(|p| p.into_inner())
}

fn changed() {
    CHANGED.notify_waiters();
}

pub fn status() -> TorStatus {
    policy().status
}

/// Whether the route is Tor (whatever the bootstrap's state).
pub fn is_on() -> bool {
    policy().tor
}

/// Switches the route to Tor at once, before any bootstrap, and cuts every direct
/// connection. Call it before the first request when the user has Tor on. Does nothing
/// when Tor is already on (a connected client is kept).
pub fn request() {
    {
        let mut p = policy();
        if p.tor {
            return;
        }
        p.tor = true;
        p.status = TorStatus::Connecting;
        p.clients = None;
        p.direct.cancel();
        p.direct = CancellationToken::new();
    }
    changed();
    tracing::info!("tor: requested; direct connections closed");
}

/// Turns Tor on and bootstraps it, within `budget` (waiting for another bootstrap in
/// progress counts against it). Returns at once when Tor is already connected. On failure
/// the route stays on Tor with status [`TorStatus::Failed`]: requests keep failing.
/// Must run inside the tokio runtime that will carry Tor's traffic (arti spawns its
/// background tasks there).
pub async fn enable(dir: &Path, budget: Duration) -> Result<TorStatus, TorError> {
    request();
    let started = tokio::time::Instant::now();
    let Ok(_guard) = tokio::time::timeout(budget, BOOTSTRAP.lock()).await else {
        // Another bootstrap holds the lock and owns the status.
        return Err(TorError::Timeout);
    };
    {
        let mut p = policy();
        if !p.tor {
            return Ok(TorStatus::Off);
        }
        if p.clients.is_some() {
            return Ok(TorStatus::Ready);
        }
        p.status = TorStatus::Connecting;
    }
    changed();
    if let Err(e) = tokio::fs::create_dir_all(dir).await {
        set_failed();
        return Err(TorError::Directory(e));
    }
    let remaining = budget.saturating_sub(started.elapsed());
    let bootstrap = Client::create_with_timeouts(
        dir,
        |permissions| {
            // arti refuses a data directory other users could reach. On Android and iOS
            // the app sandbox is the boundary and the container's parents are group
            // accessible, which it would reject.
            #[cfg(any(target_os = "android", target_os = "ios"))]
            permissions.dangerously_trust_everyone();
            #[cfg(not(any(target_os = "android", target_os = "ios")))]
            let _ = permissions;
        },
        Timeouts::default(),
    );
    match bounded(remaining, bootstrap).await {
        Ok(client) => Ok(install(client)),
        Err(e) => {
            set_failed();
            tracing::warn!("tor: {e}");
            Err(e)
        }
    }
}

/// Waits for a bootstrap for at most `budget`, and gives up as soon as Tor is turned off
/// (dropping the future stops arti's work).
async fn bounded<T, E: std::fmt::Display>(
    budget: Duration,
    bootstrap: impl Future<Output = Result<T, E>>,
) -> Result<T, TorError> {
    tokio::pin!(bootstrap);
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        tokio::select! {
            r = &mut bootstrap => return r.map_err(|e| TorError::Bootstrap(e.to_string())),
            _ = tokio::time::sleep_until(deadline) => return Err(TorError::Timeout),
            _ = tokio::time::sleep(ABANDON_POLL) => {
                if !is_on() {
                    return Err(TorError::Abandoned);
                }
            }
        }
    }
}

/// Publishes a bootstrapped client, unless Tor was turned off meanwhile.
fn install(client: Client) -> TorStatus {
    let status = {
        let mut p = policy();
        if !p.tor {
            return TorStatus::Off;
        }
        client.set_dormant(dormant_mode(p.dormant));
        p.clients = Some(Clients::new(client));
        p.status = TorStatus::Ready;
        p.status
    };
    changed();
    tracing::info!("tor: ready");
    status
}

fn set_failed() {
    {
        let mut p = policy();
        p.clients = None;
        // A route that went direct meanwhile keeps its status.
        if p.tor {
            p.status = TorStatus::Failed;
        }
    }
    changed();
}

/// Switches back to direct connections and drops the Tor client.
pub fn disable() {
    {
        let mut p = policy();
        p.tor = false;
        p.status = TorStatus::Off;
        p.clients = None;
    }
    changed();
    tracing::info!("tor: off; connections are direct");
}

fn dormant_mode(dormant: bool) -> DormantMode {
    if dormant {
        DormantMode::Soft
    } else {
        DormantMode::Normal
    }
}

/// Puts Tor to sleep while the app is in the background (arti otherwise keeps its guard
/// connection busy), or wakes it. Remembered for a client that is still bootstrapping.
/// A request made while dormant wakes it by itself.
pub fn set_dormant(dormant: bool) {
    let mut p = policy();
    p.dormant = dormant;
    if let Some(c) = &p.clients {
        c.base.set_dormant(dormant_mode(dormant));
    }
}

/// How one connection goes out.
pub(crate) enum Route {
    Direct(DirectLease),
    Tor(Box<Client>),
}

/// The route for the policy as it stands. Pure, so the fail-closed rules are unit-tested.
fn decide(
    tor: bool,
    status: TorStatus,
    client: Option<Client>,
    direct: &CancellationToken,
) -> Result<Route, Blocked> {
    if !tor {
        return Ok(Route::Direct(DirectLease(direct.clone())));
    }
    match (client, status) {
        (Some(client), TorStatus::Ready) => Ok(Route::Tor(Box::new(client))),
        (_, TorStatus::Failed) => Err(Blocked::Failed),
        _ => Err(Blocked::Connecting),
    }
}

fn current(purpose: Purpose) -> Result<Route, Blocked> {
    let p = policy();
    let client = p.clients.as_ref().map(|c| c.for_purpose(purpose));
    decide(p.tor, p.status, client, &p.direct)
}

/// The route for one connection. While Tor is bootstrapping this waits up to `wait`, then
/// fails with [`Blocked::Connecting`]; a failed Tor fails at once. Never direct while
/// Tor is on.
pub(crate) async fn route(purpose: Purpose, wait: Duration) -> Result<Route, Blocked> {
    let deadline = tokio::time::Instant::now() + wait;
    loop {
        let notified = CHANGED.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        let decided = current(purpose);
        match decided {
            Err(Blocked::Connecting) => {}
            other => return other,
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(Blocked::Connecting);
        }
        tokio::select! {
            _ = notified => {}
            _ = tokio::time::sleep_until(deadline) => {}
        }
    }
}

/// Permission for one direct connection, revoked when Tor is turned on.
#[derive(Clone)]
pub(crate) struct DirectLease(CancellationToken);

impl DirectLease {
    pub(crate) fn is_revoked(&self) -> bool {
        self.0.is_cancelled()
    }

    /// Runs `fut`, abandoning it (and its connection) if Tor is turned on meanwhile.
    pub(crate) async fn guard<T, E>(
        &self,
        fut: impl Future<Output = Result<T, E>>,
        revoked: impl FnOnce() -> E,
    ) -> Result<T, E> {
        tokio::select! {
            biased;
            _ = self.0.cancelled() => Err(revoked()),
            r = fut => r,
        }
    }

    /// Wraps a direct connection so its reads and writes fail once Tor is turned on.
    pub(crate) fn wrap<T>(&self, inner: T) -> DirectIo<T> {
        DirectIo {
            inner,
            revoked: Box::pin(self.0.clone().cancelled_owned()),
        }
    }
}

pub(crate) fn revoked_error() -> io::Error {
    io::Error::new(
        io::ErrorKind::ConnectionAborted,
        "direct connection closed: Tor was turned on",
    )
}

/// A direct connection that stops working the moment Tor is turned on.
pub(crate) struct DirectIo<T> {
    inner: T,
    revoked: Pin<Box<WaitForCancellationFutureOwned>>,
}

impl<T> DirectIo<T> {
    /// Also registers the task to be woken when the lease is revoked.
    fn check(&mut self, cx: &mut Context<'_>) -> io::Result<()> {
        match self.revoked.as_mut().poll(cx) {
            Poll::Ready(()) => Err(revoked_error()),
            Poll::Pending => Ok(()),
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for DirectIo<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        self.check(cx)?;
        Pin::new(&mut self.inner).poll_read(cx, buf)
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for DirectIo<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.check(cx)?;
        Pin::new(&mut self.inner).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.check(cx)?;
        Pin::new(&mut self.inner).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // Closing is always allowed.
        Pin::new(&mut self.inner).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.check(cx)?;
        Pin::new(&mut self.inner).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }
}

/// tonic connector for direct lightwalletd connections: plain TCP (tonic adds TLS on
/// top), wrapped so the connection dies when Tor is turned on, and refusing to connect
/// (also on tonic's reconnects) once it was.
#[derive(Clone)]
pub(crate) struct DirectConnector(pub(crate) DirectLease);

impl tower::Service<tonic::transport::Uri> for DirectConnector {
    type Response = hyper_util::rt::TokioIo<DirectIo<tokio::net::TcpStream>>;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = io::Result<Self::Response>> + Send>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, uri: tonic::transport::Uri) -> Self::Future {
        let lease = self.0.clone();
        Box::pin(async move {
            if lease.is_revoked() {
                return Err(revoked_error());
            }
            let (host, port) = host_port(&uri)?;
            let stream = lease
                .guard(
                    tokio::net::TcpStream::connect((host.as_str(), port)),
                    revoked_error,
                )
                .await?;
            stream.set_nodelay(true)?;
            Ok(hyper_util::rt::TokioIo::new(lease.wrap(stream)))
        })
    }
}

fn host_port(uri: &tonic::transport::Uri) -> io::Result<(String, u16)> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "no host in the URL");
    let host = uri.host().ok_or_else(invalid)?;
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let port = uri.port_u16().unwrap_or(match uri.scheme_str() {
        Some("https") => 443,
        _ => 80,
    });
    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn is_direct(r: &Result<Route, Blocked>) -> bool {
        matches!(r, Ok(Route::Direct(_)))
    }

    #[test]
    fn off_is_direct() {
        let token = CancellationToken::new();
        assert!(is_direct(&decide(false, TorStatus::Off, None, &token)));
    }

    #[test]
    fn on_without_a_client_never_goes_direct() {
        let token = CancellationToken::new();
        for status in [TorStatus::Off, TorStatus::Connecting, TorStatus::Ready] {
            assert!(matches!(
                decide(true, status, None, &token),
                Err(Blocked::Connecting)
            ));
        }
        assert!(matches!(
            decide(true, TorStatus::Failed, None, &token),
            Err(Blocked::Failed)
        ));
    }

    #[test]
    fn urls_give_host_and_port() {
        let uri = |s: &str| s.parse::<tonic::transport::Uri>().unwrap();
        assert_eq!(
            host_port(&uri("https://zec.rocks")).unwrap(),
            ("zec.rocks".into(), 443)
        );
        assert_eq!(
            host_port(&uri("http://127.0.0.1:9067")).unwrap(),
            ("127.0.0.1".into(), 9067)
        );
        assert_eq!(
            host_port(&uri("http://[::1]:80")).unwrap(),
            ("::1".into(), 80)
        );
    }

    #[tokio::test]
    async fn a_revoked_lease_cuts_its_connection() {
        let (a, mut b) = tokio::io::duplex(64);
        let token = CancellationToken::new();
        let lease = DirectLease(token.clone());
        let mut io = lease.wrap(a);
        b.write_all(b"hi").await.unwrap();
        let mut buf = [0u8; 2];
        io.read_exact(&mut buf).await.unwrap();

        // A read waiting for data fails as soon as the lease is revoked.
        let reading = tokio::spawn(async move {
            let mut buf = [0u8; 1];
            io.read(&mut buf).await
        });
        tokio::time::sleep(Duration::from_millis(50)).await;
        token.cancel();
        let error = tokio::time::timeout(Duration::from_secs(2), reading)
            .await
            .expect("the read was not woken")
            .unwrap()
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::ConnectionAborted);
        assert!(lease.is_revoked());
    }

    #[tokio::test]
    async fn a_guarded_request_is_abandoned_when_revoked() {
        let token = CancellationToken::new();
        let lease = DirectLease(token.clone());
        let pending = lease.guard(std::future::pending::<Result<(), &str>>(), || "revoked");
        token.cancel();
        assert_eq!(pending.await, Err("revoked"));
    }

    #[tokio::test]
    async fn a_stalled_bootstrap_times_out() {
        let r = bounded(
            Duration::from_millis(10),
            std::future::pending::<Result<(), String>>(),
        )
        .await;
        assert!(matches!(r, Err(TorError::Timeout)));
    }
}
