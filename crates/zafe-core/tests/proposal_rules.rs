//! What a member's phone does with proposals under the November 2026 review's rules
//! ("Proposals"), over a real relay: a signing share leaves only for votes in the log, a
//! reject never follows a released signature, a leader drops a signer whose share or
//! commitments are bad, and an approval checks the cancelled payments that may still mine.

use std::collections::{BTreeMap, BTreeSet};

use orchard::keys::Scope;
use rand::{rngs::StdRng, SeedableRng};
use reddsa::frost::redpallas::{round1::SigningCommitments, Identifier};
use serde::Serialize;
use zafe_core::{
    node::{
        append_event, approve, cancel, encode_request, finalize, load_state, reject, request_hash,
        request_signatures, respond, sign_own_shares, ChainView, NodeError, SeqCounter,
        SilentSigners, VaultMaterial,
    },
    nonce_store::{FileNonceStore, NonceKey},
    relay_client::RelayClient,
    session::{pczt_hash, Member, MemoryNonceStore, MemoryPoolStore, NonceStore, SigningRequest},
    signing::signing_package,
    tx,
    vault::VaultEvent,
};
use zafe_proto::{
    version::{self, Format},
    Envelope, Identity, Kind,
};
use zcash_protocol::memo::MemoBytes;

mod common;
use common::{
    build_pczt, nu6_3_network, receive_ironwood_note,
    relay_vault::{vault, vault_on},
    witness, Out, TARGET_HEIGHT,
};

const TIP: u32 = TARGET_HEIGHT - 1;

fn view() -> ChainView {
    ChainView {
        tip: TIP,
        spends: Vec::new(),
        dropped: Vec::new(),
    }
}

/// A note of the vault, received at its external address `i`.
fn note(m: &VaultMaterial, i: u32) -> orchard::Note {
    let fvk = m.vault_keys().unwrap().fvk().clone();
    receive_ironwood_note(&fvk, fvk.address_at(i, Scope::External), 1_000_000)
}

/// Logs (as `author`) a proposal spending `note` back to the vault's change address.
async fn propose(
    relay: &RelayClient,
    author: &Identity,
    m: &VaultMaterial,
    id: u8,
    note: orchard::Note,
) -> [u8; 16] {
    let fvk = m.vault_keys().unwrap().fvk().clone();
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
    let event = VaultEvent::Proposal {
        id: [id; 16],
        payments: vec![],
        pczt_hash: pczt_hash(&pczt).unwrap(),
        pczt: pczt.serialize().unwrap(),
        tip_height: TIP,
        created_at: 0,
        signing_spends: 1,
        auto_send: false,
    };
    let (mut chain, mut state) = load_state(relay, author, m).await.unwrap();
    let mut rng = StdRng::seed_from_u64(u64::from(id));
    append_event(relay, author, m, &mut chain, &mut state, &event, &mut rng)
        .await
        .unwrap();
    [id; 16]
}

/// `member`'s approval on its phone (interactive: no pool), with `store` as its nonces.
async fn approve_with(
    relay: &RelayClient,
    ids: &[Identity],
    ms: &[VaultMaterial],
    member: usize,
    proposal: [u8; 16],
    store: &mut impl NonceStore,
) -> Result<zafe_core::node::Approved, NodeError> {
    approve(
        relay,
        &ids[member],
        &ms[member],
        &nu6_3_network(),
        &view(),
        proposal,
        store,
        &mut MemoryPoolStore::default(),
        &mut StdRng::seed_from_u64(member as u64),
    )
    .await
}

fn frost_id(m: &VaultMaterial) -> Identifier {
    *m.key_package().unwrap().identifier()
}

/// Sends `request` sealed from `leader` to `to`, as a leader's phone does (or a dishonest
/// leader would).
async fn send_request(
    relay: &RelayClient,
    leader: &Identity,
    to: &Identity,
    m: &VaultMaterial,
    request: &SigningRequest,
) {
    let env = Envelope::sealed(
        leader,
        to.public(),
        m.label(),
        SeqCounter::default().next_seq(),
        Kind::SigningRequest,
        &encode_request(request).unwrap(),
        &mut StdRng::seed_from_u64(5),
    )
    .unwrap();
    relay.send(leader, &env).await.unwrap();
}

