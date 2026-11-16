use std::collections::BTreeMap;

use orchard::keys::Scope;
use pczt::{
    roles::{prover::Prover, tx_extractor::TransactionExtractor},
    Pczt,
};
use rand::{rngs::StdRng, SeedableRng};
use reddsa::frost::redpallas::{round2::SignatureShare, Identifier};
use zafe_core::{
    keygen::KeygenOutput,
    session::{Leader, Member, MemoryNonceStore, ProposalId, SessionError, SigningRequest},
    tx,
    verify::{Expectations, Payment},
};
use zcash_protocol::memo::{Memo, MemoBytes};

pub mod common;
use common::*;

const PROPOSAL: ProposalId = [0x42; 16];

struct Fixture {
    members: Vec<(Identifier, KeygenOutput)>,
    stores: Vec<MemoryNonceStore>,
    pczt: Pczt,
    expected: Expectations,
}

impl Fixture {
    fn new(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let members = run_keygen(params(2, 3), &mut rng);
        let fvk = members[0].1.vault_keys.fvk().clone();
        let note = receive_ironwood_note(&fvk, fvk.address_at(0u32, Scope::External), 1_000_000);
        let (anchor, path) = witness(&note);
        let payee = outside_address(7);
        let memo = Memo::from_bytes(b"Invoice #123").unwrap().encode();
        let outputs = vec![
            Out {
                ovk: Some(fvk.to_ovk(Scope::External)),
                recipient: payee,
                value: 400_000,
                memo: memo.clone(),
            },
            Out {
                ovk: Some(fvk.to_ovk(Scope::Internal)),
                recipient: fvk.address_at(0u32, Scope::Internal),
                value: 590_000,
                memo: MemoBytes::empty(),
            },
        ];
        let pczt = build_pczt(&fvk, note, path, anchor, outputs);
        let expected = expectations(vec![Payment {
            recipient: payee,
            amount_zat: 400_000,
            memo: *memo.as_array(),
        }]);
        Self {
            stores: (0..3).map(|_| MemoryNonceStore::default()).collect(),
            members,
            pczt,
            expected,
        }
    }

    fn member(&self, i: usize) -> Member<'_> {
        let (id, out) = &self.members[i];
        Member {
            identifier: *id,
            key_package: &out.key_package,
            vault_fvk: out.vault_keys.fvk(),
        }
    }

    fn approve(&mut self, i: usize, leader: &mut Leader, rng: &mut StdRng) {
        let (id, out) = &self.members[i];
        let member = Member {
            identifier: *id,
            key_package: &out.key_package,
            vault_fvk: out.vault_keys.fvk(),
        };
        let (approval, _) = member
            .approve(
                PROPOSAL,
                &self.pczt,
                &self.expected,
                &mut self.stores[i],
                rng,
            )
            .unwrap();
        assert!(leader.add_approval(approval));
    }

    fn sign(
        &mut self,
        i: usize,
        request: &SigningRequest,
    ) -> Result<Vec<SignatureShare>, SessionError> {
        let (id, out) = &self.members[i];
        let member = Member {
            identifier: *id,
            key_package: &out.key_package,
            vault_fvk: out.vault_keys.fvk(),
        };
        member.sign(request, &self.pczt, &self.expected, &mut self.stores[i])
    }

    fn leader(&self) -> Leader {
        let verified = zafe_core::verify::verify_pczt(
            &self.pczt,
            self.members[0].1.vault_keys.fvk(),
            &self.expected,
        )
        .unwrap();
        Leader::new(PROPOSAL, &self.pczt, &verified, 2).unwrap()
    }

    fn id(&self, i: usize) -> Identifier {
        self.members[i].0
    }

    fn finalize(&self, signatures: &[(usize, [u8; 64])]) {
        let signed = tx::apply_signatures(self.pczt.clone(), signatures).unwrap();
        let proved = Prover::new(signed)
            .create_ironwood_proof(zafe_core::zrng::zrng(), proving_key())
            .unwrap()
            .finish();
        TransactionExtractor::new(proved)
            .with_orchard(verifying_key())
            .extract(zafe_core::zrng::zrng())
            .expect("valid transaction");
    }
}

#[test]
fn approve_request_sign_aggregate() {
    let mut rng = StdRng::seed_from_u64(40);
    let mut f = Fixture::new(40);
    let mut leader = f.leader();
    for i in 0..3 {
        f.approve(i, &mut leader, &mut rng);
    }
    assert_eq!(leader.available_signers().len(), 3);

    let request = leader.request(&[f.id(0), f.id(2)]).unwrap();
    let mut shares = BTreeMap::new();
    for i in [0, 2] {
        let s = f.sign(i, &request).unwrap();
        shares.insert(f.id(i), s);
    }
    let signatures = leader
        .aggregate(&request, &shares, &f.members[0].1.public_key_package)
        .unwrap();
    f.finalize(&signatures);
}

