//! Mutation fuzz (security review 2026-11-13, finding J): mutate every byte of an honest
//! proposal PCZT and look for a mutation that still passes member verification but signs a
//! different transaction (a different sighash) or reports different facts.

use orchard::keys::Scope;
use pczt::Pczt;
use rand::{rngs::StdRng, SeedableRng};
use zafe_core::verify::{verify_pczt, Expectations, Payment};
use zcash_protocol::memo::{Memo, MemoBytes};

mod common;
use common::*;

/// The bundle-level fields (flags, value sum, anchor, proof, bsk) sit at the end: the fast
/// run covers them. On 4a4fe29 a changed Ironwood `value_sum` (amount or sign) or `flags`
/// (spends off) still verifies, with the same fee shown, but a different sighash: members
/// sign a transaction that can't be mined.
#[test]
fn bundle_field_mutations_never_verify_to_another_tx() {
    mutate(true);
}

/// Every byte (slow: run by hand with --ignored).
#[test]
#[ignore]
fn single_byte_mutations_never_verify_to_another_tx() {
    mutate(false);
}

fn mutate(tail_only: bool) {
    let mut rng = StdRng::seed_from_u64(30);
    let members = run_keygen(params(2, 3), &mut rng);
    let fvk = members[0].1.vault_keys.fvk().clone();
    let funds = 1_000_000u64;
    let note = receive_ironwood_note(&fvk, fvk.address_at(0u32, Scope::External), funds);
    let (anchor, path) = witness(&note);
    let payee = outside_address(7);
    let memo: MemoBytes = Memo::from_bytes(b"Invoice").unwrap().encode();
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
                value: funds - 410_000,
                memo: MemoBytes::empty(),
            },
        ],
    );
    let exp = Expectations {
        payments: vec![Payment {
            recipient: payee,
            amount_zat: 400_000,
            memo: *memo.as_array(),
        }],
        consensus_branch_id: branch_id(),
        tip_height: TARGET_HEIGHT - 1,
        max_expiry_delta: 100,
    };
    let base = verify_pczt(&pczt, &fvk, &exp).unwrap();
    let bytes = pczt.serialize().unwrap();
    println!("pczt {} bytes", bytes.len());
    let from = if tail_only {
        bytes.len().saturating_sub(400)
    } else {
        0
    };
    let (mut parsed, mut verified, mut same_tx) = (0, 0, 0);
    let mut suspicious = Vec::new();
    for i in from..bytes.len() {
        for x in [0x01u8, 0x80, 0xff] {
            let mut m = bytes.clone();
            m[i] ^= x;
            let Ok(p) = Pczt::parse(&m) else { continue };
            parsed += 1;
            let Ok(v) = verify_pczt(&p, &fvk, &exp) else {
                continue;
            };
            verified += 1;
            if v.sighash == base.sighash
                && v.spends_to_sign == base.spends_to_sign
                && v.change_total_zat == base.change_total_zat
                && v.fee_zat == base.fee_zat
            {
                same_tx += 1;
            } else {
                suspicious.push((i, x, v.sighash != base.sighash, v.expiry_height));
            }
        }
    }
    println!("parsed {parsed}, verified {verified}, same tx {same_tx}");
    for s in &suspicious {
        println!(
            "SUSPICIOUS byte {} ^{:#x}: sighash changed {} expiry {}",
            s.0, s.1, s.2, s.3
        );
    }
    assert!(
        suspicious.is_empty(),
        "{} mutations verify but sign another transaction (first: byte {})",
        suspicious.len(),
        suspicious[0].0
    );
}
