//! End-to-end offline spend from a 2-of-3 vault on a local NU6.3 network:
//! vault keygen → Ironwood note to the vault → PCZT → find spends → local sighash →
//! FROST sign each spend with its alpha → inject → prove → extract and verify.
//!
//! Modeled on `pczt`'s own `wallet_can_set_ironwood_witness_after_signing` test.

use std::collections::BTreeMap;

use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use pczt::roles::{prover::Prover, tx_extractor::TransactionExtractor};
use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{signing, tx};
use zcash_protocol::memo::MemoBytes;

mod common;
use common::*;

fn pay_outside(
    fvk: &FullViewingKey,
    note: orchard::note::Note,
    path: orchard::tree::MerklePath,
    anchor: orchard::Anchor,
) -> pczt::Pczt {
    build_pczt(
        fvk,
        note,
        path,
        anchor,
        vec![Out {
            ovk: None,
            recipient: outside_address(7),
            value: 990_000,
            memo: MemoBytes::empty(),
        }],
    )
}

#[test]
fn vault_spends_ironwood_note_with_frost() {
    let mut rng = StdRng::seed_from_u64(20);
    let members = run_keygen(params(2, 3), &mut rng);
    let vault = &members[0].1;
    let vault_fvk = vault.vault_keys.fvk().clone();
    let vault_address = vault_fvk.address_at(0u32, Scope::External);

    // The vault receives 0.01 ZEC in the Ironwood pool.
    let note = receive_ironwood_note(&vault_fvk, vault_address, 1_000_000);
    let (anchor, path) = witness(&note);
    let pczt = pay_outside(&vault_fvk, note, path, anchor);

    // Every member independently finds the spends to sign and computes the sighash.
    let spends = tx::spends_to_sign(&pczt, &vault_fvk).unwrap();
    assert_eq!(
        spends.len(),
        1,
        "one real vault spend; the padding dummy is already signed"
    );
    let sighash = tx::shielded_sighash(&pczt).unwrap();

    // Two members sign: round 1 (commitments), leader builds packages, round 2 (shares).
    let signers = &members[1..3];
    let mut signatures = Vec::new();
    for spend in &spends {
        let mut nonces = BTreeMap::new();
        let mut commitments = BTreeMap::new();
        for (id, m) in signers {
            let (n, c) = signing::commit(&m.key_package, &mut rng);
            nonces.insert(*id, n);
            commitments.insert(*id, c);
        }
        let package = signing::signing_package(commitments, &sighash);
        let shares: BTreeMap<_, _> = signers
            .iter()
            .map(|(id, m)| {
                let share = signing::sign(
                    &package,
                    nonces.remove(id).unwrap(),
                    &m.key_package,
                    spend.alpha,
                )
                .unwrap();
                (*id, share)
            })
            .collect();
        let sig =
            signing::aggregate(&package, &shares, &vault.public_key_package, spend.alpha).unwrap();
        signatures.push((spend.action_index, sig));
    }

    // Inject (checked against rk), prove, and extract; extraction verifies the whole
    // Ironwood bundle: spend auth signatures, binding signature and the Halo 2 proof.
    let signed = tx::apply_signatures(pczt, &signatures).unwrap();
    assert!(tx::spends_to_sign(&signed, &vault_fvk).unwrap().is_empty());
    let proved = Prover::new(signed)
        .create_ironwood_proof(zafe_core::zrng::zrng(), proving_key())
        .unwrap()
        .finish();
    let transaction = TransactionExtractor::new(proved)
        .with_orchard(verifying_key())
        .extract(zafe_core::zrng::zrng())
        .expect("fully authorized, valid Ironwood transaction");
    assert!(transaction.ironwood_bundle().is_some());
}

#[test]
fn rejects_spend_of_foreign_note() {
    let mut rng = StdRng::seed_from_u64(21);
    let members = run_keygen(params(2, 3), &mut rng);
    let vault_fvk = members[0].1.vault_keys.fvk().clone();

    // A PCZT that spends someone else's note.
    let other_fvk = FullViewingKey::from(&SpendingKey::from_bytes([3; 32]).unwrap());
    let note = receive_ironwood_note(
        &other_fvk,
        other_fvk.address_at(0u32, Scope::External),
        1_000_000,
    );
    let (anchor, path) = witness(&note);
    let pczt = pay_outside(&other_fvk, note, path, anchor);

    assert!(matches!(
        tx::spends_to_sign(&pczt, &vault_fvk),
        Err(tx::TxError::NotVaultSpend(_))
    ));
}

#[test]
fn signature_with_wrong_alpha_is_rejected_on_injection() {
    let mut rng = StdRng::seed_from_u64(22);
    let members = run_keygen(params(2, 3), &mut rng);
    let vault = &members[0].1;
    let vault_fvk = vault.vault_keys.fvk().clone();
    let note = receive_ironwood_note(
        &vault_fvk,
        vault_fvk.address_at(0u32, Scope::External),
        1_000_000,
    );
    let (anchor, path) = witness(&note);
    let pczt = pay_outside(&vault_fvk, note, path, anchor);

    let spend = &tx::spends_to_sign(&pczt, &vault_fvk).unwrap()[0];
    let sighash = tx::shielded_sighash(&pczt).unwrap();
    let wrong_alpha = <pasta_curves::pallas::Scalar as ff::Field>::random(&mut rng);

    let signers = &members[..2];
    let mut nonces = BTreeMap::new();
    let mut commitments = BTreeMap::new();
    for (id, m) in signers {
        let (n, c) = signing::commit(&m.key_package, &mut rng);
        nonces.insert(*id, n);
        commitments.insert(*id, c);
    }
    let package = signing::signing_package(commitments, &sighash);
    let shares: BTreeMap<_, _> = signers
        .iter()
        .map(|(id, m)| {
            (
                *id,
                signing::sign(
                    &package,
                    nonces.remove(id).unwrap(),
                    &m.key_package,
                    wrong_alpha,
                )
                .unwrap(),
            )
        })
        .collect();
    let sig =
        signing::aggregate(&package, &shares, &vault.public_key_package, wrong_alpha).unwrap();

    assert!(matches!(
        tx::apply_signatures(pczt, &[(spend.action_index, sig)]),
        Err(tx::TxError::Sign(_))
    ));
}
