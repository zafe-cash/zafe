//! Shared test helpers: an in-process simulation of vault creation.
#![allow(dead_code)]

use std::collections::BTreeMap;

use rand::rngs::StdRng;
use reddsa::frost::redpallas::{keys::dkg, Identifier};
use zafe_core::keygen::{member_identifier, KeygenOutput, KeygenParams, Round1, SkContribution};

pub fn params(t: u16, n: u16) -> KeygenParams {
    KeygenParams::new([0x5a; 16], t, n).unwrap()
}

pub fn identifiers(params: &KeygenParams) -> Vec<Identifier> {
    (0..params.max_signers)
        .map(|i| member_identifier(&[i as u8 + 1; 32], &params.vault_id).unwrap())
        .collect()
}

/// Runs the full ceremony for every member, as if messages were relayed faithfully.
pub fn run_keygen(params: KeygenParams, rng: &mut StdRng) -> Vec<(Identifier, KeygenOutput)> {
    let ids = identifiers(&params);

    let round1: Vec<Round1> = ids
        .iter()
        .map(|id| Round1::start(params, *id, rng).unwrap())
        .collect();
    let broadcast: BTreeMap<Identifier, dkg::round1::Package> = round1
        .iter()
        .map(|r| (r.identifier(), r.package().clone()))
        .collect();

    let mut round2 = Vec::new();
    let mut sealed: BTreeMap<(Identifier, Identifier), dkg::round2::Package> = BTreeMap::new();
    for r1 in round1 {
        let me = r1.identifier();
        let others = broadcast
            .iter()
            .filter(|(id, _)| **id != me)
            .map(|(i, p)| (*i, p.clone()))
            .collect();
        let (r2, outgoing) = r1.advance(others).unwrap();
        for (to, pkg) in outgoing {
            sealed.insert((me, to), pkg);
        }
        round2.push((me, r2));
    }

    let echoes: Vec<[u8; 32]> = round2.iter().map(|(_, r2)| r2.echo()).collect();
    assert!(echoes.windows(2).all(|w| w[0] == w[1]), "echo mismatch");

    let dkg_results: Vec<_> = round2
        .into_iter()
        .map(|(me, r2)| {
            let received = sealed
                .iter()
                .filter(|((_, to), _)| *to == me)
                .map(|((from, _), pkg)| (*from, pkg.clone()))
                .collect();
            (me, r2.finish(received).unwrap())
        })
        .collect();

    let contributions: BTreeMap<Identifier, SkContribution> = ids
        .iter()
        .map(|id| (*id, SkContribution::generate(rng)))
        .collect();

    dkg_results
        .into_iter()
        .map(|(me, res)| (me, res.finish(&contributions).unwrap()))
        .collect()
}

// --- Ironwood transaction fixtures on a local NU6.3 network --------------------------

use std::sync::OnceLock;

use orchard::{
    builder::BundleType,
    bundle::BundleVersion,
    circuit::{OrchardCircuitVersion, ProvingKey, VerifyingKey},
    keys::{FullViewingKey, OutgoingViewingKey, Scope, SpendingKey},
    note::Note,
    note_encryption::IronwoodDomain,
    tree::{MerkleHashOrchard, MerklePath},
    Address, Anchor,
};
use pczt::{
    roles::{creator::Creator, io_finalizer::IoFinalizer},
    Pczt,
};
use shardtree::{store::memory::MemoryShardStore, ShardTree};
use zcash_note_encryption::try_note_decryption;
use zcash_primitives::transaction::{
    builder::{BuildConfig, Builder, BundlePadding, PcztResult},
    fees::zip317,
};
use zcash_protocol::{
    consensus::{BlockHeight, BranchId},
    local_consensus::LocalNetwork,
    memo::{Memo, MemoBytes},
    value::Zatoshis,
};

/// Height the fixture transactions target; the tip is one below.
pub const TARGET_HEIGHT: u32 = 10_000_000;

pub fn proving_key() -> &'static ProvingKey {
    static PK: OnceLock<ProvingKey> = OnceLock::new();
    PK.get_or_init(|| ProvingKey::build(OrchardCircuitVersion::PostNu6_3))
}

pub fn verifying_key() -> &'static VerifyingKey {
    static VK: OnceLock<VerifyingKey> = OnceLock::new();
    VK.get_or_init(|| VerifyingKey::build(OrchardCircuitVersion::PostNu6_3))
}

pub fn nu6_3_network() -> LocalNetwork {
    LocalNetwork {
        overwinter: Some(BlockHeight::from_u32(1)),
        sapling: Some(BlockHeight::from_u32(2)),
        blossom: Some(BlockHeight::from_u32(3)),
        heartwood: Some(BlockHeight::from_u32(4)),
        canopy: Some(BlockHeight::from_u32(5)),
        nu5: Some(BlockHeight::from_u32(6)),
        nu6: Some(BlockHeight::from_u32(7)),
        nu6_1: Some(BlockHeight::from_u32(8)),
        nu6_2: Some(BlockHeight::from_u32(9)),
        nu6_3: Some(BlockHeight::from_u32(10)),
        nu7: None,
    }
}

