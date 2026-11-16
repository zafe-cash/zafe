use std::collections::{BTreeMap, BTreeSet};

use orchard::keys::FullViewingKey;
use pczt::Pczt;
use rand_core::{CryptoRng, RngCore};
use reddsa::frost::redpallas::{
    keys::{KeyPackage, PublicKeyPackage},
    round1::{SigningCommitments, SigningNonces},
    round2::SignatureShare,
    Identifier, SigningPackage,
};

use crate::{
    signing::{self, SigningError},
    tx::SpendToSign,
    verify::{verify_pczt, Expectations, VerifiedTx, VerifyError},
};

const PERSONAL_PCZT_HASH: &[u8; 16] = b"Zafe_PCZT_Hash__";

pub type ProposalId = [u8; 16];

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error(transparent)]
    Verify(#[from] VerifyError),
    #[error(transparent)]
    Signing(#[from] SigningError),
    #[error("PCZT serialization: {0}")]
    Encoding(String),
    #[error("the request is for a different PCZT than the proposal")]
    PcztMismatch,
    #[error("this member already approved this proposal")]
    AlreadyApproved,
    #[error("no unused nonces for this proposal (already signed, or never approved)")]
    NoNonces,
    #[error("request has {got} signing packages, but the PCZT has {expected} spends to sign")]
    WrongPackageCount { expected: usize, got: usize },
    #[error("signing package {0} does not sign the locally computed sighash")]
    WrongMessage(usize),
    #[error("signing package {0} does not contain this member's commitments")]
    NotOurCommitments(usize),
    #[error("need {needed} signers, got {got}")]
    NotEnoughSigners { needed: usize, got: usize },
    #[error("member has no commitments for this proposal")]
    UnknownApprover,
    #[error("share set from member does not cover every spend")]
    IncompleteShares,
    #[error("nonce storage: {0}")]
    Storage(String),
}

pub fn pczt_hash(pczt: &Pczt) -> Result<[u8; 32], SessionError> {
    let bytes = pczt
        .clone()
        .serialize()
        .map_err(|e| SessionError::Encoding(format!("{e:?}")))?;
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(PERSONAL_PCZT_HASH)
        .hash(&bytes);
    Ok(hash.as_bytes().try_into().expect("32 bytes"))
}

pub trait NonceStore {
    fn contains(&self, proposal: &ProposalId, pczt_hash: &[u8; 32]) -> bool;
    fn commitments(
        &self,
        proposal: &ProposalId,
        pczt_hash: &[u8; 32],
    ) -> Option<Vec<SigningCommitments>>;
    fn put(
        &mut self,
        proposal: ProposalId,
        pczt_hash: [u8; 32],
        nonces: Vec<SigningNonces>,
    ) -> Result<(), SessionError>;
    fn take(&mut self, proposal: &ProposalId, pczt_hash: &[u8; 32]) -> Option<Vec<SigningNonces>>;
    fn claimed(&self, proposal: &ProposalId, pczt_hash: &[u8; 32]) -> bool;

    fn keep_reply(&mut self, request_hash: &[u8; 32], reply: &[u8]) -> Result<(), SessionError>;
    fn kept_reply(&self, request_hash: &[u8; 32]) -> Option<Vec<u8>>;
    fn drop_reply(&mut self, request_hash: &[u8; 32]);
    fn kept_replies(&self) -> Vec<Vec<u8>>;
}

pub trait PoolStore {
    fn put(&mut self, commitment: &[u8], nonces: SigningNonces) -> Result<(), SessionError>;
    fn contains(&self, commitment: &[u8]) -> bool;
    fn take(&mut self, commitment: &[u8]) -> Option<SigningNonces>;
    fn forget(&mut self, commitment: &[u8]);

    fn keep_vote(
        &mut self,
        _proposal: &ProposalId,
        _pczt_hash: &[u8; 32],
        _event: &[u8],
    ) -> Result<(), SessionError> {
        Ok(())
    }
    fn kept_vote(&self, _proposal: &ProposalId, _pczt_hash: &[u8; 32]) -> Option<Vec<u8>> {
        None
    }
    fn drop_vote(&mut self, _proposal: &ProposalId, _pczt_hash: &[u8; 32]) {}
}

#[derive(Default)]
pub struct MemoryPoolStore(
    BTreeMap<Vec<u8>, SigningNonces>,
    BTreeMap<(ProposalId, [u8; 32]), Vec<u8>>,
);

impl PoolStore for MemoryPoolStore {
    fn keep_vote(
        &mut self,
        proposal: &ProposalId,
        pczt_hash: &[u8; 32],
        event: &[u8],
    ) -> Result<(), SessionError> {
        self.1.insert((*proposal, *pczt_hash), event.to_vec());
        Ok(())
    }
    fn kept_vote(&self, proposal: &ProposalId, pczt_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.1.get(&(*proposal, *pczt_hash)).cloned()
    }
    fn drop_vote(&mut self, proposal: &ProposalId, pczt_hash: &[u8; 32]) {
        self.1.remove(&(*proposal, *pczt_hash));
    }
    fn put(&mut self, commitment: &[u8], nonces: SigningNonces) -> Result<(), SessionError> {
        self.0.insert(commitment.to_vec(), nonces);
        Ok(())
    }
    fn contains(&self, commitment: &[u8]) -> bool {
        self.0.contains_key(commitment)
    }
    fn take(&mut self, commitment: &[u8]) -> Option<SigningNonces> {
        self.0.remove(commitment)
    }
    fn forget(&mut self, commitment: &[u8]) {
        self.0.remove(commitment);
    }
}

pub fn new_pool_commitments<R: RngCore + CryptoRng>(
    key_package: &KeyPackage,
    count: usize,
    pool: &mut impl PoolStore,
    rng: &mut R,
) -> Result<Vec<Vec<u8>>, SessionError> {
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let (nonces, commitments) = signing::commit(key_package, rng);
        let bytes = commitments
            .serialize()
            .map_err(|e| SessionError::Encoding(format!("{e:?}")))?;
        pool.put(&bytes, nonces)?;
        out.push(bytes);
    }
    Ok(out)
}

