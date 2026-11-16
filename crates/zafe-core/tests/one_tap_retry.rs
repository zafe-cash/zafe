use std::collections::{BTreeMap, BTreeSet};

use orchard::keys::Scope;
use rand::{rngs::StdRng, SeedableRng};
use reddsa::frost::redpallas::Identifier;
use serde::Deserialize;
use zafe_core::{
    node::{
        append_event, approve, load_state, one_tap_stuck, read_inbox, request_signatures, respond,
        sign_own_shares, ChainView, SilentSigners, VaultMaterial,
    },
    relay_client::RelayClient,
    session::{
        aggregate_request, pczt_hash, GroupPlan, Member, MemoryNonceStore, MemoryPoolStore,
        PoolStore,
    },
    vault::{GroupShares, VaultEvent},
};
use zafe_proto::{
    version::{self, Format},
    Identity, Kind,
};
use zcash_protocol::memo::MemoBytes;

pub mod common;
use common::{
    build_pczt, nu6_3_network, receive_ironwood_note, relay_vault::vault, witness, Out,
    TARGET_HEIGHT,
};

fn view() -> ChainView {
    ChainView {
        tip: TARGET_HEIGHT - 1,
        spends: Vec::new(),
        dropped: Vec::new(),
    }
}

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
        &nu6_3_network(),
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
    let (relay, ids, materials, mut pools) = vault(2, 2, true).await;
    let net = nu6_3_network();
    let tip = view();
    let proposal = propose_sweep(&relay, &ids, &materials[0]).await;
    let mut rng = StdRng::seed_from_u64(1);

    sign_and_lose(&relay, &ids[1], &materials[1], &mut pools[1], proposal).await;
    let (_, state) = load_state(&relay, &ids[1], &materials[1]).await.unwrap();
    assert!(state.proposals[&proposal].shares.is_empty());

    let again = approve(
        &relay,
        &ids[1],
        &materials[1],
        &net,
        &tip,
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

    let twice = approve(
        &relay,
        &ids[1],
        &materials[1],
        &net,
        &tip,
        proposal,
        &mut MemoryNonceStore::default(),
        &mut pools[1],
        &mut rng,
    )
    .await
    .unwrap();
    assert!(twice.signed);

    let last = approve(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        &tip,
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

#[derive(Deserialize)]
struct SharesMsg {
    proposal: [u8; 16],
    request_hash: [u8; 32],
    shares: Vec<Vec<u8>>,
}

#[tokio::test]
async fn a_one_tap_vote_is_completed_interactively() {
    let (relay, ids, materials, mut pools) = vault(2, 2, true).await;
    let net = nu6_3_network();
    let proposal = propose_sweep(&relay, &ids, &materials[0]).await;
    let mut rng = StdRng::seed_from_u64(2);
    let mut stores = [MemoryNonceStore::default(), MemoryNonceStore::default()];

    let first = approve(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        &view(),
        proposal,
        &mut stores[0],
        &mut pools[0],
        &mut rng,
    )
    .await
    .unwrap();
    assert!(first.signed);

    let second = approve(
        &relay,
        &ids[1],
        &materials[1],
        &net,
        &view(),
        proposal,
        &mut stores[1],
        &mut MemoryPoolStore::default(),
        &mut rng,
    )
    .await
    .unwrap();
    assert!(!second.signed);
    let (_, state) = load_state(&relay, &ids[1], &materials[1]).await.unwrap();
    let p = &state.proposals[&proposal];
    assert!(p.one_tap_blocked(), "the only group can't complete one tap");
    let pk = |i: usize| ids[i].public().sig_pk;
    assert!(one_tap_stuck(
        &state,
        &proposal,
        &pk(1),
        &MemoryPoolStore::default()
    ));

    let again = approve(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        &view(),
        proposal,
        &mut stores[0],
        &mut pools[0],
        &mut rng,
    )
    .await
    .unwrap();
    assert!(again.signed);
    let (_, state) = load_state(&relay, &ids[0], &materials[0]).await.unwrap();
    let p = &state.proposals[&proposal];
    assert!(state.ignored.is_empty(), "{:?}", state.ignored);
    assert!(p.shares.contains_key(&pk(0)), "the one-tap shares stay");
    assert!(p.approvals.values().all(|c| !c.is_empty()));
    assert!(p.possibly_signed);

    let silent = SilentSigners::at(
        std::env::temp_dir().join(format!("zafe-silent-{}-compose.bin", std::process::id())),
    );
    let failed = request_signatures(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        TARGET_HEIGHT - 1,
        proposal,
        &BTreeSet::new(),
        &silent,
        &mut rng,
    )
    .await
    .unwrap();
    sign_own_shares(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        TARGET_HEIGHT - 1,
        &failed.request,
        &mut stores[0],
    )
    .await
    .unwrap()
    .unwrap();
    let used = p.approvals[&pk(0)].clone();
    approve(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        &view(),
        proposal,
        &mut stores[0],
        &mut pools[0],
        &mut rng,
    )
    .await
    .unwrap();
    let (_, state) = load_state(&relay, &ids[0], &materials[0]).await.unwrap();
    let p = &state.proposals[&proposal];
    assert_ne!(
        p.approvals[&pk(0)],
        used,
        "fresh commitments for the next round"
    );

    let sent = request_signatures(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        TARGET_HEIGHT - 1,
        proposal,
        &BTreeSet::new(),
        &silent,
        &mut rng,
    )
    .await
    .unwrap();
    let report = respond(
        &relay,
        &ids[1],
        &materials[1],
        &net,
        TARGET_HEIGHT - 1,
        &mut stores[1],
        &mut rng,
    )
    .await
    .unwrap();
    assert_eq!(report.answered, vec![proposal], "{:?}", report.skipped);
    assert_eq!(report.skipped.len(), 1, "the unfinished round's request");
    let own = sign_own_shares(
        &relay,
        &ids[0],
        &materials[0],
        &net,
        TARGET_HEIGHT - 1,
        &sent.request,
        &mut stores[0],
    )
    .await
    .unwrap()
    .unwrap();

    let members = zafe_core::node::members_by_pk(&state);
    let wanted = zafe_core::node::request_hash(&sent.request).unwrap();
    let mut shares = BTreeMap::new();
    let parse = |bytes: &[Vec<u8>]| {
        bytes
            .iter()
            .map(|b| reddsa::frost::redpallas::round2::SignatureShare::deserialize(b).unwrap())
            .collect::<Vec<_>>()
    };
    for (_, from, env) in read_inbox(&relay, &ids[0], materials[0].label(), &members, 0)
        .await
        .unwrap()
    {
        if env.header.kind != Kind::SignatureShares {
            continue;
        }
        let msg: SharesMsg =
            version::decode(Format::SignatureShares, &env.open(&ids[0], &from).unwrap()).unwrap();
        assert_eq!(msg.proposal, proposal);
        assert_eq!(msg.request_hash, wanted);
        let id = Identifier::deserialize(&state.descriptor.member(&from.sig_pk).unwrap().frost_id)
            .unwrap();
        shares.insert(id, parse(&msg.shares));
    }
    shares.insert(
        *materials[0].key_package().unwrap().identifier(),
        parse(&own),
    );
    let pczt = pczt::Pczt::parse(&p.pczt).unwrap();
    let fvk = materials[0].vault_keys().unwrap().fvk().clone();
    let spends = zafe_core::tx::spends_to_sign(&pczt, &fvk).unwrap();
    let signatures = aggregate_request(
        &sent.request,
        &spends,
        &shares,
        &materials[0].public_key_package().unwrap(),
    )
    .unwrap();
    zafe_core::tx::apply_signatures(pczt, &signatures).unwrap();
}