/// A request for `proposal` signed by the members whose commitments are `commitments`
/// (one per spend), over the PCZT's sighash.
fn request_for(
    state: &zafe_core::vault::VaultState,
    proposal: [u8; 16],
    commitments: &[(Identifier, Vec<u8>)],
) -> SigningRequest {
    let p = &state.proposals[&proposal];
    let pczt = pczt::Pczt::parse(&p.pczt).unwrap();
    let sighash = tx::shielded_sighash(&pczt).unwrap();
    let map: BTreeMap<_, _> = commitments
        .iter()
        .map(|(id, c)| (*id, SigningCommitments::deserialize(c).unwrap()))
        .collect();
    SigningRequest {
        proposal,
        pczt_hash: p.pczt_hash,
        signers: map.keys().copied().collect(),
        packages: vec![signing_package(map, &sighash)],
    }
}

/// The member's interactive commitment for the proposal's one spend, as stored on its phone.
fn stored(store: &impl NonceStore, state: &zafe_core::vault::VaultState, p: [u8; 16]) -> Vec<u8> {
    store
        .commitments(&p, &state.proposals[&p].pczt_hash)
        .unwrap()[0]
        .serialize()
        .unwrap()
}

/// Rule 5's precondition: a share leaves a phone only when the log holds its own approve
/// vote with exactly those commitments, so a signature never exists without the proposal
/// having reached Approved in every member's log. Here the vote's append failed after the
/// nonces were stored, and a leader asks anyway. On 4b5ae0e the phone signed.
#[tokio::test]
async fn a_share_needs_its_vote_in_the_log() {
    let (relay, ids, ms, _) = vault(2, 3, false).await;
    let p = propose(&relay, &ids[0], &ms[0], 1, note(&ms[0], 0)).await;
    let mut leader_store = MemoryNonceStore::default();
    approve_with(&relay, &ids, &ms, 0, p, &mut leader_store)
        .await
        .unwrap();

    // Member 1 approves on its phone, but its vote never reaches the log.
    let (_, state) = load_state(&relay, &ids[1], &ms[1]).await.unwrap();
    let pczt = pczt::Pczt::parse(&state.proposals[&p].pczt).unwrap();
    let key_package = ms[1].key_package().unwrap();
    let keys = ms[1].vault_keys().unwrap();
    let expected = zafe_core::node::expectations(
        &nu6_3_network(),
        &[],
        TIP,
        ms[1].descriptor.proposal_expiry_blocks,
    )
    .unwrap();
    let mut store = MemoryNonceStore::default();
    Member {
        identifier: *key_package.identifier(),
        key_package: &key_package,
        vault_fvk: keys.fvk(),
    }
    .approve(
        p,
        &pczt,
        &expected,
        &mut store,
        &mut StdRng::seed_from_u64(9),
    )
    .unwrap();

    let request = request_for(
        &state,
        p,
        &[
            (
                frost_id(&ms[0]),
                state.proposals[&p].approvals[&ids[0].public().sig_pk][0].clone(),
            ),
            (frost_id(&ms[1]), stored(&store, &state, p)),
        ],
    );
    send_request(&relay, &ids[0], &ids[1], &ms[0], &request).await;
    let report = respond(
        &relay,
        &ids[1],
        &ms[1],
        &nu6_3_network(),
        TIP,
        &mut store,
        &mut StdRng::seed_from_u64(3),
    )
    .await
    .unwrap();
    assert!(
        report.answered.is_empty(),
        "no share without the vote in the log"
    );
    assert_eq!(report.skipped.len(), 1);
    assert!(
        store.contains(&p, &state.proposals[&p].pczt_hash),
        "the nonces are untouched"
    );

    // Approving again posts those same commitments; then the request is answered.
    approve_with(&relay, &ids, &ms, 1, p, &mut store)
        .await
        .unwrap();
    let report = respond(
        &relay,
        &ids[1],
        &ms[1],
        &nu6_3_network(),
        TIP,
        &mut store,
        &mut StdRng::seed_from_u64(4),
    )
    .await
    .unwrap();
    assert_eq!(report.answered, vec![p], "{:?}", report.skipped);
}

