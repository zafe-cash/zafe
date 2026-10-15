//! Vault key derivation for FROST vaults (ZIP 2005, `use_qsk = true`).
//!
//! The FROST DKG yields the Spend validating key `ak`. All other Orchard-protocol key
//! components are derived from the vault secret `sk`, which every member holds:
//!
//! ```text
//! nk       = ToBase^Orchard( PRF^expand_sk([0x07]) )
//! qsk      = truncate_32( PRF^expand_sk([0x0C]) )
//! qk       = BLAKE3.derive_key("Zcash ZIP 2005 qk-derivation v1", qsk, 32)
//! rivk_ext = ToScalar^Orchard( PRF^expand_qk([0x0D] || I2LEOSP_256(ak) || I2LEOSP_256(nk)) )
//! ```
//!
//! ZIP 2005 § "Changes to the Protocol Specification", § 4.2.3 ‘Orchard Key Components’.
//! `H^ask(sk)` is deliberately not computed: `ask` exists only as FROST shares.
//!
//! No upstream crate implements this derivation yet (spec §7.4.1), so this module is one of
//! the few places Zafe implements key derivation itself. Keep it minimal, and cross-check it
//! against `orchard`/`zcash_spec` and the test vectors in `test-vectors/`.

use ff::{FromUniformBytes, PrimeField};
use orchard::keys::FullViewingKey;
use pasta_curves::pallas;
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_spec::PrfExpand;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// BLAKE3 `derive_key` context for `H^qk` (ZIP 2005).
pub const QK_CONTEXT: &str = "Zcash ZIP 2005 qk-derivation v1";

/// `PRF^expand` domain separator for `H^qsk` (ZIP 2005).
const DOMAIN_QSK: u8 = 0x0C;
/// `PRF^expand` domain separator for `H^rivk_ext_qk` (ZIP 2005).
const DOMAIN_RIVK_EXT: u8 = 0x0D;

const PRF_EXPAND_PERSONALIZATION: &[u8; 16] = b"Zcash_ExpandSeed";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyError {
    /// `repr(ak)` has its sign bit set. ZIP 2005 requires the ỹ bit of `ak^P` to be 0;
    /// the DKG output must be normalized with redpallas `EvenY` first.
    #[error("ak does not have an even y-coordinate")]
    OddAk,
    /// `ak` is not a valid non-identity Pallas point, or the key components were rejected
    /// by `orchard::keys::FullViewingKey::from_bytes`.
    #[error("invalid Orchard full viewing key components")]
    InvalidFvk,
    #[error("failed to build unified full viewing key: {0}")]
    Ufvk(String),
}

/// The vault secret `sk`, shared by all members (ZIP 2005 "Usage with FROST").
///
/// It is not a spending key: `ask` is FROST-shared and is never derived from `sk`.
/// Holding `sk` gives full viewing capability and the quantum spending key `qsk`.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct VaultSecret([u8; 32]);

impl VaultSecret {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// `qsk = H^qsk(sk)`. Must be protected as carefully as a FROST share (ZIP 2005).
    pub fn qsk(&self) -> QuantumSpendingKey {
        let mut expanded = prf_expand(&self.0, &[&[DOMAIN_QSK]]);
        let mut qsk = [0u8; 32];
        qsk.copy_from_slice(&expanded[..32]);
        expanded.zeroize();
        QuantumSpendingKey(qsk)
    }
}

impl core::fmt::Debug for VaultSecret {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("VaultSecret(<redacted>)")
    }
}

/// The quantum spending key `qsk` (ZIP 2005).
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct QuantumSpendingKey([u8; 32]);

impl QuantumSpendingKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// `qk = H^qk(qsk)`.
    pub fn qk(&self) -> [u8; 32] {
        blake3::derive_key(QK_CONTEXT, &self.0)
    }
}

impl core::fmt::Debug for QuantumSpendingKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("QuantumSpendingKey(<redacted>)")
    }
}

/// Viewing key material for a vault.
#[derive(Clone)]
pub struct VaultKeys {
    ak: [u8; 32],
    nk: pallas::Base,
    qk: [u8; 32],
    rivk_ext: pallas::Scalar,
    fvk: FullViewingKey,
}

/// Only the public `ak`: `nk`, `qk` and `rivk_ext` (and the full viewing key built from
/// them) must never reach a log or an error message (audit L10).
impl core::fmt::Debug for VaultKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VaultKeys")
            .field("ak", &hex::encode(self.ak))
            .finish_non_exhaustive()
    }
}

