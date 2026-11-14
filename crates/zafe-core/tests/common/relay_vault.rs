//! A vault made by real members over an in-process relay, for tests of what a member's
//! phone does (approve, respond, reject, send) through `zafe_core::node`.

use std::time::Duration;

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    node::{create_vault, join_vault, run_keygen, seal, top_up_pool, KeygenOptions, VaultMaterial},
    relay_client::RelayClient,
    session::MemoryPoolStore,
};
use zafe_proto::Identity;

use super::nu6_3_network;

pub async fn start_relay() -> RelayClient {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, zafe_relay::Relay::new().router())
            .await
            .unwrap();
    });
    RelayClient::new(format!("http://{addr}"))
}

/// A `t`-of-`n` vault made over the relay. With `pools`, every member publishes a full
/// one-tap pool (proposals then get signer groups); without, proposals are interactive.
pub async fn vault(
    t: u16,
    n: u16,
    pools: bool,
) -> (
    RelayClient,
    Vec<Identity>,
    Vec<VaultMaterial>,
    Vec<MemoryPoolStore>,
) {
    let relay = start_relay().await;
    let mut rng = StdRng::seed_from_u64(70 + u64::from(n));
    let ids: Vec<Identity> = (0..n).map(|_| Identity::generate(&mut rng)).collect();
    let invite = create_vault(&relay, &ids[0], "Retry", t, n, &mut rng)
        .await
        .unwrap();
    for id in &ids[1..] {
        join_vault(&relay, id, &invite).await.unwrap();
    }
    seal(&relay, &ids[0], &invite).await.unwrap();
    let number = futures::future::join_all(ids.iter().map(|id| safety_words(&relay, id, &invite)))
        .await
        .into_iter()
        .map(Result::unwrap)
        .reduce(|a, b| {
            assert_eq!(a, b, "every member sees the same safety words");
            a
        })
        .unwrap();
    let net = nu6_3_network();
    let runs = ids.iter().enumerate().map(|(i, id)| {
        let (relay, invite, number, net) = (&relay, &invite, &number, &net);
        async move {
            let mut rng = StdRng::seed_from_u64(100 + i as u64);
            let birthday = (i == 0).then_some(2);
            run_keygen(
                relay,
                id,
                invite,
                number,
                net,
                "regtest",
                KeygenOptions {
                    creator_birthday_height: birthday,
                    creator_expiry_blocks: None,
                    max_birthday: None,
                },
                member_dir(relay, id),
                setup_key(id),
                &mut rng,
                Duration::from_secs(60),
            )
            .await
            .unwrap()
        }
    });
    let materials = futures::future::join_all(runs).await;
    let mut stores = Vec::new();
    for (i, (id, m)) in ids.iter().zip(&materials).enumerate() {
        let mut pool = MemoryPoolStore::default();
        if pools {
            let mut rng = StdRng::seed_from_u64(200 + i as u64);
            assert!(
                top_up_pool(&relay, id, m, &mut pool, false, None, &mut rng)
                    .await
                    .unwrap()
                    > 0
            );
        }
        stores.push(pool);
    }
    (relay, ids, materials, stores)
}

/// One member's setup state dir in this test's relay: the safety check and key generation
/// share it (the safety value lives there).
pub fn member_dir(relay: &RelayClient, id: &Identity) -> std::path::PathBuf {
    let port = relay
        .base_url()
        .rsplit(':')
        .next()
        .unwrap_or_default()
        .to_owned();
    std::env::temp_dir().join(format!(
        "zafe-setup-{}-{port}-{}",
        std::process::id(),
        hex::encode(&id.public().sig_pk[..8])
    ))
}

/// The key that seals one member's setup state (any fixed key per member will do).
pub fn setup_key(id: &Identity) -> zafe_core::nonce_store::NonceKey {
    zafe_core::nonce_store::NonceKey::derive(&id.public().sig_pk, b"setup")
}

/// One member's safety words for `invite`, polled from its setup state dir.
pub async fn safety_words(
    relay: &RelayClient,
    id: &Identity,
    invite: &zafe_core::node::Invite,
) -> Result<String, zafe_core::node::NodeError> {
    zafe_core::node::wait_for_safety_words(
        relay,
        id,
        invite,
        &member_dir(relay, id),
        &setup_key(id),
        Duration::from_secs(30),
    )
    .await
}
