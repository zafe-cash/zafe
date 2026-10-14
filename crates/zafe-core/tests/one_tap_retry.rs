//! One-tap approvals that don't reach the log on the first try. Signing consumes the pool
//! nonces, so the vote is kept on the device and posted again on the next approve; and an
//! interactive approval that could never be used is refused instead of being logged.

use std::time::Duration;

use orchard::keys::Scope;
use rand::{rngs::StdRng, SeedableRng};
use reddsa::frost::redpallas::Identifier;
use zafe_core::{
    node::{
        append_event, approve, create_vault, join_vault, load_state, membership, run_keygen, seal,
        top_up_pool, NodeError, VaultMaterial,
    },
    relay_client::RelayClient,
    session::{pczt_hash, GroupPlan, Member, MemoryNonceStore, MemoryPoolStore, PoolStore},
    vault::{GroupShares, VaultEvent},
    wallet::regtest_network,
};
use zafe_proto::Identity;
use zcash_protocol::memo::MemoBytes;

mod common;
use common::{build_pczt, receive_ironwood_note, witness, Out, TARGET_HEIGHT};

async fn start_relay() -> RelayClient {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, zafe_relay::Relay::new().router())
            .await
            .unwrap();
    });
    RelayClient::new(format!("http://{addr}"))
}

/// A `t`-of-`n` vault made over the relay; every member publishes a full pool.
async fn vault(
    t: u16,
    n: u16,
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
    let (_, _, number) = membership(&relay, &ids[0], &invite).await.unwrap();
    let net = regtest_network();
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
                birthday,
                None,
                &mut rng,
                Duration::from_secs(60),
            )
            .await
            .unwrap()
        }
    });
    let materials = futures::future::join_all(runs).await;
    let mut pools = Vec::new();
    for (i, (id, m)) in ids.iter().zip(&materials).enumerate() {
        let mut pool = MemoryPoolStore::default();
        let mut rng = StdRng::seed_from_u64(200 + i as u64);
        assert!(
            top_up_pool(&relay, id, m, &mut pool, false, &mut rng)
                .await
                .unwrap()
                > 0
        );
        pools.push(pool);
    }
    (relay, ids, materials, pools)
}

/// Logs a sweep (no payments: everything back to the vault) as member 0; one-tap groups
/// are assigned from the pools.
async fn propose_sweep(relay: &RelayClient, ids: &[Identity], m: &VaultMaterial) -> [u8; 16] {
    let fvk = m.vault_keys().unwrap().fvk().clone();
    let note = receive_ironwood_note(&fvk, fvk.address_at(0u32, Scope::External), 1_000_000);
    let (anchor, path) = witness(&note);
    let pczt = build_pczt(
        &fvk,
        note,
        path,
        anchor,
        vec![Out {
            ovk: Some(fvk.to_ovk(Scope::Internal)),
            recipient: fvk.address_at(0u32, Scope::Internal),
            value: 990_000,
            memo: MemoBytes::empty(),
        }],
    );
    let id = [7u8; 16];
    let event = VaultEvent::Proposal {
        id,
        payments: vec![],
        pczt_hash: pczt_hash(&pczt).unwrap(),
        pczt: pczt.serialize().unwrap(),
        tip_height: TARGET_HEIGHT - 1,
        created_at: 0,
        signing_spends: 1,
        auto_send: false,
    };
    let (mut chain, mut state) = load_state(relay, &ids[0], m).await.unwrap();
    let mut rng = StdRng::seed_from_u64(9);
    append_event(relay, &ids[0], m, &mut chain, &mut state, &event, &mut rng)
        .await
        .unwrap();
    let (_, state) = load_state(relay, &ids[0], m).await.unwrap();
    assert!(
        state.proposals[&id].preprocessed.is_some(),
        "one-tap groups assigned"
    );
    id
}

