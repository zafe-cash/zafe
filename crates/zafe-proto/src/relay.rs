//! Relay API types (spec §6). Request and response bodies are postcard behind a
//! [`version::RELAY_API`] tag (envelopes and log entries carry their own tags); every
//! request is signed by a member identity, over the API version too. A relay answers a
//! body tagged with a version it doesn't speak with HTTP 426 and
//! [`UNSUPPORTED_VERSION_HEADER`] set to the version it supports. The relay never sees
//! plaintext vault data.

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use crate::{
    envelope::{encode, Envelope, MailboxId},
    identity::{Identity, IdentityPublic},
    log::LogEntry,
    version::{self, Format},
    ProtoError,
};

const SIGNATURE_DOMAIN: &[u8] = b"Zafe relay request v1";

/// Response header of a 426 (Upgrade Required): the version of the rejected format that
/// the relay supports.
pub const UNSUPPORTED_VERSION_HEADER: &str = "zafe-supported-version";

/// Read requests must be at most this old (and not from the future) when they arrive.
pub const MAX_REQUEST_SKEW_SECS: u64 = 300;

/// A request payload signed by `signer`.
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

    /// Checks the signature against the embedded signer. Callers must separately check
    /// that the signer is allowed to make the request.
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

/// Opens a mailbox for a vault being created. Signed by the creator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateMailbox {
    pub mailbox: MailboxId,
    /// BLAKE2b hash of the invite's join token. The token is shared by all invited members
    /// and stops working when the creator seals membership.
    pub join_token_hash: [u8; 32],
    /// The vault's member count `n` (creator included); joins beyond it are refused.
    pub max_members: u16,
}

/// Removes a joined member before sealing (e.g. a stranger with a leaked invite). Signed by
/// the creator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Remove {
    pub mailbox: MailboxId,
    pub member: [u8; 32],
}

/// Cancels a vault setup: deletes the mailbox, its members and anything stored for it, so
/// the invite stops working. Signed by the creator, only before sealing (`POST
/// /v1/mailbox/cancel`, a newer route: a relay without it answers 404 and the app just
/// forgets the vault locally).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelSetup {
    pub mailbox: MailboxId,
}

/// How long an invite lets a new member join, in seconds, from when it was issued or
/// last renewed. An invite is a bearer credential until the setup locks: this bounds a
/// leaked one. Members already in the setup can always rejoin.
pub const INVITE_TTL_SECS: u64 = 72 * 3600;

/// Restarts an unlocked setup's invite clock ([`INVITE_TTL_SECS`]). Signed by the creator
/// (`POST /v1/mailbox/renew`, a newer route: a relay without it answers 404, and its
/// invites never expire).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenewInvite {
    pub mailbox: MailboxId,
}

/// Joins a mailbox using the invite's token. Signed by the joining member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Join {
    pub mailbox: MailboxId,
    pub join_token: [u8; 32],
}

/// Freezes membership to exactly `members` (their Ed25519 keys). Signed by the creator.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Seal {
    pub mailbox: MailboxId,
    pub members: Vec<[u8; 32]>,
}

/// Reads envelopes delivered to the signer after `after` (exclusive cursor).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxRead {
    pub mailbox: MailboxId,
    pub after: u64,
    pub timestamp: u64,
}

/// Most cursors one [`InboxAck`] may name.
pub const MAX_ACK_CURSORS: usize = 256;

/// Deletes envelopes the signer has handled (`POST /v1/inbox/ack`), by delivery cursor.
/// Only the signer's own deliveries are touched; unknown cursors are ignored. Ranges are
/// deliberately not supported: one inbox holds messages for different flows (a member's
/// signing requests and, as leader, the shares it collects), and each flow acknowledges
/// only what it finished. A relay without this endpoint answers 404; its deliveries
/// expire after the retention period instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxAck {
    pub mailbox: MailboxId,
    pub cursors: Vec<u64>,
    pub timestamp: u64,
}

/// How many deliveries an [`InboxAck`] deleted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxAckResponse {
    pub deleted: u64,
}

/// Most items one read returns (log entries, inbox envelopes). A log read that returns
/// fewer has reached the end of the log, so a client needs no second read to learn that.
pub const READ_PAGE: u64 = 500;

/// Reads log entries from index `from`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRead {
    pub mailbox: MailboxId,
    pub from: u64,
    pub timestamp: u64,
}

/// Lists the mailbox's current members.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembersRead {
    pub mailbox: MailboxId,
    pub timestamp: u64,
}

