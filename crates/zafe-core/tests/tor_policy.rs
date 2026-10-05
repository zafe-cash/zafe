//! The Tor route policy is fail-closed: once Tor is on, the relay client and lightwalletd
//! connections go through Tor or fail; they never connect directly, and direct
//! connections already open are cut. The policy is process-wide, so these tests run in
//! their own process (this file) and one at a time.
//!
//! The live test bootstraps real Tor and needs the internet:
//! `cargo test -p zafe-core --test tor_policy -- --ignored --nocapture`.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use rand::{rngs::StdRng, RngCore, SeedableRng};
use zafe_core::{
    net::NetFailure,
    relay_client::{RelayClient, RelayClientError},
    tor::{self, TorStatus},
    wallet::{self, WalletError},
};
use zafe_proto::Identity;

/// The policy is global: one test at a time, each leaving Tor off.
static SERIAL: Mutex<()> = Mutex::new(());

struct Serial(#[allow(dead_code)] std::sync::MutexGuard<'static, ()>);

impl Drop for Serial {
    fn drop(&mut self) {
        tor::disable();
    }
}

fn serial() -> Serial {
    let guard = SERIAL.lock().unwrap_or_else(|p| p.into_inner());
    tor::disable();
    Serial(guard)
}

async fn serve_relay() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let router = zafe_relay::Relay::new().router();
    tokio::spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// A server that only counts the connections it gets.
async fn counting_server() -> (String, Arc<AtomicUsize>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let count = Arc::new(AtomicUsize::new(0));
    let seen = count.clone();
    tokio::spawn(async move {
        let mut held = Vec::new();
        while let Ok((socket, _)) = listener.accept().await {
            seen.fetch_add(1, Ordering::SeqCst);
            held.push(socket);
        }
    });
    (format!("http://{addr}"), count)
}

/// A path Tor can't use as its data directory (a file), so a bootstrap fails at once
/// without touching the network.
fn unusable_dir() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("tor");
    std::fs::write(&path, b"").unwrap();
    (dir, path)
}

fn relay_failure(e: &RelayClientError) -> Option<NetFailure> {
    match e {
        RelayClientError::Transport { failure, .. } => Some(*failure),
        _ => None,
    }
}

fn wallet_failure(e: &WalletError) -> Option<NetFailure> {
    match e {
        WalletError::Remote { failure, .. } => Some(*failure),
        _ => None,
    }
}

#[tokio::test]
async fn tor_on_never_connects_directly() {
    let _serial = serial();
    let relay_url = serve_relay().await;
    RelayClient::new(relay_url.clone()).health().await.unwrap();

    let (url, connections) = counting_server().await;
    tor::request();
    assert_eq!(tor::status(), TorStatus::Connecting);

    // Requests made while Tor bootstraps wait; a failed bootstrap ends the wait at once.
    let waiting = tokio::spawn({
        let url = url.clone();
        async move { RelayClient::new(url).health().await }
    });
    let lightwalletd = tokio::spawn({
        let url = url.clone();
        async move { wallet::connect(&url).await.map(|_| ()) }
    });
    tokio::time::sleep(Duration::from_millis(200)).await;
    let (_dir, bad) = unusable_dir();
    let started = Instant::now();
    assert!(tor::enable(&bad, Duration::from_secs(5)).await.is_err());
    assert_eq!(tor::status(), TorStatus::Failed);
    let relay_err = waiting.await.unwrap().unwrap_err();
    assert_eq!(relay_failure(&relay_err), Some(NetFailure::TorFailed));
    let lwd_err = lightwalletd.await.unwrap().unwrap_err();
    assert_eq!(wallet_failure(&lwd_err), Some(NetFailure::TorFailed));
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "waiters weren't woken"
    );

    // Still failed: new requests fail at once, and nothing ever reached the server.
    let e = RelayClient::new(relay_url.clone())
        .health()
        .await
        .unwrap_err();
    assert_eq!(relay_failure(&e), Some(NetFailure::TorFailed));
    assert_eq!(connections.load(Ordering::SeqCst), 0);

    // Off again: direct connections work.
    tor::disable();
    assert_eq!(tor::status(), TorStatus::Off);
    RelayClient::new(relay_url).health().await.unwrap();
}

