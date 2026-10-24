//! Vault creation over a real HTTP relay with three concurrent members (spec §7.2).

use std::time::Duration;

use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    node::{
        create_vault, join_vault, load_state, membership, respond, run_keygen, seal, set_name,
        Invite, KeygenOptions, NodeError,
    },
    relay_client::RelayClient,
    session::MemoryNonceStore,
    wallet::regtest_network,
};
use zafe_proto::Identity;
use zafe_relay::{
    limits::{Limits, Rate},
    quota::Quotas,
};

/// A relay over HTTP with `limits` (the hosted defaults unless a test needs others, so
/// these flows prove they fit in them).
async fn start_relay(limits: Limits) -> String {
    start_relay_with(limits, Quotas::hosted()).await
}

async fn start_relay_with(limits: Limits, quotas: Quotas) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            zafe_relay::Relay::new()
                .with_limits(limits)
                .with_quotas(quotas)
                .router(),
        )
        .await
        .unwrap();
    });
    format!("http://{addr}")
}

async fn setup() -> (RelayClient, Vec<Identity>, Invite, String) {
    let relay = RelayClient::new(start_relay(Limits::hosted()).await);
    let mut rng = StdRng::seed_from_u64(60);
    let ids: Vec<Identity> = (0..3).map(|_| Identity::generate(&mut rng)).collect();
    let invite = create_vault(&relay, &ids[0], "Grants", 2, 3, &mut rng)
        .await
        .unwrap();

    // The invite travels out of band as a string.
    let invite = Invite::decode(&invite.encode()).unwrap();
    for id in &ids[1..] {
        join_vault(&relay, id, &invite).await.unwrap();
    }
    seal(&relay, &ids[0], &invite).await.unwrap();

    // Membership is locked; every member sees the same safety words once all values are in.
    let m = membership(&relay, &ids[0], &invite).await.unwrap();
    assert!(m.sealed);
    assert_eq!(m.members.len(), 3);
    let number = futures::future::join_all(ids.iter().map(|id| {
        zafe_core::node::wait_for_safety_words(&relay, id, &invite, Duration::from_secs(30))
    }))
    .await
    .into_iter()
    .map(Result::unwrap)
    .reduce(|a, b| {
        assert_eq!(a, b, "every member sees the same safety words");
        a
    })
    .unwrap();
    (relay, ids, invite, number)
}

#[tokio::test]
async fn three_members_create_a_vault_over_the_relay() {
    let (relay, ids, invite, number) = setup().await;
    let net = regtest_network();
    let timeout = Duration::from_secs(60);
    let (mut r0, mut r1, mut r2) = (
        StdRng::seed_from_u64(1),
        StdRng::seed_from_u64(2),
        StdRng::seed_from_u64(3),
    );

    let (a, b, c) = tokio::join!(
        run_keygen(
            &relay,
            &ids[0],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: Some(2),
                creator_expiry_blocks: Some(3 * 1152),
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r0,
            timeout
        ),
        run_keygen(
            &relay,
            &ids[1],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r1,
            timeout
        ),
        run_keygen(
            &relay,
            &ids[2],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r2,
            timeout
        ),
    );
    let (a, b, c) = (a.unwrap(), b.unwrap(), c.unwrap());

    assert_eq!(a.descriptor, b.descriptor);
    assert_eq!(a.descriptor, c.descriptor);
    assert_eq!(
        a.descriptor.birthday_height, 2,
        "birthday comes from the creator"
    );
    assert_eq!(
        a.descriptor.proposal_expiry_blocks,
        3 * 1152,
        "the creator's expiry window, in every member's signed descriptor"
    );
    assert!(a.descriptor.address.starts_with("uregtest1"));
    assert_eq!(a.vault_secret, b.vault_secret);
    assert_eq!(a.log_key, c.log_key);
    assert_ne!(
        a.key_package, b.key_package,
        "each member has its own share"
    );
    // Material round-trips into working keys.
    assert_eq!(
        a.vault_keys().unwrap().fvk().to_bytes(),
        c.vault_keys().unwrap().fvk().to_bytes()
    );
    assert_eq!(*a.key_package().unwrap().min_signers(), 2);

    // Once the vault exists, a member's regular `respond` deletes its keygen messages
    // from the relay; nothing is left to delete the next time.
    for (id, material) in ids.iter().zip([&a, &b, &c]) {
        let vault = material.label();
        assert!(!relay.inbox(id, vault, 0).await.unwrap().is_empty());
        let mut store = MemoryNonceStore::default();
        let report = respond(&relay, id, material, &net, 100, &mut store, &mut r0)
            .await
            .unwrap();
        assert!(report.acknowledged > 0, "keygen messages acknowledged");
        assert!(relay.inbox(id, vault, 0).await.unwrap().is_empty());
        let again = respond(&relay, id, material, &net, 100, &mut store, &mut r0)
            .await
            .unwrap();
        assert_eq!(again.acknowledged, 0);
    }

    // Members share their names through the log; the others see them on replay.
    set_name(&relay, &ids[0], &a, "  Alice ", &mut r0)
        .await
        .unwrap();
    set_name(&relay, &ids[1], &b, "Bob", &mut r1).await.unwrap();
    let (chain, state) = load_state(&relay, &ids[2], &c).await.unwrap();
    assert_eq!(state.names[&ids[0].public().sig_pk], "Alice", "trimmed");
    assert_eq!(state.names[&ids[1].public().sig_pk], "Bob");
    assert!(!state.names.contains_key(&ids[2].public().sig_pk));
    // Setting the same name again logs nothing; clearing it does.
    let entries = chain.len();
    set_name(&relay, &ids[1], &b, "Bob", &mut r1).await.unwrap();
    assert_eq!(
        load_state(&relay, &ids[2], &c).await.unwrap().0.len(),
        entries
    );
    set_name(&relay, &ids[1], &b, "", &mut r1).await.unwrap();
    let (_, state) = load_state(&relay, &ids[0], &a).await.unwrap();
    assert!(!state.names.contains_key(&ids[1].public().sig_pk));
    // Invalid names are refused before anything is logged.
    assert!(set_name(&relay, &ids[2], &c, &"x".repeat(33), &mut r2)
        .await
        .is_err());
}

