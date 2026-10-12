//! Regression vectors for ZIP 2005 `use_qsk = true` vault key derivation.
//!
//! The vectors in `test-vectors/zip2005_use_qsk.json` are independently checked by
//! `scripts/check_zip2005_vectors.py`. Regenerate with `ZAFE_REGEN_VECTORS=1`.

use ff::PrimeField;
use orchard::keys::{FullViewingKey, Scope, SpendingKey};
use serde_json::{json, Value};
use zafe_core::keys::{VaultKeys, VaultSecret};
use zcash_protocol::consensus::Network;

const PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/test-vectors/zip2005_use_qsk.json"
);

/// Deterministic, valid `ak` values: taken from orchard spending keys, so they have ỹ = 0.
fn ak_for(i: u8) -> [u8; 32] {
    let mut seed = [i.wrapping_mul(29).wrapping_add(3); 32];
    loop {
        let sk = SpendingKey::from_bytes(seed);
        if sk.is_some().into() {
            return FullViewingKey::from(&sk.unwrap()).to_bytes()[..32]
                .try_into()
                .unwrap();
        }
        seed[0] = seed[0].wrapping_add(1);
    }
}

fn vault_sk_for(i: u8) -> [u8; 32] {
    core::array::from_fn(|j| (i as usize * 32 + j) as u8)
}

fn generate() -> Value {
    let vectors: Vec<Value> = (0u8..8)
        .map(|i| {
            let sk = VaultSecret::from_bytes(vault_sk_for(i));
            let ak = ak_for(i);
            let keys = VaultKeys::derive(&sk, &ak).expect("valid inputs");
            let qsk = sk.qsk();
            json!({
                "sk": hex::encode(sk.as_bytes()),
                "ak": hex::encode(ak),
                "nk": hex::encode(keys.nk().to_repr()),
                "qsk": hex::encode(qsk.as_bytes()),
                "qk": hex::encode(keys.qk()),
                "rivk_ext": hex::encode(keys.rivk_ext().to_repr()),
                "fvk": hex::encode(keys.fvk().to_bytes()),
                "default_address_raw": hex::encode(
                    keys.fvk().address_at(0u32, Scope::External).to_raw_address_bytes()
                ),
                "ufvk_testnet": keys.ufvk().unwrap().encode(&Network::TestNetwork).unwrap(),
            })
        })
        .collect();
    json!({
        "description": "Zafe ZIP 2005 use_qsk=true vault key derivation vectors (not official)",
        "qk_context": zafe_core::keys::QK_CONTEXT,
        "vectors": vectors,
    })
}

#[test]
fn zip2005_use_qsk_vectors() {
    let generated = generate();
    if std::env::var_os("ZAFE_REGEN_VECTORS").is_some() {
        let text = serde_json::to_string_pretty(&generated).unwrap() + "\n";
        std::fs::write(PATH, text).unwrap();
        return;
    }
    let stored: Value = serde_json::from_str(
        &std::fs::read_to_string(PATH).expect("vectors missing: run with ZAFE_REGEN_VECTORS=1"),
    )
    .unwrap();
    assert_eq!(stored, generated, "derivation output changed; see {PATH}");
}