pub fn group_package(
    commitments: &[Vec<u8>],
    group: &[Identifier],
    sighash: &[u8; 32],
) -> Result<SigningPackage, SessionError> {
    let map = group
        .iter()
        .zip(commitments)
        .map(|(id, c)| {
            SigningCommitments::deserialize(c)
                .map(|c| (*id, c))
                .map_err(|e| SessionError::Encoding(format!("{e:?}")))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()?;
    if map.len() != group.len() {
        return Err(SessionError::Encoding("group/commitment mismatch".into()));
    }
    Ok(signing::signing_package(map, sighash))
}

#[derive(Default)]
pub struct MemoryNonceStore(
    BTreeMap<(ProposalId, [u8; 32]), Vec<SigningNonces>>,
    BTreeMap<[u8; 32], Vec<u8>>,
);

impl NonceStore for MemoryNonceStore {
    fn contains(&self, proposal: &ProposalId, pczt_hash: &[u8; 32]) -> bool {
        self.0.contains_key(&(*proposal, *pczt_hash))
    }
    fn commitments(
        &self,
        proposal: &ProposalId,
        pczt_hash: &[u8; 32],
    ) -> Option<Vec<SigningCommitments>> {
        self.0
            .get(&(*proposal, *pczt_hash))
            .map(|nonces| nonces.iter().map(|n| *n.commitments()).collect())
    }
    fn put(
        &mut self,
        proposal: ProposalId,
        pczt_hash: [u8; 32],
        nonces: Vec<SigningNonces>,
    ) -> Result<(), SessionError> {
        self.0.insert((proposal, pczt_hash), nonces);
        Ok(())
    }
    fn take(&mut self, proposal: &ProposalId, pczt_hash: &[u8; 32]) -> Option<Vec<SigningNonces>> {
        self.0.remove(&(*proposal, *pczt_hash))
    }
    fn claimed(&self, _: &ProposalId, _: &[u8; 32]) -> bool {
        false
    }
    fn keep_reply(&mut self, request_hash: &[u8; 32], reply: &[u8]) -> Result<(), SessionError> {
        self.1.insert(*request_hash, reply.to_vec());
        Ok(())
    }
    fn kept_reply(&self, request_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.1.get(request_hash).cloned()
    }
    fn drop_reply(&mut self, request_hash: &[u8; 32]) {
        self.1.remove(request_hash);
    }
    fn kept_replies(&self) -> Vec<Vec<u8>> {
        self.1.values().cloned().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Approval {
    pub member: Identifier,
    pub proposal: ProposalId,
    pub pczt_hash: [u8; 32],
    pub commitments: Vec<SigningCommitments>,
}

pub struct Member<'a> {
    pub identifier: Identifier,
    pub key_package: &'a KeyPackage,
    pub vault_fvk: &'a FullViewingKey,
}

impl Member<'_> {
    pub fn approve<R: RngCore + CryptoRng>(
        &self,
        proposal: ProposalId,
        pczt: &Pczt,
        expected: &Expectations,
        store: &mut impl NonceStore,
        rng: &mut R,
    ) -> Result<(Approval, VerifiedTx), SessionError> {
        let verified = verify_pczt(pczt, self.vault_fvk, expected)?;
        let hash = pczt_hash(pczt)?;
        if store.contains(&proposal, &hash) {
            return Err(SessionError::AlreadyApproved);
        }
        let (nonces, commitments): (Vec<_>, Vec<_>) = verified
            .spends_to_sign
            .iter()
            .map(|_| signing::commit(self.key_package, rng))
            .unzip();
        store.put(proposal, hash, nonces)?;
        Ok((
            Approval {
                member: self.identifier,
                proposal,
                pczt_hash: hash,
                commitments,
            },
            verified,
        ))
    }

    pub fn sign(
        &self,
        request: &SigningRequest,
        pczt: &Pczt,
        expected: &Expectations,
        store: &mut impl NonceStore,
    ) -> Result<Vec<SignatureShare>, SessionError> {
        let verified = verify_pczt(pczt, self.vault_fvk, expected)?;
        if pczt_hash(pczt)? != request.pczt_hash {
            return Err(SessionError::PcztMismatch);
        }
        let spends = &verified.spends_to_sign;
        if request.packages.len() != spends.len() {
            return Err(SessionError::WrongPackageCount {
                expected: spends.len(),
                got: request.packages.len(),
            });
        }
        for (i, package) in request.packages.iter().enumerate() {
            if package.message() != verified.sighash.as_slice() {
                return Err(SessionError::WrongMessage(i));
            }
        }

        let ours = store
            .commitments(&request.proposal, &request.pczt_hash)
            .ok_or(SessionError::NoNonces)?;
        if ours.len() != spends.len() {
            return Err(SessionError::NoNonces);
        }
        for (i, (package, commitment)) in request.packages.iter().zip(&ours).enumerate() {
            if package.signing_commitment(&self.identifier) != Some(*commitment) {
                return Err(SessionError::NotOurCommitments(i));
            }
        }
        let nonces = store
            .take(&request.proposal, &request.pczt_hash)
            .ok_or(SessionError::NoNonces)?;
        if nonces.iter().map(|n| *n.commitments()).collect::<Vec<_>>() != ours {
            return Err(SessionError::NoNonces);
        }

        let mut shares = Vec::with_capacity(spends.len());
        for ((package, nonce), spend) in request.packages.iter().zip(nonces).zip(spends) {
            shares.push(signing::sign(
                package,
                nonce,
                self.key_package,
                spend.alpha,
            )?);
        }
        Ok(shares)
    }
}

pub type GroupPlan = (usize, Vec<Identifier>, Vec<Vec<Vec<u8>>>);

pub type GroupSignatures = Vec<(usize, Vec<SignatureShare>)>;

impl Member<'_> {
    pub fn sign_groups(
        &self,
        pczt: &Pczt,
        expected: &Expectations,
        my_groups: &[GroupPlan],
        pool: &mut impl PoolStore,
    ) -> Result<(VerifiedTx, GroupSignatures), SessionError> {
        let verified = verify_pczt(pczt, self.vault_fvk, expected)?;
        let spends = &verified.spends_to_sign;

        let mut planned = Vec::with_capacity(my_groups.len());
        for (group, ids, commitments) in my_groups {
            if commitments.len() != spends.len() {
                return Err(SessionError::WrongPackageCount {
                    expected: spends.len(),
                    got: commitments.len(),
                });
            }
            let pos = ids
                .iter()
                .position(|id| *id == self.identifier)
                .ok_or(SessionError::UnknownApprover)?;
            let mut packages = Vec::with_capacity(spends.len());
            for (spend, per_member) in commitments.iter().enumerate() {
                let mine = per_member
                    .get(pos)
                    .ok_or(SessionError::NotOurCommitments(spend))?;
                if !pool.contains(mine) {
                    return Err(SessionError::NoNonces);
                }
                packages.push((
                    group_package(per_member, ids, &verified.sighash)?,
                    mine.clone(),
                ));
            }
            planned.push((*group, packages));
        }

        let mut out = Vec::with_capacity(planned.len());
        for (group, packages) in planned {
            let mut shares = Vec::with_capacity(packages.len());
            for ((package, mine), spend) in packages.into_iter().zip(spends) {
                let nonces = pool.take(&mine).ok_or(SessionError::NoNonces)?;
                let expected_commitment = SigningCommitments::deserialize(&mine)
                    .map_err(|e| SessionError::Encoding(format!("{e:?}")))?;
                if *nonces.commitments() != expected_commitment {
                    return Err(SessionError::NotOurCommitments(0));
                }
                shares.push(signing::sign(
                    &package,
                    nonces,
                    self.key_package,
                    spend.alpha,
                )?);
            }
            out.push((group, shares));
        }
        Ok((verified, out))
    }
}

pub fn aggregate_group(
    ids: &[Identifier],
    commitments: &[Vec<Vec<u8>>],
    shares: &BTreeMap<Identifier, Vec<SignatureShare>>,
    verified: &VerifiedTx,
    public_key_package: &PublicKeyPackage,
) -> Result<Vec<(usize, [u8; 64])>, SessionError> {
    let spends = &verified.spends_to_sign;
    if commitments.len() != spends.len() {
        return Err(SessionError::WrongPackageCount {
            expected: spends.len(),
            got: commitments.len(),
        });
    }
    let mut signatures = Vec::with_capacity(spends.len());
    for (i, (spend, per_member)) in spends.iter().zip(commitments).enumerate() {
        let package = group_package(per_member, ids, &verified.sighash)?;
        let for_spend = ids
            .iter()
            .map(|id| {
                shares
                    .get(id)
                    .and_then(|s| s.get(i))
                    .map(|s| (*id, *s))
                    .ok_or(SessionError::IncompleteShares)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let sig = signing::aggregate(&package, &for_spend, public_key_package, spend.alpha)?;
        signatures.push((spend.action_index, sig));
    }
    Ok(signatures)
}

#[derive(Clone, Debug)]
pub struct SigningRequest {
    pub proposal: ProposalId,
    pub pczt_hash: [u8; 32],
    pub signers: BTreeSet<Identifier>,
    pub packages: Vec<SigningPackage>,
}

pub struct Leader {
    proposal: ProposalId,
    pczt_hash: [u8; 32],
    threshold: usize,
    sighash: [u8; 32],
    spends: Vec<SpendToSign>,
    approvals: BTreeMap<Identifier, Vec<SigningCommitments>>,
    used: BTreeSet<Identifier>,
}

impl Leader {
    pub fn new(
        proposal: ProposalId,
        pczt: &Pczt,
        verified: &VerifiedTx,
        threshold: u16,
    ) -> Result<Self, SessionError> {
        Ok(Self {
            proposal,
            pczt_hash: pczt_hash(pczt)?,
            threshold: usize::from(threshold),
            sighash: verified.sighash,
            spends: verified.spends_to_sign.clone(),
            approvals: BTreeMap::new(),
            used: BTreeSet::new(),
        })
    }

    pub fn add_approval(&mut self, approval: Approval) -> bool {
        let valid = approval.proposal == self.proposal
            && approval.pczt_hash == self.pczt_hash
            && approval.commitments.len() == self.spends.len();
        if valid && self.approvals.get(&approval.member) != Some(&approval.commitments) {
            self.used.remove(&approval.member);
            self.approvals.insert(approval.member, approval.commitments);
        }
        valid
    }

    pub fn available_signers(&self) -> Vec<Identifier> {
        self.approvals
            .keys()
            .filter(|id| !self.used.contains(id))
            .copied()
            .collect()
    }

    pub fn request(&mut self, signers: &[Identifier]) -> Result<SigningRequest, SessionError> {
        let signers: BTreeSet<Identifier> = signers.iter().copied().collect();
        if signers.len() != self.threshold {
            return Err(SessionError::NotEnoughSigners {
                needed: self.threshold,
                got: signers.len(),
            });
        }
        if signers
            .iter()
            .any(|id| !self.approvals.contains_key(id) || self.used.contains(id))
        {
            return Err(SessionError::UnknownApprover);
        }
        let packages = (0..self.spends.len())
            .map(|spend| {
                let commitments = signers
                    .iter()
                    .map(|id| (*id, self.approvals[id][spend]))
                    .collect();
                signing::signing_package(commitments, &self.sighash)
            })
            .collect();
        self.used.extend(signers.iter().copied());
        Ok(SigningRequest {
            proposal: self.proposal,
            pczt_hash: self.pczt_hash,
            signers,
            packages,
        })
    }

    pub fn release(&mut self, signers: &[Identifier]) {
        for id in signers {
            self.used.remove(id);
        }
    }

    pub fn aggregate(
        &self,
        request: &SigningRequest,
        shares: &BTreeMap<Identifier, Vec<SignatureShare>>,
        public_key_package: &PublicKeyPackage,
    ) -> Result<Vec<(usize, [u8; 64])>, SessionError> {
        aggregate_request(request, &self.spends, shares, public_key_package)
    }
}

pub fn aggregate_request(
    request: &SigningRequest,
    spends: &[SpendToSign],
    shares: &BTreeMap<Identifier, Vec<SignatureShare>>,
    public_key_package: &PublicKeyPackage,
) -> Result<Vec<(usize, [u8; 64])>, SessionError> {
    if shares.keys().copied().collect::<BTreeSet<_>>() != request.signers {
        return Err(SessionError::NotEnoughSigners {
            needed: request.signers.len(),
            got: shares.len(),
        });
    }
    if request.packages.len() != spends.len() {
        return Err(SessionError::WrongPackageCount {
            expected: spends.len(),
            got: request.packages.len(),
        });
    }
    let mut signatures = Vec::with_capacity(spends.len());
    for (i, (spend, package)) in spends.iter().zip(&request.packages).enumerate() {
        let per_spend = shares
            .iter()
            .map(|(id, s)| {
                s.get(i)
                    .map(|share| (*id, *share))
                    .ok_or(SessionError::IncompleteShares)
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        let sig = signing::aggregate(package, &per_spend, public_key_package, spend.alpha)?;
        signatures.push((spend.action_index, sig));
    }
    Ok(signatures)
}
