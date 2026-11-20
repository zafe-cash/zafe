use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    envelope::{encode, Envelope, MailboxId},
    identity::{Identity, IdentityPublic},
    log::LogEntry,
    version::{self, Format},
    ProtoError,
};

const SIGNATURE_DOMAIN: &[u8] = b"Zafe relay request v1";

pub const UNSUPPORTED_VERSION_HEADER: &str = "zafe-supported-version";

pub const MAX_REQUEST_SKEW_SECS: u64 = 300;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Signed<T> {
    pub payload: T,
    pub signer: IdentityPublic,
    pub signature: Vec<u8>,
}

impl<T: Serialize + DeserializeOwned> Signed<T> {
    pub fn new(identity: &Identity, payload: T) -> Result<Self, ProtoError> {
        let signature = identity.sign(&signed_bytes(&payload)?).to_vec();
        Ok(Self {
            payload,
            signer: *identity.public(),
            signature,
        })
    }

    pub fn verify(&self) -> Result<&T, ProtoError> {
        self.signer
            .verify(&signed_bytes(&self.payload)?, &self.signature)?;
        Ok(&self.payload)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtoError> {
        encode_body(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtoError> {
        decode_body(bytes)
    }
}

fn signed_bytes<T: Serialize>(payload: &T) -> Result<Vec<u8>, ProtoError> {
    let mut out = SIGNATURE_DOMAIN.to_vec();
    out.extend_from_slice(&version::RELAY_API.to_le_bytes());
    out.extend_from_slice(&encode(payload)?);
    Ok(out)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateMailbox {
    pub mailbox: MailboxId,
    pub join_token_hash: [u8; 32],
    pub max_members: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remove {
    pub mailbox: MailboxId,
    pub member: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelSetup {
    pub mailbox: MailboxId,
}

pub const INVITE_TTL_SECS: u64 = 72 * 3600;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenewInvite {
    pub mailbox: MailboxId,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Join {
    pub mailbox: MailboxId,
    pub join_token: [u8; 32],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seal {
    pub mailbox: MailboxId,
    pub members: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxRead {
    pub mailbox: MailboxId,
    pub after: u64,
    pub timestamp: u64,
}

pub const MAX_ACK_CURSORS: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxAck {
    pub mailbox: MailboxId,
    pub cursors: Vec<u64>,
    pub timestamp: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxAckResponse {
    pub deleted: u64,
}

pub const READ_PAGE: u64 = 500;

pub const MAX_BODY_BYTES: usize = 1024 * 1024;

pub const MAX_ENTRY_BYTES: usize = MAX_BODY_BYTES - 64 * 1024;

pub const RECIPIENT_FULL: &str = "this member's inbox is full for this sender";

pub const LOG_FULL: &str = "this vault's history is full on this relay";

pub const RELAY_FULL: &str = "this relay is full";

pub const READ_BUDGET_BYTES: usize = 16 * 1024 * 1024;

pub fn page_bytes<'a>(items: impl IntoIterator<Item = &'a [u8]>) -> usize {
    items.into_iter().map(<[u8]>::len).sum()
}

pub fn page_may_continue(len: usize, bytes: usize) -> bool {
    len as u64 >= READ_PAGE || bytes > READ_BUDGET_BYTES - MAX_BODY_BYTES
}

pub fn page_has_room(len: usize, bytes: usize, next: usize) -> bool {
    len == 0 || (len < READ_PAGE as usize && bytes + next <= READ_BUDGET_BYTES)
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRead {
    pub mailbox: MailboxId,
    pub from: u64,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembersRead {
    pub mailbox: MailboxId,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetThreshold {
    pub mailbox: MailboxId,
    pub threshold: u16,
}

const REPLACE_DOMAIN: &[u8] = b"Zafe replace member v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceApproval {
    pub mailbox: MailboxId,
    pub old: [u8; 32],
    pub new: IdentityPublic,
}

impl ReplaceApproval {
    fn signing_bytes(&self) -> Result<Vec<u8>, ProtoError> {
        let mut out = REPLACE_DOMAIN.to_vec();
        out.extend_from_slice(&encode(self)?);
        Ok(out)
    }

    pub fn sign(&self, identity: &Identity) -> Result<Vec<u8>, ProtoError> {
        Ok(identity.sign(&self.signing_bytes()?).to_vec())
    }

    pub fn verify(&self, by: &IdentityPublic, signature: &[u8]) -> Result<(), ProtoError> {
        by.verify(&self.signing_bytes()?, signature)
    }
}

const SEAT_APPROVAL_DOMAIN: &[u8] = b"Zafe seat move approval v2";
const SEAT_FINISH_DOMAIN: &[u8] = b"Zafe seat move finish v1";
const SEAT_CANCEL_DOMAIN: &[u8] = b"Zafe seat move cancel v1";

pub fn seat_move_id(
    vault_id: &MailboxId,
    old: &[u8; 32],
    new: &IdentityPublic,
    round: u32,
) -> [u8; 32] {
    let mut h = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"Zafe_SeatMoveId_")
        .to_state();
    h.update(vault_id);
    h.update(old);
    h.update(&new.sig_pk);
    h.update(&new.enc_pk);
    h.update(&round.to_le_bytes());
    h.finalize().as_bytes().try_into().expect("32 bytes")
}

fn domain_bytes<T: Serialize>(domain: &[u8], value: &T) -> Result<Vec<u8>, ProtoError> {
    let mut out = domain.to_vec();
    out.extend_from_slice(&encode(value)?);
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatMoveApproval {
    pub vault_id: MailboxId,
    pub old: [u8; 32],
    pub new: IdentityPublic,
    pub round: u32,
}

impl SeatMoveApproval {
    pub fn move_id(&self) -> [u8; 32] {
        seat_move_id(&self.vault_id, &self.old, &self.new, self.round)
    }

    pub fn sign(&self, identity: &Identity) -> Result<Vec<u8>, ProtoError> {
        Ok(identity
            .sign(&domain_bytes(SEAT_APPROVAL_DOMAIN, self)?)
            .to_vec())
    }

    pub fn verify(&self, by: &IdentityPublic, signature: &[u8]) -> Result<(), ProtoError> {
        by.verify(&domain_bytes(SEAT_APPROVAL_DOMAIN, self)?, signature)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatFinish {
    pub mailbox: MailboxId,
    pub vault_id: MailboxId,
    pub move_id: [u8; 32],
    pub old: [u8; 32],
    pub new: [u8; 32],
}

impl SeatFinish {
    pub fn sign(&self, identity: &Identity) -> Result<Vec<u8>, ProtoError> {
        Ok(identity
            .sign(&domain_bytes(SEAT_FINISH_DOMAIN, self)?)
            .to_vec())
    }

    pub fn verify(&self, by: &IdentityPublic, signature: &[u8]) -> Result<(), ProtoError> {
        by.verify(&domain_bytes(SEAT_FINISH_DOMAIN, self)?, signature)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatCancel {
    pub mailbox: MailboxId,
    pub vault_id: MailboxId,
    pub move_id: [u8; 32],
    pub old: [u8; 32],
    pub new: [u8; 32],
    pub at: u64,
}

impl SeatCancel {
    pub fn sign(&self, identity: &Identity) -> Result<Vec<u8>, ProtoError> {
        Ok(identity
            .sign(&domain_bytes(SEAT_CANCEL_DOMAIN, self)?)
            .to_vec())
    }

    pub fn verify(&self, by: &IdentityPublic, signature: &[u8]) -> Result<(), ProtoError> {
        by.verify(&domain_bytes(SEAT_CANCEL_DOMAIN, self)?, signature)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatFinishPost {
    pub finish: SeatFinish,
    pub by: IdentityPublic,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatCancelPost {
    pub cancel: SeatCancel,
    pub signature: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelledSeat {
    pub cancel: SeatCancel,
    pub signature: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingSeat {
    pub move_id: [u8; 32],
    pub old: [u8; 32],
    pub new: IdentityPublic,
    pub round: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RecoveryValueKind {
    Announce,
    Commit,
    Challenge,
    Reveal,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryPut {
    pub subject: [u8; 32],
    pub cosigner: [u8; 32],
    pub kind: RecoveryValueKind,
    pub value: [u8; 32],
    pub timestamp: u64,
}

pub const PQ_KEY_LEN: usize = 1184;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PqKeyPut {
    pub subject: [u8; 32],
    pub owner: [u8; 32],
    pub key: Vec<u8>,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRead {
    pub subject: [u8; 32],
    pub timestamp: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryValues {
    pub values: Vec<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelledReplace {
    pub mailbox: MailboxId,
    pub approval: SeatMoveApproval,
    pub approvals: Vec<([u8; 32], Vec<u8>)>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reseed {
    pub mailbox: MailboxId,
    pub members: Vec<IdentityPublic>,
    pub threshold: u16,
    pub from: u64,
    pub entries: Vec<Vec<u8>>,
    pub finish: bool,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReseedResponse {
    pub len: u64,
    pub open: bool,
    pub restored: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxesRead {
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxesResponse {
    pub mailboxes: Vec<MailboxId>,
}

pub const MAX_WAIT_SECS: u32 = 25;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitRequest {
    pub mailbox: MailboxId,
    pub log_len: u64,
    pub inbox_after: u64,
    pub max_wait_secs: u32,
    pub timestamp: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitResponse {
    pub log_len: u64,
    pub inbox_cursor: u64,
}

impl WaitResponse {
    pub fn is_news(&self, log_len: u64, inbox_after: u64) -> bool {
        self.log_len > log_len || self.inbox_cursor > inbox_after
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembersResponse {
    pub members: Vec<IdentityPublic>,
    pub sealed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxInfo {
    pub members: Vec<IdentityPublic>,
    pub sealed: bool,
    pub threshold: u16,
    pub pending: Vec<PendingSeat>,
    pub cancelled: Vec<CancelledSeat>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxResponse {
    pub envelopes: Vec<(u64, Vec<u8>)>,
}

impl InboxResponse {
    pub fn decoded(&self) -> Vec<(u64, Envelope)> {
        self.envelopes
            .iter()
            .filter_map(|(cursor, bytes)| Some((*cursor, Envelope::from_bytes(bytes).ok()?)))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogResponse {
    pub entries: Vec<Vec<u8>>,
}

impl LogResponse {
    pub fn decoded(&self) -> Result<Vec<LogEntry>, ProtoError> {
        self.entries
            .iter()
            .map(|b| LogEntry::from_bytes(b))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppendResult {
    Appended { index: u64 },
    Conflict { len: u64 },
}

pub fn join_token_hash(token: &[u8; 32]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"Zafe_JoinToken__")
        .hash(token)
        .as_bytes()
        .try_into()
        .expect("32 bytes")
}

pub fn encode_body<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtoError> {
    Ok(version::encode(Format::RelayApi, value)?)
}

pub fn decode_body<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtoError> {
    Ok(version::decode(Format::RelayApi, bytes)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PushPlatform {
    Apns,
    Fcm,
    FcmFid,
}

impl PushPlatform {
    pub fn as_str(&self) -> &'static str {
        match self {
            PushPlatform::Apns => "apns",
            PushPlatform::Fcm => "fcm",
            PushPlatform::FcmFid => "fcm-fid",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterPush {
    pub mailbox: MailboxId,
    pub platform: PushPlatform,
    pub token: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgetPush {
    pub mailbox: MailboxId,
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelledAppend {
    pub mailbox: MailboxId,
    pub entry: Vec<u8>,
    pub timestamp: u64,
}

impl LabelledAppend {
    pub fn new(mailbox: MailboxId, entry: &LogEntry, timestamp: u64) -> Result<Self, ProtoError> {
        Ok(Self {
            mailbox,
            entry: entry.to_bytes()?,
            timestamp,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostEnvelope {
    pub envelope: Vec<u8>,
}

pub const CREATE_TOKEN_HEADER: &str = "zafe-create-token";

pub const TOO_MANY_SEAT_MOVES: &str = "too many open moves for this seat";

pub const MAX_PENDING_SEAT_MOVES: usize = 16;

pub const UNKNOWN_MAILBOX: &str = "unknown mailbox";

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayInfo {
    pub versions: RelayVersions,
    pub limits: RelayLimits,
    pub push: bool,
    pub create_token: bool,
    pub expiry: RelayExpiry,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayVersions {
    pub relay_api: u16,
    pub envelope: u16,
    pub log_entry: u16,
}

impl RelayVersions {
    pub const fn current() -> Self {
        Self {
            relay_api: version::RELAY_API,
            envelope: version::ENVELOPE,
            log_entry: version::LOG_ENTRY,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayLimits {
    pub max_body_bytes: u64,
    pub max_entry_bytes: u64,
    pub max_members: u16,
    pub log_bytes: Option<u64>,
    pub delivery_bytes: Option<u64>,
    pub envelopes_per_pair: Option<u64>,
    pub pair_bytes: Option<u64>,
    pub author_log_bytes: Option<u64>,
    pub mailboxes_per_key: Option<u64>,
    pub requests_per_minute: Option<u32>,
    pub delivery_retention_days: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayExpiry {
    pub idle_days: Option<u32>,
    pub unused_days: Option<u32>,
}
