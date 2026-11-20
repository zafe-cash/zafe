use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use ml_kem::{
    Ciphertext, Decapsulate, DecapsulationKey768, EncapsulationKey768, Key, KeyExport, MlKem768,
    Seed, B32,
};
use rand::{CryptoRng, RngCore};
use zafe_proto::identity::IdentitySeeds;
use zeroize::Zeroizing;

pub const PUBLIC_KEY_LEN: usize = 1184;
pub const CIPHERTEXT_LEN: usize = 1088;
const NONCE_LEN: usize = 24;
const PERSONAL_SEED: &[u8; 16] = b"Zafe_PqKemSeed__";
const PERSONAL_SEAL: &[u8; 16] = b"Zafe_PqSealKey__";
const PERSONAL_KEY_ID: &[u8; 16] = b"Zafe_PqKeyHash__";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PqError {
    #[error("not a valid ML-KEM-768 public key")]
    BadPublicKey,
    #[error("post-quantum sealed payload is malformed or was not sealed to this key")]
    Open,
}

#[derive(Clone, PartialEq, Eq)]
pub struct PqPublic(EncapsulationKey768);

impl core::fmt::Debug for PqPublic {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PqPublic({})", hex::encode(&self.hash()[..8]))
    }
}

impl PqPublic {
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, PqError> {
        let key: &Key<EncapsulationKey768> = bytes.try_into().map_err(|_| PqError::BadPublicKey)?;
        EncapsulationKey768::new(key)
            .map(Self)
            .map_err(|_| PqError::BadPublicKey)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        self.0.to_bytes().to_vec()
    }

    pub fn hash(&self) -> [u8; 32] {
        blake2b_simd::Params::new()
            .hash_length(32)
            .personal(PERSONAL_KEY_ID)
            .hash(&self.to_bytes())
            .as_bytes()
            .try_into()
            .expect("32 bytes")
    }

    pub fn seal<R: RngCore + CryptoRng>(
        &self,
        context: &[u8],
        plaintext: &[u8],
        rng: &mut R,
    ) -> Vec<u8> {
        let mut m = Zeroizing::new(B32::default());
        rng.fill_bytes(&mut m);
        let (ct, shared) = self.0.encapsulate_deterministic(&m);
        let shared = Zeroizing::new(shared);
        let key = seal_key(shared.as_slice(), &ct, &self.hash(), context);
        let mut nonce = [0u8; NONCE_LEN];
        rng.fill_bytes(&mut nonce);
        let body = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key.as_ref()))
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plaintext,
                    aad: context,
                },
            )
            .expect("XChaCha20-Poly1305 encryption cannot fail");
        let mut out = Vec::with_capacity(CIPHERTEXT_LEN + NONCE_LEN + body.len());
        out.extend_from_slice(&ct);
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&body);
        out
    }
}

pub struct PqKeys {
    dk: DecapsulationKey768,
    public: PqPublic,
}

impl PqKeys {
    pub fn derive(seeds: &IdentitySeeds) -> Self {
        let mut seed = Zeroizing::new(Seed::default());
        let hash = blake2b_simd::Params::new()
            .hash_length(64)
            .personal(PERSONAL_SEED)
            .to_state()
            .update(&seeds.sig_seed)
            .update(&seeds.enc_seed)
            .finalize();
        seed.copy_from_slice(hash.as_bytes());
        let dk = DecapsulationKey768::from_seed(*seed);
        let public = PqPublic(dk.encapsulation_key().clone());
        Self { dk, public }
    }

    pub fn public(&self) -> &PqPublic {
        &self.public
    }

    pub fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Vec<u8>, PqError> {
        if sealed.len() < CIPHERTEXT_LEN + NONCE_LEN {
            return Err(PqError::Open);
        }
        let (ct, rest) = sealed.split_at(CIPHERTEXT_LEN);
        let (nonce, body) = rest.split_at(NONCE_LEN);
        let ct: Ciphertext<MlKem768> = ct.try_into().map_err(|_| PqError::Open)?;
        let shared = Zeroizing::new(self.dk.decapsulate(&ct));
        let key = seal_key(shared.as_slice(), &ct, &self.public.hash(), context);
        XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key.as_ref()))
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: body,
                    aad: context,
                },
            )
            .map_err(|_| PqError::Open)
    }
}

fn seal_key(shared: &[u8], ct: &[u8], key_hash: &[u8; 32], context: &[u8]) -> Zeroizing<[u8; 32]> {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(PERSONAL_SEAL)
        .to_state()
        .update(shared)
        .update(ct)
        .update(key_hash)
        .update(&(context.len() as u64).to_le_bytes())
        .update(context)
        .finalize();
    let mut key = Zeroizing::new([0u8; 32]);
    key.copy_from_slice(hash.as_bytes());
    key
}