#[tokio::test]
async fn turning_tor_on_cuts_direct_connections() {
    let _serial = serial();
    let relay_url = serve_relay().await;
    let relay = RelayClient::new(relay_url);
    let mut rng = StdRng::seed_from_u64(7);
    let ids: Arc<Vec<Identity>> = Arc::new((0..2).map(|_| Identity::generate(&mut rng)).collect());
    let (mut mailbox, mut token) = ([0u8; 16], [0u8; 32]);
    rng.fill_bytes(&mut mailbox);
    rng.fill_bytes(&mut token);
    relay
        .create_mailbox(&ids[0], mailbox, &token, 2)
        .await
        .unwrap();
    relay.join(&ids[1], mailbox, token).await.unwrap();
    relay
        .seal(
            &ids[0],
            mailbox,
            ids.iter().map(|i| i.public().sig_pk).collect(),
        )
        .await
        .unwrap();

    // A long poll held open over a direct connection...
    let polling = tokio::spawn({
        let (relay, ids) = (relay.clone(), ids.clone());
        async move {
            relay
                .wait_for_activity(&ids[1], mailbox, 0, 0, Duration::from_secs(20))
                .await
        }
    });
    tokio::time::sleep(Duration::from_millis(300)).await;
    // ...ends the moment Tor is turned on, instead of finishing directly.
    let started = Instant::now();
    tor::request();
    let e = polling.await.unwrap().unwrap_err();
    assert_eq!(relay_failure(&e), Some(NetFailure::TorConnecting));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test]
async fn a_lightwalletd_stream_is_cut_when_tor_is_turned_on() {
    let _serial = serial();
    // A server that accepts and never answers: the call would wait for the request
    // timeout (30 s) if the connection weren't cut.
    let (url, connections) = counting_server().await;
    let call = tokio::spawn(async move {
        let mut client = wallet::connect(&url).await?;
        wallet::latest_height(&mut client).await
    });
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(connections.load(Ordering::SeqCst), 1);
    let started = Instant::now();
    tor::request();
    let result = tokio::time::timeout(Duration::from_secs(5), call)
        .await
        .expect("the direct call wasn't cut")
        .unwrap();
    assert!(result.is_err());
    assert!(started.elapsed() < Duration::from_secs(5));
    // tonic may try to reconnect: our connector refuses once Tor was requested.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn dormancy_can_be_set_before_tor_exists() {
    let _serial = serial();
    tor::set_dormant(true);
    tor::set_dormant(false);
    assert_eq!(tor::status(), TorStatus::Off);
}

/// Bootstraps real Tor into a temporary directory, then talks to the public testnet
/// lightwalletd, an HTTPS site and the ZEC price through it.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs the internet and the Tor network"]
async fn live_tor_reaches_lightwalletd_and_https() {
    let _serial = serial();
    let dir = tempfile::tempdir().unwrap();
    let started = Instant::now();
    let status = tor::enable(dir.path(), tor::BOOTSTRAP_TIMEOUT)
        .await
        .expect("bootstrap");
    assert_eq!(status, TorStatus::Ready);
    println!("bootstrapped in {:?}", started.elapsed());

    let started = Instant::now();
    let mut client = wallet::connect("https://testnet.zec.rocks:443")
        .await
        .expect("lightwalletd through Tor");
    let tip = wallet::latest_height(&mut client).await.expect("tip");
    println!("testnet tip {tip} through Tor in {:?}", started.elapsed());
    assert!(tip > 4_000_000);

    // Any HTTP answer proves the relay client's HTTPS-over-Tor path: this site isn't a
    // relay, so `health` reports its status.
    let started = Instant::now();
    match RelayClient::new("https://www.torproject.org")
        .health()
        .await
    {
        Err(RelayClientError::Status { status, .. }) => {
            println!("https over Tor: HTTP {status} in {:?}", started.elapsed())
        }
        other => panic!("expected an HTTP answer, got {other:?}"),
    }

    // The price through Tor: several exchanges on the price circuit.
    let started = Instant::now();
    let usd = zafe_core::price::zec_usd()
        .await
        .expect("price through Tor");
    println!("ZEC = ${usd} through Tor in {:?}", started.elapsed());
    assert!(usd > 1.0 && usd < 100_000.0);

    // Dormant and awake again: a request still works.
    tor::set_dormant(true);
    tor::set_dormant(false);
    // Enabling again is instant while connected.
    assert_eq!(
        tor::enable(dir.path(), Duration::from_secs(1))
            .await
            .unwrap(),
        TorStatus::Ready
    );
}
