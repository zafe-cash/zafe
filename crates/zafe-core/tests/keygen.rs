use std::collections::BTreeMap;

use ff::Field;
use pasta_curves::pallas;
use rand::{rngs::StdRng, RngCore, SeedableRng};
use reddsa::frost::redpallas::{
    self,
    rerandomized::{RandomizedParams, Randomizer},
};
use zafe_core::keygen::{
    check_contribution, combine_vault_secret, echo_with_commitments, round1_echo, KeygenError,
    KeygenParams, Round1, SkContribution,
};

pub mod common;
use common::{identifiers, params, run_keygen};

#[test]
fn members_agree_on_vault_keys() {
    let mut rng = StdRng::seed_from_u64(10);
    for (t, n) in [(2, 3), (3, 5), (2, 2)] {
        let outputs = run_keygen(params(t, n), &mut rng);
        let (_, first) = &outputs[0];
        let ak = first.vault_keys.ak();
        assert_eq!(ak[31] & 0x80, 0, "ak must have even y");

        for (_, out) in &outputs {
            assert_eq!(out.transcript_hash, first.transcript_hash);
            assert_eq!(out.vault_secret.as_bytes(), first.vault_secret.as_bytes());
            assert_eq!(
                out.vault_keys.fvk().to_bytes(),
                first.vault_keys.fvk().to_bytes()
            );
            assert_eq!(out.public_key_package, first.public_key_package);
            assert_eq!(*out.key_package.min_signers(), t);
        }
        let shares: std::collections::BTreeSet<_> = outputs
            .iter()
            .map(|(_, o)| o.key_package.signing_share().serialize())
            .collect();
        assert_eq!(shares.len(), usize::from(n));
    }
}

#[test]
fn rerandomized_signature_verifies_as_orchard_spend_auth() {
    let mut rng = StdRng::seed_from_u64(11);
    let outputs = run_keygen(params(2, 3), &mut rng);
    let pubkeys = outputs[0].1.public_key_package.clone();
    let ak = *outputs[0].1.vault_keys.ak();

    let alpha = pallas::Scalar::random(&mut rng);
    let mut sighash = [0u8; 32];
    rng.fill_bytes(&mut sighash);

    let signers = &outputs[..2];
    let mut nonces = BTreeMap::new();
    let mut commitments = BTreeMap::new();
    for (id, out) in signers {
        let (n, c) = redpallas::round1::commit(out.key_package.signing_share(), &mut rng);
        nonces.insert(*id, n);
        commitments.insert(*id, c);
    }
    let signing_package = redpallas::SigningPackage::new(commitments, &sighash);
    let randomizer = Randomizer::from_scalar(alpha);

    let mut shares = BTreeMap::new();
    for (id, out) in signers {
        let share = zafe_core::signing::sign(
            &signing_package,
            nonces[id].clone(),
            &out.key_package,
            alpha,
        )
        .unwrap();
        shares.insert(*id, share);
    }
    let params = RandomizedParams::from_randomizer(pubkeys.verifying_key(), randomizer);
    let signature =
        redpallas::rerandomized::aggregate(&signing_package, &shares, &pubkeys, &params).unwrap();

    let sig_bytes: [u8; 64] = signature.serialize().unwrap().try_into().unwrap();
    let rk = reddsa::VerificationKey::<reddsa::orchard::SpendAuth>::try_from(ak)
        .unwrap()
        .randomize(&alpha);
    rk.verify(&sighash, &reddsa::Signature::from(sig_bytes))
        .expect("aggregate signature must verify under rk");

    let wrong_rk = reddsa::VerificationKey::<reddsa::orchard::SpendAuth>::try_from(ak)
        .unwrap()
        .randomize(&pallas::Scalar::random(&mut rng));
    assert!(wrong_rk
        .verify(&sighash, &reddsa::Signature::from(sig_bytes))
        .is_err());
}

#[test]
fn equivocating_relay_is_detected_by_echo() {
    let mut rng = StdRng::seed_from_u64(12);
    let params = params(2, 3);
    let ids = identifiers(&params);
    let r1: Vec<Round1> = ids
        .iter()
        .map(|id| Round1::start(params, *id, &mut rng).unwrap())
        .collect();
    let honest: BTreeMap<_, _> = r1
        .iter()
        .map(|r| (r.identifier(), r.package().clone()))
        .collect();

    let forged = Round1::start(params, ids[0], &mut rng).unwrap();
    let mut seen_by_2 = honest.clone();
    seen_by_2.insert(ids[0], forged.package().clone());

    assert_ne!(
        round1_echo(&params, &honest).unwrap(),
        round1_echo(&params, &seen_by_2).unwrap()
    );
}