#[tokio::test]
async fn a_joiner_refuses_a_birthday_ahead_of_the_chain() {
    let (relay, ids, invite, number) = setup().await;
    let net = regtest_network();
    let timeout = Duration::from_secs(10);
    let (mut r0, mut r1, mut r2) = (
        StdRng::seed_from_u64(1),
        StdRng::seed_from_u64(2),
        StdRng::seed_from_u64(3),
    );
    // The creator picks a birthday far past the joiners' tip (100 + leeway).
    let (_, b, _) = tokio::join!(
        run_keygen(
            &relay,
            &ids[0],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: Some(5_000_000),
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r0,
            timeout
        ),
        run_keygen(
            &relay,
            &ids[1],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: Some(110)
            },
            keygen_dir(),
            keygen_key(),
            &mut r1,
            timeout
        ),
        run_keygen(
            &relay,
            &ids[2],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r2,
            timeout
        ),
    );
    match b {
        Err(NodeError::Protocol(m)) => assert!(m.contains("ahead of the chain"), "{m}"),
        other => panic!("{other:?}"),
    }
}

/// A member who never takes part is named when key generation gives up, so the others
/// know whom to ask (or to abandon the setup).
#[tokio::test]
async fn keygen_names_the_member_who_never_took_part() {
    let (relay, ids, invite, number) = setup().await;
    let net = regtest_network();
    let timeout = Duration::from_secs(3);
    let (mut r0, mut r1) = (StdRng::seed_from_u64(1), StdRng::seed_from_u64(2));
    let (a, b) = tokio::join!(
        run_keygen(
            &relay,
            &ids[0],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: Some(2),
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r0,
            timeout
        ),
        run_keygen(
            &relay,
            &ids[1],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r1,
            timeout
        ),
    );
    for result in [a, b] {
        match result {
            Err(NodeError::KeygenStalled { missing, .. }) => {
                assert_eq!(missing, vec![ids[2].public().sig_pk]);
            }
            other => panic!("{other:?}"),
        }
    }
}

#[tokio::test]
async fn keygen_refuses_an_unconfirmed_safety_number() {
    let (relay, ids, invite, _number) = setup().await;
    let net = regtest_network();
    let mut rng = StdRng::seed_from_u64(4);
    let err = run_keygen(
        &relay,
        &ids[1],
        &invite,
        "0000 0000 0000",
        &net,
        "regtest",
        KeygenOptions {
            creator_birthday_height: None,
            creator_expiry_blocks: None,
            max_birthday: None,
        },
        keygen_dir(),
        keygen_key(),
        &mut rng,
        Duration::from_secs(5),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, NodeError::SafetyNumberMismatch { .. }),
        "{err:?}"
    );
}

#[tokio::test]
async fn a_rate_limited_client_gets_a_typed_error() {
    let relay = RelayClient::new(
        // Creating a vault takes two requests (the mailbox, then its threshold).
        start_relay(Limits {
            per_key: Some(Rate::new(1, 2)),
            ..Limits::none()
        })
        .await,
    );
    let mut rng = StdRng::seed_from_u64(61);
    let me = Identity::generate(&mut rng);
    create_vault(&relay, &me, "Grants", 2, 3, &mut rng)
        .await
        .unwrap();
    let again = create_vault(&relay, &me, "Grants", 2, 3, &mut rng).await;
    assert!(
        matches!(
            again,
            Err(NodeError::Relay(
                zafe_core::relay_client::RelayClientError::RateLimited {
                    retry_after_secs: 60
                }
            ))
        ),
        "{again:?}"
    );
}

