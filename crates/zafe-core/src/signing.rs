use std::collections::BTreeMap;

use pasta_curves::pallas;
use rand_core::{CryptoRng, RngCore};
use reddsa::frost::redpallas::{
    self,
    keys::{KeyPackage, PublicKeyPackage},
    rerandomized::{RandomizedParams, Randomizer},
    round1::{SigningCommitments, SigningNonces},
    round2::SignatureShare,
    Identifier, PallasBlake2b512, SigningPackage,
};

#[derive(Debug, thiserror::Error)]
pub enum SigningError {
    #[error("FROST: {0}")]
    Frost(#[from] redpallas::Error),
    #[error("signature encoding")]
    Encoding,
}

pub fn commit<R: RngCore + CryptoRng>(
    key_package: &KeyPackage,
    rng: &mut R,
) -> (SigningNonces, SigningCommitments) {
    redpallas::round1::commit(key_package.signing_share(), rng)
}

pub fn signing_package(
    commitments: BTreeMap<Identifier, SigningCommitments>,
    sighash: &[u8; 32],
) -> SigningPackage {
    SigningPackage::new(commitments, sighash)
}

pub fn sign(
    signing_package: &SigningPackage,
    nonces: SigningNonces,
    key_package: &KeyPackage,
    alpha: pallas::Scalar,
) -> Result<SignatureShare, SigningError> {
    #[allow(deprecated)]
    let share = frost_rerandomized::sign::<PallasBlake2b512>(
        signing_package,
        &nonces,
        key_package,
        Randomizer::from_scalar(alpha),
    )?;
    Ok(share)
}

pub fn aggregate(
    signing_package: &SigningPackage,
    shares: &BTreeMap<Identifier, SignatureShare>,
    public_key_package: &PublicKeyPackage,
    alpha: pallas::Scalar,
) -> Result<[u8; 64], SigningError> {
    let params = RandomizedParams::from_randomizer(
        public_key_package.verifying_key(),
        Randomizer::from_scalar(alpha),
    );
    let signature =
        redpallas::rerandomized::aggregate(signing_package, shares, public_key_package, &params)?;
    signature
        .serialize()?
        .try_into()
        .map_err(|_| SigningError::Encoding)
}