impl VaultKeys {
    /// Derives the vault's key components from the vault secret and the FROST group
    /// verifying key `ak` (its 32-byte encoding, `I2LEOSP_256(ak)`).
    pub fn derive(sk: &VaultSecret, ak: &[u8; 32]) -> Result<Self, KeyError> {
        // repr_P(ak^P) must have ỹ = 0, in which case it equals I2LEOSP_256(Extract_P(ak^P)).
        if ak[31] & 0x80 != 0 {
            return Err(KeyError::OddAk);
        }

        let nk = pallas::Base::from_uniform_bytes(&PrfExpand::ORCHARD_NK.with(sk.as_bytes()));
        let qk = sk.qsk().qk();
        let nk_repr = nk.to_repr();
        let rivk_ext = pallas::Scalar::from_uniform_bytes(&prf_expand(
            &qk,
            &[&[DOMAIN_RIVK_EXT], ak, &nk_repr],
        ));

        let mut fvk_bytes = [0u8; 96];
        fvk_bytes[..32].copy_from_slice(ak);
        fvk_bytes[32..64].copy_from_slice(&nk_repr);
        fvk_bytes[64..].copy_from_slice(&rivk_ext.to_repr());
        let fvk = FullViewingKey::from_bytes(&fvk_bytes).ok_or(KeyError::InvalidFvk)?;

        Ok(Self {
            ak: *ak,
            nk,
            qk,
            rivk_ext,
            fvk,
        })
    }

    pub fn ak(&self) -> &[u8; 32] {
        &self.ak
    }

    pub fn nk(&self) -> pallas::Base {
        self.nk
    }

    /// The quantum intermediate key. Semi-sensitive (ZIP 2005): do not publish it.
    pub fn qk(&self) -> &[u8; 32] {
        &self.qk
    }

    pub fn rivk_ext(&self) -> pallas::Scalar {
        self.rivk_ext
    }

    pub fn fvk(&self) -> &FullViewingKey {
        &self.fvk
    }

    /// The vault UFVK: an Orchard-only unified full viewing key.
    pub fn ufvk(&self) -> Result<UnifiedFullViewingKey, KeyError> {
        UnifiedFullViewingKey::from_orchard_fvk(self.fvk.clone())
            .map_err(|e| KeyError::Ufvk(format!("{e:?}")))
    }
}