/// Records the vault's approval threshold `t` on the relay, so that moving a lost member's
/// seat to a new key ([`ReplaceMember`]) later needs `t` members' approvals. Signed by the
/// creator, before sealing (`POST /v1/mailbox/threshold`, a newer route: a relay without it
/// answers 404 and its vaults can't move seats).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetThreshold {
    pub mailbox: MailboxId,
    pub threshold: u16,
}

const REPLACE_DOMAIN: &[u8] = b"Zafe replace member v1";

/// What a member signs to approve moving the seat of `old` (a member who lost their
/// device) to the new key `new` (spec §10.1). The same signature goes into the vault log
/// (every member checks it there) and to the relay ([`ReplaceMember`]).
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

/// Which value of a seat recovery's safety check (commit, then reveal) a record holds.
/// Appended only: the postcard index is the wire format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RecoveryValueKind {
    /// The new phone's commitment to its secret value (signed by the new phone).
    Commit,
    /// A co-signer's random value (signed by that co-signer).
    Challenge,
    /// The new phone's secret value, sent once a challenge exists (signed by the new phone).
    Reveal,
}

/// Leaves one value of the safety check for the seat recovery of the new phone whose
/// signing key is `subject` (`POST /v1/recovery/put`, a newer route: an older relay
/// answers 404). The new phone isn't a vault member yet, so this can't go through a
/// mailbox. The relay keeps the signed bytes and serves them back; clients verify them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryPut {
    pub subject: [u8; 32],
    pub kind: RecoveryValueKind,
    pub value: [u8; 32],
    pub timestamp: u64,
}

/// Reads every value left for `subject` (`POST /v1/recovery/read`), signed by anyone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRead {
    pub subject: [u8; 32],
    pub timestamp: u64,
}

/// The values left for a subject: each a `Signed<RecoveryPut>` as it was posted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryValues {
    pub values: Vec<Vec<u8>>,
}

/// Moves a seat on the relay: `approval.old` leaves the member list and `approval.new`
/// joins, given approvals (signatures over `approval`) from at least the mailbox's
/// threshold of current members other than `old`. Signed by a current member or by `new`
/// (`POST /v1/mailbox/replace`). Repeating a replacement that already happened succeeds.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplaceMember {
    pub approval: ReplaceApproval,
    /// `(approver sig_pk, signature)`.
    pub approvals: Vec<([u8; 32], Vec<u8>)>,
}

/// [`ReplaceMember`] on a labelled mailbox (`POST /v1/mailbox/replace-labelled`, a newer
/// route: an older relay answers 404 with no body). The approvals are signed over
/// `approval`, whose `mailbox` is the **vault id** (the same signatures are in the vault
/// log); `mailbox` here is this relay's label for the vault, and the relay requires
/// `approval.mailbox` to be the vault id it stored from the mailbox's first entry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelledReplace {
    pub mailbox: MailboxId,
    pub approval: ReplaceApproval,
    /// `(approver sig_pk, signature)`.
    pub approvals: Vec<([u8; 32], Vec<u8>)>,
}

/// Restores a vault on a relay that lost it (wiped database, or moving to another relay)
/// from a member's saved copy of the log (`POST /v1/mailbox/reseed`; a newer route: a relay
/// without it answers 404). Signed by one of `members`.
///
/// The first request creates the mailbox with `members` and `threshold` (the vault's
/// membership as of the end of the log). Each request appends `entries` (as
/// [`LogEntry::to_bytes`]) at log position `from`; the relay checks their index, mailbox
/// and hash chain and each author's signature, but not that the authors are current
/// members (seats move: earlier entries were signed by earlier keys; members verify
/// authorship against the replayed membership as always). The last request sets `finish`,
/// which opens the mailbox for normal use. Until then only the signer, who made the
/// mailbox, can continue it, and it can't be joined (it has no join token). On a mailbox
/// that is already open the request changes nothing and the answer says where its log is:
/// catch up with ordinary appends instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reseed {
    pub mailbox: MailboxId,
    pub members: Vec<IdentityPublic>,
    pub threshold: u16,
    pub from: u64,
    pub entries: Vec<Vec<u8>>,
    pub finish: bool,
    /// Unix seconds; refused when older than [`MAX_REQUEST_SKEW_SECS`], so a recorded
    /// request can't be replayed to give a restored mailbox an outdated member list.
    pub timestamp: u64,
}

