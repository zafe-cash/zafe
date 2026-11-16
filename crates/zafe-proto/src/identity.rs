use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use hpke::{kem::X25519HkdfSha256, Deserializable, Kem as _, Serializable};
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{version::Format, ProtoError};

pub(crate) type Kem = X25519HkdfSha256;

const PERSONAL_FINGERPRINT: &[u8; 16] = b"Zafe_IdentityFP_";

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct IdentityPublic {
    pub sig_pk: [u8; 32],
    pub enc_pk: [u8; 32],
}

impl IdentityPublic {
    pub fn verify(&self, message: &[u8], signature: &[u8]) -> Result<(), ProtoError> {
        let key = VerifyingKey::from_bytes(&self.sig_pk).map_err(|_| ProtoError::BadKey)?;
        let sig = Signature::from_slice(signature).map_err(|_| ProtoError::BadSignature)?;
        key.verify_strict(message, &sig)
            .map_err(|_| ProtoError::BadSignature)
    }

    pub(crate) fn hpke_public(&self) -> Result<<Kem as hpke::Kem>::PublicKey, ProtoError> {
        <Kem as hpke::Kem>::PublicKey::from_bytes(&self.enc_pk).map_err(|_| ProtoError::BadKey)
    }

    pub fn fingerprint(&self) -> [u8; 32] {
        let mut state = blake2b_simd::Params::new()
            .hash_length(32)
            .personal(PERSONAL_FINGERPRINT)
            .to_state();
        state.update(&self.sig_pk);
        state.update(&self.enc_pk);
        state.finalize().as_bytes().try_into().expect("32 bytes")
    }
}

#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct IdentitySeeds {
    pub sig_seed: [u8; 32],
    pub enc_seed: [u8; 32],
}

impl IdentitySeeds {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut raw = zeroize::Zeroizing::new([0u8; 64]);
        raw[..32].copy_from_slice(&self.sig_seed);
        raw[32..].copy_from_slice(&self.enc_seed);
        crate::version::frame(Format::IdentitySeeds, raw.as_ref())
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtoError> {
        let raw = crate::version::unframe(Format::IdentitySeeds, bytes)?;
        if raw.len() != 64 {
            return Err(ProtoError::Encoding);
        }
        Ok(Self {
            sig_seed: raw[..32].try_into().expect("32 bytes"),
            enc_seed: raw[32..].try_into().expect("32 bytes"),
        })
    }
}

pub struct Identity {
    seeds: IdentitySeeds,
    signing: SigningKey,
    enc_sk: <Kem as hpke::Kem>::PrivateKey,
    public: IdentityPublic,
}

impl Identity {
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let mut seeds = IdentitySeeds {
            sig_seed: [0; 32],
            enc_seed: [0; 32],
        };
        rng.fill_bytes(&mut seeds.sig_seed);
        rng.fill_bytes(&mut seeds.enc_seed);
        Self::from_seeds(seeds)
    }

    pub fn from_seeds(seeds: IdentitySeeds) -> Self {
        let signing = SigningKey::from_bytes(&seeds.sig_seed);
        let (enc_sk, enc_pk) = Kem::derive_keypair(&seeds.enc_seed);
        let public = IdentityPublic {
            sig_pk: signing.verifying_key().to_bytes(),
            enc_pk: enc_pk.to_bytes().into(),
        };
        Self {
            seeds,
            signing,
            enc_sk,
            public,
        }
    }

    pub fn seeds(&self) -> &IdentitySeeds {
        &self.seeds
    }

    pub fn public(&self) -> &IdentityPublic {
        &self.public
    }

    pub fn sign(&self, message: &[u8]) -> [u8; 64] {
        self.signing.sign(message).to_bytes()
    }

    pub(crate) fn hpke_private(&self) -> &<Kem as hpke::Kem>::PrivateKey {
        &self.enc_sk
    }
}

impl core::fmt::Debug for Identity {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Identity")
            .field("public", &self.public)
            .finish_non_exhaustive()
    }
}