pub fn branch_id() -> u32 {
    BranchId::for_height(&nu6_3_network(), BlockHeight::from_u32(TARGET_HEIGHT)).into()
}

/// An address controlled by nobody in the test (a stand-in for a payee or attacker).
pub fn outside_address(seed: u8) -> Address {
    FullViewingKey::from(&SpendingKey::from_bytes([seed; 32]).unwrap())
        .address_at(0u32, Scope::External)
}

/// Simulates receiving an Ironwood note at `recipient`, decrypted with `fvk`.
pub fn receive_ironwood_note(fvk: &FullViewingKey, recipient: Address, value: u64) -> Note {
    let version = BundleVersion::ironwood_v3();
    let mut builder = orchard::builder::Builder::new(
        BundleType::DEFAULT,
        version,
        version.default_flags(),
        Anchor::empty_tree(),
    )
    .unwrap();
    builder
        .add_output(
            None,
            recipient,
            orchard::value::NoteValue::from_raw(value),
            Memo::Empty.encode().into_bytes(),
        )
        .unwrap();
    let (bundle, meta) = builder
        .build::<i64>(&mut zafe_core::zrng::zrng())
        .unwrap()
        .unwrap();
    let action = &bundle.actions()[meta.output_action_index(0).unwrap()];
    let ivk = fvk.to_ivk(Scope::External).prepare();
    let (note, _, _) =
        try_note_decryption(&IronwoodDomain::for_action(action), &ivk, action).unwrap();
    note
}

/// A single-leaf Ironwood tree containing `note`.
pub fn witness(note: &Note) -> (Anchor, MerklePath) {
    let cmx: orchard::note::ExtractedNoteCommitment = note.commitment().into();
    let leaf = MerkleHashOrchard::from_cmx(&cmx);
    let mut tree =
        ShardTree::<_, 32, 16>::new(MemoryShardStore::<MerkleHashOrchard, u32>::empty(), 100);
    tree.append(leaf, incrementalmerkletree::Retention::Marked)
        .unwrap();
    tree.checkpoint(9_999_999).unwrap();
    let path = tree
        .witness_at_checkpoint_depth(0.into(), 0)
        .unwrap()
        .unwrap();
    (path.root(leaf).into(), path.into())
}

/// One output for [`build_pczt`].
pub struct Out {
    pub ovk: Option<OutgoingViewingKey>,
    pub recipient: Address,
    pub value: u64,
    pub memo: MemoBytes,
}

/// Builds an IO-finalized PCZT spending `note` (owned by `spend_fvk`) to `outputs`,
/// paying the ZIP 317 fee.
pub fn build_pczt(
    spend_fvk: &FullViewingKey,
    note: Note,
    path: MerklePath,
    anchor: Anchor,
    outputs: Vec<Out>,
) -> Pczt {
    build_pczt_with_fee(spend_fvk, note, path, anchor, outputs, None)
}

/// Like [`build_pczt`], optionally forcing a non-standard fixed fee (to model a
/// malicious proposer).
pub fn build_pczt_with_fee(
    spend_fvk: &FullViewingKey,
    note: Note,
    path: MerklePath,
    anchor: Anchor,
    outputs: Vec<Out>,
    fixed_fee: Option<u64>,
) -> Pczt {
    use zcash_primitives::transaction::fees::fixed;

    let mut builder = Builder::new(
        nu6_3_network(),
        TARGET_HEIGHT.into(),
        BuildConfig::Standard {
            sapling_anchor: None,
            orchard_anchor: None,
            ironwood_anchor: Some(anchor),
            orchard_padding: BundlePadding::DEFAULT,
            ironwood_padding: BundlePadding::DEFAULT,
        },
    );
    builder
        .add_ironwood_spend::<zip317::FeeRule>(spend_fvk.clone(), note, path)
        .unwrap();
    for out in outputs {
        builder
            .add_ironwood_output::<zip317::FeeRule>(
                out.ovk,
                out.recipient,
                Zatoshis::const_from_u64(out.value),
                out.memo,
            )
            .unwrap();
    }
    let PcztResult { pczt_parts, .. } = match fixed_fee {
        None => builder
            .build_for_pczt(zafe_core::zrng::zrng(), &zip317::FeeRule::standard())
            .unwrap(),
        Some(fee) => builder
            .build_for_pczt(
                zafe_core::zrng::zrng(),
                &fixed::FeeRule::non_standard(Zatoshis::const_from_u64(fee)),
            )
            .unwrap(),
    };
    IoFinalizer::new(Creator::build_from_parts(pczt_parts).unwrap())
        .finalize_io(zafe_core::zrng::zrng())
        .unwrap()
}

/// Member expectations for fixture transactions.
pub fn expectations(payments: Vec<zafe_core::verify::Payment>) -> zafe_core::verify::Expectations {
    zafe_core::verify::Expectations {
        payments,
        consensus_branch_id: branch_id(),
        tip_height: TARGET_HEIGHT - 1,
        max_expiry_delta: 100,
    }
}
