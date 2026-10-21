//! Cross-checks against ZcashFoundation/frost-tools `zcash-sign`, the reference external
//! signer for FROST + PCZT (spec §19 M0: "cross-check signatures and transactions").

use std::{
    collections::BTreeMap,
    io::Write,
    process::{Command, Stdio},
};

use orchard::keys::Scope;
use pczt::{
    roles::{
        prover::Prover, signer::extract_orchard_spend_auth_signatures,
        tx_extractor::TransactionExtractor,
    },
    Pczt,
};
use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{signing, tx};
use zcash_protocol::memo::MemoBytes;

mod common;
use common::*;

/// Our sighash on zcash-sign's real testnet fixture must equal the sighash that
/// transaction was actually signed against on chain.
#[test]
fn sighash_matches_real_testnet_ironwood_transaction() {
    let bytes = include_bytes!("../test-vectors/zcash-sign_ironwood_v6.pczt");
    let pczt = Pczt::parse(bytes).expect("zcash-sign fixture parses with pczt 0.9.3");
    assert_eq!(
        hex::encode(tx::shielded_sighash(&pczt).unwrap()),
        "7d48149e6ae74a301bc74c7e1a5af48c52c0d20550a086899db07555e73f53a5"
    );
}

/// Runs `zcash-sign sign` on a PCZT built and FROST-signed by Zafe, feeding it Zafe's
/// signatures, and compares sighash, randomizers and the resulting signatures byte for byte.
///
/// Build zcash-sign from frost-tools and run with:
///   ZCASH_SIGN_BIN=/path/to/zcash-sign cargo test -p zafe-core --test zcash_sign_crosscheck -- --ignored
#[test]
#[ignore = "needs ZCASH_SIGN_BIN"]
fn zcash_sign_agrees_with_zafe() {
    let bin = std::env::var("ZCASH_SIGN_BIN").expect("set ZCASH_SIGN_BIN");
    let mut rng = StdRng::seed_from_u64(70);
    let members = run_keygen(params(2, 3), &mut rng);
    let vault = &members[0].1;
    let fvk = vault.vault_keys.fvk().clone();

    // One vault note spent to a payment plus change (the padding dummy is pre-signed).
    let n1 = receive_ironwood_note(&fvk, fvk.address_at(0u32, Scope::External), 600_000);
    let (anchor, path) = witness(&n1);
    let outputs = vec![
        Out {
            ovk: Some(fvk.to_ovk(Scope::External)),
            recipient: outside_address(7),
            value: 400_000,
            memo: MemoBytes::empty(),
        },
        Out {
            ovk: Some(fvk.to_ovk(Scope::Internal)),
            recipient: fvk.address_at(0u32, Scope::Internal),
            value: 190_000,
            memo: MemoBytes::empty(),
        },
    ];
    let pczt = build_pczt(&fvk, n1, path, anchor, outputs);

    // Zafe side.
    let sighash = tx::shielded_sighash(&pczt).unwrap();
    let spends = tx::spends_to_sign(&pczt, &fvk).unwrap();
    let mut ours = Vec::new();
    for spend in &spends {
        let mut nonces = BTreeMap::new();
        let mut commitments = BTreeMap::new();
        for (id, m) in &members[..2] {
            let (n, c) = signing::commit(&m.key_package, &mut rng);
            nonces.insert(*id, n);
            commitments.insert(*id, c);
        }
        let package = signing::signing_package(commitments, &sighash);
        let shares: BTreeMap<_, _> = members[..2]
            .iter()
            .map(|(id, m)| {
                (
                    *id,
                    signing::sign(
                        &package,
                        nonces.remove(id).unwrap(),
                        &m.key_package,
                        spend.alpha,
                    )
                    .unwrap(),
                )
            })
            .collect();
        ours.push((
            spend.action_index,
            signing::aggregate(&package, &shares, &vault.public_key_package, spend.alpha).unwrap(),
        ));
    }
    let zafe_signed = tx::apply_signatures(pczt.clone(), &ours).unwrap();

    // zcash-sign side: same unsigned PCZT, Zafe's signatures on stdin in the order it asks.
    let dir = std::env::temp_dir().join(format!("zafe-zcash-sign-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("unsigned.pczt");
    let output = dir.join("signed.pczt");
    std::fs::write(&input, pczt.clone().serialize().unwrap()).unwrap();
    let mut child = Command::new(&bin)
        .args(["sign", "--tx-plan"])
        .arg(&input)
        .arg("--tx")
        .arg(&output)
        .args(["--network", "test"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("run zcash-sign");
    {
        let stdin = child.stdin.as_mut().unwrap();
        for (_, sig) in &ours {
            writeln!(stdin, "{}", hex::encode(sig)).unwrap();
        }
    }
    let result = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(result.status.success(), "zcash-sign failed:\n{stdout}");

    // 1. Same sighash.
    assert!(
        stdout.contains(&format!("SIGHASH: {}", hex::encode(sighash))),
        "{stdout}"
    );
    // 2. Same randomizers, same order, same action indices.
    let theirs: Vec<(usize, String)> = stdout
        .lines()
        .filter_map(|l| l.strip_prefix("Randomizer #"))
        .map(|rest| {
            let (idx, tail) = rest.split_once(' ').unwrap();
            (
                idx.parse().unwrap(),
                tail.rsplit(' ').next().unwrap().to_owned(),
            )
        })
        .collect();
    let mine: Vec<(usize, String)> = spends
        .iter()
        .map(|s| {
            (
                s.action_index,
                hex::encode(ff::PrimeField::to_repr(&s.alpha)),
            )
        })
        .collect();
    assert_eq!(theirs, mine, "randomizers differ:\n{stdout}");

    // 3. zcash-sign accepted Zafe's FROST signatures (it verifies each against rk), and the
    //    signatures it wrote are byte-identical to the ones Zafe injected.
    let their_signed = Pczt::parse(&std::fs::read(&output).unwrap()).unwrap();
    let sigs = |p: &Pczt| {
        extract_orchard_spend_auth_signatures(p)
            .iter()
            .map(|s| (s.action_index(), *s.signature()))
            .collect::<Vec<_>>()
    };
    assert_eq!(sigs(&their_signed), sigs(&zafe_signed));

    // 4. zcash-sign's signed PCZT proves and extracts into a fully valid transaction.
    let proved = Prover::new(their_signed)
        .create_ironwood_proof(zafe_core::zrng::zrng(), proving_key())
        .unwrap()
        .finish();
    TransactionExtractor::new(proved)
        .with_orchard(verifying_key())
        .extract(zafe_core::zrng::zrng())
        .expect("valid transaction");
}