/// `PRF^expand_k(t) = BLAKE2b-512("Zcash_ExpandSeed", k || t)`, with `t` given as parts.
///
/// Used only for the ZIP 2005 domains that `zcash_spec` does not define yet.
fn prf_expand(key: &[u8], parts: &[&[u8]]) -> [u8; 64] {
    let mut state = blake2b_simd::Params::new()
        .hash_length(64)
        .personal(PRF_EXPAND_PERSONALIZATION)
        .to_state();
    state.update(key);
    for part in parts {
        state.update(part);
    }
    *state.finalize().as_array()
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchard::keys::SpendingKey;
    use rand::{rngs::StdRng, RngCore, SeedableRng};

    fn random_spending_key(rng: &mut StdRng) -> SpendingKey {
        loop {
            let mut bytes = [0u8; 32];
            rng.fill_bytes(&mut bytes);
            let sk = SpendingKey::from_bytes(bytes);
            if sk.is_some().into() {
                return sk.unwrap();
            }
        }
    }

    /// Our `prf_expand` agrees with `zcash_spec` on the domains both define.
    #[test]
    fn prf_expand_matches_zcash_spec() {
        let mut rng = StdRng::seed_from_u64(1);
        for _ in 0..32 {
            let mut sk = [0u8; 32];
            rng.fill_bytes(&mut sk);
            assert_eq!(prf_expand(&sk, &[&[0x07]]), PrfExpand::ORCHARD_NK.with(&sk));
            assert_eq!(
                prf_expand(&sk, &[&[0x08]]),
                PrfExpand::ORCHARD_RIVK.with(&sk)
            );
        }
    }

    /// Recomputing the *legacy* (`use_qsk = false`) derivation with the same building
    /// blocks reproduces `orchard`'s FVK byte for byte. This checks `PRF^expand`,
    /// `ToBase`, `ToScalar`, byte order and the 96-byte FVK layout.
    #[test]
    fn legacy_path_matches_orchard() {
        let mut rng = StdRng::seed_from_u64(2);
        for _ in 0..32 {
            let sk = random_spending_key(&mut rng);
            let expected = FullViewingKey::from(&sk).to_bytes();

            let nk = pallas::Base::from_uniform_bytes(&prf_expand(sk.to_bytes(), &[&[0x07]]));
            let rivk = pallas::Scalar::from_uniform_bytes(&prf_expand(sk.to_bytes(), &[&[0x08]]));
            let mut ours = [0u8; 96];
            ours[..32].copy_from_slice(&expected[..32]); // ak, as orchard derives it
            ours[32..64].copy_from_slice(&nk.to_repr());
            ours[64..].copy_from_slice(&rivk.to_repr());

            assert_eq!(ours, expected);
        }
    }

    #[test]
    fn use_qsk_keeps_ak_and_nk_but_changes_rivk() {
        let mut rng = StdRng::seed_from_u64(3);
        let orchard_sk = random_spending_key(&mut rng);
        let legacy = FullViewingKey::from(&orchard_sk).to_bytes();
        let ak: [u8; 32] = legacy[..32].try_into().unwrap();

        // Same sk for both paths, so nk must agree; rivk must not.
        let keys =
            VaultKeys::derive(&VaultSecret::from_bytes(*orchard_sk.to_bytes()), &ak).unwrap();
        let ours = keys.fvk().to_bytes();
        assert_eq!(ours[..64], legacy[..64]);
        assert_ne!(ours[64..], legacy[64..]);
    }

    #[test]
    fn derivation_is_deterministic_and_depends_on_ak() {
        let mut rng = StdRng::seed_from_u64(4);
        let ak1: [u8; 32] = FullViewingKey::from(&random_spending_key(&mut rng)).to_bytes()[..32]
            .try_into()
            .unwrap();
        let ak2: [u8; 32] = FullViewingKey::from(&random_spending_key(&mut rng)).to_bytes()[..32]
            .try_into()
            .unwrap();
        let sk = VaultSecret::from_bytes([7u8; 32]);

        let a = VaultKeys::derive(&sk, &ak1).unwrap();
        let b = VaultKeys::derive(&sk, &ak1).unwrap();
        let c = VaultKeys::derive(&sk, &ak2).unwrap();
        assert_eq!(a.fvk().to_bytes(), b.fvk().to_bytes());
        assert_eq!(a.nk(), c.nk());
        assert_ne!(a.rivk_ext(), c.rivk_ext());
    }

    #[test]
    fn debug_output_shows_no_secret_component() {
        let mut rng = StdRng::seed_from_u64(4);
        let ak: [u8; 32] = FullViewingKey::from(&random_spending_key(&mut rng)).to_bytes()[..32]
            .try_into()
            .unwrap();
        let keys = VaultKeys::derive(&VaultSecret::from_bytes([7u8; 32]), &ak).unwrap();
        let shown = format!("{keys:?}");
        assert!(shown.contains(&hex::encode(ak)), "{shown}");
        for secret in ["nk", "qk", "rivk", "fvk"] {
            assert!(!shown.contains(secret), "{shown}");
        }
    }

    #[test]
    fn rejects_odd_or_invalid_ak() {
        let sk = VaultSecret::from_bytes([1u8; 32]);
        let mut odd = [0u8; 32];
        odd[0] = 1;
        odd[31] = 0x80;
        assert_eq!(VaultKeys::derive(&sk, &odd).unwrap_err(), KeyError::OddAk);
        // The identity encoding is rejected by orchard.
        assert_eq!(
            VaultKeys::derive(&sk, &[0u8; 32]).unwrap_err(),
            KeyError::InvalidFvk
        );
    }

    #[test]
    fn ufvk_is_orchard_only() {
        let mut rng = StdRng::seed_from_u64(5);
        let ak: [u8; 32] = FullViewingKey::from(&random_spending_key(&mut rng)).to_bytes()[..32]
            .try_into()
            .unwrap();
        let keys = VaultKeys::derive(&VaultSecret::from_bytes([9u8; 32]), &ak).unwrap();
        let ufvk = keys.ufvk().unwrap();
        assert!(ufvk.orchard().is_some());
        assert!(ufvk.sapling().is_none());
        assert_eq!(ufvk.orchard().unwrap().to_bytes(), keys.fvk().to_bytes());
    }
}