/// An interactive request may use only commitments members logged in a vote, never a
/// pool commitment assigned to a one-tap group (members may now hold both, `VAULT_EVENT`
/// 12). On 4b5ae0e a member checked only its own commitment and signed.
#[tokio::test]
async fn an_interactive_request_never_uses_a_pool_commitment() {
    let (relay, ids, ms, _) = vault(2, 3, true).await;
    let p = propose(&relay, &ids[0], &ms[0], 1, note(&ms[0], 0)).await;
    let mut store = MemoryNonceStore::default();
    approve_with(&relay, &ids, &ms, 0, p, &mut store)
        .await
        .unwrap();
    let (_, state) = load_state(&relay, &ids[0], &ms[0]).await.unwrap();
    let pre = state.proposals[&p]
        .preprocessed
        .as_ref()
        .expect("one-tap groups");
    let pk1 = ids[1].public().sig_pk;
    let g = pre.groups_of(&pk1)[0];
    let pool_commitment = pre.commitment(g, 0, &pk1).unwrap().to_vec();

    // Member 2 leads with member 1's pool commitment in member 0's request.
    let request = request_for(
        &state,
        p,
        &[
            (frost_id(&ms[0]), stored(&store, &state, p)),
            (frost_id(&ms[1]), pool_commitment),
        ],
    );
    send_request(&relay, &ids[2], &ids[0], &ms[2], &request).await;
    let report = respond(
        &relay,
        &ids[0],
        &ms[0],
        &nu6_3_network(),
        TIP,
        &mut store,
        &mut StdRng::seed_from_u64(3),
    )
    .await
    .unwrap();
    assert!(report.answered.is_empty());
    assert!(store.contains(&p, &state.proposals[&p].pczt_hash));
}

/// A member's nonce files, in a fresh directory.
fn file_store(tag: &str, m: &VaultMaterial) -> (FileNonceStore, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("zafe-rules-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    (
        FileNonceStore::new(
            &dir,
            NonceKey::derive(tag.as_bytes(), &m.descriptor.vault_id),
        ),
        dir,
    )
}

async fn rejections(relay: &RelayClient, id: &Identity, m: &VaultMaterial, p: [u8; 16]) -> usize {
    let (_, state) = load_state(relay, id, m).await.unwrap();
    state.proposals[&p].rejections.len()
}

/// Finding G: a phone that released its signature never logs a reject (the others would
/// read "no signature"); it may still reject an approval whose nonces it holds, and the
/// reject destroys them first. On 4b5ae0e every one of these rejects was logged.
#[tokio::test]
async fn a_released_signature_is_never_rejected() {
    let (relay, ids, ms, _) = vault(2, 3, false).await;
    let p = propose(&relay, &ids[0], &ms[0], 1, note(&ms[0], 0)).await;
    let mut rng = StdRng::seed_from_u64(8);
    let no_pool = MemoryPoolStore::default();
    let (mut s0, _) = file_store("leader", &ms[0]);
    let (mut s1, _) = file_store("signer", &ms[1]);
    let (mut s2, dir2) = file_store("racing", &ms[2]);
    approve_with(&relay, &ids, &ms, 0, p, &mut s0)
        .await
        .unwrap();
    approve_with(&relay, &ids, &ms, 1, p, &mut s1)
        .await
        .unwrap();

    // Member 1 answered a signing request: its reply is out (nothing of it is left on
    // the phone but the logged approval whose nonces are gone).
    let silent = SilentSigners::at(
        std::env::temp_dir().join(format!("zafe-rules-{}-reject.silent", std::process::id())),
    );
    request_signatures(
        &relay,
        &ids[0],
        &ms[0],
        &nu6_3_network(),
        TIP,
        p,
        &BTreeSet::new(),
        &silent,
        &mut rng,
    )
    .await
    .unwrap();
    let report = respond(
        &relay,
        &ids[1],
        &ms[1],
        &nu6_3_network(),
        TIP,
        &mut s1,
        &mut rng,
    )
    .await
    .unwrap();
    assert_eq!(report.answered, vec![p]);
    let err = reject(&relay, &ids[1], &ms[1], p, &mut s1, &no_pool, &mut rng)
        .await
        .unwrap_err();
    assert!(matches!(err, NodeError::AlreadySigned), "{err:?}");
    assert_eq!(rejections(&relay, &ids[1], &ms[1], p).await, 0);

    // Member 2: a background respond on its phone is signing with an earlier set of its
    // nonces (claimed: renamed) while a re-approval wrote fresh ones. The reject takes the
    // fresh ones, then sees the claim, and logs nothing.
    approve_with(&relay, &ids, &ms, 2, p, &mut s2)
        .await
        .unwrap();
    let (_, state) = load_state(&relay, &ids[2], &ms[2]).await.unwrap();
    let hash = state.proposals[&p].pczt_hash;
    let file = dir2.join(format!("{}-{}.bin", hex::encode(p), hex::encode(hash)));
    std::fs::copy(&file, file.with_extension("bin.00112233aabbccdd.taking")).unwrap();
    assert!(s2.claimed(&p, &hash));
    let err = reject(&relay, &ids[2], &ms[2], p, &mut s2, &no_pool, &mut rng)
        .await
        .unwrap_err();
    assert!(matches!(err, NodeError::AlreadySigned), "{err:?}");
    assert_eq!(rejections(&relay, &ids[2], &ms[2], p).await, 0);
    assert!(
        !s2.contains(&p, &hash),
        "the reject destroyed the fresh nonces first"
    );

    // Member 0 approved but signed nothing yet (the leader signs its part when it sends):
    // its reject is logged, and the nonces are gone first, so no share can follow it.
    reject(&relay, &ids[0], &ms[0], p, &mut s0, &no_pool, &mut rng)
        .await
        .unwrap();
    assert_eq!(rejections(&relay, &ids[0], &ms[0], p).await, 1);
    assert!(!s0.contains(&p, &hash) && !s0.claimed(&p, &hash));
}