/// Where the relay's log for a [`Reseed`] stands after the request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReseedResponse {
    /// Entries the relay holds now. Differs from `from + entries.len()` when `from` didn't
    /// match (continue from `len`).
    pub len: u64,
    /// The mailbox is open for normal use (it was, or this request finished it).
    pub open: bool,
    /// This request built the mailbox (it created it or continued its restore); `false`
    /// when the mailbox was already open before the request and nothing changed.
    pub restored: bool,
}

/// Lists the mailboxes the signer is a member of (`POST /v1/mailboxes`): a device that
/// recovers a lost member's seat finds its vault once the seat has moved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxesRead {
    pub timestamp: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxesResponse {
    pub mailboxes: Vec<MailboxId>,
}

/// The longest a relay holds a [`WaitRequest`] open; longer requests are cut to this. It
/// stays below common proxy idle timeouts (Caddy, Fly.io), so a quiet wait ends with an
/// answer rather than a dropped connection.
pub const MAX_WAIT_SECS: u32 = 25;

/// Long poll (`POST /v1/wait`): answers as soon as the vault log is longer than `log_len`
/// or an envelope for the signer is delivered past cursor `inbox_after`, and otherwise
/// after `max_wait_secs` (at most [`MAX_WAIT_SECS`]). Signed by a member. The answer says
/// only where the log and the inbox are now; the client reads them as usual. A relay
/// without this endpoint answers 404 (clients fall back to polling); an unknown mailbox
/// gets 403 here, never 404, so the two can't be confused.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitRequest {
    pub mailbox: MailboxId,
    pub log_len: u64,
    pub inbox_after: u64,
    pub max_wait_secs: u32,
    pub timestamp: u64,
}

/// Where the mailbox is when a [`WaitRequest`] returns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitResponse {
    /// Entries in the vault log.
    pub log_len: u64,
    /// The newest delivery cursor for the signer (0: none stored).
    pub inbox_cursor: u64,
}

impl WaitResponse {
    /// Whether this is news for a client that has `log_len` entries and read its inbox up
    /// to `inbox_after`.
    pub fn is_news(&self, log_len: u64, inbox_after: u64) -> bool {
        self.log_len > log_len || self.inbox_cursor > inbox_after
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MembersResponse {
    pub members: Vec<IdentityPublic>,
    pub sealed: bool,
}

/// What the relay holds about a mailbox (`POST /v1/mailbox/info`, body [`MembersRead`]; a
/// newer route: a relay without it answers 404). Members compare it with the membership
/// the replayed log ends with, so a relay seeded with other keys or another threshold
/// (a malicious restorer) is noticed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailboxInfo {
    pub members: Vec<IdentityPublic>,
    pub sealed: bool,
    /// The approvals the relay wants to move a seat (0: unknown, seats can't move).
    pub threshold: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxResponse {
    /// `(cursor, Envelope::to_bytes)` in delivery order. Kept as bytes so one envelope in
    /// a version this client can't read doesn't make the whole page unreadable.
    pub envelopes: Vec<(u64, Vec<u8>)>,
}

impl InboxResponse {
    /// The envelopes this build can decode (others are dropped, like badly signed ones).
    pub fn decoded(&self) -> Vec<(u64, Envelope)> {
        self.envelopes
            .iter()
            .filter_map(|(cursor, bytes)| Some((*cursor, Envelope::from_bytes(bytes).ok()?)))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogResponse {
    /// `LogEntry::to_bytes` of each entry, in index order.
    pub entries: Vec<Vec<u8>>,
}

impl LogResponse {
    /// Decodes every entry. An entry in an unknown version stops here with
    /// [`ProtoError::UnsupportedVersion`]: the chain can't be followed past it.
    pub fn decoded(&self) -> Result<Vec<LogEntry>, ProtoError> {
        self.entries
            .iter()
            .map(|b| LogEntry::from_bytes(b))
            .collect()
    }
}

/// Result of a log append.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AppendResult {
    Appended {
        index: u64,
    },
    /// The entry did not extend the head; fetch from `len` and retry.
    Conflict {
        len: u64,
    },
}

/// `BLAKE2b-256("Zafe_JoinToken__", token)`.
pub fn join_token_hash(token: &[u8; 32]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"Zafe_JoinToken__")
        .hash(token)
        .as_bytes()
        .try_into()
        .expect("32 bytes")
}

/// A relay API body: `version (u16) || postcard(value)`.
pub fn encode_body<T: Serialize>(value: &T) -> Result<Vec<u8>, ProtoError> {
    Ok(version::encode(Format::RelayApi, value)?)
}

/// Decodes a relay API body, rejecting other API versions.
pub fn decode_body<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, ProtoError> {
    Ok(version::decode(Format::RelayApi, bytes)?)
}

/// Push notification platform for a member device.
///
/// `FcmFid` (appended 2026-10-22, so older values keep their postcard index): the target
/// is a Firebase Installation ID, sent in FCM v1's `fid` field, from apps that register
/// with `FirebaseMessaging.register()` (firebase_messaging 17; `token` is deprecated by
/// FCM). A relay without the variant rejects it: update relays before such apps ship.
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

/// Registers (or replaces) this member's push token for a mailbox. Notifications carry no
/// content, only "vault activity" (spec §6.1). Signed by the member.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisterPush {
    pub mailbox: MailboxId,
    pub platform: PushPlatform,
    pub token: String,
}

/// Deletes the signer's push token for `mailbox` (`POST /v1/push/forget`, a newer route: an
/// older relay answers 404). A phone sends it when its vault leaves this relay. Only the
/// signer's own registration is touched; an unknown mailbox or no token is not an error,
/// so this route never answers 404 itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForgetPush {
    pub mailbox: MailboxId,
    pub timestamp: u64,
}