/// What `approve` does before its append: sign every group this member is in (consuming
/// the nonces) and keep the vote. The append itself is "lost".
async fn sign_and_lose(
    relay: &RelayClient,
    me: &Identity,
    m: &VaultMaterial,
    pool: &mut MemoryPoolStore,
    proposal: [u8; 16],
) {
    let (_, state) = load_state(relay, me, m).await.unwrap();
    let p = &state.proposals[&proposal];
    let pre = p.preprocessed.as_ref().unwrap();
    let my_pk = me.public().sig_pk;
    let frost_id = |pk: &[u8; 32]| {
        Identifier::deserialize(&state.descriptor.member(pk).unwrap().frost_id).unwrap()
    };
    let plans: Vec<GroupPlan> = pre
        .groups_of(&my_pk)
        .into_iter()
        .map(|g| {
            (
                g,
                pre.subsets[g].iter().map(frost_id).collect(),
                pre.commitments[g].clone(),
            )
        })
        .collect();
    let key_package = m.key_package().unwrap();
    let keys = m.vault_keys().unwrap();
    let member = Member {
        identifier: *key_package.identifier(),
        key_package: &key_package,
        vault_fvk: keys.fvk(),
    };
    let pczt = pczt::Pczt::parse(&p.pczt).unwrap();
    let expected = zafe_core::node::expectations(
        &regtest_network(),
        &[],
        TARGET_HEIGHT - 1,
        m.descriptor.proposal_expiry_blocks,
    )
    .unwrap();
    let (_, shares) = member.sign_groups(&pczt, &expected, &plans, pool).unwrap();
    let event = VaultEvent::Vote {
        proposal,
        pczt_hash: p.pczt_hash,
        approve: true,
        commitments: vec![],
        shares: shares
            .into_iter()
            .map(|(g, s)| GroupShares {
                group: g as u16,
                shares: s.iter().map(|x| x.serialize()).collect(),
            })
            .collect(),
    };
    pool.keep_vote(&proposal, &p.pczt_hash, &event.to_bytes().unwrap())
        .unwrap();
}

#[tokio::test]
async fn a_lost_one_tap_vote_is_posted_again() {
    let (relay, ids, materials, mut pools) = vault(2, 2).await;
    let net = regtest_network();
    let tip = TARGET_HEIGHT - 1;
    let proposal = propose_sweep(&relay, &ids, &materials[0]).await;
    let mut rng = StdRng::seed_from_u64(1);

    // Member 1 signs, but its vote never reaches the relay; its nonces are gone.
    sign_and_lose(&relay, &ids[1], &materials[1], &mut pools[1], proposal).await;
    let (_, state) = load_state(&relay, &ids[1], &materials[1]).await.unwrap();
    assert!(state.proposals[&proposal].shares.is_empty());

    // Approving again posts the kept shares: still one tap.
    let again = approve(
        &relay,
        &ids[1],
        &materials[1],
        &net,
        tip,
        proposal,
        &mut MemoryNonceStore::default(),
        &mut pools[1],
        &mut rng,
    )
    .await
    .unwrap();
    assert!(again.signed);
    let pczt_hash = state.proposals[&proposal].pczt_hash;
    assert!(pools[1].kept_vote(&proposal, &pczt_hash).is_none());

    // And once more: the vote is already in the log, so this is a no-op, not an error.
    let twice = approve(
        &relay,
        &ids[1],
        &materials[1],
        &net,
        tip,
        proposal,
        &mut MemoryNonceStore::default(),
        &mut pools[1],
        &mut rng,
    )
    .await
    .unwrap();
    assert!(twice.signed);

    // Member 0 completes the 2-of-2 group the one-tap way.
    let last = approve(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        tip,
        proposal,
        &mut MemoryNonceStore::default(),
        &mut pools[0],
        &mut rng,
    )
    .await
    .unwrap();
    assert!(last.signed && last.completed);
    let (_, state) = load_state(&relay, &ids[0], &materials[0]).await.unwrap();
    assert!(state.proposals[&proposal].ready_group.is_some());
    assert!(state.ignored.is_empty());
}

#[tokio::test]
async fn a_useless_interactive_approval_is_refused() {
    let (relay, ids, materials, mut pools) = vault(2, 2).await;
    let net = regtest_network();
    let tip = TARGET_HEIGHT - 1;
    let proposal = propose_sweep(&relay, &ids, &materials[0]).await;
    let mut rng = StdRng::seed_from_u64(2);

    // Member 0's one-tap vote is final.
    approve(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        tip,
        proposal,
        &mut MemoryNonceStore::default(),
        &mut pools[0],
        &mut rng,
    )
    .await
    .unwrap();

    // Member 1 lost its nonces (and kept no vote): an interactive approval could never
    // reach 2 signers, so it is refused rather than logged.
    let err = approve(
        &relay,
        &ids[1],
        &materials[1],
        &net,
        tip,
        proposal,
        &mut MemoryNonceStore::default(),
        &mut MemoryPoolStore::default(),
        &mut rng,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, NodeError::NotReady(_)), "{err:?}");
    let (_, state) = load_state(&relay, &ids[1], &materials[1]).await.unwrap();
    assert_eq!(state.proposals[&proposal].approvals.len(), 1);
}