/// A one-tap vote with shares is final for a reject too, and says so plainly.
#[tokio::test]
async fn a_one_tap_signer_does_not_reject() {
    let (relay, ids, ms, mut pools) = vault(2, 3, true).await;
    let p = propose(&relay, &ids[0], &ms[0], 1, note(&ms[0], 0)).await;
    let mut rng = StdRng::seed_from_u64(6);
    let approved = approve(
        &relay,
        &ids[1],
        &ms[1],
        &nu6_3_network(),
        &view(),
        p,
        &mut MemoryNonceStore::default(),
        &mut pools[1],
        &mut rng,
    )
    .await
    .unwrap();
    assert!(approved.signed);
    let err = reject(
        &relay,
        &ids[1],
        &ms[1],
        p,
        &mut MemoryNonceStore::default(),
        &pools[1],
        &mut rng,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, NodeError::AlreadySigned), "{err:?}");
}

/// The leader's reply format (`node::SharesMsg`), for a dishonest signer's reply.
#[derive(Serialize)]
struct SharesMsg {
    proposal: [u8; 16],
    request_hash: [u8; 32],
    shares: Vec<Vec<u8>>,
}

/// A leader that gets a share that doesn't verify stops the round with the culprit named
/// and asks it last next time; an approver whose logged commitments are unusable is left
/// out and asked last too. On 4b5ae0e the bad share was a protocol error naming nobody.
#[tokio::test]
async fn a_leader_drops_a_signer_with_a_bad_share() {
    let (relay, ids, ms, _) = vault(2, 3, false).await;
    let p = propose(&relay, &ids[0], &ms[0], 1, note(&ms[0], 0)).await;
    let mut rng = StdRng::seed_from_u64(10);
    let mut s0 = MemoryNonceStore::default();
    approve_with(&relay, &ids, &ms, 0, p, &mut s0)
        .await
        .unwrap();
    approve_with(&relay, &ids, &ms, 1, p, &mut MemoryNonceStore::default())
        .await
        .unwrap();
    // Member 2's vote carries commitments no signing package can use.
    let (mut chain, mut state) = load_state(&relay, &ids[2], &ms[2]).await.unwrap();
    let junk = VaultEvent::Vote {
        proposal: p,
        pczt_hash: state.proposals[&p].pczt_hash,
        approve: true,
        commitments: vec![vec![1, 2, 3]],
        shares: vec![],
    };
    append_event(
        &relay, &ids[2], &ms[2], &mut chain, &mut state, &junk, &mut rng,
    )
    .await
    .unwrap();

    let silent_path = std::env::temp_dir().join(format!(
        "zafe-rules-{}-bad-share.silent",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&silent_path);
    let silent = SilentSigners::at(&silent_path);
    let sent = request_signatures(
        &relay,
        &ids[0],
        &ms[0],
        &nu6_3_network(),
        TIP,
        p,
        &BTreeSet::new(),
        &silent,
        &mut rng,
    )
    .await
    .unwrap();
    assert!(
        silent.load().contains(&frost_id(&ms[2])),
        "unusable commitments: asked last"
    );
    assert_eq!(
        sent.request.signers,
        BTreeSet::from([frost_id(&ms[0]), frost_id(&ms[1])])
    );

    // Member 1 replies with a share that doesn't verify.
    let mut one = [0u8; 32];
    one[0] = 1;
    let reply = version::encode(
        Format::SignatureShares,
        &SharesMsg {
            proposal: p,
            request_hash: request_hash(&sent.request).unwrap(),
            shares: vec![one.to_vec()],
        },
    )
    .unwrap();
    let env = Envelope::sealed(
        &ids[1],
        ids[0].public(),
        ms[1].label(),
        SeqCounter::default().next_seq(),
        Kind::SignatureShares,
        &reply,
        &mut rng,
    )
    .unwrap();
    relay.send(&ids[1], &env).await.unwrap();
    let own = sign_own_shares(
        &relay,
        &ids[0],
        &ms[0],
        &nu6_3_network(),
        TIP,
        &sent.request,
        &mut s0,
    )
    .await
    .unwrap();
    // Never connects: the round fails before anything is broadcast.
    let mut lightwalletd = zafe_core::wallet::Client::new(
        tonic::transport::Channel::from_static("http://127.0.0.1:9").connect_lazy(),
    );
    let err = finalize(
        &relay,
        &ids[0],
        &ms[0],
        &sent.request,
        own.as_deref(),
        &mut lightwalletd,
        std::time::Duration::from_secs(30),
        |_| {},
        &mut rng,
    )
    .await
    .unwrap_err();
    match err {
        NodeError::InvalidShares(culprits) => assert_eq!(culprits, vec![frost_id(&ms[1])]),
        e => panic!("{e:?}"),
    }
}

/// Approving checks the cancelled payments that may still mine (`check_vote`): a new
/// payment that doesn't spend one of their notes is refused on every member's phone.
#[tokio::test]
async fn approving_respends_a_cancelled_signed_payment() {
    let (relay, ids, ms, _) = vault(2, 3, false).await;
    let (a, b) = (note(&ms[0], 0), note(&ms[0], 1));
    let w = propose(&relay, &ids[0], &ms[0], 1, a).await;
    approve_with(&relay, &ids, &ms, 0, w, &mut MemoryNonceStore::default())
        .await
        .unwrap();
    approve_with(&relay, &ids, &ms, 1, w, &mut MemoryNonceStore::default())
        .await
        .unwrap();
    cancel(&relay, &ids[0], &ms[0], w, &mut StdRng::seed_from_u64(1))
        .await
        .unwrap();

    let other = propose(&relay, &ids[2], &ms[2], 2, b).await;
    let err = approve_with(
        &relay,
        &ids,
        &ms,
        1,
        other,
        &mut MemoryNonceStore::default(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, NodeError::Verification(_)), "{err:?}");
    let respend = propose(&relay, &ids[2], &ms[2], 3, a).await;
    approve_with(
        &relay,
        &ids,
        &ms,
        1,
        respend,
        &mut MemoryNonceStore::default(),
    )
    .await
    .unwrap();
}

/// A restored phone has no nonces (backups carry none): for a proposal that never reached
/// Approved nobody can have signed, so its reject goes through; once Approved, its logged
/// approval's nonces may have signed, so it doesn't. In the first batch-3 draft both were
/// refused.
#[tokio::test]
async fn a_restored_phone_rejects_what_nobody_signed() {
    let (relay, ids, ms, _) = vault(2, 3, false).await;
    let open = propose(&relay, &ids[0], &ms[0], 1, note(&ms[0], 0)).await;
    let approved = propose(&relay, &ids[0], &ms[0], 2, note(&ms[0], 1)).await;
    let mut rng = StdRng::seed_from_u64(12);
    let no_pool = MemoryPoolStore::default();
    approve_with(&relay, &ids, &ms, 1, open, &mut MemoryNonceStore::default())
        .await
        .unwrap();
    approve_with(
        &relay,
        &ids,
        &ms,
        0,
        approved,
        &mut MemoryNonceStore::default(),
    )
    .await
    .unwrap();
    approve_with(
        &relay,
        &ids,
        &ms,
        1,
        approved,
        &mut MemoryNonceStore::default(),
    )
    .await
    .unwrap();

    // Member 1's phone is restored: a fresh, empty nonce directory.
    let (mut restored, _) = file_store("restored", &ms[1]);
    reject(
        &relay,
        &ids[1],
        &ms[1],
        open,
        &mut restored,
        &no_pool,
        &mut rng,
    )
    .await
    .unwrap();
    assert_eq!(rejections(&relay, &ids[1], &ms[1], open).await, 1);
    let err = reject(
        &relay,
        &ids[1],
        &ms[1],
        approved,
        &mut restored,
        &no_pool,
        &mut rng,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, NodeError::AlreadySigned), "{err:?}");
    assert_eq!(rejections(&relay, &ids[1], &ms[1], approved).await, 0);
}

