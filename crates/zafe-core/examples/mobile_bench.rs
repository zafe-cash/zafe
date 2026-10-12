//! Measures the device-side costs that decide the mobile design (spec §19 V7, V8):
//!
//! - `prove`: build the post-NU6.3 Halo 2 proving key and prove a vault spend (what the
//!   proposer's or leader's phone does).
//! - `sign`: a member's round-2 work (full PCZT verification plus FROST signing), which
//!   must fit in an iOS Notification Service Extension if round 2 is to run from a push.
//!
//! Run one mode per process so peak memory is attributable:
//!   /usr/bin/time -v target/release/examples/mobile_bench prove
//!   /usr/bin/time -v target/release/examples/mobile_bench sign
//! Set RAYON_NUM_THREADS to control proving parallelism.

use std::{collections::BTreeMap, time::Instant};

use orchard::keys::Scope;
use pczt::roles::prover::Prover;
use rand::{rngs::StdRng, SeedableRng};
use zafe_core::{
    session::{Leader, Member, MemoryNonceStore},
    verify::{verify_pczt, Payment},
};
use zcash_protocol::memo::MemoBytes;

#[path = "../tests/common/mod.rs"]
mod common;
use common::*;

/// Peak resident memory of this process in KiB (Linux/Android `VmHWM`), if available.
fn peak_rss_kb() -> u64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmHWM:"))
                .and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse().ok()))
        })
        .unwrap_or(0)
}

fn main() {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "prove".into());
    let threads = std::env::var("RAYON_NUM_THREADS").unwrap_or_else(|_| "default".into());

    // A 2-of-3 vault paying a recipient with change: 2 Ironwood actions.
    let mut rng = StdRng::seed_from_u64(1);
    let members = run_keygen(params(2, 3), &mut rng);
    let fvk = members[0].1.vault_keys.fvk().clone();
    let note = receive_ironwood_note(&fvk, fvk.address_at(0u32, Scope::External), 1_000_000);
    let (anchor, path) = witness(&note);
    let payee = outside_address(7);
    let outputs = vec![
        Out {
            ovk: Some(fvk.to_ovk(Scope::External)),
            recipient: payee,
            value: 400_000,
            memo: MemoBytes::empty(),
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
        memo: *MemoBytes::empty().as_array(),
    }]);

    match mode.as_str() {
        "prove" => {
            let t = Instant::now();
            let pk = orchard::circuit::ProvingKey::build(
                orchard::circuit::OrchardCircuitVersion::PostNu6_3,
            );
            let keygen_ms = t.elapsed().as_millis();
            let t = Instant::now();
            let _proved = Prover::new(pczt)
                .create_ironwood_proof(zafe_core::zrng::zrng(), &pk)
                .unwrap()
                .finish();
            let prove_ms = t.elapsed().as_millis();
            println!(
                r#"{{"mode":"prove","threads":"{threads}","proving_key_build_ms":{keygen_ms},"prove_2_actions_ms":{prove_ms},"peak_rss_kb":{}}}"#,
                peak_rss_kb()
            );
        }
        "sign" => {
            // Approval (round 1) happens earlier, in the app; measure round 2 as the NSE would run it.
            let proposal = [1u8; 16];
            let verified = verify_pczt(&pczt, &fvk, &expected).unwrap();
            let mut leader = Leader::new(proposal, &pczt, &verified, 2).unwrap();
            let mut stores: Vec<MemoryNonceStore> =
                (0..2).map(|_| MemoryNonceStore::default()).collect();
            for (i, store) in stores.iter_mut().enumerate() {
                let (id, out) = &members[i];
                let m = Member {
                    identifier: *id,
                    key_package: &out.key_package,
                    vault_fvk: out.vault_keys.fvk(),
                };
                leader.add_approval(
                    m.approve(proposal, &pczt, &expected, store, &mut rng)
                        .unwrap()
                        .0,
                );
            }
            let request = leader.request(&[members[0].0, members[1].0]).unwrap();

            let (id, out) = &members[1];
            let m = Member {
                identifier: *id,
                key_package: &out.key_package,
                vault_fvk: out.vault_keys.fvk(),
            };
            let t = Instant::now();
            let shares = m.sign(&request, &pczt, &expected, &mut stores[1]).unwrap();
            let round2_ms = t.elapsed().as_micros() as f64 / 1000.0;
            let mut all = BTreeMap::new();
            all.insert(*id, shares);
            println!(
                r#"{{"mode":"sign","round2_verify_and_sign_ms":{round2_ms:.2},"peak_rss_kb":{}}}"#,
                peak_rss_kb()
            );
        }
        other => panic!("unknown mode {other}; use prove or sign"),
    }
}