/// Appends a log entry to the mailbox `mailbox` names on this relay (`POST
/// /v1/log/append-labelled`, a newer route: an older relay answers 404 with no body).
/// Signed by a member of that mailbox, who need not be the entry's author (catching a relay
/// up copies other members' entries). The mailbox id is this relay's label for the vault and
/// may differ from the vault id the entry carries (`EntryHeader::mailbox`); the relay keeps
/// the vault id of the mailbox's first entry and refuses entries of any other vault.
/// Answers [`AppendResult`], like `/v1/log/append`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LabelledAppend {
    pub mailbox: MailboxId,
    /// [`LogEntry::to_bytes`].
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

/// Request header carrying a private relay's create token (`ZAFE_RELAY_CREATE_TOKEN`), sent
/// with `/v1/mailbox/create` and `/v1/mailbox/reseed`. A relay that needs one and didn't get
/// the right one answers 401 with `www-authenticate:` [`CREATE_TOKEN_HEADER`].
pub const CREATE_TOKEN_HEADER: &str = "zafe-create-token";

/// The body of the relay's 404 for a mailbox it doesn't hold. A 404 without it comes from a
/// relay (or proxy) that doesn't have the route at all.
pub const UNKNOWN_MAILBOX: &str = "unknown mailbox";

/// What a relay says about itself (`GET /v1/info`, unauthenticated, a newer route: an older
/// relay answers 404). Nothing in it is about any mailbox. JSON, not a tagged postcard body,
/// so fields can be added without a [`version::RELAY_API`] bump: readers ignore unknown
/// fields and default missing ones.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayInfo {
    /// Versions of the formats the relay reads and writes.
    pub versions: RelayVersions,
    pub limits: RelayLimits,
    /// Whether the relay sends push notifications (it has a push service key).
    pub push: bool,
    /// Whether creating or restoring a mailbox needs a create token ([`CREATE_TOKEN_HEADER`]).
    pub create_token: bool,
    /// When the relay deletes mailboxes on its own.
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
    /// The versions this build speaks.
    pub const fn current() -> Self {
        Self {
            relay_api: version::RELAY_API,
            envelope: version::ENVELOPE,
            log_entry: version::LOG_ENTRY,
        }
    }
}

/// A relay's limits. `None` is unlimited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayLimits {
    /// Largest request body (bytes).
    pub max_body_bytes: u64,
    /// Most members one mailbox may have.
    pub max_members: u16,
    /// Bytes of vault log one mailbox may keep.
    pub log_bytes: Option<u64>,
    /// Bytes of undelivered envelopes one mailbox may keep.
    pub delivery_bytes: Option<u64>,
    /// Undelivered envelopes one member may have in one mailbox.
    pub envelopes_per_recipient: Option<u64>,
    /// Mailboxes one signing key may create.
    pub mailboxes_per_key: Option<u64>,
    /// Requests per minute per signing key.
    pub requests_per_minute: Option<u32>,
    /// Days an undelivered envelope is kept.
    pub delivery_retention_days: u32,
}

/// When a relay deletes a mailbox (and everything stored for it) on its own. `None`: never.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct RelayExpiry {
    /// Days without any member request.
    pub idle_days: Option<u32>,
    /// Days after creation within which a member other than the creator must make a request.
    pub unused_days: Option<u32>,
}