/// Why one member's logged shares make a proposal possibly signed (rule 5), even though
/// the log never reaches Approved: a one-tap group's signature is plain FROST arithmetic,
/// not a log event. Member 1 (honest) approves one-tap: its shares for every group it is
/// in go into the log. Member 2 (dishonest, one of fewer than t) holds the nonces of its
/// own pool commitments for the shared group, computes its share on its own phone and
/// never logs it. Those two shares aggregate into valid spend signatures: the transaction
/// is signed while every honest member's log shows one approval, status Open.
#[tokio::test]
async fn one_logged_share_vote_and_an_unlogged_one_sign() {
    use zafe_core::session::{aggregate_group, Member};
    let (relay, ids, ms, mut pools) = vault(2, 3, true).await;
    let p = propose(&relay, &ids[0], &ms[0], 1, note(&ms[0], 0)).await;
    let mut rng = StdRng::seed_from_u64(9);
    let approved = approve(
        &relay,
        &ids[1],
        &ms[1],
        &nu6_3_network(),
        &view(),
        p,
        &mut MemoryNonceStore::default(),
        &mut pools[1],
        &mut rng,
    )
    .await
    .unwrap();
    assert!(approved.signed);

    let (_, state) = load_state(&relay, &ids[0], &ms[0]).await.unwrap();
    let prop = &state.proposals[&p];
    assert_eq!(prop.status, zafe_core::vault::ProposalStatus::Open);
    assert_eq!(prop.approvals.len(), 1, "only member 1 voted");
    assert!(
        prop.possibly_signed,
        "rule 5 must see this one as possibly signed"
    );

    // A group with members 1 and 2.
    let pre = prop.preprocessed.as_ref().unwrap();
    let (pk1, pk2) = (ids[1].public().sig_pk, ids[2].public().sig_pk);
    let g = (0..pre.subsets.len())
        .find(|g| pre.subsets[*g].contains(&pk1) && pre.subsets[*g].contains(&pk2))
        .unwrap();
    let group_ids: Vec<Identifier> = pre.subsets[g]
        .iter()
        .map(|pk| Identifier::deserialize(&state.descriptor.member(pk).unwrap().frost_id).unwrap())
        .collect();
    let pczt = pczt::Pczt::parse(&prop.pczt).unwrap();
    let expected = zafe_core::node::expectations(
        &nu6_3_network(),
        &prop.payments,
        TIP,
        ms[2].descriptor.proposal_expiry_blocks,
    )
    .unwrap();

    // Member 2 signs off the log with its own pool nonces.
    let kp2 = ms[2].key_package().unwrap();
    let keys2 = ms[2].vault_keys().unwrap();
    let dishonest = Member {
        identifier: *kp2.identifier(),
        key_package: &kp2,
        vault_fvk: keys2.fvk(),
    };
    let plan = vec![(g, group_ids.clone(), pre.commitments[g].clone())];
    let (verified, mine) = dishonest
        .sign_groups(&pczt, &expected, &plan, &mut pools[2])
        .unwrap();

    // Member 1's logged shares for that group, plus member 2's.
    let logged = prop.shares[&pk1]
        .iter()
        .find(|s| usize::from(s.group) == g)
        .unwrap();
    let mut shares = BTreeMap::new();
    shares.insert(
        frost_id(&ms[1]),
        logged
            .shares
            .iter()
            .map(|b| reddsa::frost::redpallas::round2::SignatureShare::deserialize(b).unwrap())
            .collect::<Vec<_>>(),
    );
    shares.insert(frost_id(&ms[2]), mine[0].1.clone());
    let signatures = aggregate_group(
        &group_ids,
        &pre.commitments[g],
        &shares,
        &verified,
        &ms[2].public_key_package().unwrap(),
    )
    .unwrap();
    // Every spend signature verifies against its rk: the transaction is signed.
    tx::apply_signatures(pczt, &signatures).unwrap();
}