#[test]
fn restart_after_signer_drops_out() {
    let mut rng = StdRng::seed_from_u64(41);
    let mut f = Fixture::new(41);
    let mut leader = f.leader();
    for i in 0..3 {
        f.approve(i, &mut leader, &mut rng);
    }

    let first = leader.request(&[f.id(0), f.id(1)]).unwrap();
    f.sign(0, &first).unwrap();

    assert_eq!(leader.available_signers(), vec![f.id(2)]);
    assert!(matches!(
        leader.request(&[f.id(0), f.id(2)]),
        Err(SessionError::UnknownApprover)
    ));

    f.approve(0, &mut leader, &mut rng);
    let second = leader.request(&[f.id(0), f.id(2)]).unwrap();
    let mut shares = BTreeMap::new();
    for i in [0, 2] {
        shares.insert(f.id(i), f.sign(i, &second).unwrap());
    }
    let signatures = leader
        .aggregate(&second, &shares, &f.members[0].1.public_key_package)
        .unwrap();
    f.finalize(&signatures);
}

#[test]
fn nonces_are_single_use() {
    let mut rng = StdRng::seed_from_u64(42);
    let mut f = Fixture::new(42);
    let mut leader = f.leader();
    for i in 0..2 {
        f.approve(i, &mut leader, &mut rng);
    }
    let request = leader.request(&[f.id(0), f.id(1)]).unwrap();
    f.sign(0, &request).unwrap();
    assert!(matches!(f.sign(0, &request), Err(SessionError::NoNonces)));
    let mut fresh = MemoryNonceStore::default();
    let m = f.member(2);
    m.approve(PROPOSAL, &f.pczt, &f.expected, &mut fresh, &mut rng)
        .unwrap();
    assert!(matches!(
        m.approve(PROPOSAL, &f.pczt, &f.expected, &mut fresh, &mut rng),
        Err(SessionError::AlreadyApproved)
    ));
}

#[test]
fn rejects_package_signing_a_different_message() {
    let mut rng = StdRng::seed_from_u64(43);
    let mut f = Fixture::new(43);
    let mut leader = f.leader();
    for i in 0..2 {
        f.approve(i, &mut leader, &mut rng);
    }
    let mut request = leader.request(&[f.id(0), f.id(1)]).unwrap();
    let commitments = request.packages[0].signing_commitments().clone();
    request.packages[0] = zafe_core::signing::signing_package(commitments, &[0xEE; 32]);
    assert!(matches!(
        f.sign(0, &request),
        Err(SessionError::WrongMessage(0))
    ));
}

#[test]
fn rejects_request_for_a_different_pczt() {
    let mut rng = StdRng::seed_from_u64(44);
    let mut f = Fixture::new(44);
    let mut leader = f.leader();
    for i in 0..2 {
        f.approve(i, &mut leader, &mut rng);
    }
    let mut request = leader.request(&[f.id(0), f.id(1)]).unwrap();
    request.pczt_hash = [0; 32];
    assert!(matches!(
        f.sign(0, &request),
        Err(SessionError::PcztMismatch)
    ));
}

#[test]
fn invalid_share_is_attributed() {
    let mut rng = StdRng::seed_from_u64(45);
    let mut f = Fixture::new(45);
    let mut leader = f.leader();
    for i in 0..2 {
        f.approve(i, &mut leader, &mut rng);
    }
    let request = leader.request(&[f.id(0), f.id(1)]).unwrap();
    let mut shares = BTreeMap::new();
    shares.insert(f.id(0), f.sign(0, &request).unwrap());
    let forged = shares[&f.id(0)].clone();
    shares.insert(f.id(1), forged);

    let err = leader
        .aggregate(&request, &shares, &f.members[0].1.public_key_package)
        .unwrap_err();
    let text = format!("{err:?}");
    assert!(text.contains("InvalidSignatureShare"), "{text}");
}

#[test]
fn bad_request_does_not_burn_nonces() {
    let mut rng = StdRng::seed_from_u64(46);
    let mut f = Fixture::new(46);
    let mut leader = f.leader();
    for i in 0..3 {
        f.approve(i, &mut leader, &mut rng);
    }
    let real = leader.request(&[f.id(0), f.id(1)]).unwrap();

    let mut forged = real.clone();
    let mut commitments = forged.packages[0].signing_commitments().clone();
    let (_, fake) = zafe_core::signing::commit(&f.members[2].1.key_package, &mut rng);
    commitments.insert(f.id(0), fake);
    forged.packages[0] = zafe_core::signing::signing_package(commitments, &[0; 32]);
    forged.packages[0] = zafe_core::signing::signing_package(
        forged.packages[0].signing_commitments().clone(),
        real.packages[0].message().as_slice().try_into().unwrap(),
    );
    assert!(matches!(
        f.sign(0, &forged),
        Err(SessionError::NotOurCommitments(0))
    ));

    let mut shares = BTreeMap::new();
    for i in [0, 1] {
        shares.insert(f.id(i), f.sign(i, &real).unwrap());
    }
    let signatures = leader
        .aggregate(&real, &shares, &f.members[0].1.public_key_package)
        .unwrap();
    f.finalize(&signatures);
}