#[test]
fn sk_contributions_are_committed_first() {
    let mut rng = StdRng::seed_from_u64(13);
    let params = params(2, 3);
    let ids = identifiers(&params);
    let (a, b) = (
        SkContribution::generate(&mut rng),
        SkContribution::generate(&mut rng),
    );
    let (vault, member) = ([1u8; 16], [2u8; 32]);
    let c = a.commitment(&vault, &member);
    assert_eq!(
        c,
        SkContribution::from_bytes(*a.as_bytes()).commitment(&vault, &member)
    );
    assert_ne!(c, b.commitment(&vault, &member), "another contribution");
    assert_ne!(c, a.commitment(&[9; 16], &member), "another vault");
    assert_ne!(c, a.commitment(&vault, &[9; 32]), "another member");

    let echo = [7u8; 32];
    let honest = BTreeMap::from([(ids[0], c), (ids[1], [3; 32]), (ids[2], [4; 32])]);
    let mut equivocated = honest.clone();
    equivocated.insert(ids[1], [5; 32]);
    assert_ne!(
        echo_with_commitments(&echo, &honest),
        echo_with_commitments(&echo, &equivocated)
    );
    assert_ne!(
        echo_with_commitments(&echo, &honest),
        echo_with_commitments(&[8; 32], &honest),
        "the round-1 packages still count"
    );
}

#[test]
fn a_contribution_changed_after_seeing_the_others_is_rejected() {
    let mut rng = StdRng::seed_from_u64(14);
    let params = params(2, 3);
    let ids = identifiers(&params);
    let vault = params.vault_id;
    let members: Vec<[u8; 32]> = (0..3u8).map(|i| [i + 1; 32]).collect();
    let contributions: Vec<SkContribution> =
        (0..3).map(|_| SkContribution::generate(&mut rng)).collect();
    let commitments: Vec<[u8; 32]> = contributions
        .iter()
        .zip(&members)
        .map(|(c, m)| c.commitment(&vault, m))
        .collect();
    for i in 0..3 {
        check_contribution(&vault, &members[i], &commitments[i], &contributions[i]).unwrap();
    }
    let others = |last: SkContribution| {
        BTreeMap::from([
            (ids[0], contributions[0].clone()),
            (ids[1], contributions[1].clone()),
            (ids[2], last),
        ])
    };
    let ground = (0..10_000u32)
        .map(|_| SkContribution::generate(&mut rng))
        .find(|c| combine_vault_secret(&vault, &[0; 32], &others(c.clone())).as_bytes()[0] == 0)
        .expect("a ground contribution");
    assert!(matches!(
        check_contribution(&vault, &members[2], &commitments[2], &ground),
        Err(KeygenError::ContributionMismatch)
    ));
    assert!(check_contribution(&vault, &members[2], &commitments[0], &contributions[0]).is_err());
}

#[test]
fn rejects_bad_inputs() {
    assert!(matches!(
        KeygenParams::new([0; 16], 1, 3),
        Err(KeygenError::InvalidParams { .. })
    ));
    assert!(matches!(
        KeygenParams::new([0; 16], 4, 3),
        Err(KeygenError::InvalidParams { .. })
    ));
    assert!(matches!(
        KeygenParams::new([0; 16], 2, 16),
        Err(KeygenError::InvalidParams { .. })
    ));

    let mut rng = StdRng::seed_from_u64(13);
    let params = params(2, 3);
    let ids = identifiers(&params);
    let r1: Vec<Round1> = ids
        .iter()
        .map(|id| Round1::start(params, *id, &mut rng).unwrap())
        .collect();
    let all: BTreeMap<_, _> = r1
        .iter()
        .map(|r| (r.identifier(), r.package().clone()))
        .collect();

    let mut it = r1.into_iter();
    let first = it.next().unwrap();
    assert!(matches!(
        first.advance(all.clone()),
        Err(KeygenError::OwnPackageReceived)
    ));

    let second = it.next().unwrap();
    let only_one: BTreeMap<_, _> = all
        .iter()
        .filter(|(id, _)| **id == ids[0])
        .map(|(i, p)| (*i, p.clone()))
        .collect();
    assert!(matches!(
        second.advance(only_one),
        Err(KeygenError::WrongPackageCount { .. })
    ));
}
