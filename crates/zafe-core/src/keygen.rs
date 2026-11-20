use std::collections::BTreeMap;

use rand_core::{CryptoRng, RngCore};
use reddsa::frost::redpallas::{
    self,
    keys::{dkg, KeyPackage, PublicKeyPackage},
    Identifier,
};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::keys::{KeyError, VaultKeys, VaultSecret};

pub const MAX_MEMBERS: u16 = 15;

const PERSONAL_ECHO: &[u8; 16] = b"Zafe_DKG_R1Echo_";
const PERSONAL_TRANSCRIPT: &[u8; 16] = b"Zafe_DKG_Transcr";
const PERSONAL_VAULT_SK: &[u8; 16] = b"Zafe_VaultSecret";
const PERSONAL_SK_COMMIT: &[u8; 16] = b"Zafe_SkCommit___";
const PERSONAL_ECHO_COMMITS: &[u8; 16] = b"Zafe_DKG_EchoSk_";
const PERSONAL_ECHO_PQ: &[u8; 16] = b"Zafe_DKG_EchoPq_";
const PERSONAL_SK_SEAL: &[u8] = b"Zafe sk contribution v1";
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeygenParams {
    pub vault_id: VaultId,
    pub min_signers: u16,
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

    pub fn secret_bytes(&self) -> Result<Vec<u8>, KeygenError> {
        Ok(self.secret.serialize()?)
    }

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

pub struct Round2 {
    params: KeygenParams,
    identifier: Identifier,
    secret: dkg::round2::SecretPackage,
    all_round1: BTreeMap<Identifier, dkg::round1::Package>,
    echo: [u8; 32],
}

impl Round2 {
    pub fn echo(&self) -> [u8; 32] {
        self.echo
    }

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

pub struct KeygenOutput {
    pub key_package: KeyPackage,
    pub public_key_package: PublicKeyPackage,
    pub vault_secret: VaultSecret,
    pub vault_keys: VaultKeys,
    pub transcript_hash: [u8; 32],
}

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

    pub fn commitment(&self, vault_id: &VaultId, member: &[u8; 32]) -> [u8; 32] {
        let mut state = blake2b_256(PERSONAL_SK_COMMIT);
        state.update(vault_id);
        state.update(member);
        state.update(&self.0);
        finalize_32(state)
    }
}

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

pub fn echo_with_pq_keys(echo: &[u8; 32], pq_keys: &BTreeMap<Identifier, [u8; 32]>) -> [u8; 32] {
    let mut state = blake2b_256(PERSONAL_ECHO_PQ);
    state.update(echo);
    for (id, key_hash) in pq_keys {
        let id_bytes = id.serialize();
        state.update(&(id_bytes.len() as u32).to_le_bytes());
        state.update(&id_bytes);
        state.update(key_hash);
    }
    finalize_32(state)
}

pub fn contribution_context(vault_id: &VaultId, from: &[u8; 32], to: &[u8; 32]) -> Vec<u8> {
    let mut context = PERSONAL_SK_SEAL.to_vec();
    context.extend_from_slice(vault_id);
    context.extend_from_slice(from);
    context.extend_from_slice(to);
    context
}

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

pub fn round1_echo(
    params: &KeygenParams,
    all_round1: &BTreeMap<Identifier, dkg::round1::Package>,
) -> Result<[u8; 32], KeygenError> {
    let mut state = blake2b_256(PERSONAL_ECHO);
    hash_params_and_round1(&mut state, params, all_round1)?;
    Ok(finalize_32(state))
}

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
