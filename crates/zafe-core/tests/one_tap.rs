//! One-tap signing (preprocessing): members sign at approval time from pre-published
//! commitments, for every signer group they are in; any complete group aggregates into
//! valid spend authorization signatures without a request round.

use std::collections::BTreeMap;

use orchard::keys::Scope;
use pczt::{
    roles::{prover::Prover, tx_extractor::TransactionExtractor},
    Pczt,
};
use rand::{rngs::StdRng, SeedableRng};
use reddsa::frost::redpallas::Identifier;
use zafe_core::{
    keygen::KeygenOutput,
    session::{
        aggregate_group, new_pool_commitments, GroupPlan, Member, MemoryPoolStore, PoolStore,
        SessionError,
    },
    tx,
    verify::{verify_pczt, Expectations, Payment},
};
use zcash_protocol::memo::{Memo, MemoBytes};

mod common;
use common::*;

struct Fixture {
    members: Vec<(Identifier, KeygenOutput)>,
    pools: Vec<MemoryPoolStore>,
    pczt: Pczt,
    expected: Expectations,
    /// The three 2-of-3 groups as member indexes, in descriptor order.
    groups: Vec<Vec<usize>>,
    /// `commitments[group][spend][position]`, as the vault log would assign them.
    commitments: Vec<Vec<Vec<Vec<u8>>>>,
}

impl Fixture {
    fn new(seed: u64) -> Self {
        let mut rng = StdRng::seed_from_u64(seed);
        let members = run_keygen(params(2, 3), &mut rng);
        let fvk = members[0].1.vault_keys.fvk().clone();
        let note = receive_ironwood_note(&fvk, fvk.address_at(0u32, Scope::External), 1_000_000);
        let (anchor, path) = witness(&note);
        let payee = outside_address(9);
        let memo = Memo::from_bytes(b"Milestone").unwrap().encode();
        let pczt = build_pczt(
            &fvk,
            note,
            path,
            anchor,
            vec![
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
            ],
        );
        let expected = expectations(vec![Payment {
            recipient: payee,
            amount_zat: 400_000,
            memo: *memo.as_array(),
        }]);

        // Preprocessing: each member publishes commitments ahead of time (2 each: one per
        // group it is in, one spend), keeping the nonces.
        let mut pools: Vec<MemoryPoolStore> = (0..3).map(|_| MemoryPoolStore::default()).collect();
        let mut published: Vec<Vec<Vec<u8>>> = Vec::new();
        for (i, pool) in pools.iter_mut().enumerate() {
            published
                .push(new_pool_commitments(&members[i].1.key_package, 2, pool, &mut rng).unwrap());
        }
        // The log assigns them in order: groups {0,1}, {0,2}, {1,2}.
        let groups = vec![vec![0, 1], vec![0, 2], vec![1, 2]];
        let mut next = [0usize; 3];
        let commitments = groups
            .iter()
            .map(|g| {
                vec![g
                    .iter()
                    .map(|&m| {
                        next[m] += 1;
                        published[m][next[m] - 1].clone()
                    })
                    .collect()]
            })
            .collect();
        Self {
            members,
            pools,
            pczt,
            expected,
            groups,
            commitments,
        }
    }

    fn plan(&self, member: usize) -> Vec<GroupPlan> {
        self.groups
            .iter()
            .enumerate()
            .filter(|(_, g)| g.contains(&member))
            .map(|(gi, g)| {
                (
                    gi,
                    g.iter().map(|&m| self.members[m].0).collect(),
                    self.commitments[gi].clone(),
                )
            })
            .collect()
    }

    fn approve(&mut self, member: usize) -> Result<BTreeMap<usize, Vec<Vec<u8>>>, SessionError> {
        let plan = self.plan(member);
        let (id, out) = &self.members[member];
        let signer = Member {
            identifier: *id,
            key_package: &out.key_package,
            vault_fvk: out.vault_keys.fvk(),
        };
        let (_, shares) =
            signer.sign_groups(&self.pczt, &self.expected, &plan, &mut self.pools[member])?;
        Ok(shares
            .into_iter()
            .map(|(g, s)| (g, s.iter().map(|x| x.serialize()).collect()))
            .collect())
    }

    fn aggregate(
        &self,
        group: usize,
        posted: &BTreeMap<usize, BTreeMap<usize, Vec<Vec<u8>>>>,
    ) -> Vec<(usize, [u8; 64])> {
        let ids: Vec<Identifier> = self.groups[group]
            .iter()
            .map(|&m| self.members[m].0)
            .collect();
        let shares = self.groups[group]
            .iter()
            .map(|&m| {
                let s = posted[&m][&group]
                    .iter()
                    .map(|b| {
                        reddsa::frost::redpallas::round2::SignatureShare::deserialize(b).unwrap()
                    })
                    .collect();
                (self.members[m].0, s)
            })
            .collect();
        let verified = verify_pczt(
            &self.pczt,
            self.members[0].1.vault_keys.fvk(),
            &self.expected,
        )
        .unwrap();
        aggregate_group(
            &ids,
            &self.commitments[group],
            &shares,
            &verified,
            &self.members[0].1.public_key_package,
        )
        .unwrap()
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
fn every_group_signs_at_approval_time() {
    let mut f = Fixture::new(60);
    let mut posted = BTreeMap::new();
    for m in 0..3 {
        let shares = f.approve(m).unwrap();
        assert_eq!(shares.len(), 2, "each member is in two 2-of-3 groups");
        posted.insert(m, shares);
    }
    // Every group aggregates into valid signatures; one is fully proved and extracted.
    for g in 0..3 {
        let signatures = f.aggregate(g, &posted);
        if g == 2 {
            f.finalize(&signatures);
        }
    }
}

#[test]
fn two_approvals_are_enough() {
    let mut f = Fixture::new(61);
    let mut posted = BTreeMap::new();
    for m in [0, 2] {
        posted.insert(m, f.approve(m).unwrap());
    }
    // Group {0,2} (index 1) is complete; member 1 never took part.
    let signatures = f.aggregate(1, &posted);
    f.finalize(&signatures);
}

#[test]
fn nonces_are_single_use() {
    let mut f = Fixture::new(62);
    f.approve(0).unwrap();
    assert!(matches!(f.approve(0), Err(SessionError::NoNonces)));
}

#[test]
fn a_bad_plan_consumes_nothing() {
    let mut f = Fixture::new(63);
    // Member 0's plan with member 1's commitment swapped in for group {0,2}: not ours.
    let mut plan = f.plan(0);
    plan[1].2[0][0] = f.commitments[0][0][1].clone();
    let (id, out) = &f.members[0];
    let signer = Member {
        identifier: *id,
        key_package: &out.key_package,
        vault_fvk: out.vault_keys.fvk(),
    };
    let err = signer
        .sign_groups(&f.pczt, &f.expected, &plan, &mut f.pools[0])
        .unwrap_err();
    assert!(matches!(err, SessionError::NoNonces));
    // Both of member 0's nonces are still there: the check ran before any was consumed.
    assert!(f.pools[0].contains(&f.commitments[0][0][0]));
    assert!(f.pools[0].contains(&f.commitments[1][0][0]));
}

#[test]
fn a_mismatching_transaction_consumes_nothing() {
    let mut f = Fixture::new(64);
    // The member expects a different amount than the PCZT pays: verification fails first.
    f.expected.payments[0].amount_zat += 1;
    assert!(matches!(f.approve(0), Err(SessionError::Verify(_))));
    assert!(f.pools[0].contains(&f.commitments[0][0][0]));
    assert!(f.pools[0].contains(&f.commitments[1][0][0]));
}