/// A signer the relay won't deliver to (507: the leader's share of its inbox is full)
/// doesn't abort the round for everyone: the leader skips it, asks it last from now on,
/// still sends the others theirs and keeps the request (security review 2026-11, NEW 6).
/// On 37ac433 `request_signatures` returned the relay's error.
#[tokio::test]
async fn a_leader_skips_a_signer_the_relay_wont_deliver_to() {
    // Each sender may leave each recipient 100 envelopes.
    let relay = zafe_relay::Relay::new().with_quotas(zafe_relay::quota::Quotas {
        envelopes_per_pair: Some(100),
        ..zafe_relay::quota::Quotas::none()
    });
    let (relay, ids, ms, _) = vault_on(relay, 2, 3, false).await;
    let p = propose(&relay, &ids[0], &ms[0], 1, note(&ms[0], 0)).await;
    for member in [0, 1, 2] {
        approve_with(
            &relay,
            &ids,
            &ms,
            member,
            p,
            &mut MemoryNonceStore::default(),
        )
        .await
        .unwrap();
    }
    // The leader (member 0) signs itself and would ask the lower id of 1 and 2; its share
    // of that member's inbox is full (never acked).
    let (full, other) = if frost_id(&ms[1]) < frost_id(&ms[2]) {
        (1, 2)
    } else {
        (2, 1)
    };
    let mut rng = StdRng::seed_from_u64(11);
    let mut seq = SeqCounter::default();
    let refused = loop {
        let env = Envelope::sealed(
            &ids[0],
            ids[full].public(),
            ms[0].label(),
            seq.next_seq(),
            Kind::DkgEcho,
            b"x",
            &mut rng,
        )
        .unwrap();
        if let Err(e) = relay.send(&ids[0], &env).await {
            break e;
        }
    };
    assert!(
        matches!(
            refused,
            zafe_core::relay_client::RelayClientError::StorageFull { .. }
        ),
        "{refused:?}"
    );
    let silent_path =
        std::env::temp_dir().join(format!("zafe-rules-{}-full.silent", std::process::id()));
    let _ = std::fs::remove_file(&silent_path);
    let silent = SilentSigners::at(&silent_path);
    // The round goes to signers it can reach: never one that can't complete.
    let sent = request_signatures(
        &relay,
        &ids[0],
        &ms[0],
        &nu6_3_network(),
        TIP,
        p,
        &BTreeSet::new(),
        &silent,
        &mut rng,
    )
    .await
    .expect("an undeliverable signer is replaced");
    assert!(silent.load().contains(&frost_id(&ms[full])), "asked last");
    assert_eq!(
        sent.request.signers,
        BTreeSet::from([frost_id(&ms[0]), frost_id(&ms[other])])
    );
    let inbox = zafe_core::node::read_inbox(
        &relay,
        &ids[other],
        ms[other].label(),
        &zafe_core::node::members_by_pk(
            &load_state(&relay, &ids[other], &ms[other]).await.unwrap().1,
        ),
        0,
    )
    .await
    .unwrap();
    assert_eq!(
        inbox
            .iter()
            .filter(|(_, _, env)| env.header.kind == Kind::SigningRequest)
            .count(),
        1,
        "one request, for the round that can complete"
    );
    // Without the leader's commitments the only set left needs the unreachable signer: the
    // round doesn't start, and the reachable one gets no request that can't complete (a
    // signer the relay refused before is asked first).
    let short = request_signatures(
        &relay,
        &ids[0],
        &ms[0],
        &nu6_3_network(),
        TIP,
        p,
        &BTreeSet::from([sent.used_commitments[0]]),
        &silent,
        &mut rng,
    )
    .await;
    assert!(
        matches!(
            short.as_ref().err(),
            Some(NodeError::Relay(
                zafe_core::relay_client::RelayClientError::StorageFull { .. }
            ))
        ),
        "{:?}",
        short.err()
    );
    let inbox = zafe_core::node::read_inbox(
        &relay,
        &ids[other],
        ms[other].label(),
        &zafe_core::node::members_by_pk(
            &load_state(&relay, &ids[other], &ms[other]).await.unwrap().1,
        ),
        0,
    )
    .await
    .unwrap();
    assert_eq!(
        inbox
            .iter()
            .filter(|(_, _, env)| env.header.kind == Kind::SigningRequest)
            .count(),
        1
    );
}
