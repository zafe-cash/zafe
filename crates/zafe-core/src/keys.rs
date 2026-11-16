use ff::{FromUniformBytes, PrimeField};
use orchard::keys::FullViewingKey;
use pasta_curves::pallas;
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_spec::PrfExpand;
use zeroize::{Zeroize, ZeroizeOnDrop};

pub const QK_CONTEXT: &str = "Zcash ZIP 2005 qk-derivation v1";

const DOMAIN_QSK: u8 = 0x0C;
const DOMAIN_RIVK_EXT: u8 = 0x0D;

const PRF_EXPAND_PERSONALIZATION: &[u8; 16] = b"Zcash_ExpandSeed";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum KeyError {
    #[error("ak does not have an even y-coordinate")]
    OddAk,
    #[error("invalid Orchard full viewing key components")]
    InvalidFvk,
    #[error("failed to build unified full viewing key: {0}")]
    Ufvk(String),
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct VaultSecret([u8; 32]);

impl VaultSecret {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

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

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct QuantumSpendingKey([u8; 32]);

impl QuantumSpendingKey {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn qk(&self) -> [u8; 32] {
        blake3::derive_key(QK_CONTEXT, &self.0)
    }
}

impl core::fmt::Debug for QuantumSpendingKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("QuantumSpendingKey(<redacted>)")
    }
}

#[derive(Clone)]
pub struct VaultKeys {
    ak: [u8; 32],
    nk: pallas::Base,
    qk: [u8; 32],
    rivk_ext: pallas::Scalar,
    fvk: FullViewingKey,
}

impl core::fmt::Debug for VaultKeys {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VaultKeys")
            .field("ak", &hex::encode(self.ak))
            .finish_non_exhaustive()
    }
}

impl VaultKeys {
    pub fn derive(sk: &VaultSecret, ak: &[u8; 32]) -> Result<Self, KeyError> {
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

    pub fn qk(&self) -> &[u8; 32] {
        &self.qk
    }

    pub fn rivk_ext(&self) -> pallas::Scalar {
        self.rivk_ext
    }

    pub fn fvk(&self) -> &FullViewingKey {
        &self.fvk
    }

    pub fn ufvk(&self) -> Result<UnifiedFullViewingKey, KeyError> {
        UnifiedFullViewingKey::from_orchard_fvk(self.fvk.clone())
            .map_err(|e| KeyError::Ufvk(format!("{e:?}")))
    }
}

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

    #[test]
    fn legacy_path_matches_orchard() {
        let mut rng = StdRng::seed_from_u64(2);
        for _ in 0..32 {
            let sk = random_spending_key(&mut rng);
            let expected = FullViewingKey::from(&sk).to_bytes();

            let nk = pallas::Base::from_uniform_bytes(&prf_expand(sk.to_bytes(), &[&[0x07]]));
            let rivk = pallas::Scalar::from_uniform_bytes(&prf_expand(sk.to_bytes(), &[&[0x08]]));
            let mut ours = [0u8; 96];
            ours[..32].copy_from_slice(&expected[..32]);
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