#[tokio::test]
async fn a_full_relay_gets_a_typed_error() {
    use zafe_core::relay_client::RelayClientError;
    use zafe_proto::{Envelope, Kind};

    let relay = RelayClient::new(
        start_relay_with(
            Limits::hosted(),
            Quotas {
                envelopes_per_recipient: Some(1),
                ..Quotas::none()
            },
        )
        .await,
    );
    let mut rng = StdRng::seed_from_u64(62);
    let a = Identity::generate(&mut rng);
    let b = Identity::generate(&mut rng);
    let mailbox = [7u8; 16];
    relay
        .create_mailbox(&a, mailbox, &[1; 32], 2)
        .await
        .unwrap();
    relay.join(&b, mailbox, [1; 32]).await.unwrap();
    let envelope = |seq| {
        Envelope::sealed(
            &a,
            b.public(),
            mailbox,
            seq,
            Kind::Approval,
            b"hi",
            &mut StdRng::seed_from_u64(seq),
        )
        .unwrap()
    };
    relay.send(&envelope(1)).await.unwrap();
    let err = relay.send(&envelope(2)).await.unwrap_err();
    assert!(
        matches!(&err, RelayClientError::StorageFull { detail } if detail.contains("inbox")),
        "{err:?}"
    );
}

/// A device restored from a backup has no nonces: `top_up_pool(reset)` retires the pool the
/// old device published and publishes a fresh one, so proposals are assigned commitments
/// this device can sign with (found by the mainnet restore test).
#[tokio::test]
async fn a_restored_device_replaces_its_dead_pool() {
    use zafe_core::{
        node::top_up_pool,
        session::{MemoryPoolStore, PoolStore},
    };

    let (relay, ids, invite, number) = setup().await;
    let net = regtest_network();
    let timeout = Duration::from_secs(60);
    let (mut r0, mut r1, mut r2) = (
        StdRng::seed_from_u64(11),
        StdRng::seed_from_u64(12),
        StdRng::seed_from_u64(13),
    );
    let (a, b, c) = tokio::join!(
        run_keygen(
            &relay,
            &ids[0],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: Some(2),
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r0,
            timeout
        ),
        run_keygen(
            &relay,
            &ids[1],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r1,
            timeout
        ),
        run_keygen(
            &relay,
            &ids[2],
            &invite,
            &number,
            &net,
            "regtest",
            KeygenOptions {
                creator_birthday_height: None,
                creator_expiry_blocks: None,
                max_birthday: None
            },
            keygen_dir(),
            keygen_key(),
            &mut r2,
            timeout
        ),
    );
    let (material, _, _) = (a.unwrap(), b.unwrap(), c.unwrap());
    let me = ids[0].public().sig_pk;

    // The original device publishes its pool.
    let mut rng = StdRng::seed_from_u64(5);
    let mut old_device = MemoryPoolStore::default();
    let published = top_up_pool(
        &relay,
        &ids[0],
        &material,
        &mut old_device,
        false,
        None,
        &mut rng,
    )
    .await
    .unwrap();
    assert!(published > 0);
    let (_, state) = load_state(&relay, &ids[0], &material).await.unwrap();
    let old: Vec<Vec<u8>> = state.pools[&me].commitments.clone();
    assert_eq!(old.len(), published);

    // The restored device has an empty store. Without a reset the log still counts the
    // dead commitments as available, so nothing is published.
    let mut restored = MemoryPoolStore::default();
    assert_eq!(
        top_up_pool(
            &relay,
            &ids[0],
            &material,
            &mut restored,
            false,
            None,
            &mut rng
        )
        .await
        .unwrap(),
        0
    );

    // With the reset, the old ones are retired and a full fresh pool follows, all of it
    // signable here.
    let n = top_up_pool(
        &relay,
        &ids[0],
        &material,
        &mut restored,
        true,
        None,
        &mut rng,
    )
    .await
    .unwrap();
    assert_eq!(n, published);
    let (_, state) = load_state(&relay, &ids[0], &material).await.unwrap();
    let pool = &state.pools[&me];
    assert_eq!(pool.available(), published);
    assert!(pool.commitments.iter().all(|c| !old.contains(c)));
    assert!(pool.commitments.iter().all(|c| restored.contains(c)));
    assert!(state.ignored.is_empty());

    // Running the reset again right away only retires and republishes: still a full pool.
    top_up_pool(
        &relay,
        &ids[0],
        &material,
        &mut restored,
        true,
        None,
        &mut rng,
    )
    .await
    .unwrap();
    let (_, state) = load_state(&relay, &ids[0], &material).await.unwrap();
    assert_eq!(state.pools[&me].available(), published);
}

/// A fresh directory for one member's key-generation state.
fn keygen_dir() -> std::path::PathBuf {
    tempfile::tempdir().unwrap().keep()
}

/// The key that encrypts it (any fixed key will do in tests).
fn keygen_key() -> zafe_core::nonce_store::NonceKey {
    zafe_core::nonce_store::NonceKey::derive(b"test member", b"keygen")
}
