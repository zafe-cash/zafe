//! Vault key generation: FROST DKG plus agreement on the vault secret `sk` (spec §7).
//!
//! This module is transport-agnostic. Each member drives its own state through
//! [`Round1`] → [`Round2`] → [`KeygenOutput`], and the caller is responsible for:
//!
//! - broadcasting round-1 packages and comparing [`round1_echo`] hashes across members
//!   (any mismatch means the relay showed members different packages: abort);
//! - sealing each round-2 package and each `sk` contribution to its recipient;
//! - collecting every member's signature over the final vault descriptor.
//!
//! The group key is normalized to even Y by `reddsa`'s `post_dkg` hook, as ZIP 2005 requires.

use std::collections::BTreeMap;

use rand_core::{CryptoRng, RngCore};
use reddsa::frost::redpallas::{
    self,
    keys::{dkg, KeyPackage, PublicKeyPackage},
    Identifier,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::keys::{KeyError, VaultKeys, VaultSecret};

/// Largest vault size supported in v1 (spec §2.1, D1).
pub const MAX_MEMBERS: u16 = 15;

const PERSONAL_ECHO: &[u8; 16] = b"Zafe_DKG_R1Echo_";
const PERSONAL_TRANSCRIPT: &[u8; 16] = b"Zafe_DKG_Transcr";
const PERSONAL_VAULT_SK: &[u8; 16] = b"Zafe_VaultSecret";
const PERSONAL_SK_COMMIT: &[u8; 16] = b"Zafe_SkCommit___";
const PERSONAL_ECHO_COMMITS: &[u8; 16] = b"Zafe_DKG_EchoSk_";
const PERSONAL_IDENTIFIER: &[u8] = b"Zafe member identifier v1";

pub type VaultId = [u8; 16];

#[derive(Debug, thiserror::Error)]
pub enum KeygenError {
    #[error("invalid parameters: need 2 <= t <= n <= {MAX_MEMBERS}, got t={t} n={n}")]
    InvalidParams { t: u16, n: u16 },
    #[error("expected packages from {expected} other members, got {got}")]
    WrongPackageCount { expected: usize, got: usize },
    #[error("expected {expected} sk contributions, got {got}")]
    WrongContributionCount { expected: usize, got: usize },
    #[error("received a package claiming to be from this member")]
    OwnPackageReceived,
    #[error("a member's vault secret contribution doesn't match its round-1 commitment")]
    ContributionMismatch,
    #[error("FROST: {0}")]
    Frost(#[from] redpallas::Error),
    #[error(transparent)]
    Keys(#[from] KeyError),
}

/// Parameters every member must agree on before starting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeygenParams {
    pub vault_id: VaultId,
    /// Threshold `t`.
    pub min_signers: u16,
    /// Number of members `n`.
    pub max_signers: u16,
}

impl KeygenParams {
    pub fn new(vault_id: VaultId, min_signers: u16, max_signers: u16) -> Result<Self, KeygenError> {
        if min_signers < 2 || min_signers > max_signers || max_signers > MAX_MEMBERS {
            return Err(KeygenError::InvalidParams {
                t: min_signers,
                n: max_signers,
            });
        }
        Ok(Self {
            vault_id,
            min_signers,
            max_signers,
        })
    }
}

/// Derives a member's FROST identifier from their identity signing key and the vault id,
/// binding the share to the identity (spec §5.1).
pub fn member_identifier(
    identity_pk: &[u8],
    vault_id: &VaultId,
) -> Result<Identifier, KeygenError> {
    let mut input =
        Vec::with_capacity(PERSONAL_IDENTIFIER.len() + vault_id.len() + identity_pk.len());
    input.extend_from_slice(PERSONAL_IDENTIFIER);
    input.extend_from_slice(vault_id);
    input.extend_from_slice(identity_pk);
    Ok(Identifier::derive(&input)?)
}

/// State after DKG part 1. `package` must be broadcast to all other members.
pub struct Round1 {
    params: KeygenParams,
    identifier: Identifier,
    secret: dkg::round1::SecretPackage,
    package: dkg::round1::Package,
}

impl Round1 {
    pub fn start<R: RngCore + CryptoRng>(
        params: KeygenParams,
        identifier: Identifier,
        rng: &mut R,
    ) -> Result<Self, KeygenError> {
        let (secret, package) = dkg::part1(
            identifier,
            params.max_signers,
            params.min_signers,
            &mut *rng,
        )?;
        Ok(Self {
            params,
            identifier,
            secret,
            package,
        })
    }

    pub fn identifier(&self) -> Identifier {
        self.identifier
    }

    /// This member's round-1 secret, to keep while a ceremony is paused (it is this
    /// member's whole contribution to the group key: store it encrypted, never send it).
    pub fn secret_bytes(&self) -> Result<Vec<u8>, KeygenError> {
        Ok(self.secret.serialize()?)
    }

    /// A round 1 saved with [`Round1::secret_bytes`] and its package. Part 2 is
    /// deterministic, so a resumed round 1 sends the same round-2 packages again.
    pub fn restore(
        params: KeygenParams,
        identifier: Identifier,
        secret: &[u8],
        package: &[u8],
    ) -> Result<Self, KeygenError> {
        Ok(Self {
            params,
            identifier,
            secret: dkg::round1::SecretPackage::deserialize(secret)?,
            package: dkg::round1::Package::deserialize(package)?,
        })
    }

    pub fn package(&self) -> &dkg::round1::Package {
        &self.package
    }

    /// Runs DKG part 2 on the round-1 packages received from the *other* members.
    /// Returns the next state and the round-2 packages to seal to each recipient.
    pub fn advance(
        self,
        received: BTreeMap<Identifier, dkg::round1::Package>,
    ) -> Result<(Round2, BTreeMap<Identifier, dkg::round2::Package>), KeygenError> {
        check_others(&self.identifier, &received, self.params.max_signers)?;
        let (secret, outgoing) = dkg::part2(self.secret, &received)?;

        let mut all_round1 = received;
        all_round1.insert(self.identifier, self.package);
        let echo = round1_echo(&self.params, &all_round1)?;

        Ok((
            Round2 {
                params: self.params,
                identifier: self.identifier,
                secret,
                all_round1,
                echo,
            },
            outgoing,
        ))
    }
}

/// State after DKG part 2.
pub struct Round2 {
    params: KeygenParams,
    identifier: Identifier,
    secret: dkg::round2::SecretPackage,
    /// Round-1 packages from every member, including this one.
    all_round1: BTreeMap<Identifier, dkg::round1::Package>,
    echo: [u8; 32],
}

impl Round2 {
    /// This member's echo hash over all round-1 packages. Broadcast it and abort unless
    /// every member reports the same value.
    pub fn echo(&self) -> [u8; 32] {
        self.echo
    }

    /// Runs DKG part 3 on the round-2 packages received from the other members.
    pub fn finish(
        self,
        received: BTreeMap<Identifier, dkg::round2::Package>,
    ) -> Result<DkgResult, KeygenError> {
        check_others(&self.identifier, &received, self.params.max_signers)?;
        let others: BTreeMap<_, _> = self
            .all_round1
            .iter()
            .filter(|(id, _)| **id != self.identifier)
            .map(|(id, pkg)| (*id, pkg.clone()))
            .collect();
        let (key_package, public_key_package) = dkg::part3(&self.secret, &others, &received)?;
        let transcript_hash = transcript_hash(&self.params, &self.all_round1, &public_key_package)?;
        Ok(DkgResult {
            params: self.params,
            key_package,
            public_key_package,
            transcript_hash,
        })
    }
}

/// FROST key material after the DKG, before `sk` agreement.
pub struct DkgResult {
    params: KeygenParams,
    key_package: KeyPackage,
    public_key_package: PublicKeyPackage,
    transcript_hash: [u8; 32],
}

impl DkgResult {
    pub fn transcript_hash(&self) -> [u8; 32] {
        self.transcript_hash
    }

    pub fn public_key_package(&self) -> &PublicKeyPackage {
        &self.public_key_package
    }

    /// Combines every member's `sk` contribution (including this member's own) into the
    /// vault secret and derives the vault keys.
    pub fn finish(
        self,
        contributions: &BTreeMap<Identifier, SkContribution>,
    ) -> Result<KeygenOutput, KeygenError> {
        let expected = usize::from(self.params.max_signers);
        if contributions.len() != expected {
            return Err(KeygenError::WrongContributionCount {
                expected,
                got: contributions.len(),
            });
        }
        let vault_secret =
            combine_vault_secret(&self.params.vault_id, &self.transcript_hash, contributions);
        let ak = group_key_bytes(&self.public_key_package)?;
        let vault_keys = VaultKeys::derive(&vault_secret, &ak)?;
        Ok(KeygenOutput {
            key_package: self.key_package,
            public_key_package: self.public_key_package,
            vault_secret,
            vault_keys,
            transcript_hash: self.transcript_hash,
        })
    }
}

/// Everything a member holds after vault creation.
pub struct KeygenOutput {
    pub key_package: KeyPackage,
    pub public_key_package: PublicKeyPackage,
    pub vault_secret: VaultSecret,
    pub vault_keys: VaultKeys,
    pub transcript_hash: [u8; 32],
}

/// One member's random contribution to `sk`. Sealed to every other member.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct SkContribution([u8; 32]);

impl SkContribution {
    pub fn generate<R: RngCore + CryptoRng>(rng: &mut R) -> Self {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        Self(bytes)
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// `BLAKE2b-256("Zafe_SkCommit___", vault_id || member sig_pk || r)`: published in DKG
    /// round 1, before anyone reveals a contribution, so no member can pick its `r` after
    /// seeing the others' (which would let the last one grind `sk`).
    pub fn commitment(&self, vault_id: &VaultId, member: &[u8; 32]) -> [u8; 32] {
        let mut state = blake2b_256(PERSONAL_SK_COMMIT);
        state.update(vault_id);
        state.update(member);
        state.update(&self.0);
        finalize_32(state)
    }
}

/// Checks a revealed `sk` contribution against the commitment `member` published in round 1.
pub fn check_contribution(
    vault_id: &VaultId,
    member: &[u8; 32],
    commitment: &[u8; 32],
    revealed: &SkContribution,
) -> Result<(), KeygenError> {
    if revealed.commitment(vault_id, member) == *commitment {
        Ok(())
    } else {
        Err(KeygenError::ContributionMismatch)
    }
}

/// The round-2 echo every member compares: the round-1 echo extended with every member's
/// `sk` commitment (by FROST identifier), so a member who sent different commitments to
/// different members is caught like a different round-1 package.
pub fn echo_with_commitments(
    round1_echo: &[u8; 32],
    commitments: &BTreeMap<Identifier, [u8; 32]>,
) -> [u8; 32] {
    let mut state = blake2b_256(PERSONAL_ECHO_COMMITS);
    state.update(round1_echo);
    for (id, c) in commitments {
        let id_bytes = id.serialize();
        state.update(&(id_bytes.len() as u32).to_le_bytes());
        state.update(&id_bytes);
        state.update(c);
    }
    finalize_32(state)
}

/// `sk = BLAKE2b-256("Zafe_VaultSecret", vault_id || transcript || r_1 || … || r_n)`,
/// contributions ordered by FROST identifier (spec §7.4).
pub fn combine_vault_secret(
    vault_id: &VaultId,
    transcript_hash: &[u8; 32],
    contributions: &BTreeMap<Identifier, SkContribution>,
) -> VaultSecret {
    let mut state = blake2b_256(PERSONAL_VAULT_SK);
    state.update(vault_id);
    state.update(transcript_hash);
    for contribution in contributions.values() {
        state.update(contribution.as_bytes());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(state.finalize().as_bytes());
    VaultSecret::from_bytes(out)
}

/// Hash of every member's round-1 package (including this member's own), ordered by
/// identifier. All members must agree on it (the broadcast-consistency check, spec §7.2).
pub fn round1_echo(
    params: &KeygenParams,
    all_round1: &BTreeMap<Identifier, dkg::round1::Package>,
) -> Result<[u8; 32], KeygenError> {
    let mut state = blake2b_256(PERSONAL_ECHO);
    hash_params_and_round1(&mut state, params, all_round1)?;
    Ok(finalize_32(state))
}

/// Binds the vault id, parameters, all round-1 packages and the resulting group key.
fn transcript_hash(
    params: &KeygenParams,
    all_round1: &BTreeMap<Identifier, dkg::round1::Package>,
    public_key_package: &PublicKeyPackage,
) -> Result<[u8; 32], KeygenError> {
    let mut state = blake2b_256(PERSONAL_TRANSCRIPT);
    hash_params_and_round1(&mut state, params, all_round1)?;
    state.update(&group_key_bytes(public_key_package)?);
    Ok(finalize_32(state))
}

fn hash_params_and_round1(
    state: &mut blake2b_simd::State,
    params: &KeygenParams,
    all_round1: &BTreeMap<Identifier, dkg::round1::Package>,
) -> Result<(), KeygenError> {
    state.update(&params.vault_id);
    state.update(&params.min_signers.to_le_bytes());
    state.update(&params.max_signers.to_le_bytes());
    for (id, package) in all_round1 {
        let id_bytes = id.serialize();
        let package_bytes = package.serialize()?;
        state.update(&(id_bytes.len() as u32).to_le_bytes());
        state.update(&id_bytes);
        state.update(&(package_bytes.len() as u32).to_le_bytes());
        state.update(&package_bytes);
    }
    Ok(())
}

/// `I2LEOSP_256(ak)`: the 32-byte encoding of the group verifying key.
pub fn group_key_bytes(public_key_package: &PublicKeyPackage) -> Result<[u8; 32], KeygenError> {
    let bytes = public_key_package.verifying_key().serialize()?;
    bytes
        .try_into()
        .map_err(|_| KeygenError::Keys(KeyError::InvalidFvk))
}

fn check_others<T>(
    own: &Identifier,
    received: &BTreeMap<Identifier, T>,
    max_signers: u16,
) -> Result<(), KeygenError> {
    if received.contains_key(own) {
        return Err(KeygenError::OwnPackageReceived);
    }
    let expected = usize::from(max_signers) - 1;
    if received.len() != expected {
        return Err(KeygenError::WrongPackageCount {
            expected,
            got: received.len(),
        });
    }
    Ok(())
}

fn blake2b_256(personal: &[u8; 16]) -> blake2b_simd::State {
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(personal)
        .to_state()
}

fn finalize_32(state: blake2b_simd::State) -> [u8; 32] {
    let mut out = [0u8; 32];
    out.copy_from_slice(state.finalize().as_bytes());
    out
}
