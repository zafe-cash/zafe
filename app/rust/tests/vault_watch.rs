//! The bridge's vault watch (`watch_vault_with`) against a real HTTP relay: events for
//! other members' activity, stopping, and the fallback on a relay without long polls.
//! One test, because the watch is process-wide.

use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::Duration,
};

use rand::{rngs::StdRng, SeedableRng};
use rust_lib_zafe::api::watch::{
    stop_vault_watch, watch_vault_with, VaultActivity, VaultActivityKind,
};
use zafe_core::{
    node::{create_vault, join_vault, membership, run_keygen, seal, top_up_pool, VaultMaterial},
    relay_client::RelayClient,
    session::MemoryPoolStore,
    wallet::regtest_network,
};
use zafe_proto::{Envelope, Identity, Kind};

fn serve(rt: &tokio::runtime::Runtime, router: axum::Router) -> String {
    let listener = rt
        .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
        .unwrap();
    let addr = listener.local_addr().unwrap();
    rt.spawn(async move { axum::serve(listener, router).await.unwrap() });
    format!("http://{addr}")
}

/// A 2-of-2 vault made over the relay.
async fn two_member_vault(relay: &RelayClient) -> (Vec<Identity>, Vec<VaultMaterial>) {
    let mut rng = StdRng::seed_from_u64(31);
    let ids: Vec<Identity> = (0..2).map(|_| Identity::generate(&mut rng)).collect();
    let invite = create_vault(relay, &ids[0], "Watch", 2, 2, &mut rng)
        .await
        .unwrap();
    join_vault(relay, &ids[1], &invite).await.unwrap();
    seal(relay, &ids[0], &invite).await.unwrap();
    let (_, _, number) = membership(relay, &ids[0], &invite).await.unwrap();
    let net = regtest_network();
    let timeout = Duration::from_secs(60);
    let (mut r0, mut r1) = (StdRng::seed_from_u64(1), StdRng::seed_from_u64(2));
    let (a, b) = tokio::join!(
        run_keygen(
            relay,
            &ids[0],
            &invite,
            &number,
            &net,
            "regtest",
            Some(2),
            None,
            &mut r0,
            timeout
        ),
        run_keygen(relay, &ids[1], &invite, &number, &net, "regtest", None, None, &mut r1, timeout),
    );
    (ids, vec![a.unwrap(), b.unwrap()])
}

fn start(
    relay_url: &str,
    id: &Identity,
    material: &VaultMaterial,
    watch_id: i64,
) -> Receiver<VaultActivity> {
    let (tx, rx) = mpsc::channel();
    watch_vault_with(
        relay_url.to_owned(),
        id.seeds().to_bytes(),
        material.to_bytes().unwrap(),
        watch_id,
        move |event| tx.send(event).is_ok(),
    )
    .unwrap();
    rx
}

fn next(rx: &Receiver<VaultActivity>) -> VaultActivityKind {
    rx.recv_timeout(Duration::from_secs(10))
        .expect("an event in time")
        .kind
}

fn assert_ended(rx: &Receiver<VaultActivity>) {
    loop {
        match rx.recv_timeout(Duration::from_secs(5)) {
            Err(RecvTimeoutError::Disconnected) => return,
            Err(RecvTimeoutError::Timeout) => panic!("the watch is still running"),
            Ok(event) => assert_ne!(event.kind, VaultActivityKind::Activity, "{event:?}"),
        }
    }
}

#[test]
fn vault_watch_reports_activity_and_stops() {
    let log_dir = std::env::temp_dir().join(format!("zafe-vault-watch-{}", std::process::id()));
    rust_lib_zafe::api::app::init_log_cache(log_dir.to_string_lossy().into_owned());
    let rt = tokio::runtime::Runtime::new().unwrap();
    let url = serve(
        &rt,
        zafe_relay::Relay::new()
            .with_limits(zafe_relay::limits::Limits::hosted())
            .router(),
    );
    let relay = RelayClient::new(url.clone());
    let (ids, materials) = rt.block_on(two_member_vault(&relay));

    let events = start(&url, &ids[0], &materials[0], 1);
    assert_eq!(next(&events), VaultActivityKind::Connected);

    // The other member publishes commitments (a log append): news.
    let mut rng = StdRng::seed_from_u64(7);
    let mut pool = MemoryPoolStore::default();
    rt.block_on(top_up_pool(
        &relay,
        &ids[1],
        &materials[1],
        &mut pool,
        &mut rng,
    ))
    .unwrap();
    assert_eq!(next(&events), VaultActivityKind::Activity);

    // A message for this member (e.g. a signing request): news.
    let vault = materials[0].descriptor.vault_id;
    let envelope = Envelope::sealed(
        &ids[1],
        ids[0].public(),
        vault,
        i64::MAX as u64 - 2,
        Kind::SigningRequest,
        b"sign",
        &mut rng,
    )
    .unwrap();
    rt.block_on(relay.send(&envelope)).unwrap();
    assert_eq!(next(&events), VaultActivityKind::Activity);

    // Stopping ends the loop (its callback is dropped) and reports nothing more.
    stop_vault_watch(1);
    assert_ended(&events);

    // A newer watch replaces the running one.
    let first = start(&url, &ids[0], &materials[0], 2);
    assert_eq!(next(&first), VaultActivityKind::Connected);
    let second = start(&url, &ids[0], &materials[0], 3);
    assert_eq!(next(&second), VaultActivityKind::Connected);
    assert_ended(&first);
    // Stopping an older id leaves the newer watch running.
    stop_vault_watch(2);
    let envelope = Envelope::sealed(
        &ids[1],
        ids[0].public(),
        vault,
        i64::MAX as u64 - 1,
        Kind::SigningRequest,
        b"sign",
        &mut rng,
    )
    .unwrap();
    rt.block_on(relay.send(&envelope)).unwrap();
    assert_eq!(next(&second), VaultActivityKind::Activity);
    stop_vault_watch(3);
    assert_ended(&second);

    // A stop that overtakes its start still wins.
    stop_vault_watch(4);
    let overtaken = start(&url, &ids[0], &materials[0], 4);
    assert_ended(&overtaken);

    // A relay without long polls: "unsupported", and the watch ends.
    let old = serve(
        &rt,
        axum::Router::new().route("/health", axum::routing::get(|| async { "ok" })),
    );
    let events = start(&old, &ids[0], &materials[0], 5);
    assert_eq!(next(&events), VaultActivityKind::Unsupported);
    assert_ended(&events);
}
