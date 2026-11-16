use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use zafe_proto::{
    relay::{ReplaceApproval, SeatCancel, SeatMoveApproval},
    version::{self, DecodeError, Format, UnsupportedVersion},
    IdentityPublic, LogEntry, LogKey,
};

use crate::session::ProposalId;

const PERSONAL_DESCRIPTOR: &[u8; 16] = b"Zafe_VaultDescr_";
const PERSONAL_EVENT_SIG: &[u8] = b"Zafe descriptor signature v1";

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum VaultError {
    #[error("encoding")]
    Encoding,
    #[error(transparent)]
    UnsupportedVersion(#[from] UnsupportedVersion),
    #[error("log entry {0} cannot be decrypted with the current log key")]
    Undecryptable(u64),
    #[error("the log must start with a VaultCreated event")]
    NotCreatedFirst,
    #[error("VaultCreated appears more than once")]
    DuplicateCreated,
    #[error("descriptor is missing a valid signature from a member")]
    MissingDescriptorSignature,
    #[error("entry {0} is authored by a non-member")]
    NotAMember(u64),
    #[error("entry {0} refers to an unknown proposal")]
    UnknownProposal(u64),
    #[error("entry {0}: vote for a proposal that is no longer open")]
    ProposalClosed(u64),
    #[error("entry {0}: vote references a different PCZT than the proposal")]
    PcztMismatch(u64),
    #[error("entry {0}: duplicate proposal id")]
    DuplicateProposal(u64),
    #[error("entry {0}: only the author can cancel a proposal")]
    NotAuthor(u64),
    #[error("entry {0}: invalid commitment batch")]
    BadCommitments(u64),
    #[error("entry {0}: signature shares that don't fit the proposal's signer groups")]
    BadShares(u64),
    #[error("entry {0}: this member already signed; an approval with shares is final")]
    VoteFinal(u64),
    #[error("entry {0}: spends a note another open proposal already spends")]
    NotesInUse(u64),
    #[error("entry {0}: broadcast txid doesn't match the proposal's transaction")]
    TxidMismatch(u64),
    #[error("entry {0}: broadcast without the proposal's signed transaction")]
    BadBroadcast(u64),
    #[error("entry {0}: not a valid display name")]
    BadName(u64),
    #[error("entry {0}: backup attestation for another key epoch")]
    BadBackup(u64),
    #[error("entry {0}: not a valid approval to move a member's seat")]
    BadReplacement(u64),
    #[error("entry {0}: not a valid change to a seat's key repair")]
    BadRepair(u64),
    #[error("entry {0}: proposal with a transaction members can't use")]
    BadProposal(u64),
    #[error("entry {0}: not a valid switch to another relay")]
    BadMove(u64),
}

pub const MAX_COMMITMENT_BATCH: usize = 256;
pub const MAX_POOL_OUTSTANDING: usize = 4096;
pub const MAX_PREPROCESSED_SUBSETS: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Pool {
    pub commitments: Vec<Vec<u8>>,
    pub next: usize,
}

impl Pool {
    pub fn available(&self) -> usize {
        self.commitments.len() - self.next
    }

    pub fn reset(&mut self) {
        self.commitments.truncate(self.next);
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preprocessed {
    pub subsets: Vec<Vec<[u8; 32]>>,
    pub commitments: Vec<Vec<Vec<Vec<u8>>>>,
}

impl Preprocessed {
    pub fn commitment(&self, group: usize, spend: usize, member: &[u8; 32]) -> Option<&[u8]> {
        let pos = self.subsets.get(group)?.iter().position(|m| m == member)?;
        Some(self.commitments.get(group)?.get(spend)?.get(pos)?)
    }

    pub fn groups_of(&self, member: &[u8; 32]) -> Vec<usize> {
        (0..self.subsets.len())
            .filter(|g| self.subsets[*g].contains(member))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupShares {
    pub group: u16,
    pub shares: Vec<Vec<u8>>,
}

fn combinations(n: usize, t: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    let mut idx: Vec<usize> = (0..t).collect();
    if t == 0 || t > n {
        return out;
    }
    loop {
        out.push(idx.clone());
        let mut i = t;
        while i > 0 && idx[i - 1] == n - t + i - 1 {
            i -= 1;
        }
        if i == 0 {
            return out;
        }
        idx[i - 1] += 1;
        for j in i..t {
            idx[j] = idx[j - 1] + 1;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemberInfo {
    pub identity: IdentityPublic,
    pub frost_id: Vec<u8>,
    pub name: String,
}

pub const DEFAULT_PROPOSAL_EXPIRY_BLOCKS: u32 = 7 * 1152;

pub const EXPIRY_TIP_SLACK_BLOCKS: u32 = 96;

pub const EXPIRY_ROUNDING_BLOCKS: u32 = 144;

pub const MIN_PROPOSAL_EXPIRY_BLOCKS: u32 = 1152;
pub const MAX_PROPOSAL_EXPIRY_BLOCKS: u32 = 30 * 1152;

pub fn expiry_height(target: u32, window: u32) -> u32 {
    expiry_height_scaled(target, window, 1)
}

pub fn expiry_height_scaled(target: u32, window: u32, scale: u32) -> u32 {
    let rounding = EXPIRY_ROUNDING_BLOCKS * scale;
    (target + window * scale).div_ceil(rounding) * rounding
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultDescriptor {
    pub vault_id: [u8; 16],
    pub version: u16,
    pub name: String,
    pub network: String,
    pub threshold: u16,
    pub members: Vec<MemberInfo>,
    pub group_public_key: [u8; 32],
    pub ufvk: String,
    pub address: String,
    pub use_qsk: bool,
    pub birthday_height: u32,
    pub proposal_expiry_blocks: u32,
    pub epoch: u32,
    pub transcript_hash: [u8; 32],
}

impl VaultDescriptor {
    pub fn hash(&self) -> Result<[u8; 32], VaultError> {
        let bytes = postcard::to_allocvec(self).map_err(|_| VaultError::Encoding)?;
        Ok(blake2b_simd::Params::new()
            .hash_length(32)
            .personal(PERSONAL_DESCRIPTOR)
            .hash(&bytes)
            .as_bytes()
            .try_into()
            .expect("32 bytes"))
    }

    pub fn signing_message(&self) -> Result<Vec<u8>, VaultError> {
        let mut msg = PERSONAL_EVENT_SIG.to_vec();
        msg.extend_from_slice(&self.hash()?);
        Ok(msg)
    }

    pub fn member(&self, sig_pk: &[u8; 32]) -> Option<&MemberInfo> {
        self.members.iter().find(|m| &m.identity.sig_pk == sig_pk)
    }

    pub fn rejection_threshold(&self) -> usize {
        self.members.len() - usize::from(self.threshold) + 1
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposedPayment {
    pub address: String,
    pub amount_zat: u64,
    pub memo: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VaultEvent {
    Created {
        descriptor: VaultDescriptor,
        signatures: Vec<([u8; 32], Vec<u8>)>,
    },
    Proposal {
        id: ProposalId,
        payments: Vec<ProposedPayment>,
        pczt: Vec<u8>,
        pczt_hash: [u8; 32],
        tip_height: u32,
        created_at: u64,
        signing_spends: u16,
        auto_send: bool,
    },
    Vote {
        proposal: ProposalId,
        pczt_hash: [u8; 32],
        approve: bool,
        commitments: Vec<Vec<u8>>,
        shares: Vec<GroupShares>,
    },
    Commitments {
        batch: Vec<Vec<u8>>,
    },
    Cancelled {
        proposal: ProposalId,
    },
    Broadcast {
        proposal: ProposalId,
        txid: [u8; 32],
    },
    Name {
        name: String,
    },
    BackupVerified {
        epoch: u32,
        at: u64,
    },
    ReplaceApproval {
        old: [u8; 32],
        new: IdentityPublic,
        signature: Vec<u8>,
    },
    RepairRetry {
        replacement: u64,
        helpers: Vec<[u8; 32]>,
    },
    RepairDone {
        replacement: u64,
    },
    PoolReset,
    VaultMoved {
        target: crate::moves::MoveTarget,
        arrivals: Vec<([u8; 32], Vec<u8>)>,
        at: u64,
    },
    KeepSeat {
        replacement: u64,
        at: u64,
        mailbox: [u8; 16],
        signature: Vec<u8>,
    },
    StopHelping {
        replacement: u64,
    },
    RepairHelped {
        replacement: u64,
        attempt: u32,
    },
    SignedBroadcast {
        proposal: ProposalId,
        tx: Vec<u8>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Moved {
    pub index: u64,
    pub target: crate::moves::MoveTarget,
    pub author: [u8; 32],
    pub arrived: Vec<[u8; 32]>,
    pub at: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingReplacement {
    pub old: [u8; 32],
    pub new: IdentityPublic,
    pub round: Option<u32>,
    pub approvals: Vec<([u8; 32], Vec<u8>)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Replacement {
    pub index: u64,
    pub old: [u8; 32],
    pub new: IdentityPublic,
    pub frost_id: Vec<u8>,
    pub approvals: Vec<([u8; 32], Vec<u8>)>,
    pub helpers: Vec<[u8; 32]>,
    pub attempt: u32,
    pub retries: BTreeMap<Vec<[u8; 32]>, Vec<[u8; 32]>>,
    pub done: bool,
    pub round: Option<u32>,
    pub helped: Vec<[u8; 32]>,
    pub switched: bool,
    pub stopped: Vec<[u8; 32]>,
}

impl Replacement {
    pub fn helpers(&self) -> Vec<[u8; 32]> {
        self.helpers.clone()
    }

    pub fn move_id(&self, vault_id: &[u8; 16]) -> Option<[u8; 32]> {
        self.round
            .map(|round| zafe_proto::relay::seat_move_id(vault_id, &self.old, &self.new, round))
    }

    pub fn is_pending(&self) -> bool {
        self.round.is_some() && !self.switched
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SeatKept {
    pub index: u64,
    pub replacement: u64,
    pub new: IdentityPublic,
    pub cancel: SeatCancel,
    pub signature: Vec<u8>,
    pub at: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackupRecord {
    pub epoch: u32,
    pub at: u64,
}

pub const MAX_NAME_CHARS: usize = 32;

pub fn valid_name(name: &str) -> bool {
    name.trim() == name
        && name.chars().count() <= MAX_NAME_CHARS
        && !name.chars().any(char::is_control)
}

impl From<DecodeError> for VaultError {
    fn from(e: DecodeError) -> Self {
        match e {
            DecodeError::Unsupported(v) => VaultError::UnsupportedVersion(v),
            DecodeError::Malformed(_) => VaultError::Encoding,
        }
    }
}

impl VaultEvent {
    pub fn to_bytes(&self) -> Result<Vec<u8>, VaultError> {
        Ok(version::encode(Format::VaultEvent, self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, VaultError> {
        Self::from_bytes_versioned(bytes).map(|(event, _)| event)
    }

    pub fn from_bytes_versioned(bytes: &[u8]) -> Result<(Self, u16), VaultError> {
        let (found, body) = version::split(Format::VaultEvent, bytes)?;
        if found == 0 || found > version::VAULT_EVENT {
            version::check(Format::VaultEvent, found)?;
        }
        let event = postcard::from_bytes(body).map_err(|_| VaultError::Encoding)?;
        Ok((event, found))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProposalStatus {
    Open,
    Approved,
    Rejected,
    Cancelled,
    Broadcast,
}

#[derive(Clone, Debug)]
pub struct ProposalState {
    pub id: ProposalId,
    pub author: [u8; 32],
    pub payments: Vec<ProposedPayment>,
    pub pczt: Vec<u8>,
    pub pczt_hash: [u8; 32],
    pub tip_height: u32,
    pub created_at: u64,
    pub log_index: u64,
    pub status: ProposalStatus,
    pub approvals: BTreeMap<[u8; 32], Vec<Vec<u8>>>,
    pub rejections: BTreeMap<[u8; 32], ()>,
    pub txid: Option<[u8; 32]>,
    pub signing_spends: u16,
    pub auto_send: bool,
    pub nullifiers: Vec<[u8; 32]>,
    pub expiry_height: u32,
    pub expected_txid: Option<[u8; 32]>,
    pub preprocessed: Option<Preprocessed>,
    pub shares: BTreeMap<[u8; 32], Vec<GroupShares>>,
    pub ready_group: Option<u16>,
    pub completed_by: Option<[u8; 32]>,
    pub possibly_signed: bool,
    pub signed_tx: Option<Vec<u8>>,
}

impl ProposalState {
    pub fn one_tap_blocked(&self) -> bool {
        let Some(pre) = &self.preprocessed else {
            return false;
        };
        let can_sign = |m: &[u8; 32], g: usize| match self.shares.get(m) {
            Some(posted) => posted.iter().any(|s| usize::from(s.group) == g),
            None => {
                !self.rejections.contains_key(m)
                    && self.approvals.get(m).is_none_or(|c| c.is_empty())
            }
        };
        self.ready_group.is_none()
            && !(0..pre.subsets.len()).any(|g| pre.subsets[g].iter().all(|m| can_sign(m, g)))
    }

    pub fn reserves(&self, threshold: u16) -> bool {
        matches!(self.status, ProposalStatus::Open | ProposalStatus::Approved)
            && self
                .approvals
                .keys()
                .any(|m| *m != self.author || threshold == 1)
    }

    fn find_ready_group(&self) -> Option<u16> {
        let pre = self.preprocessed.as_ref()?;
        (0..pre.subsets.len())
            .find(|g| {
                pre.subsets[*g].iter().all(|m| {
                    self.shares
                        .get(m)
                        .is_some_and(|gs| gs.iter().any(|s| usize::from(s.group) == *g))
                })
            })
            .map(|g| g as u16)
    }
}

#[derive(Clone, Debug)]
pub struct VaultState {
    pub descriptor: VaultDescriptor,
    pub proposals: BTreeMap<ProposalId, ProposalState>,
    pub applied: u64,
    pub ignored: Vec<(u64, VaultError)>,
    pub pools: BTreeMap<[u8; 32], Pool>,
    pub names: BTreeMap<[u8; 32], String>,
    pub backups: BTreeMap<[u8; 32], BackupRecord>,
    pub pending_replacements: BTreeMap<([u8; 32], [u8; 32]), PendingReplacement>,
    pub replacements: Vec<Replacement>,
    pub seat_keeps: BTreeMap<[u8; 32], Vec<SeatKept>>,
    pub seat_changes: u64,
    pub former_members: Vec<IdentityPublic>,
    pub max_event_version: u16,
    pub member_versions: BTreeMap<[u8; 32], u16>,
    pub moves: Vec<Moved>,
    pub entry_hashes: Vec<[u8; 32]>,
}

impl VaultState {
    pub fn replay(entries: &[LogEntry], key: &LogKey) -> Result<Self, VaultError> {
        let (first, rest) = entries.split_first().ok_or(VaultError::NotCreatedFirst)?;
        let (event, first_version) = VaultEvent::from_bytes_versioned(
            &first
                .decrypt(key)
                .map_err(|_| VaultError::Undecryptable(first.header.index))?,
        )?;
        let VaultEvent::Created {
            descriptor,
            signatures,
        } = event
        else {
            return Err(VaultError::NotCreatedFirst);
        };
        version::check(Format::Descriptor, descriptor.version)?;
        if descriptor.member(&first.header.author).is_none() {
            return Err(VaultError::NotAMember(first.header.index));
        }
        check_descriptor_signatures(&descriptor, &signatures)?;
        let mut state = VaultState {
            descriptor,
            proposals: BTreeMap::new(),
            applied: first.header.index + 1,
            ignored: Vec::new(),
            pools: BTreeMap::new(),
            names: BTreeMap::new(),
            backups: BTreeMap::new(),
            pending_replacements: BTreeMap::new(),
            replacements: Vec::new(),
            seat_keeps: BTreeMap::new(),
            seat_changes: 0,
            former_members: Vec::new(),
            max_event_version: 0,
            member_versions: BTreeMap::from([(first.header.author, first_version)]),
            moves: Vec::new(),
            entry_hashes: Vec::new(),
        };
        state.record_hash(first);
        for entry in rest {
            state.apply_entry(entry, key);
        }
        Ok(state)
    }

    pub fn apply_entry(&mut self, entry: &LogEntry, key: &LogKey) {
        let index = entry.header.index;
        let author = entry.header.author;
        self.record_hash(entry);
        let current = self.descriptor.member(&author).is_some();
        let result = entry
            .decrypt(key)
            .map_err(|_| VaultError::Undecryptable(index))
            .and_then(|bytes| VaultEvent::from_bytes_versioned(&bytes))
            .and_then(|(event, found)| {
                let effective = self.effective_version(found);
                if current || effective < CURRENT_AUTHORS_FROM {
                    self.max_event_version = self.max_event_version.max(found);
                    let seen = self.member_versions.entry(author).or_default();
                    *seen = (*seen).max(found);
                }
                self.apply_versioned(index, author, event, effective)
            });
        if let Err(e) = result {
            let e = match e {
                VaultError::UnsupportedVersion(_) if !current => VaultError::NotAMember(index),
                e => e,
            };
            self.ignored.push((index, e));
        }
        self.applied = index + 1;
    }

    fn record_hash(&mut self, entry: &LogEntry) {
        if entry.header.index == self.entry_hashes.len() as u64 {
            if let Ok(hash) = entry.hash() {
                self.entry_hashes.push(hash);
            }
        }
    }

    pub fn moved(&self) -> Option<&Moved> {
        self.moves.last()
    }

    pub fn switch_threshold(&self) -> usize {
        crate::moves::switch_threshold(
            self.descriptor.members.len(),
            usize::from(self.descriptor.threshold),
        )
    }

    pub fn members_needing_update(&self) -> Vec<[u8; 32]> {
        self.members_below(MOVE_FROM)
    }

    pub fn members_below(&self, version: u16) -> Vec<[u8; 32]> {
        self.descriptor
            .members
            .iter()
            .map(|m| m.identity.sig_pk)
            .filter(|pk| self.member_versions.get(pk).copied().unwrap_or(0) < version)
            .collect()
    }

    fn effective_version(&self, found: u16) -> u16 {
        if self.max_event_version >= VERSION_FLOOR_FROM {
            found.max(self.max_event_version)
        } else {
            found
        }
    }

    pub fn newer_version_entries(&self) -> usize {
        self.ignored
            .iter()
            .filter(|(_, e)| matches!(e, VaultError::UnsupportedVersion(v) if v.is_newer()))
            .count()
    }

    pub fn check(&self, author: [u8; 32], event: &VaultEvent) -> Result<(), VaultError> {
        self.clone().apply(self.applied, author, event.clone())
    }

    pub fn apply(
        &mut self,
        index: u64,
        author: [u8; 32],
        event: VaultEvent,
    ) -> Result<(), VaultError> {
        self.apply_versioned(index, author, event, version::VAULT_EVENT)
    }

    pub fn apply_versioned(
        &mut self,
        index: u64,
        author: [u8; 32],
        event: VaultEvent,
        event_version: u16,
    ) -> Result<(), VaultError> {
        if self.descriptor.member(&author).is_none() {
            return Err(VaultError::NotAMember(index));
        }
        let threshold = usize::from(self.descriptor.threshold);
        let rejection_threshold = self.descriptor.rejection_threshold();
        match event {
            VaultEvent::Created { .. } => return Err(VaultError::DuplicateCreated),
            VaultEvent::Proposal {
                id,
                payments,
                pczt,
                pczt_hash,
                tip_height,
                created_at,
                signing_spends,
                auto_send,
            } => {
                if self.proposals.contains_key(&id) {
                    return Err(VaultError::DuplicateProposal(index));
                }
                let expected_txid = pczt::Pczt::parse(&pczt)
                    .ok()
                    .and_then(|p| crate::tx::shielded_sighash(&p).ok());
                let (nullifiers, expiry_height) = pczt::Pczt::parse(&pczt)
                    .ok()
                    .and_then(|p| {
                        let nfs = crate::tx::spent_nullifiers(&p).ok()?;
                        Some((nfs, *p.global().expiry_height()))
                    })
                    .unwrap_or_default();
                if event_version >= VERSION_FLOOR_FROM
                    && (expected_txid.is_none()
                        || signing_spends == 0
                        || usize::from(signing_spends) > nullifiers.len())
                {
                    return Err(VaultError::BadProposal(index));
                }
                if self.proposals.values().any(|p| {
                    let live = if event_version >= PROPOSAL_RULES_FROM {
                        p.reserves(self.descriptor.threshold)
                            && self.reservation_end(p) > tip_height
                    } else {
                        matches!(p.status, ProposalStatus::Open | ProposalStatus::Approved)
                            && p.expiry_height > tip_height
                    };
                    live && p.nullifiers.iter().any(|nf| nullifiers.contains(nf))
                }) {
                    return Err(VaultError::NotesInUse(index));
                }
                let preprocessed = self.assign_commitments(signing_spends, event_version);
                self.proposals.insert(
                    id,
                    ProposalState {
                        id,
                        author,
                        payments,
                        pczt,
                        pczt_hash,
                        tip_height,
                        created_at,
                        log_index: index,
                        status: ProposalStatus::Open,
                        approvals: BTreeMap::new(),
                        rejections: BTreeMap::new(),
                        txid: None,
                        signing_spends,
                        auto_send,
                        nullifiers,
                        expiry_height,
                        expected_txid,
                        preprocessed,
                        shares: BTreeMap::new(),
                        ready_group: None,
                        completed_by: None,
                        possibly_signed: false,
                        signed_tx: None,
                    },
                );
            }
            VaultEvent::Vote {
                proposal,
                pczt_hash,
                approve,
                commitments,
                shares,
            } => {
                let p = self
                    .proposals
                    .get_mut(&proposal)
                    .ok_or(VaultError::UnknownProposal(index))?;
                if !matches!(p.status, ProposalStatus::Open | ProposalStatus::Approved) {
                    return Err(VaultError::ProposalClosed(index));
                }
                if pczt_hash != p.pczt_hash {
                    return Err(VaultError::PcztMismatch(index));
                }
                if p.shares.contains_key(&author) {
                    let adds_commitments = event_version >= PROPOSAL_RULES_FROM
                        && approve
                        && shares.is_empty()
                        && !commitments.is_empty();
                    if !adds_commitments {
                        return Err(VaultError::VoteFinal(index));
                    }
                }
                if !shares.is_empty() {
                    let pre = p
                        .preprocessed
                        .as_ref()
                        .ok_or(VaultError::BadShares(index))?;
                    let mut seen = std::collections::BTreeSet::new();
                    let valid = approve
                        && commitments.is_empty()
                        && shares.iter().all(|gs| {
                            let g = usize::from(gs.group);
                            seen.insert(g)
                                && pre.subsets.get(g).is_some_and(|m| m.contains(&author))
                                && gs.shares.len() == usize::from(p.signing_spends)
                        });
                    if !valid {
                        return Err(VaultError::BadShares(index));
                    }
                    p.shares.insert(author, shares);
                    p.possibly_signed = true;
                    if p.ready_group.is_none() {
                        p.ready_group = p.find_ready_group();
                        if p.ready_group.is_some() {
                            p.completed_by = Some(author);
                        }
                    }
                }
                if approve {
                    p.rejections.remove(&author);
                    p.approvals.insert(author, commitments);
                } else {
                    p.approvals.remove(&author);
                    p.rejections.insert(author, ());
                }
                p.status = if p.rejections.len() >= rejection_threshold {
                    ProposalStatus::Rejected
                } else if p.approvals.len() >= threshold {
                    ProposalStatus::Approved
                } else {
                    ProposalStatus::Open
                };
                p.possibly_signed |= p.status == ProposalStatus::Approved;
            }
            VaultEvent::Commitments { batch } => {
                use reddsa::frost::redpallas::round1::SigningCommitments;
                let pool = self.pools.entry(author).or_default();
                if batch.is_empty()
                    || batch.len() > MAX_COMMITMENT_BATCH
                    || pool.available() + batch.len() > MAX_POOL_OUTSTANDING
                    || batch
                        .iter()
                        .any(|c| SigningCommitments::deserialize(c).is_err())
                {
                    return Err(VaultError::BadCommitments(index));
                }
                pool.commitments.extend(batch);
            }
            VaultEvent::Cancelled { proposal } => {
                let p = self
                    .proposals
                    .get_mut(&proposal)
                    .ok_or(VaultError::UnknownProposal(index))?;
                if p.author != author {
                    return Err(VaultError::NotAuthor(index));
                }
                if !matches!(p.status, ProposalStatus::Open | ProposalStatus::Approved) {
                    return Err(VaultError::ProposalClosed(index));
                }
                p.status = ProposalStatus::Cancelled;
            }
            VaultEvent::Broadcast { proposal, txid } => {
                let p = self
                    .proposals
                    .get_mut(&proposal)
                    .ok_or(VaultError::UnknownProposal(index))?;
                if p.status != ProposalStatus::Approved {
                    return Err(VaultError::ProposalClosed(index));
                }
                if event_version >= PROPOSAL_RULES_FROM {
                    return Err(VaultError::BadBroadcast(index));
                }
                if event_version >= BROADCAST_CHECKED_FROM
                    && p.expected_txid.is_some_and(|t| t != txid)
                {
                    return Err(VaultError::TxidMismatch(index));
                }
                p.status = ProposalStatus::Broadcast;
                p.txid = Some(txid);
            }
            VaultEvent::Name { name } => {
                if !valid_name(&name) {
                    return Err(VaultError::BadName(index));
                }
                if name.is_empty() {
                    self.names.remove(&author);
                } else {
                    self.names.insert(author, name);
                }
            }
            VaultEvent::BackupVerified { epoch, at } => {
                if epoch != self.descriptor.epoch {
                    return Err(VaultError::BadBackup(index));
                }
                self.backups.insert(author, BackupRecord { epoch, at });
            }
            VaultEvent::ReplaceApproval {
                old,
                new,
                signature,
            } => {
                let bad = || VaultError::BadReplacement(index);
                let was_member =
                    |pk: &[u8; 32]| self.former_members.iter().any(|m| &m.sig_pk == pk);
                if author == old
                    || self.descriptor.member(&old).is_none()
                    || self.descriptor.member(&new.sig_pk).is_some()
                    || was_member(&new.sig_pk)
                    || new.sig_pk == old
                {
                    return Err(bad());
                }
                let by = self
                    .descriptor
                    .member(&author)
                    .expect("checked above")
                    .identity;
                let round = if event_version >= PENDING_SEATS_FROM {
                    if self
                        .replacements
                        .iter()
                        .any(|r| r.is_pending() && (r.old == old || r.new.sig_pk == new.sig_pk))
                    {
                        return Err(bad());
                    }
                    let round = self.seat_round(&old);
                    let approval = SeatMoveApproval {
                        vault_id: self.descriptor.vault_id,
                        old,
                        new,
                        round,
                    };
                    approval.verify(&by, &signature).map_err(|_| bad())?;
                    Some(round)
                } else {
                    let approval = ReplaceApproval {
                        mailbox: self.descriptor.vault_id,
                        old,
                        new,
                    };
                    approval.verify(&by, &signature).map_err(|_| bad())?;
                    None
                };
                let pending = self
                    .pending_replacements
                    .entry((old, new.sig_pk))
                    .or_insert_with(|| PendingReplacement {
                        old,
                        new,
                        round,
                        approvals: Vec::new(),
                    });
                if pending.round != round {
                    pending.round = round;
                    pending.approvals.clear();
                }
                if pending.new != new || pending.approvals.iter().any(|(pk, _)| *pk == author) {
                    return Err(bad());
                }
                pending.approvals.push((author, signature));
                self.seat_changes += 1;
                if pending.approvals.len() >= threshold {
                    let approvals = pending.approvals.clone();
                    let frost_id = self
                        .descriptor
                        .member(&old)
                        .map(|m| m.frost_id.clone())
                        .unwrap_or_default();
                    let helpers = approvals.iter().map(|(pk, _)| *pk).collect();
                    self.pending_replacements.retain(|(o, _), _| *o != old);
                    let switched = round.is_none();
                    if switched {
                        self.move_seat(old, new, event_version);
                    }
                    self.replacements.push(Replacement {
                        index,
                        old,
                        new,
                        frost_id,
                        approvals,
                        helpers,
                        attempt: 0,
                        retries: BTreeMap::new(),
                        done: false,
                        round,
                        helped: Vec::new(),
                        switched,
                        stopped: Vec::new(),
                    });
                }
            }
            VaultEvent::RepairRetry {
                replacement,
                helpers,
            } => {
                let bad = || VaultError::BadRepair(index);
                let mut distinct = helpers.clone();
                distinct.sort();
                distinct.dedup();
                let r = self
                    .replacements
                    .iter()
                    .position(|r| r.index == replacement && !r.done)
                    .ok_or_else(bad)?;
                let (new, old) = (self.replacements[r].new.sig_pk, self.replacements[r].old);
                let own_seat = event_version >= PENDING_SEATS_FROM && helpers.contains(&old);
                let stopped = helpers
                    .iter()
                    .any(|h| self.replacements[r].stopped.contains(h));
                if author == new
                    || own_seat
                    || stopped
                    || !helpers.contains(&author)
                    || distinct.len() != helpers.len()
                    || helpers.len() != threshold
                    || helpers
                        .iter()
                        .any(|h| *h == new || self.descriptor.member(h).is_none())
                {
                    return Err(bad());
                }
                let r = &mut self.replacements[r];
                if event_version >= REPAIR_QUORUM_FROM {
                    let asked = r.retries.entry(helpers.clone()).or_default();
                    if asked.contains(&author) {
                        return Err(bad());
                    }
                    asked.push(author);
                    if asked.len() < threshold {
                        return Ok(());
                    }
                }
                r.helpers = helpers;
                r.attempt += 1;
                r.retries.clear();
                r.helped.clear();
            }
            VaultEvent::RepairDone { replacement } => {
                let r = self
                    .replacements
                    .iter_mut()
                    .find(|r| {
                        r.index == replacement && !r.done && r.switched && r.new.sig_pk == author
                    })
                    .ok_or(VaultError::BadRepair(index))?;
                r.done = true;
            }
            VaultEvent::PoolReset => {
                if event_version < POOL_RESET_FROM {
                    return Err(VaultError::Encoding);
                }
                if let Some(pool) = self.pools.get_mut(&author) {
                    pool.reset();
                }
            }
            VaultEvent::VaultMoved {
                target,
                arrivals,
                at,
            } => {
                if event_version < MOVE_FROM {
                    return Err(VaultError::Encoding);
                }
                let arrived = self.check_move(index, &target, &arrivals, at)?;
                self.moves.push(Moved {
                    index,
                    target,
                    author,
                    arrived,
                    at,
                });
            }
            VaultEvent::KeepSeat {
                replacement,
                at,
                mailbox,
                signature,
            } => {
                if event_version < PENDING_SEATS_FROM {
                    return Err(VaultError::Encoding);
                }
                let bad = || VaultError::BadRepair(index);
                let pos = self
                    .replacements
                    .iter()
                    .position(|r| r.index == replacement && r.is_pending())
                    .ok_or_else(bad)?;
                let r = &self.replacements[pos];
                if r.round != Some(0) {
                    return Err(bad());
                }
                let old = self.descriptor.member(&r.old).ok_or_else(bad)?.identity;
                let cancel = SeatCancel {
                    mailbox,
                    vault_id: self.descriptor.vault_id,
                    move_id: r.move_id(&self.descriptor.vault_id).ok_or_else(bad)?,
                    old: r.old,
                    new: r.new.sig_pk,
                    at,
                };
                cancel.verify(&old, &signature).map_err(|_| bad())?;
                let r = self.replacements.remove(pos);
                self.pending_replacements.retain(|(o, _), _| *o != r.old);
                self.seat_keeps.entry(r.old).or_default().push(SeatKept {
                    index,
                    replacement,
                    new: r.new,
                    cancel,
                    signature,
                    at,
                });
                self.seat_changes += 1;
            }
            VaultEvent::StopHelping { replacement } => {
                if event_version < PENDING_SEATS_FROM {
                    return Err(VaultError::Encoding);
                }
                let r = self
                    .replacements
                    .iter_mut()
                    .find(|r| r.index == replacement && r.is_pending() && !r.done)
                    .filter(|r| r.helpers.contains(&author) && !r.stopped.contains(&author))
                    .ok_or(VaultError::BadRepair(index))?;
                r.stopped.push(author);
                self.seat_changes += 1;
            }
            VaultEvent::RepairHelped {
                replacement,
                attempt,
            } => {
                if event_version < PENDING_SEATS_FROM {
                    return Err(VaultError::Encoding);
                }
                let bad = || VaultError::BadRepair(index);
                let r = self
                    .replacements
                    .iter_mut()
                    .find(|r| r.index == replacement && r.is_pending() && !r.done)
                    .ok_or_else(bad)?;
                if attempt != r.attempt
                    || !r.helpers.contains(&author)
                    || r.helped.contains(&author)
                    || r.stopped.contains(&author)
                {
                    return Err(bad());
                }
                r.helped.push(author);
                self.seat_changes += 1;
                if r.helpers.iter().all(|h| r.helped.contains(h)) {
                    r.switched = true;
                    let (old, new) = (r.old, r.new);
                    self.move_seat(old, new, event_version);
                }
            }
            VaultEvent::SignedBroadcast { proposal, tx } => {
                if event_version < PROPOSAL_RULES_FROM {
                    return Err(VaultError::Encoding);
                }
                let p = self
                    .proposals
                    .get_mut(&proposal)
                    .ok_or(VaultError::UnknownProposal(index))?;
                if p.status != ProposalStatus::Approved {
                    return Err(VaultError::ProposalClosed(index));
                }
                let txid = pczt::Pczt::parse(&p.pczt)
                    .ok()
                    .and_then(|pczt| crate::tx::check_signed(&pczt, &tx).ok())
                    .filter(|txid| Some(*txid) == p.expected_txid)
                    .ok_or(VaultError::BadBroadcast(index))?;
                p.status = ProposalStatus::Broadcast;
                p.txid = Some(txid);
                p.signed_tx = Some(tx);
            }
        }
        Ok(())
    }
}

impl VaultState {
    pub fn block_scale(&self, height: u32) -> u32 {
        crate::wallet::ZafeNetwork::from_name(&self.descriptor.network)
            .map_or(1, |n| crate::wallet::block_scale(&n, height))
    }

    pub fn reorg_margin(&self, height: u32) -> u32 {
        REORG_MARGIN_BLOCKS * self.block_scale(height)
    }

    pub fn reservation_end(&self, p: &ProposalState) -> u32 {
        let window = self.descriptor.proposal_expiry_blocks
            + EXPIRY_TIP_SLACK_BLOCKS
            + EXPIRY_ROUNDING_BLOCKS;
        let cap = p.tip_height.saturating_add(
            window.saturating_mul(self.block_scale(p.tip_height.saturating_add(1))),
        );
        p.expiry_height.min(cap)
    }
}

impl VaultState {
    pub fn check_move(
        &self,
        index: u64,
        target: &crate::moves::MoveTarget,
        arrivals: &[([u8; 32], Vec<u8>)],
        at: u64,
    ) -> Result<Vec<[u8; 32]>, VaultError> {
        let bad = || VaultError::BadMove(index);
        if target.vault_id != self.descriptor.vault_id
            || target.base_index >= index
            || self.entry_hashes.get(target.base_index as usize) != Some(&target.base_hash)
            || at > target.expiry
            || at.saturating_add(crate::moves::MOVE_LIFETIME_SECS) < target.expiry
            || crate::moves::normalize_relay_url(&target.relay).as_deref()
                != Some(target.relay.as_str())
        {
            return Err(bad());
        }
        if self
            .moves
            .iter()
            .any(|m| m.target.move_id == target.move_id || target.base_index < m.index)
        {
            return Err(bad());
        }
        let mut seen = Vec::with_capacity(arrivals.len());
        for (pk, signature) in arrivals {
            let member = self.descriptor.member(pk).ok_or_else(bad)?;
            if seen.contains(pk) || !target.verify(&member.identity, signature) {
                return Err(bad());
            }
            seen.push(*pk);
        }
        if seen.len() < self.switch_threshold() {
            return Err(bad());
        }
        Ok(seen)
    }

    pub fn member_identities(&self) -> Vec<IdentityPublic> {
        self.descriptor.members.iter().map(|m| m.identity).collect()
    }

    pub fn log_authors(&self) -> Vec<IdentityPublic> {
        let mut authors = self.member_identities();
        authors.extend(self.former_members.iter().copied());
        authors
    }

    pub fn replacement_to(&self, new: &[u8; 32]) -> Option<&Replacement> {
        self.replacements
            .iter()
            .rev()
            .find(|r| &r.new.sig_pk == new && r.switched)
    }

    pub fn pending_seat_move(&self, old: &[u8; 32]) -> Option<&Replacement> {
        self.replacements
            .iter()
            .find(|r| r.is_pending() && &r.old == old)
    }

    pub fn seat_round(&self, old: &[u8; 32]) -> u32 {
        self.seat_keeps.get(old).map_or(0, |k| k.len() as u32)
    }

    fn move_seat(&mut self, old: [u8; 32], new: IdentityPublic, event_version: u16) {
        let rekey = |pk: &mut [u8; 32]| {
            if *pk == old {
                *pk = new.sig_pk;
            }
        };
        for m in &mut self.descriptor.members {
            if m.identity.sig_pk == old {
                self.former_members.push(m.identity);
                m.identity = new;
            }
        }
        let threshold = usize::from(self.descriptor.threshold);
        for p in self.proposals.values_mut() {
            rekey(&mut p.author);
            if let Some(v) = p.approvals.remove(&old) {
                if v.is_empty() || p.shares.contains_key(&old) {
                    p.approvals.insert(new.sig_pk, Vec::new());
                } else if p.status == ProposalStatus::Approved && p.approvals.len() < threshold {
                    p.status = ProposalStatus::Open;
                }
            }
            if p.rejections.remove(&old).is_some() {
                p.rejections.insert(new.sig_pk, ());
            }
            if let Some(v) = p.shares.remove(&old) {
                p.shares.insert(new.sig_pk, v);
            }
            if let Some(pre) = &mut p.preprocessed {
                pre.subsets.iter_mut().flatten().for_each(rekey);
            }
            if let Some(c) = &mut p.completed_by {
                rekey(c);
            }
        }
        self.pools.remove(&old);
        self.member_versions.remove(&old);
        if let Some(name) = self.names.remove(&old) {
            self.names.insert(new.sig_pk, name);
        }
        self.backups.remove(&old);
        for r in self.replacements.iter_mut().filter(|r| !r.done) {
            r.stopped.iter_mut().for_each(rekey);
            if !r.helpers.contains(&old) {
                continue;
            }
            r.helpers.iter_mut().for_each(rekey);
            if event_version >= PENDING_SEATS_FROM {
                r.attempt += 1;
                r.retries.clear();
                r.helped.clear();
            }
        }
        self.pending_replacements.retain(|(o, _), _| *o != old);
        for p in self.pending_replacements.values_mut() {
            p.approvals.retain(|(pk, _)| *pk != old);
        }
    }

    fn assign_commitments(&mut self, spends: u16, event_version: u16) -> Option<Preprocessed> {
        let members: Vec<[u8; 32]> = self
            .descriptor
            .members
            .iter()
            .map(|m| m.identity.sig_pk)
            .collect();
        let t = usize::from(self.descriptor.threshold);
        let groups = combinations(members.len(), t);
        let spends = usize::from(spends);
        if spends == 0 || groups.is_empty() || groups.len() > MAX_PREPROCESSED_SUBSETS {
            return None;
        }
        let groups = if event_version < PARTIAL_GROUPS_FROM {
            let per_member = groups.iter().filter(|g| g.contains(&0)).count() * spends;
            if members
                .iter()
                .any(|m| self.pools.get(m).map_or(0, Pool::available) < per_member)
            {
                return None;
            }
            groups
        } else {
            let mut taken = vec![0usize; members.len()];
            let covered: Vec<Vec<usize>> = groups
                .into_iter()
                .filter(|group| {
                    let ok = group.iter().all(|&i| {
                        self.pools.get(&members[i]).map_or(0, Pool::available) >= taken[i] + spends
                    });
                    if ok {
                        group.iter().for_each(|&i| taken[i] += spends);
                    }
                    ok
                })
                .collect();
            if covered.is_empty() {
                return None;
            }
            covered
        };
        let mut commitments = Vec::with_capacity(groups.len());
        for group in &groups {
            let mut per_spend = Vec::with_capacity(spends);
            for _ in 0..spends {
                let mut per_member = Vec::with_capacity(t);
                for &i in group {
                    let pool = self.pools.get_mut(&members[i]).expect("checked above");
                    per_member.push(pool.commitments[pool.next].clone());
                    pool.next += 1;
                }
                per_spend.push(per_member);
            }
            commitments.push(per_spend);
        }
        Some(Preprocessed {
            subsets: groups
                .iter()
                .map(|g| g.iter().map(|&i| members[i]).collect())
                .collect(),
            commitments,
        })
    }
}

const PARTIAL_GROUPS_FROM: u16 = 6;

const BROADCAST_CHECKED_FROM: u16 = 7;

const POOL_RESET_FROM: u16 = 8;

pub const MOVE_FROM: u16 = 10;

const CURRENT_AUTHORS_FROM: u16 = 11;

pub const REPAIR_QUORUM_FROM: u16 = 11;

pub const PENDING_SEATS_FROM: u16 = 12;

pub const PROPOSAL_RULES_FROM: u16 = 12;

pub const REORG_MARGIN_BLOCKS: u32 = 10;

const VERSION_FLOOR_FROM: u16 = 9;

fn check_descriptor_signatures(
    descriptor: &VaultDescriptor,
    signatures: &[([u8; 32], Vec<u8>)],
) -> Result<(), VaultError> {
    let message = descriptor.signing_message()?;
    for member in &descriptor.members {
        let ok = signatures.iter().any(|(pk, sig)| {
            pk == &member.identity.sig_pk && member.identity.verify(&message, sig).is_ok()
        });
        if !ok {
            return Err(VaultError::MissingDescriptorSignature);
        }
    }
    Ok(())
}
