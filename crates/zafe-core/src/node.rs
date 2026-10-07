//! Member orchestration over the relay (spec §7, §9): what one member's device does.
//!
//! Transport-level steps are async functions over a [`RelayClient`]. They are written so a
//! CLI can run each step as a separate command, and so the mobile app can reuse them.

use std::{
    collections::BTreeMap,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use orchard::keys::Scope;
use rand_core::{CryptoRng, RngCore};
use reddsa::frost::redpallas::{
    keys::{dkg, KeyPackage, PublicKeyPackage},
    Identifier,
};
use serde::{Deserialize, Serialize};
use zafe_proto::{
    relay::AppendResult,
    safety_number,
    version::{self, DecodeError, Format, UnsupportedVersion},
    Chain, Envelope, Identity, IdentityPublic, Kind, LogEntry, LogKey, MailboxId, ProtoError,
};
use zcash_keys::address::UnifiedAddress;
use zcash_protocol::consensus::Parameters;

use crate::{
    keygen::{member_identifier, KeygenParams, Round1, SkContribution},
    keys::VaultSecret,
    relay_client::{RelayClient, RelayClientError},
    vault::{MemberInfo, VaultDescriptor, VaultError, VaultEvent, VaultState},
};

#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    #[error(transparent)]
    Relay(#[from] RelayClientError),
    #[error("invalid invite")]
    BadInvite,
    #[error("membership is not ready: {0}")]
    NotReady(String),
    #[error("safety number mismatch: relay shows {actual}, you confirmed {confirmed}")]
    SafetyNumberMismatch { actual: String, confirmed: String },
    #[error("echo mismatch from a member: the relay showed members different round-1 packages")]
    EchoMismatch,
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    /// This member's independent check of a proposal failed: do not approve or sign it.
    #[error("verification failed: {0}")]
    Verification(String),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error(transparent)]
    Wallet(#[from] crate::wallet::WalletError),
    /// The event is not valid on top of the current log (every member would ignore it).
    #[error("event no longer valid: {0}")]
    Invalid(crate::vault::VaultError),
    /// The relay has fewer log entries than this device already saw (an older database
    /// restored, or lost data): `local_len` entries are saved here. Nothing it says about
    /// the vault can be trusted until a member restores the log from a device copy.
    #[error("the relay lost part of the vault log: this device has {local_len} entries")]
    RelayRolledBack { local_len: u64 },
    /// The relay served another entry than the one this device saved at `index`: the log
    /// it shows has forked from the history this device has seen.
    #[error("the relay's vault log differs from this device's copy at entry {index}")]
    RelayForked { index: u64 },
    /// The relay doesn't know this vault any more (wiped database) although this device
    /// saved `local_len` entries of its log.
    #[error("the relay no longer has this vault: this device has {local_len} log entries")]
    RelayLostVault { local_len: u64 },
    /// This device's copy of the log can't be read or written.
    #[error(transparent)]
    LogCopy(#[from] crate::log_cache::LogCacheError),
    /// Data or a message in a version this build doesn't read (see
    /// [`UnsupportedVersion::is_newer`]: newer means "update the app").
    #[error(transparent)]
    UnsupportedVersion(#[from] UnsupportedVersion),
}

fn proto(e: impl core::fmt::Debug) -> NodeError {
    NodeError::Protocol(format!("{e:?}"))
}

impl From<DecodeError> for NodeError {
    fn from(e: DecodeError) -> Self {
        match e {
            DecodeError::Unsupported(v) => NodeError::UnsupportedVersion(v),
            DecodeError::Malformed(f) => NodeError::Protocol(format!("malformed {f}")),
        }
    }
}

/// Keeps version errors typed; everything else becomes a protocol error.
fn vault_err(e: crate::vault::VaultError) -> NodeError {
    match e {
        crate::vault::VaultError::UnsupportedVersion(v) => NodeError::UnsupportedVersion(v),
        e => proto(e),
    }
}

fn chain_err(e: zafe_proto::ChainError) -> NodeError {
    match e {
        // An entry that doesn't extend the head we hold: the relay forked the history.
        zafe_proto::ChainError::Fork(index) => NodeError::RelayForked { index },
        zafe_proto::ChainError::Invalid {
            source: ProtoError::UnsupportedVersion(v),
            ..
        } => NodeError::UnsupportedVersion(v),
        e => proto(e),
    }
}

/// Everything a new member needs to join (shared out of band as a string or QR code).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub mailbox: MailboxId,
    pub join_token: [u8; 32],
    /// Creator's Ed25519 key, so joiners know whose round-1 message carries the birthday.
    pub creator: [u8; 32],
    pub threshold: u16,
    pub members: u16,
    pub name: String,
}

impl Invite {
    /// Text form: `zafe-invite-v<version::INVITE>:<hex postcard>`.
    const PREFIX: &'static str = "zafe-invite-v";

    pub fn encode(&self) -> String {
        format!(
            "{}{}:{}",
            Self::PREFIX,
            version::INVITE,
            hex::encode(postcard::to_allocvec(self).expect("encodable"))
        )
    }

    /// Parses an invite; one from another version of Zafe fails with
    /// [`NodeError::UnsupportedVersion`] rather than "invalid invite".
    pub fn decode(s: &str) -> Result<Self, NodeError> {
        let rest = s
            .trim()
            .strip_prefix(Self::PREFIX)
            .ok_or(NodeError::BadInvite)?;
        let (found, body) = rest.split_once(':').ok_or(NodeError::BadInvite)?;
        let found: u16 = found.parse().map_err(|_| NodeError::BadInvite)?;
        version::check(Format::Invite, found)?;
        postcard::from_bytes(&hex::decode(body).map_err(|_| NodeError::BadInvite)?)
            .map_err(|_| NodeError::BadInvite)
    }
}

/// Per-sender envelope sequence numbers: strictly increasing and clock-based, so they stay
/// increasing across separate processes without persistence.
#[derive(Default)]
pub struct SeqCounter(u64);

impl SeqCounter {
    pub fn next_seq(&mut self) -> u64 {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);
        self.0 = self.0.saturating_add(1).max(now);
        self.0
    }
}

/// A member's long-lived vault material after creation. `vault_secret`, `key_package` and
/// `log_key` are secrets: on devices they belong in secure storage (spec §14).
#[derive(Clone, Serialize, Deserialize)]
pub struct VaultMaterial {
    pub descriptor: VaultDescriptor,
    pub key_package: Vec<u8>,
    pub public_key_package: Vec<u8>,
    pub vault_secret: [u8; 32],
    pub log_key_epoch: u32,
    pub log_key: [u8; 32],
}

impl core::fmt::Debug for VaultMaterial {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VaultMaterial")
            .field("vault", &self.descriptor.name)
            .field("address", &self.descriptor.address)
            .finish_non_exhaustive()
    }
}

impl VaultMaterial {
    /// Versioned storage form (device secure storage, CLI `vault.bin`, backups). Secret.
    pub fn to_bytes(&self) -> Result<Vec<u8>, NodeError> {
        Ok(version::encode(Format::VaultMaterial, self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NodeError> {
        let material: Self = version::decode(Format::VaultMaterial, bytes)?;
        version::check(Format::Descriptor, material.descriptor.version)?;
        Ok(material)
    }

    pub fn key_package(&self) -> Result<KeyPackage, NodeError> {
        KeyPackage::deserialize(&self.key_package).map_err(proto)
    }

    pub fn public_key_package(&self) -> Result<PublicKeyPackage, NodeError> {
        PublicKeyPackage::deserialize(&self.public_key_package).map_err(proto)
    }

    pub fn log_key(&self) -> LogKey {
        LogKey::from_bytes(self.log_key_epoch, self.log_key)
    }

    pub fn vault_keys(&self) -> Result<crate::keys::VaultKeys, NodeError> {
        crate::keys::VaultKeys::derive(
            &VaultSecret::from_bytes(self.vault_secret),
            &self.descriptor.group_public_key,
        )
        .map_err(proto)
    }

    pub fn member_identities(&self) -> Vec<IdentityPublic> {
        self.descriptor.members.iter().map(|m| m.identity).collect()
    }
}

// --- Setup: create, join, seal, safety number ------------------------------------------

pub async fn create_vault<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    creator: &Identity,
    name: &str,
    threshold: u16,
    members: u16,
    rng: &mut R,
) -> Result<Invite, NodeError> {
    let mut mailbox = [0u8; 16];
    let mut join_token = [0u8; 32];
    rng.fill_bytes(&mut mailbox);
    rng.fill_bytes(&mut join_token);
    KeygenParams::new(mailbox, threshold, members).map_err(proto)?;
    relay
        .create_mailbox(creator, mailbox, &join_token, members)
        .await?;
    // Lets the relay check approvals when a lost member's seat moves (spec §10.1). An
    // older relay without the route still hosts the vault, without seat moves.
    relay.set_threshold(creator, mailbox, threshold).await?;
    Ok(Invite {
        mailbox,
        join_token,
        creator: creator.public().sig_pk,
        threshold,
        members,
        name: name.to_owned(),
    })
}

pub async fn join_vault(
    relay: &RelayClient,
    member: &Identity,
    invite: &Invite,
) -> Result<(), NodeError> {
    relay
        .join(member, invite.mailbox, invite.join_token)
        .await?;
    Ok(())
}

/// Current members (as the relay reports them), whether membership is sealed, and the
/// safety number to compare out of band.
pub async fn membership(
    relay: &RelayClient,
    who: &Identity,
    invite: &Invite,
) -> Result<(Vec<IdentityPublic>, bool, String), NodeError> {
    let response = relay.members(who, invite.mailbox).await?;
    let number = safety_number(&invite.mailbox, &response.members);
    Ok((response.members, response.sealed, number))
}

/// Creator only: freezes membership once all `n` members have joined.
pub async fn seal(
    relay: &RelayClient,
    creator: &Identity,
    invite: &Invite,
) -> Result<(), NodeError> {
    let (members, _, _) = membership(relay, creator, invite).await?;
    if members.len() != usize::from(invite.members) {
        return Err(NodeError::NotReady(format!(
            "{} of {} members joined",
            members.len(),
            invite.members
        )));
    }
    relay
        .seal(
            creator,
            invite.mailbox,
            members.iter().map(|m| m.sig_pk).collect(),
        )
        .await?;
    Ok(())
}

// --- Key generation over the relay -----------------------------------------------------

#[derive(Serialize, Deserialize)]
struct Round1Msg {
    package: Vec<u8>,
    /// Set by the creator only: the vault birthday height all descriptors use.
    birthday_height: Option<u32>,
    /// Set by the creator only: the proposal expiry window, in blocks.
    proposal_expiry_blocks: Option<u32>,
    /// Commitment to this member's `sk` contribution (revealed in round 2). Format 3.
    sk_commitment: [u8; 32],
}

/// Collects opened envelopes by (kind, sender) from the inbox, keeping the cursor.
struct Inbox<'a> {
    relay: &'a RelayClient,
    me: &'a Identity,
    mailbox: MailboxId,
    members: BTreeMap<[u8; 32], IdentityPublic>,
    cursor: u64,
    received: BTreeMap<(u8, [u8; 32]), Vec<u8>>,
}

fn kind_tag(kind: Kind) -> u8 {
    kind as u8
}

impl Inbox<'_> {
    async fn poll(&mut self) -> Result<(), NodeError> {
        for (cursor, sender, envelope) in read_inbox(
            self.relay,
            self.me,
            self.mailbox,
            &self.members,
            self.cursor,
        )
        .await?
        {
            self.cursor = cursor;
            if let Ok(payload) = envelope.open(self.me, &sender) {
                // First message of each kind from each sender wins; later copies are ignored.
                self.received
                    .entry((kind_tag(envelope.header.kind), envelope.header.from))
                    .or_insert(payload);
            }
        }
        Ok(())
    }

    /// Waits until a message of `kind` has arrived from every sender in `from`.
    async fn wait_all(
        &mut self,
        kind: Kind,
        from: &[[u8; 32]],
        deadline: Instant,
        what: &'static str,
    ) -> Result<BTreeMap<[u8; 32], Vec<u8>>, NodeError> {
        loop {
            self.poll().await?;
            let got: BTreeMap<_, _> = from
                .iter()
                .filter_map(|pk| {
                    self.received
                        .get(&(kind_tag(kind), *pk))
                        .map(|p| (*pk, p.clone()))
                })
                .collect();
            if got.len() == from.len() {
                return Ok(got);
            }
            if Instant::now() > deadline {
                return Err(NodeError::Timeout(what));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

/// Runs the whole key-generation ceremony for this member (spec §7.2), blocking until done.
///
/// `confirmed_safety_number` is what the user compared out of band; the ceremony refuses to
/// start if the relay's member set produces a different one. The creator passes the vault
/// birthday height and the proposal expiry window (`None`: the default); other members
/// take both from the creator's round-1 message. Every member signs the descriptor that
/// holds them, so a creator can't give members different values.
#[allow(clippy::too_many_arguments)]
pub async fn run_keygen<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    invite: &Invite,
    confirmed_safety_number: &str,
    network: &P,
    network_name: &str,
    creator_birthday_height: Option<u32>,
    creator_expiry_blocks: Option<u32>,
    rng: &mut R,
    timeout: Duration,
) -> Result<VaultMaterial, NodeError> {
    let deadline = Instant::now() + timeout;
    let mut seq = SeqCounter::default();
    let is_creator = me.public().sig_pk == invite.creator;

    // Membership must be sealed and match what the user confirmed.
    let (members, sealed, number) = membership(relay, me, invite).await?;
    if !sealed || members.len() != usize::from(invite.members) {
        return Err(NodeError::NotReady("membership is not sealed yet".into()));
    }
    if number != confirmed_safety_number.trim() {
        return Err(NodeError::SafetyNumberMismatch {
            actual: number,
            confirmed: confirmed_safety_number.into(),
        });
    }
    let params =
        KeygenParams::new(invite.mailbox, invite.threshold, invite.members).map_err(proto)?;
    let by_pk: BTreeMap<[u8; 32], IdentityPublic> =
        members.iter().map(|m| (m.sig_pk, *m)).collect();
    let others: Vec<[u8; 32]> = by_pk
        .keys()
        .filter(|pk| **pk != me.public().sig_pk)
        .copied()
        .collect();
    let frost_id = |pk: &[u8; 32]| member_identifier(pk, &invite.mailbox).map_err(proto);
    let id_to_pk: BTreeMap<Identifier, [u8; 32]> = by_pk
        .keys()
        .map(|pk| Ok((frost_id(pk)?, *pk)))
        .collect::<Result<_, NodeError>>()?;

    let mut inbox = Inbox {
        relay,
        me,
        mailbox: invite.mailbox,
        members: by_pk.clone(),
        cursor: 0,
        received: BTreeMap::new(),
    };

    // Round 1: broadcast, with a commitment to this member's `sk` contribution.
    let round1 = Round1::start(params, frost_id(&me.public().sig_pk)?, rng).map_err(proto)?;
    let contribution = SkContribution::generate(rng);
    let my_commitment = contribution.commitment(&invite.mailbox, &me.public().sig_pk);
    let msg = Round1Msg {
        sk_commitment: my_commitment,
        package: round1.package().serialize().map_err(proto)?,
        birthday_height: if is_creator {
            creator_birthday_height
        } else {
            None
        },
        proposal_expiry_blocks: if is_creator {
            Some(creator_expiry_blocks.unwrap_or(crate::vault::DEFAULT_PROPOSAL_EXPIRY_BLOCKS))
        } else {
            None
        },
    };
    let payload = version::encode(Format::DkgRound1, &msg)?;
    relay
        .send(
            &Envelope::public(
                me,
                invite.mailbox,
                seq.next_seq(),
                Kind::DkgRound1,
                &payload,
            )
            .map_err(proto)?,
        )
        .await?;

    let r1 = inbox
        .wait_all(Kind::DkgRound1, &others, deadline, "round-1 packages")
        .await?;
    let mut birthday_height = if is_creator {
        creator_birthday_height
    } else {
        None
    };
    let mut expiry_blocks = msg.proposal_expiry_blocks;
    let mut received1 = BTreeMap::new();
    let mut commitments = BTreeMap::from([(frost_id(&me.public().sig_pk)?, my_commitment)]);
    for (pk, bytes) in &r1 {
        let m: Round1Msg = version::decode(Format::DkgRound1, bytes)?;
        commitments.insert(frost_id(pk)?, m.sk_commitment);
        if *pk == invite.creator {
            birthday_height = m.birthday_height;
            expiry_blocks = m.proposal_expiry_blocks;
        }
        received1.insert(
            frost_id(pk)?,
            dkg::round1::Package::deserialize(&m.package).map_err(proto)?,
        );
    }
    let birthday_height =
        birthday_height.ok_or_else(|| NodeError::Protocol("creator sent no birthday".into()))?;
    let proposal_expiry_blocks = expiry_blocks
        .filter(|b| {
            (crate::vault::MIN_PROPOSAL_EXPIRY_BLOCKS..=crate::vault::MAX_PROPOSAL_EXPIRY_BLOCKS)
                .contains(b)
        })
        .ok_or_else(|| {
            NodeError::Protocol("creator sent no valid proposal expiry window".into())
        })?;

    // Round 2: echo hash (broadcast), round-2 packages and sk contributions (sealed).
    let (round2, outgoing) = round1.advance(received1).map_err(proto)?;
    // Members also agree on every `sk` commitment before any contribution counts.
    let echo = crate::keygen::echo_with_commitments(&round2.echo(), &commitments);
    relay
        .send(
            &Envelope::public(me, invite.mailbox, seq.next_seq(), Kind::DkgEcho, &echo)
                .map_err(proto)?,
        )
        .await?;
    for (to_id, package) in outgoing {
        let to = by_pk[&id_to_pk[&to_id]];
        let bytes = package.serialize().map_err(proto)?;
        relay
            .send(
                &Envelope::sealed(
                    me,
                    &to,
                    invite.mailbox,
                    seq.next_seq(),
                    Kind::DkgRound2,
                    &bytes,
                    rng,
                )
                .map_err(proto)?,
            )
            .await?;
        relay
            .send(
                &Envelope::sealed(
                    me,
                    &to,
                    invite.mailbox,
                    seq.next_seq(),
                    Kind::SkContribution,
                    contribution.as_bytes(),
                    rng,
                )
                .map_err(proto)?,
            )
            .await?;
    }

    for (_, their_echo) in inbox
        .wait_all(Kind::DkgEcho, &others, deadline, "echo hashes")
        .await?
    {
        if their_echo.as_slice() != echo.as_slice() {
            return Err(NodeError::EchoMismatch);
        }
    }
    let r2 = inbox
        .wait_all(Kind::DkgRound2, &others, deadline, "round-2 packages")
        .await?;
    let received2 = r2
        .iter()
        .map(|(pk, b)| {
            Ok((
                frost_id(pk)?,
                dkg::round2::Package::deserialize(b).map_err(proto)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>, NodeError>>()?;
    let dkg_result = round2.finish(received2).map_err(proto)?;

    let sk_msgs = inbox
        .wait_all(Kind::SkContribution, &others, deadline, "sk contributions")
        .await?;
    let mut contributions = BTreeMap::new();
    contributions.insert(frost_id(&me.public().sig_pk)?, contribution);
    for (pk, bytes) in sk_msgs {
        let arr: [u8; 32] = bytes.as_slice().try_into().map_err(proto)?;
        let id = frost_id(&pk)?;
        let revealed = SkContribution::from_bytes(arr);
        crate::keygen::check_contribution(&invite.mailbox, &pk, &commitments[&id], &revealed)
            .map_err(|e| NodeError::Protocol(e.to_string()))?;
        contributions.insert(id, revealed);
    }
    let output = dkg_result.finish(&contributions).map_err(proto)?;

    // Descriptor: identical on every member, then signed by all.
    let fvk = output.vault_keys.fvk();
    let address =
        UnifiedAddress::from_receivers(Some(fvk.address_at(0u32, Scope::External)), None, None)
            .ok_or_else(|| NodeError::Protocol("cannot build unified address".into()))?
            .encode(network);
    let descriptor = VaultDescriptor {
        vault_id: invite.mailbox,
        version: version::DESCRIPTOR,
        name: invite.name.clone(),
        network: network_name.to_owned(),
        threshold: invite.threshold,
        members: by_pk
            .values()
            .map(|m| {
                Ok(MemberInfo {
                    identity: *m,
                    frost_id: frost_id(&m.sig_pk)?.serialize(),
                    name: hex::encode(&m.sig_pk[..4]),
                })
            })
            .collect::<Result<_, NodeError>>()?,
        group_public_key: *output.vault_keys.ak(),
        ufvk: output.vault_keys.ufvk().map_err(proto)?.encode(network),
        address,
        use_qsk: true,
        proposal_expiry_blocks,
        birthday_height,
        epoch: 0,
        transcript_hash: output.transcript_hash,
    };
    let my_sig = me
        .sign(&descriptor.signing_message().map_err(proto)?)
        .to_vec();
    relay
        .send(
            &Envelope::public(
                me,
                invite.mailbox,
                seq.next_seq(),
                Kind::DescriptorSignature,
                &my_sig,
            )
            .map_err(proto)?,
        )
        .await?;
    let mut signatures = vec![(me.public().sig_pk, my_sig)];
    let message = descriptor.signing_message().map_err(proto)?;
    for (pk, sig) in inbox
        .wait_all(
            Kind::DescriptorSignature,
            &others,
            deadline,
            "descriptor signatures",
        )
        .await?
    {
        by_pk[&pk]
            .verify(&message, &sig)
            .map_err(|_| NodeError::Protocol("bad descriptor signature".into()))?;
        signatures.push((pk, sig));
    }

    // Log key: the creator generates it and seals it to every member, then writes the
    // VaultCreated entry. Others wait for both.
    let log_key = if is_creator {
        let key = LogKey::generate(0, rng);
        for pk in &others {
            let mut bytes = 0u32.to_le_bytes().to_vec();
            bytes.extend_from_slice(key.as_bytes());
            relay
                .send(
                    &Envelope::sealed(
                        me,
                        &by_pk[pk],
                        invite.mailbox,
                        seq.next_seq(),
                        Kind::LogKey,
                        &bytes,
                        rng,
                    )
                    .map_err(proto)?,
                )
                .await?;
        }
        let event = VaultEvent::Created {
            descriptor: descriptor.clone(),
            signatures,
        };
        let entry = LogEntry::create(
            me,
            &key,
            invite.mailbox,
            0,
            [0; 32],
            &event.to_bytes().map_err(proto)?,
            rng,
        )
        .map_err(proto)?;
        match relay.append_log(&entry).await? {
            AppendResult::Appended { .. } => {}
            AppendResult::Conflict { len } => {
                return Err(NodeError::Protocol(format!(
                    "log already has {len} entries"
                )))
            }
        }
        key
    } else {
        let msgs = inbox
            .wait_all(Kind::LogKey, &[invite.creator], deadline, "log key")
            .await?;
        let bytes = &msgs[&invite.creator];
        if bytes.len() != 36 {
            return Err(NodeError::Protocol("bad log key".into()));
        }
        let epoch = u32::from_le_bytes(bytes[..4].try_into().expect("4 bytes"));
        LogKey::from_bytes(epoch, bytes[4..].try_into().expect("32 bytes"))
    };

    // Everyone checks the log's first entry is the descriptor they signed.
    let state = loop {
        let entries = relay.read_log(me, invite.mailbox, 0).await?;
        if !entries.is_empty() {
            let mut chain = Chain::new(invite.mailbox);
            for e in entries {
                chain.append(e, &members).map_err(chain_err)?;
            }
            break VaultState::replay(chain.entries(), &log_key).map_err(vault_err)?;
        }
        if Instant::now() > deadline {
            return Err(NodeError::Timeout("VaultCreated log entry"));
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    };
    if state.descriptor != descriptor {
        return Err(NodeError::Protocol(
            "logged descriptor differs from ours".into(),
        ));
    }

    Ok(VaultMaterial {
        descriptor,
        key_package: output.key_package.serialize().map_err(proto)?,
        public_key_package: output.public_key_package.serialize().map_err(proto)?,
        vault_secret: *output.vault_secret.as_bytes(),
        log_key_epoch: log_key.epoch,
        log_key: *log_key.as_bytes(),
    })
}

// --- Relay reads -------------------------------------------------------------------------

/// Reads every envelope delivered to `me` after `after`, following pagination. Envelopes
/// for another mailbox, from non-members, with bad signatures, or replayed/reordered
/// (non-increasing seq per sender) are dropped. Returns `(cursor, sender, envelope)`.
pub async fn read_inbox(
    relay: &RelayClient,
    me: &Identity,
    mailbox: MailboxId,
    members: &BTreeMap<[u8; 32], IdentityPublic>,
    after: u64,
) -> Result<Vec<(u64, IdentityPublic, Envelope)>, NodeError> {
    let mut out = Vec::new();
    let mut cursor = after;
    let mut guard = ReplayGuard::default();
    loop {
        let page = relay.inbox(me, mailbox, cursor).await?;
        let Some((last, _)) = page.last() else { break };
        cursor = *last;
        for (c, envelope) in page {
            if envelope.header.mailbox != mailbox {
                continue;
            }
            let Some(sender) = members.get(&envelope.header.from) else {
                continue;
            };
            if envelope.verify(sender).is_err() || guard.check_and_record(&envelope.header).is_err()
            {
                continue;
            }
            out.push((c, *sender, envelope));
        }
    }
    Ok(out)
}

// --- Vault log helpers -------------------------------------------------------------------

/// Reads new log entries after the chain's head (following pagination), verifying each
/// against the membership as of that entry (seats can move) and applying it to `state`.
/// Saves the longer chain to this device's copy.
async fn catch_up(
    relay: &RelayClient,
    me: &Identity,
    key: &LogKey,
    chain: &mut Chain,
    state: &mut VaultState,
) -> Result<(), NodeError> {
    let before = chain.len();
    loop {
        let batch = relay
            .read_log(me, state.descriptor.vault_id, chain.len())
            .await?;
        if batch.is_empty() {
            break;
        }
        for entry in batch {
            chain
                .append(entry.clone(), &state.member_identities())
                .map_err(chain_err)?;
            state.apply_entry(&entry, key);
        }
    }
    if chain.len() > before {
        save_copy(relay, chain);
    }
    Ok(())
}

/// Saves the verified chain to this device's log copy, when there is one. Best effort: a
/// failed write leaves the older (shorter) anchor in place, and the log itself is safe on
/// the relay.
fn save_copy(relay: &RelayClient, chain: &Chain) {
    if let Some(cache) = relay.log_cache() {
        if let Err(e) = cache.write(&chain.mailbox(), chain.entries()) {
            eprintln!("log copy: {e}");
        }
    }
}

/// Starts a chain and replay from `first`, which must be the vault's `Created` entry.
fn start_chain(
    first: &LogEntry,
    mailbox: MailboxId,
    key: &LogKey,
) -> Result<(Chain, VaultState), NodeError> {
    let state = VaultState::replay(std::slice::from_ref(first), key).map_err(vault_err)?;
    if state.descriptor.vault_id != mailbox {
        return Err(NodeError::Protocol(
            "the vault log belongs to another vault".into(),
        ));
    }
    let mut chain = Chain::new(mailbox);
    chain
        .append(first.clone(), &state.member_identities())
        .map_err(chain_err)?;
    Ok((chain, state))
}

/// Verifies `entries` on top of `chain` and applies them to `state`.
fn extend_chain(
    chain: &mut Chain,
    state: &mut VaultState,
    entries: impl IntoIterator<Item = LogEntry>,
    key: &LogKey,
) -> Result<(), NodeError> {
    for entry in entries {
        chain
            .append(entry.clone(), &state.member_identities())
            .map_err(chain_err)?;
        state.apply_entry(&entry, key);
    }
    Ok(())
}

/// The chain and state rebuilt from the entries this device saved, verified like entries
/// from the relay. `None` when there is no usable copy.
fn load_copy(
    relay: &RelayClient,
    mailbox: MailboxId,
    key: &LogKey,
) -> Result<Option<(Chain, VaultState)>, NodeError> {
    let Some(cache) = relay.log_cache() else {
        return Ok(None);
    };
    let saved = cache.read(&mailbox)?;
    let Some((first, rest)) = saved.split_first() else {
        return Ok(None);
    };
    // A copy that no longer verifies (a different key, damage) is not an anchor.
    let rebuilt = start_chain(first, mailbox, key).and_then(|(mut chain, mut state)| {
        extend_chain(&mut chain, &mut state, rest.iter().cloned(), key)?;
        Ok((chain, state))
    });
    Ok(rebuilt.ok())
}

/// Reads and verifies the whole log, returning the chain and the replayed state.
pub async fn load_state(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
) -> Result<(Chain, VaultState), NodeError> {
    // Boxed: callers nest this in large async functions, and the unboxed future made
    // their layout deeper than rustc's query depth limit.
    let (chain, state) = Box::pin(load_log(
        relay,
        me,
        material.descriptor.vault_id,
        &material.log_key(),
    ))
    .await?;
    let d = &state.descriptor;
    if d.group_public_key != material.descriptor.group_public_key
        || d.ufvk != material.descriptor.ufvk
    {
        return Err(NodeError::Protocol(
            "the vault log belongs to another vault".into(),
        ));
    }
    Ok((chain, state))
}

/// Reads and verifies the log of `mailbox` with `key`, without the member's material (a
/// device recovering a seat has only the log key at first). The `Created` entry is checked
/// against the descriptor it carries, which every member signed; later entries against the
/// membership as of each entry.
pub async fn load_log(
    relay: &RelayClient,
    me: &Identity,
    mailbox: MailboxId,
    key: &LogKey,
) -> Result<(Chain, VaultState), NodeError> {
    let (mut chain, mut state, saved_len) = match load_copy(relay, mailbox, key)? {
        Some((mut chain, mut state)) => {
            let saved_len = chain.len();
            // The copy is the anchor: the relay must still hold its last entry, and the
            // same one. Everything after it is new and verified on top.
            let last = chain.entries().last().expect("a saved copy has entries");
            let last_hash = last.hash().map_err(proto)?;
            let from = saved_len - 1;
            let page = match relay.read_log(me, mailbox, from).await {
                Err(RelayClientError::Status { status: 404, .. }) => {
                    return Err(NodeError::RelayLostVault {
                        local_len: saved_len,
                    })
                }
                other => other?,
            };
            let Some(first) = page.first() else {
                return Err(NodeError::RelayRolledBack {
                    local_len: saved_len,
                });
            };
            if first.header.index != from || first.hash().map_err(proto)? != last_hash {
                return Err(NodeError::RelayForked { index: from });
            }
            extend_chain(&mut chain, &mut state, page.into_iter().skip(1), key)?;
            (chain, state, saved_len)
        }
        None => {
            let first_page = relay.read_log(me, mailbox, 0).await?;
            let first = first_page.first().ok_or(NodeError::Invalid(
                crate::vault::VaultError::NotCreatedFirst,
            ))?;
            let (mut chain, mut state) = start_chain(first, mailbox, key)?;
            extend_chain(&mut chain, &mut state, first_page.into_iter().skip(1), key)?;
            (chain, state, 0)
        }
    };
    catch_up(relay, me, key, &mut chain, &mut state).await?;
    if saved_len == 0 || chain.len() > saved_len {
        save_copy(relay, &chain);
    }
    Ok((chain, state))
}

/// What [`reseed_relay`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reseeded {
    /// The relay had no such vault: it was recreated with this many log entries.
    Restored { entries: u64 },
    /// The relay had an older part of the log: this many entries were appended to it.
    CaughtUp { appended: u64 },
    /// The relay's log is as long as, or longer than, this device's copy and agrees with it.
    Current,
}

/// Roughly how much log goes into one reseed request (the relay's body limit is 1 MiB).
const RESEED_BATCH_BYTES: usize = 512 * 1024;

/// Restores the vault's log on the relay from this device's copy (spec §6.3): after the
/// relay lost its database (or was rolled back to an older one), or when the members move
/// to another relay (change the URL in Settings, then restore from the member whose copy
/// is longest). A relay that doesn't know the vault is recreated with the membership
/// the log ends with and the whole log; one that holds an older part of the same log gets
/// the missing entries appended. Pending messages (signing requests, repair messages) are
/// not restored: they are asked for again. The device re-registers its push token after.
///
/// Never makes the relay agree with a log it doesn't already match: a relay whose entries
/// differ from the copy is [`NodeError::RelayForked`].
pub async fn reseed_relay(
    relay: &RelayClient,
    me: &Identity,
    mailbox: MailboxId,
    key: &LogKey,
) -> Result<Reseeded, NodeError> {
    use zafe_proto::relay::Reseed;
    let (chain, state) = load_copy(relay, mailbox, key)?.ok_or_else(|| {
        NodeError::Protocol("this device has no copy of the vault log to restore from".into())
    })?;
    let total = chain.len();
    let raw: Vec<Vec<u8>> = chain
        .entries()
        .iter()
        .map(|e| e.to_bytes().map_err(proto))
        .collect::<Result<_, _>>()?;
    let members = state.member_identities();
    let threshold = u16::from(state.descriptor.threshold);

    let mut from = 0usize;
    for _ in 0..(raw.len() + 8) {
        let mut bytes = 0;
        let mut end = from;
        while end < raw.len() && (end == from || bytes + raw[end].len() <= RESEED_BATCH_BYTES) {
            bytes += raw[end].len();
            end += 1;
        }
        let request = Reseed {
            mailbox,
            members: members.clone(),
            threshold,
            from: from as u64,
            entries: raw[from..end].to_vec(),
            finish: end == raw.len(),
            timestamp: 0, // set by the client when it signs
        };
        let answer = relay.reseed(me, request).await?;
        if !answer.restored {
            // The relay had the vault already: check it against our copy and catch it up.
            return catch_relay_up(relay, me, mailbox, &chain, answer.len).await;
        }
        if answer.open {
            return Ok(Reseeded::Restored { entries: total });
        }
        // Continue where the relay says its log ends (it differs from `end` only when a
        // request was lost or repeated).
        from = usize::try_from(answer.len)
            .ok()
            .filter(|n| *n <= raw.len())
            .ok_or_else(|| NodeError::Protocol("the relay holds more than we sent".into()))?;
    }
    Err(NodeError::Protocol(
        "could not restore the log on the relay".into(),
    ))
}

/// The relay holds `relay_len` entries of the vault's log: checks that they are this
/// device's, and appends what's missing.
async fn catch_relay_up(
    relay: &RelayClient,
    me: &Identity,
    mailbox: MailboxId,
    chain: &Chain,
    relay_len: u64,
) -> Result<Reseeded, NodeError> {
    let ours = chain.entries();
    let shared = relay_len.min(chain.len());
    if shared > 0 {
        let at = shared - 1;
        let page = relay.read_log(me, mailbox, at).await?;
        let theirs = page.first().ok_or(NodeError::RelayRolledBack {
            local_len: chain.len(),
        })?;
        let same = theirs.header.index == at
            && theirs.hash().map_err(proto)? == ours[at as usize].hash().map_err(proto)?;
        if !same {
            return Err(NodeError::RelayForked { index: at });
        }
    }
    if relay_len >= chain.len() {
        return Ok(Reseeded::Current);
    }
    let mut appended = 0;
    for entry in &ours[relay_len as usize..] {
        match relay.append_log(entry).await? {
            AppendResult::Appended { .. } => appended += 1,
            AppendResult::Conflict { .. } => {
                return Err(NodeError::Protocol(
                    "the relay's log changed while it was being restored".into(),
                ))
            }
        }
    }
    Ok(Reseeded::CaughtUp { appended })
}

/// Appends `event` on top of an already-loaded chain and state. The event is checked
/// against the current state first (so members don't write entries everyone will ignore);
/// on a conflict, only the new entries are fetched, the check is repeated, and the append
/// is retried.
pub async fn append_event<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    chain: &mut Chain,
    state: &mut VaultState,
    event: &VaultEvent,
    rng: &mut R,
) -> Result<u64, NodeError> {
    let bytes = event.to_bytes().map_err(proto)?;
    let key = material.log_key();
    for _ in 0..10 {
        state
            .check(me.public().sig_pk, event)
            .map_err(NodeError::Invalid)?;
        let entry = LogEntry::create(
            me,
            &key,
            material.descriptor.vault_id,
            chain.len(),
            chain.head(),
            &bytes,
            rng,
        )
        .map_err(proto)?;
        match relay.append_log(&entry).await? {
            AppendResult::Appended { index } => {
                chain
                    .append(entry.clone(), &state.member_identities())
                    .map_err(proto)?;
                state.apply_entry(&entry, &key);
                save_copy(relay, chain);
                return Ok(index);
            }
            AppendResult::Conflict { .. } => catch_up(relay, me, &key, chain, state).await?,
        }
    }
    Err(NodeError::Protocol(
        "could not append after 10 conflicts".into(),
    ))
}

// --- Proposals and signing over the relay ----------------------------------------------

use std::collections::BTreeSet;

use pczt::{
    roles::{prover::Prover, tx_extractor::TransactionExtractor},
    Pczt,
};
use reddsa::frost::redpallas::{
    round1::SigningCommitments, round2::SignatureShare, SigningPackage,
};
use zafe_proto::ReplayGuard;

use crate::{
    session::{
        aggregate_request, pczt_hash, Leader, Member, NonceStore, PoolStore, ProposalId,
        SigningRequest,
    },
    tx,
    vault::{GroupShares, ProposalStatus, ProposedPayment},
    verify::{verify_pczt, Expectations, Payment, VerifiedTx},
    wallet::{Client, PaymentRequest, VaultWallet},
};
use zcash_protocol::consensus::{BlockHeight, BranchId};

#[derive(Serialize, Deserialize)]
struct SigningRequestMsg {
    proposal: ProposalId,
    pczt_hash: [u8; 32],
    signers: Vec<Vec<u8>>,
    packages: Vec<Vec<u8>>,
}

#[derive(Serialize, Deserialize)]
struct SharesMsg {
    proposal: ProposalId,
    /// Hash of the encoded signing request these shares answer (binds shares to one round).
    request_hash: [u8; 32],
    shares: Vec<Vec<u8>>,
}

fn hash_request_bytes(bytes: &[u8]) -> [u8; 32] {
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"Zafe_SignRequest")
        .hash(bytes)
        .as_bytes()
        .try_into()
        .expect("32 bytes")
}

/// Hash identifying a signing request (and the round its shares belong to).
pub fn request_hash(request: &SigningRequest) -> Result<[u8; 32], NodeError> {
    Ok(hash_request_bytes(&encode_request(request)?))
}

/// Hash identifying one member's set of round-1 commitments (to avoid reusing them).
pub fn commitments_hash(commitments: &[Vec<u8>]) -> [u8; 32] {
    let mut state = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"Zafe_Commitments")
        .to_state();
    for c in commitments {
        state.update(&(c.len() as u32).to_le_bytes());
        state.update(c);
    }
    state.finalize().as_bytes().try_into().expect("32 bytes")
}

/// Decodes a unified (or Orchard-receiver-bearing) address to its Orchard receiver.
pub fn orchard_receiver<P: Parameters>(
    network: &P,
    address: &str,
) -> Result<orchard::Address, NodeError> {
    match zcash_keys::address::Address::decode(network, address) {
        Some(zcash_keys::address::Address::Unified(ua)) => ua
            .orchard()
            .copied()
            .ok_or_else(|| NodeError::Protocol("address has no Orchard receiver".into())),
        _ => Err(NodeError::Protocol(format!(
            "unsupported recipient address {address}"
        ))),
    }
}

/// What this member expects for a proposal, from the log (payments), the vault descriptor
/// (expiry window) and its own wallet (chain tip), never from the proposer.
pub fn expectations<P: Parameters>(
    network: &P,
    payments: &[ProposedPayment],
    tip_height: u32,
    proposal_expiry_blocks: u32,
) -> Result<Expectations, NodeError> {
    Ok(Expectations {
        payments: payments
            .iter()
            .map(|p| {
                Ok(Payment {
                    recipient: orchard_receiver(network, &p.address)?,
                    amount_zat: p.amount_zat,
                    memo: p
                        .memo
                        .as_slice()
                        .try_into()
                        .map_err(|_| NodeError::Protocol("memo must be 512 bytes".into()))?,
                })
            })
            .collect::<Result<_, NodeError>>()?,
        consensus_branch_id: BranchId::for_height(network, BlockHeight::from_u32(tip_height + 1))
            .into(),
        tip_height,
        // The proposer sets expiry = its target height (tip + 1) + the vault's window.
        max_expiry_delta: proposal_expiry_blocks
            + 1
            + crate::vault::EXPIRY_TIP_SLACK_BLOCKS
            + crate::vault::EXPIRY_ROUNDING_BLOCKS,
    })
}

/// The current members (after seat moves) by signing key.
pub fn members_by_pk(state: &VaultState) -> BTreeMap<[u8; 32], IdentityPublic> {
    state
        .member_identities()
        .into_iter()
        .map(|m| (m.sig_pk, m))
        .collect()
}

/// Builds a PCZT for `payments` from this member's wallet and logs it as a proposal.
/// Notes that other live proposals spend are held back first; if another member's
/// proposal takes the same notes meanwhile (the log accepts only the first), the PCZT is
/// rebuilt from the notes left.
#[allow(clippy::too_many_arguments)]
pub async fn propose<P: Parameters + Clone + Send + Sync + 'static, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    wallet: &mut VaultWallet<P>,
    lightwalletd: &mut Client,
    sent: &SentTxs,
    payments: &[PaymentRequest],
    auto_send: bool,
    rng: &mut R,
) -> Result<ProposalId, NodeError> {
    let fvk = material.vault_keys()?.fvk().clone();
    // Every member's check would reject it: an output to the vault counts as change.
    for p in payments {
        if orchard_receiver(wallet.params(), &p.address)
            .is_ok_and(|a| fvk.scope_for_address(&a).is_some())
        {
            return Err(crate::wallet::WalletError::Payment(
                "that is this vault's own address".into(),
            )
            .into());
        }
    }
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    for _ in 0..3 {
        reserve_notes(&state, wallet, lightwalletd, sent).await?;
        let pczt = wallet.propose(payments, material.descriptor.proposal_expiry_blocks)?;
        let tip = wallet
            .chain_height()
            .map_err(proto)?
            .ok_or_else(|| NodeError::NotReady("wallet not synced".into()))?;
        let signing_spends = tx::spends_to_sign(&pczt, &fvk).map_err(proto)?.len() as u16;
        let mut id = [0u8; 16];
        rng.fill_bytes(&mut id);
        let event = VaultEvent::Proposal {
            id,
            payments: payments
                .iter()
                .map(|p| ProposedPayment {
                    address: p.address.clone(),
                    amount_zat: p.amount_zat,
                    memo: p
                        .memo
                        .clone()
                        .unwrap_or_else(zcash_protocol::memo::MemoBytes::empty)
                        .as_array()
                        .to_vec(),
                })
                .collect(),
            pczt_hash: pczt_hash(&pczt).map_err(proto)?,
            pczt: pczt.serialize().map_err(proto)?,
            tip_height: tip,
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs()),
            signing_spends,
            auto_send,
        };
        match append_event(relay, me, material, &mut chain, &mut state, &event, rng).await {
            Ok(_) => return Ok(id),
            // `state` now includes the proposal that won the notes: hold them and rebuild.
            Err(NodeError::Invalid(VaultError::NotesInUse(_))) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(NodeError::NotReady(
        "other proposals keep taking the same notes; try again".into(),
    ))
}

/// Makes a cancelled proposal unsendable: proposes spending exactly its notes back to the
/// vault (its internal address; no payments, so members check every output is the
/// vault's). Needs t approvals like any spend; once mined, the cancelled transaction can
/// never be valid, even if every signature for it is out. Sent automatically when the
/// approvals complete.
#[allow(clippy::too_many_arguments)]
pub async fn invalidate<P: Parameters + Clone + Send + Sync + 'static, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    wallet: &mut VaultWallet<P>,
    lightwalletd: &mut Client,
    sent: &SentTxs,
    cancelled: ProposalId,
    rng: &mut R,
) -> Result<ProposalId, NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let target = state
        .proposals
        .get(&cancelled)
        .ok_or_else(|| NodeError::Protocol("unknown proposal".into()))?;
    if target.status != ProposalStatus::Cancelled {
        return Err(NodeError::NotReady(
            "only a cancelled payment can be made unsendable".into(),
        ));
    }
    let nullifiers = target.nullifiers.clone();
    let keys = material.vault_keys()?;
    let fvk = keys.fvk().clone();
    let internal = zcash_keys::address::UnifiedAddress::from_receivers(
        Some(fvk.address_at(0u32, orchard::keys::Scope::Internal)),
        None,
        None,
    )
    .ok_or_else(|| NodeError::Protocol("vault internal address".into()))?
    .encode(wallet.params());
    let recipient = zcash_address::ZcashAddress::try_from_encoded(&internal).map_err(proto)?;

    reserve_notes(&state, wallet, lightwalletd, sent).await?;
    let pczt = wallet.propose_sweep(
        &nullifiers,
        recipient,
        material.descriptor.proposal_expiry_blocks,
    )?;
    let tip = wallet
        .chain_height()
        .map_err(proto)?
        .ok_or_else(|| NodeError::NotReady("wallet not synced".into()))?;
    let signing_spends = tx::spends_to_sign(&pczt, &fvk).map_err(proto)?.len() as u16;
    let mut id = [0u8; 16];
    rng.fill_bytes(&mut id);
    let event = VaultEvent::Proposal {
        id,
        payments: vec![],
        pczt_hash: pczt_hash(&pczt).map_err(proto)?,
        pczt: pczt.serialize().map_err(proto)?,
        tip_height: tip,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs()),
        signing_spends,
        auto_send: true,
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(id)
}

/// Refreshes this member's note holds from the vault log (see [`note_holds`]), so its
/// spendable balance and its next proposal leave out notes live proposals spend.
///
/// A broadcast transaction that isn't mined and isn't in lightwalletd's mempool any more
/// (dropped) is sent again when this device broadcast it and kept its bytes (`sent`);
/// otherwise, or if the node refuses it now, it releases its notes. The whole mempool is fetched rather than looking up the
/// txid, so lightwalletd doesn't learn which transaction belongs to the vault. If the
/// chain moved past the wallet meanwhile, nothing is released this time (the transaction
/// may just have been mined).
pub async fn reserve_notes<P: Parameters + Clone + Send + Sync + 'static>(
    state: &VaultState,
    wallet: &mut VaultWallet<P>,
    lightwalletd: &mut Client,
    sent: &SentTxs,
) -> Result<usize, NodeError> {
    let tip = wallet.chain_height()?.unwrap_or(0);
    let mut pending = Vec::new();
    for p in state.proposals.values() {
        if let (ProposalStatus::Broadcast, Some(txid)) = (p.status, p.txid) {
            if p.expiry_height > tip && !wallet.tx_mined(&txid)? {
                pending.push((p.id, txid));
            } else {
                sent.remove(&txid); // mined or expired: nothing left to resend
            }
        }
    }
    let mut dropped = BTreeSet::new();
    if !pending.is_empty() {
        let mempool = crate::wallet::mempool_txids(lightwalletd).await?;
        let chain_tip = crate::wallet::latest_height(lightwalletd).await?;
        // Only when lightwalletd agrees with the wallet's tip: behind (a restarting node
        // reports 0) or ahead (the transaction may just have been mined) proves nothing.
        if chain_tip == tip {
            for (id, txid) in pending {
                if mempool.contains(&txid) {
                    continue;
                }
                // Dropped from the mempool. The broadcaster still has the bytes: send
                // them again; otherwise, or if the node refuses them now, let the notes go.
                // The bytes stay until the transaction is mined or expires: a refusal can be
                // temporary, and resending a transaction that lost its notes is harmless.
                let resent = match sent.get(&txid) {
                    Some(raw) => send_raw(&raw, lightwalletd).await?.is_ok(),
                    None => false,
                };
                if !resent {
                    dropped.insert(id);
                }
            }
        }
    }
    Ok(wallet.reserve(&note_holds(state, &dropped))?)
}

/// The notes each live proposal spends, in log order, held until its transaction's expiry
/// height: open and approved proposals, and broadcast ones except those in `dropped`.
/// Cancelled proposals hold nothing, so a new proposal can respend (invalidate) their notes.
pub fn note_holds(
    state: &VaultState,
    dropped: &BTreeSet<ProposalId>,
) -> Vec<crate::wallet::NoteHold> {
    let mut proposals: Vec<_> = state
        .proposals
        .values()
        .filter(|p| match p.status {
            ProposalStatus::Open | ProposalStatus::Approved => true,
            ProposalStatus::Broadcast => !dropped.contains(&p.id),
            ProposalStatus::Cancelled | ProposalStatus::Rejected => false,
        })
        .collect();
    proposals.sort_by_key(|p| p.log_index);
    proposals
        .into_iter()
        .map(|p| crate::wallet::NoteHold {
            owner: p.pczt_hash,
            nullifiers: p.nullifiers.clone(),
            expiry_height: p.expiry_height,
        })
        .collect()
}

fn proposal_pczt(
    state: &VaultState,
    id: &ProposalId,
) -> Result<(Pczt, Vec<ProposedPayment>), NodeError> {
    let p = state
        .proposals
        .get(id)
        .ok_or_else(|| NodeError::Protocol("unknown proposal".into()))?;
    let pczt = Pczt::parse(&p.pczt).map_err(proto)?;
    if pczt_hash(&pczt).map_err(proto)? != p.pczt_hash {
        return Err(NodeError::Protocol(
            "logged PCZT does not match its hash".into(),
        ));
    }
    Ok((pczt, p.payments.clone()))
}

/// Verifies a logged proposal independently against this member's view of the chain tip,
/// without voting (what the review screen shows).
pub fn review<P: Parameters>(
    state: &VaultState,
    material: &VaultMaterial,
    network: &P,
    tip_height: u32,
    proposal: ProposalId,
) -> Result<VerifiedTx, NodeError> {
    let (pczt, payments) = proposal_pczt(state, &proposal)?;
    let keys = material.vault_keys()?;
    verify_pczt(
        &pczt,
        keys.fvk(),
        &expectations(
            network,
            &payments,
            tip_height,
            material.descriptor.proposal_expiry_blocks,
        )?,
    )
    .map_err(|e| NodeError::Verification(e.to_string()))
}

/// What an approval did.
#[derive(Debug)]
pub struct Approved {
    pub verified: VerifiedTx,
    /// Signed at approval time (one-tap); otherwise commitments were published and
    /// signing happens in a later interactive round.
    pub signed: bool,
    /// This approval completed a signer group: the proposal can be sent now.
    pub completed: bool,
    /// The proposer asked for the completing member to send right away.
    pub auto_send: bool,
}

/// Verifies a proposal independently and, if it passes, votes Approve. For a preprocessed
/// proposal whose nonces this device holds, the vote carries this member's signature
/// shares for every signer group it is in (one tap: nothing more is needed from this
/// member). Otherwise it carries fresh round-1 commitments for interactive signing.
#[allow(clippy::too_many_arguments)]
pub async fn approve<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    tip_height: u32,
    proposal: ProposalId,
    store: &mut impl NonceStore,
    pool: &mut impl PoolStore,
    rng: &mut R,
) -> Result<Approved, NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let (pczt, payments) = proposal_pczt(&state, &proposal)?;
    let expected = expectations(
        network,
        &payments,
        tip_height,
        material.descriptor.proposal_expiry_blocks,
    )?;
    let key_package = material.key_package()?;
    let keys = material.vault_keys()?;
    let member = Member {
        identifier: *key_package.identifier(),
        key_package: &key_package,
        vault_fvk: keys.fvk(),
    };
    let my_pk = me.public().sig_pk;
    let p = &state.proposals[&proposal];
    let (pczt_hash, auto_send, signing_spends) = (p.pczt_hash, p.auto_send, p.signing_spends);

    // One-tap path: every commitment assigned to us must still have its nonce here.
    let one_tap = match &p.preprocessed {
        Some(pre) => {
            let mut groups = Vec::new();
            for g in pre.groups_of(&my_pk) {
                let ids = pre.subsets[g]
                    .iter()
                    .map(|pk| frost_id_of(&state.descriptor, pk))
                    .collect::<Result<Vec<_>, _>>()?;
                groups.push((g, ids, pre.commitments[g].clone()));
            }
            let all_here = groups.iter().all(|(g, _, _)| {
                (0..usize::from(signing_spends)).all(|j| {
                    pre.commitment(*g, j, &my_pk)
                        .is_some_and(|c| pool.contains(c))
                })
            });
            (all_here && !groups.is_empty()).then_some(groups)
        }
        None => None,
    };

    let (event, verified, signed) = if let Some(groups) = one_tap {
        let (verified, shares) = member
            .sign_groups(&pczt, &expected, &groups, pool)
            .map_err(approve_error)?;
        check_signing_spends(&verified, signing_spends)?;
        let shares = shares
            .into_iter()
            .map(|(g, s)| GroupShares {
                group: g as u16,
                shares: s.iter().map(|x| x.serialize()).collect(),
            })
            .collect();
        let event = VaultEvent::Vote {
            proposal,
            pczt_hash,
            approve: true,
            commitments: vec![],
            shares,
        };
        (event, verified, true)
    } else {
        let (approval, verified) = member
            .approve(proposal, &pczt, &expected, store, rng)
            .map_err(approve_error)?;
        check_signing_spends(&verified, signing_spends)?;
        let commitments = approval
            .commitments
            .iter()
            .map(|c| c.serialize().map_err(proto))
            .collect::<Result<Vec<_>, _>>()?;
        let event = VaultEvent::Vote {
            proposal,
            pczt_hash,
            approve: true,
            commitments,
            shares: vec![],
        };
        (event, verified, false)
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    let p = &state.proposals[&proposal];
    Ok(Approved {
        verified,
        signed,
        completed: p.completed_by == Some(my_pk),
        auto_send,
    })
}

fn approve_error(e: crate::session::SessionError) -> NodeError {
    match e {
        crate::session::SessionError::Verify(v) => NodeError::Verification(v.to_string()),
        crate::session::SessionError::AlreadyApproved => {
            NodeError::NotReady("you already approved this proposal".into())
        }
        e => proto(e),
    }
}

/// The proposer declares how many spends need signatures (it sizes the preprocessed
/// groups); a mismatch with the PCZT means the proposal is malformed.
fn check_signing_spends(verified: &VerifiedTx, declared: u16) -> Result<(), NodeError> {
    if verified.spends_to_sign.len() != usize::from(declared) {
        return Err(NodeError::Verification(format!(
            "proposal declares {declared} spend(s) to sign, the transaction has {}",
            verified.spends_to_sign.len()
        )));
    }
    Ok(())
}

pub(crate) fn frost_id_of(
    descriptor: &crate::vault::VaultDescriptor,
    sig_pk: &[u8; 32],
) -> Result<Identifier, NodeError> {
    let info = descriptor
        .member(sig_pk)
        .ok_or_else(|| NodeError::Protocol("unknown member".into()))?;
    Identifier::deserialize(&info.frost_id).map_err(proto)
}

// --- One-tap signing: nonce pools and sending ---------------------------------------------

/// Single-spend proposals a full pool covers. Every proposal takes commitments from
/// **every** member's pool when it enters the log (one per signing group the member is in,
/// per spend), approving or not, so a member who is offline for a while drains too; a
/// deep pool keeps one-tap working until their next background check tops it up.
pub const POOL_PROPOSALS: usize = 16;

/// Smallest pool target (commitments are 64 bytes on the log; small vaults can afford it).
pub const MIN_POOL_TARGET: usize = 32;

/// Commitments this member keeps available for one-tap signing: [`POOL_PROPOSALS`]
/// single-spend proposals' worth (each takes C(n-1, t-1)), at least [`MIN_POOL_TARGET`] and
/// at most one `Commitments` batch. Refilled when below half (see [`top_up_pool`]).
pub fn pool_target(descriptor: &crate::vault::VaultDescriptor) -> usize {
    pool_target_for(descriptor.members.len(), usize::from(descriptor.threshold))
}

fn pool_target_for(n: usize, t: usize) -> usize {
    let per_proposal = binomial(n.saturating_sub(1), t.saturating_sub(1));
    (POOL_PROPOSALS * per_proposal).clamp(MIN_POOL_TARGET, crate::vault::MAX_COMMITMENT_BATCH)
}

fn binomial(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    (0..k).fold(1usize, |acc, i| acc.saturating_mul(n - i) / (i + 1))
}

/// Publishes fresh commitments when this member's pool is below half its target. Nonces
/// are stored before the commitments are logged. Returns how many were published.
pub async fn top_up_pool<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    pool: &mut impl PoolStore,
    rng: &mut R,
) -> Result<usize, NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let available = state
        .pools
        .get(&me.public().sig_pk)
        .map_or(0, crate::vault::Pool::available);
    let target = pool_target(&material.descriptor);
    if available * 2 >= target {
        return Ok(0);
    }
    let batch = crate::session::new_pool_commitments(
        &material.key_package()?,
        target - available,
        pool,
        rng,
    )
    .map_err(proto)?;
    let count = batch.len();
    append_event(
        relay,
        me,
        material,
        &mut chain,
        &mut state,
        &VaultEvent::Commitments { batch },
        rng,
    )
    .await?;
    Ok(count)
}

/// Deletes this device's nonces for commitments assigned to proposals that are closed
/// (sent, rejected, cancelled, or expired as of the synced tip `tip_height`): they can
/// never be used. Returns how many were deleted.
pub fn forget_closed(
    state: &VaultState,
    me: &[u8; 32],
    tip_height: Option<u32>,
    pool: &mut impl PoolStore,
) -> usize {
    let mut n = 0;
    for p in state.proposals.values() {
        let expired = p.expiry_height > 0 && tip_height.is_some_and(|tip| tip >= p.expiry_height);
        let closed = expired
            || matches!(
                p.status,
                ProposalStatus::Broadcast | ProposalStatus::Rejected | ProposalStatus::Cancelled
            );
        let Some(pre) = p.preprocessed.as_ref().filter(|_| closed) else {
            continue;
        };
        for g in pre.groups_of(me) {
            for j in 0..usize::from(p.signing_spends) {
                if let Some(c) = pre.commitment(g, j, me) {
                    if pool.contains(c) {
                        pool.forget(c);
                        n += 1;
                    }
                }
            }
        }
    }
    n
}

/// Deletes this device's interactive-signing nonces for proposals that are closed or
/// expired (as `forget_closed`): no signing round can use them any more. Returns how many
/// proposals had nonces here.
pub fn forget_closed_nonces(
    state: &VaultState,
    tip_height: Option<u32>,
    store: &mut impl NonceStore,
) -> usize {
    state
        .proposals
        .values()
        .filter(|p| {
            let expired =
                p.expiry_height > 0 && tip_height.is_some_and(|tip| tip >= p.expiry_height);
            expired
                || matches!(
                    p.status,
                    ProposalStatus::Broadcast
                        | ProposalStatus::Rejected
                        | ProposalStatus::Cancelled
                )
        })
        // `take` deletes before returning; the nonces are dropped unused.
        .filter(|p| store.take(&p.id, &p.pczt_hash).is_some())
        .count()
}

/// Whether a proposal has a complete signer group (one-tap) and can be sent by anyone.
pub fn is_ready(state: &VaultState, proposal: &ProposalId) -> bool {
    state
        .proposals
        .get(proposal)
        .is_some_and(|p| p.ready_group.is_some() && p.status == ProposalStatus::Approved)
}

/// Sends a one-tap proposal: aggregates the complete signer group's shares from the log
/// (verifying each), proves, broadcasts, and logs the broadcast. Needs no other member.
#[allow(clippy::too_many_arguments)]
pub async fn send_ready<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    tip_height: u32,
    proposal: ProposalId,
    lightwalletd: &mut Client,
    rng: &mut R,
) -> Result<Sent, NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let p = state
        .proposals
        .get(&proposal)
        .ok_or_else(|| NodeError::Protocol("unknown proposal".into()))?;
    let (Some(group), Some(pre)) = (p.ready_group, p.preprocessed.as_ref()) else {
        return Err(NodeError::NotReady(
            "no signer group is complete yet".into(),
        ));
    };
    let group = usize::from(group);
    let (pczt, payments) = proposal_pczt(&state, &proposal)?;
    // The sender verifies the transaction like any signer before broadcasting it.
    let keys = material.vault_keys()?;
    let verified = verify_pczt(
        &pczt,
        keys.fvk(),
        &expectations(
            network,
            &payments,
            tip_height,
            material.descriptor.proposal_expiry_blocks,
        )?,
    )
    .map_err(|e| NodeError::Verification(e.to_string()))?;
    let ids = pre.subsets[group]
        .iter()
        .map(|pk| frost_id_of(&state.descriptor, pk))
        .collect::<Result<Vec<_>, _>>()?;
    let mut shares = BTreeMap::new();
    for (pk, id) in pre.subsets[group].iter().zip(&ids) {
        let gs = p.shares[pk]
            .iter()
            .find(|s| usize::from(s.group) == group)
            .ok_or_else(|| NodeError::Protocol("group shares missing".into()))?;
        let parsed = gs
            .shares
            .iter()
            .map(|b| SignatureShare::deserialize(b).map_err(proto))
            .collect::<Result<Vec<_>, _>>()?;
        shares.insert(*id, parsed);
    }
    let signatures = crate::session::aggregate_group(
        &ids,
        &pre.commitments[group],
        &shares,
        &verified,
        &material.public_key_package()?,
    )
    .map_err(proto)?;

    let to_prove = pczt.clone();
    let proved = tokio::task::spawn_blocking(move || {
        Prover::new(to_prove)
            .create_ironwood_proof(proving_key())
            .map(|p| p.finish())
            .map_err(proto)
    })
    .await
    .map_err(|e| NodeError::Protocol(format!("proving task: {e}")))??;
    let signed = tx::apply_signatures(proved, &signatures).map_err(proto)?;
    let sent = broadcast(signed, lightwalletd).await?;
    let txid = sent.txid;
    let event = VaultEvent::Broadcast { proposal, txid };
    if let Err(e) = append_event(relay, me, material, &mut chain, &mut state, &event, rng).await {
        // Another member may have sent it first; the transaction is the same either way.
        if !matches!(
            state.proposals.get(&proposal).map(|p| p.status),
            Some(ProposalStatus::Broadcast)
        ) {
            return Err(NodeError::Protocol(format!(
                "broadcast {} but could not log it: {e}",
                hex::encode(txid)
            )));
        }
    }
    Ok(sent)
}

/// A transaction this device broadcast: its id and raw bytes (kept to resend it).
#[derive(Clone, Debug)]
pub struct Sent {
    pub txid: [u8; 32],
    pub raw: Vec<u8>,
}

/// Extracts (fully verifying) and broadcasts a signed, proved PCZT.
async fn broadcast(signed: Pczt, lightwalletd: &mut Client) -> Result<Sent, NodeError> {
    let transaction = TransactionExtractor::new(signed)
        .with_orchard(verifying_key())
        .extract()
        .map_err(proto)?;
    let mut raw = Vec::new();
    transaction.write(&mut raw).map_err(proto)?;
    send_raw(&raw, lightwalletd)
        .await?
        .map_err(|m| NodeError::Protocol(format!("broadcast rejected: {m}")))?;
    Ok(Sent {
        txid: *transaction.txid().as_ref(),
        raw,
    })
}

/// Submits raw transaction bytes. `Ok(Err(message))` when the node refused it.
async fn send_raw(raw: &[u8], lightwalletd: &mut Client) -> Result<Result<(), String>, NodeError> {
    let reply = lightwalletd
        .send_transaction(zcash_client_backend::proto::service::RawTransaction {
            data: raw.to_vec(),
            height: 0,
        })
        .await
        .map_err(|e| NodeError::Protocol(e.to_string()))?
        .into_inner();
    Ok(if reply.error_code == 0 {
        Ok(())
    } else {
        Err(reply.error_message)
    })
}

/// Raw transactions this device broadcast, one file per txid, kept until mined or expired
/// so a transaction that drops out of the mempool can be sent again (`reserve_notes`).
/// Only the broadcaster has the bytes: signatures and proof aren't in the vault log.
pub struct SentTxs(Option<std::path::PathBuf>);

impl SentTxs {
    pub fn in_dir(dir: impl Into<std::path::PathBuf>) -> Self {
        Self(Some(dir.into()))
    }

    /// Keeps nothing (nothing is resent).
    pub fn none() -> Self {
        Self(None)
    }

    fn file(&self, txid: &[u8; 32]) -> Option<std::path::PathBuf> {
        self.0
            .as_ref()
            .map(|d| d.join(format!("{}.tx", hex::encode(txid))))
    }

    /// Best effort: losing the file only means the transaction can't be resent.
    pub fn put(&self, sent: &Sent) {
        if let (Some(dir), Some(file)) = (&self.0, self.file(&sent.txid)) {
            let _ = std::fs::create_dir_all(dir);
            let tmp = file.with_extension("tmp");
            if std::fs::write(&tmp, &sent.raw).is_ok() {
                let _ = std::fs::rename(&tmp, &file);
            }
        }
    }

    pub fn get(&self, txid: &[u8; 32]) -> Option<Vec<u8>> {
        std::fs::read(self.file(txid)?).ok()
    }

    pub fn remove(&self, txid: &[u8; 32]) {
        if let Some(file) = self.file(txid) {
            let _ = std::fs::remove_file(file);
        }
    }
}

/// Votes Reject.
pub async fn reject<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    proposal: ProposalId,
    rng: &mut R,
) -> Result<(), NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let pczt_hash = state
        .proposals
        .get(&proposal)
        .ok_or_else(|| NodeError::Protocol("unknown proposal".into()))?
        .pczt_hash;
    let event = VaultEvent::Vote {
        proposal,
        pczt_hash,
        approve: false,
        commitments: vec![],
        shares: vec![],
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

/// Cancels a proposal this member authored (open or approved, not yet sent). A proposal
/// whose signatures are already complete could still be sent by anyone holding them until
/// it expires; respending its notes in a new proposal invalidates it.
pub async fn cancel<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    proposal: ProposalId,
    rng: &mut R,
) -> Result<(), NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let event = VaultEvent::Cancelled { proposal };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

/// Sets this member's display name for the other members (trimmed; empty clears it).
/// Logged only when it changes. Members on an older app skip the entry.
pub async fn set_name<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    name: &str,
    rng: &mut R,
) -> Result<(), NodeError> {
    let name = name.trim();
    if !crate::vault::valid_name(name) {
        return Err(NodeError::Protocol("not a valid display name".into()));
    }
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let current = state.names.get(&me.public().sig_pk).map(String::as_str);
    if current.unwrap_or("") == name {
        return Ok(());
    }
    let event = VaultEvent::Name {
        name: name.to_owned(),
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

/// Records in the log that this member has a backup of its current keys that opens
/// (spec §12.2, backup health). Logged only when the member has no attestation for the
/// current epoch yet, so repeated exports don't grow the log.
pub async fn attest_backup<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    at: u64,
    rng: &mut R,
) -> Result<(), NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let epoch = state.descriptor.epoch;
    if state
        .backups
        .get(&me.public().sig_pk)
        .is_some_and(|b| b.epoch == epoch)
    {
        return Ok(());
    }
    let event = VaultEvent::BackupVerified { epoch, at };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

fn member_by_frost_id(
    descriptor: &crate::vault::VaultDescriptor,
    id: &Identifier,
) -> Result<IdentityPublic, NodeError> {
    let bytes = id.serialize();
    descriptor
        .members
        .iter()
        .find(|m| m.frost_id == bytes)
        .map(|m| m.identity)
        .ok_or_else(|| NodeError::Protocol("unknown FROST identifier".into()))
}

/// A signing request sent by the leader, plus the commitment sets it consumed (the leader
/// must not put them in another request).
pub struct SentRequest {
    pub request: SigningRequest,
    pub used_commitments: Vec<[u8; 32]>,
}

/// Leader: once the proposal is approved, sends sealed signing requests to `threshold`
/// approvers whose current commitments are not in `used` (commitment sets already put in an
/// earlier request). Members re-approve to provide fresh commitments after a failed round.
#[allow(clippy::too_many_arguments)]
pub async fn request_signatures<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    tip_height: u32,
    proposal: ProposalId,
    used: &BTreeSet<[u8; 32]>,
    rng: &mut R,
) -> Result<SentRequest, NodeError> {
    let (_, state) = load_state(relay, me, material).await?;
    let p = state
        .proposals
        .get(&proposal)
        .ok_or_else(|| NodeError::Protocol("unknown proposal".into()))?;
    if p.status != ProposalStatus::Approved {
        return Err(NodeError::NotReady(format!("proposal is {:?}", p.status)));
    }
    let (pczt, payments) = proposal_pczt(&state, &proposal)?;
    let keys = material.vault_keys()?;
    let verified = verify_pczt(
        &pczt,
        keys.fvk(),
        &expectations(
            network,
            &payments,
            tip_height,
            material.descriptor.proposal_expiry_blocks,
        )?,
    )
    .map_err(proto)?;
    let mut leader =
        Leader::new(proposal, &pczt, &verified, material.descriptor.threshold).map_err(proto)?;

    let mut hash_of = BTreeMap::new();
    for (author, commitments) in &p.approvals {
        let hash = commitments_hash(commitments);
        if used.contains(&hash) {
            continue;
        }
        let info = state
            .descriptor
            .member(author)
            .ok_or_else(|| NodeError::Protocol("approval from non-member".into()))?;
        let member = Identifier::deserialize(&info.frost_id).map_err(proto)?;
        let parsed = commitments
            .iter()
            .map(|c| SigningCommitments::deserialize(c).map_err(proto))
            .collect::<Result<Vec<_>, _>>()?;
        leader.add_approval(crate::session::Approval {
            member,
            proposal,
            pczt_hash: p.pczt_hash,
            commitments: parsed,
        });
        hash_of.insert(member, hash);
    }
    let threshold = usize::from(material.descriptor.threshold);
    let chosen: Vec<Identifier> = leader
        .available_signers()
        .into_iter()
        .take(threshold)
        .collect();
    if chosen.len() < threshold {
        return Err(NodeError::NotReady(format!(
            "{} of {threshold} signers are ready. Signers from the unfinished round need to \
             approve again, or more signers need to approve",
            chosen.len()
        )));
    }
    let request = leader.request(&chosen).map_err(proto)?;

    let bytes = encode_request(&request)?;
    let mut seq = SeqCounter::default();
    for id in &request.signers {
        let to = member_by_frost_id(&state.descriptor, id)?;
        if to.sig_pk == me.public().sig_pk {
            continue; // the leader signs its own part locally (`sign_own_shares`)
        }
        let env = Envelope::sealed(
            me,
            &to,
            material.descriptor.vault_id,
            seq.next_seq(),
            Kind::SigningRequest,
            &bytes,
            rng,
        )
        .map_err(proto)?;
        relay.send(&env).await?;
    }
    let used_commitments = chosen.iter().map(|id| hash_of[id]).collect();
    Ok(SentRequest {
        request,
        used_commitments,
    })
}

/// Outcome of answering signing requests.
#[derive(Debug, Default)]
pub struct RespondReport {
    pub answered: Vec<ProposalId>,
    /// Requests that were not answered, with the reason (stale, expired, forged, ...).
    pub skipped: Vec<(ProposalId, String)>,
    /// Handled envelopes the relay deleted (0 on a relay without `/v1/inbox/ack`).
    pub acknowledged: u64,
}

/// Member: answers every pending signing request addressed to this member. Each request is
/// re-verified from the log; a bad or stale request is skipped and reported, never fatal.
///
/// Also asks the relay to delete what this member is done with (best effort): keygen
/// messages (the vault exists, so its material is saved), signing requests that were
/// answered or can never be (no nonces), and, as a leader, shares for proposals that are
/// closed (sent, rejected, cancelled or expired at `tip_height`). A request that failed for
/// a reason that may pass (e.g. a stale tip) stays for the next attempt.
#[allow(clippy::too_many_arguments)]
pub async fn respond<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    tip_height: u32,
    store: &mut impl NonceStore,
    rng: &mut R,
) -> Result<RespondReport, NodeError> {
    let (_, state) = load_state(relay, me, material).await?;
    let members = members_by_pk(&state);
    let key_package = material.key_package()?;
    let keys = material.vault_keys()?;
    let member = Member {
        identifier: *key_package.identifier(),
        key_package: &key_package,
        vault_fvk: keys.fvk(),
    };
    let mut seq = SeqCounter::default();
    let mut report = RespondReport::default();
    let mut done = Vec::new();
    let closed = |proposal: &ProposalId| {
        state.proposals.get(proposal).is_none_or(|p| {
            !matches!(p.status, ProposalStatus::Open | ProposalStatus::Approved)
                || (p.expiry_height > 0 && tip_height >= p.expiry_height)
        })
    };

    for (cursor, leader, env) in
        read_inbox(relay, me, material.descriptor.vault_id, &members, 0).await?
    {
        match env.header.kind {
            Kind::Join
            | Kind::DkgRound1
            | Kind::DkgEcho
            | Kind::DkgRound2
            | Kind::SkContribution
            | Kind::DescriptorSignature
            | Kind::LogKey => {
                done.push(cursor);
                continue;
            }
            Kind::SignatureShares => {
                let finished = env
                    .open(me, &leader)
                    .ok()
                    .and_then(|b| version::decode::<SharesMsg>(Format::SignatureShares, &b).ok())
                    .is_none_or(|msg| closed(&msg.proposal));
                if finished {
                    done.push(cursor);
                }
                continue;
            }
            Kind::SigningRequest => {}
            _ => continue,
        }
        let Ok(bytes) = env.open(me, &leader) else {
            done.push(cursor);
            continue;
        };
        let Ok(msg) = version::decode::<SigningRequestMsg>(Format::SigningRequest, &bytes) else {
            done.push(cursor);
            continue;
        };
        if !store.contains(&msg.proposal, &msg.pczt_hash) {
            done.push(cursor); // already answered, or never approved: never answerable
            continue;
        }
        let result: Result<Vec<u8>, NodeError> = (|| {
            let (pczt, payments) = proposal_pczt(&state, &msg.proposal)?;
            let request = decode_request(&bytes)?;
            let shares = member
                .sign(
                    &request,
                    &pczt,
                    &expectations(
                        network,
                        &payments,
                        tip_height,
                        material.descriptor.proposal_expiry_blocks,
                    )?,
                    store,
                )
                .map_err(proto)?;
            let reply = SharesMsg {
                proposal: msg.proposal,
                request_hash: hash_request_bytes(&bytes),
                shares: shares.iter().map(|s| s.serialize()).collect(),
            };
            Ok(version::encode(Format::SignatureShares, &reply)?)
        })();
        match result {
            Ok(reply) => {
                let env = Envelope::sealed(
                    me,
                    &leader,
                    material.descriptor.vault_id,
                    seq.next_seq(),
                    Kind::SignatureShares,
                    &reply,
                    rng,
                )
                .map_err(proto)?;
                relay.send(&env).await?;
                report.answered.push(msg.proposal);
                done.push(cursor);
            }
            Err(e) => report.skipped.push((msg.proposal, e.to_string())),
        }
    }
    if !done.is_empty() {
        // Best effort: whatever stays expires on the relay after its retention period.
        report.acknowledged = relay
            .ack_inbox(me, material.descriptor.vault_id, &done)
            .await
            .unwrap_or(0);
    }
    Ok(report)
}

/// The Ironwood (post-NU6.3) proving key, built once per process (seconds on a phone).
pub fn proving_key() -> &'static orchard::circuit::ProvingKey {
    static PK: std::sync::OnceLock<orchard::circuit::ProvingKey> = std::sync::OnceLock::new();
    PK.get_or_init(|| {
        orchard::circuit::ProvingKey::build(orchard::circuit::OrchardCircuitVersion::PostNu6_3)
    })
}

/// The Ironwood verifying key, built once per process.
pub fn verifying_key() -> &'static orchard::circuit::VerifyingKey {
    static VK: std::sync::OnceLock<orchard::circuit::VerifyingKey> = std::sync::OnceLock::new();
    VK.get_or_init(|| {
        orchard::circuit::VerifyingKey::build(orchard::circuit::OrchardCircuitVersion::PostNu6_3)
    })
}

/// How far the leader got with collecting shares for a request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareProgress {
    pub received: usize,
    pub needed: usize,
}

/// Leader: reads the shares answering exactly `request` that have arrived so far.
async fn read_shares(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    state: &VaultState,
    request: &SigningRequest,
) -> Result<BTreeMap<Identifier, Vec<SignatureShare>>, NodeError> {
    let members = members_by_pk(state);
    let wanted = request_hash(request)?;
    let mut shares = BTreeMap::new();
    for (_, from, env) in read_inbox(relay, me, material.descriptor.vault_id, &members, 0).await? {
        if env.header.kind != Kind::SignatureShares {
            continue;
        }
        let Ok(bytes) = env.open(me, &from) else {
            continue;
        };
        let Ok(msg) = version::decode::<SharesMsg>(Format::SignatureShares, &bytes) else {
            continue;
        };
        if msg.proposal != request.proposal || msg.request_hash != wanted {
            continue; // a share from another proposal or an earlier round
        }
        let Some(info) = state.descriptor.member(&from.sig_pk) else {
            continue;
        };
        let id = Identifier::deserialize(&info.frost_id).map_err(proto)?;
        if !request.signers.contains(&id) {
            continue;
        }
        let Ok(parsed) = msg
            .shares
            .iter()
            .map(|s| SignatureShare::deserialize(s))
            .collect::<Result<Vec<_>, _>>()
        else {
            continue;
        };
        shares.insert(id, parsed);
    }
    Ok(shares)
}

/// Leader: how many of the request's signers have answered so far.
pub async fn share_progress(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    request: &SigningRequest,
) -> Result<ShareProgress, NodeError> {
    let (_, state) = load_state(relay, me, material).await?;
    Ok(ShareProgress {
        received: read_shares(relay, me, material, &state, request)
            .await?
            .len(),
        needed: request.signers.len(),
    })
}

/// Leader: its own signature shares when it is one of the request's signers (the relay
/// never carries a member's envelope to itself). Re-verifies the proposal like any signer.
/// Returns serialized shares for the caller to keep until `finalize` succeeds: the nonces
/// are consumed here, so a retry must reuse these shares rather than sign again. `None` if
/// the leader is not a signer of this request.
#[allow(clippy::too_many_arguments)]
pub async fn sign_own_shares<P: Parameters>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    tip_height: u32,
    request: &SigningRequest,
    store: &mut impl NonceStore,
) -> Result<Option<Vec<Vec<u8>>>, NodeError> {
    let key_package = material.key_package()?;
    if !request.signers.contains(key_package.identifier()) {
        return Ok(None);
    }
    let (_, state) = load_state(relay, me, material).await?;
    let (pczt, payments) = proposal_pczt(&state, &request.proposal)?;
    let keys = material.vault_keys()?;
    let member = Member {
        identifier: *key_package.identifier(),
        key_package: &key_package,
        vault_fvk: keys.fvk(),
    };
    let shares = member
        .sign(
            request,
            &pczt,
            &expectations(
                network,
                &payments,
                tip_height,
                material.descriptor.proposal_expiry_blocks,
            )?,
            store,
        )
        .map_err(proto)?;
    Ok(Some(shares.iter().map(|s| s.serialize()).collect()))
}

/// Leader: waits for the shares answering exactly `request` (plus `own_shares` from
/// `sign_own_shares` when the leader is a signer), aggregates (verifying every
/// share), and broadcasts via lightwalletd, then logs the broadcast. Returns the txid.
///
/// The proof does not depend on the spend authorization signatures, so it is created on a
/// blocking thread while the shares arrive (as hardware-signer wallets do). `on_progress` is
/// called whenever the number of received shares changes. Safe to call again with the
/// same request after a timeout: shares already sent are still in the inbox.
#[allow(clippy::too_many_arguments)]
pub async fn finalize<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    request: &SigningRequest,
    own_shares: Option<&[Vec<u8>]>,
    lightwalletd: &mut Client,
    timeout: Duration,
    mut on_progress: impl FnMut(ShareProgress),
    rng: &mut R,
) -> Result<Sent, NodeError> {
    let deadline = Instant::now() + timeout;
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let (pczt, _) = proposal_pczt(&state, &request.proposal)?;

    let to_prove = pczt.clone();
    let proving = tokio::task::spawn_blocking(move || {
        Prover::new(to_prove)
            .create_ironwood_proof(proving_key())
            .map(|p| p.finish())
            .map_err(proto)
    });

    let needed = request.signers.len();
    let mut reported = usize::MAX;
    let own = match own_shares {
        Some(bytes) => Some((
            *material.key_package()?.identifier(),
            bytes
                .iter()
                .map(|b| SignatureShare::deserialize(b).map_err(proto))
                .collect::<Result<Vec<_>, _>>()?,
        )),
        None => None,
    };
    let shares = loop {
        let mut shares = read_shares(relay, me, material, &state, request).await?;
        if let Some((id, own)) = &own {
            shares.insert(*id, own.clone());
        }
        if shares.len() != reported {
            reported = shares.len();
            on_progress(ShareProgress {
                received: reported,
                needed,
            });
        }
        if shares.len() == needed {
            break shares;
        }
        if Instant::now() > deadline {
            return Err(NodeError::Timeout("signature shares"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    };

    let keys = material.vault_keys()?;
    let spends = tx::spends_to_sign(&pczt, keys.fvk()).map_err(proto)?;
    let signatures = aggregate_request(request, &spends, &shares, &material.public_key_package()?)
        .map_err(proto)?;

    let proved = proving
        .await
        .map_err(|e| NodeError::Protocol(format!("proving task: {e}")))??;
    let signed = tx::apply_signatures(proved, &signatures).map_err(proto)?;
    let sent = broadcast(signed, lightwalletd).await?;
    let txid = sent.txid;
    // The transaction is on its way regardless; if the log entry is no longer valid (e.g.
    // the author cancelled meanwhile), report the txid anyway.
    let event = VaultEvent::Broadcast {
        proposal: request.proposal,
        txid,
    };
    if let Err(e) = append_event(relay, me, material, &mut chain, &mut state, &event, rng).await {
        return Err(NodeError::Protocol(format!(
            "broadcast {} but could not log it: {e}",
            hex::encode(txid)
        )));
    }
    Ok(sent)
}

/// Serializes a signing request (versioned; the leader also keeps it as `<id>.req`).
pub fn encode_request(request: &SigningRequest) -> Result<Vec<u8>, NodeError> {
    let msg = SigningRequestMsg {
        proposal: request.proposal,
        pczt_hash: request.pczt_hash,
        signers: request.signers.iter().map(|id| id.serialize()).collect(),
        packages: request
            .packages
            .iter()
            .map(|p| p.serialize().map_err(proto))
            .collect::<Result<_, _>>()?,
    };
    Ok(version::encode(Format::SigningRequest, &msg)?)
}

pub fn decode_request(bytes: &[u8]) -> Result<SigningRequest, NodeError> {
    let msg: SigningRequestMsg = version::decode(Format::SigningRequest, bytes)?;
    Ok(SigningRequest {
        proposal: msg.proposal,
        pczt_hash: msg.pczt_hash,
        signers: msg
            .signers
            .iter()
            .map(|b| Identifier::deserialize(b).map_err(proto))
            .collect::<Result<_, _>>()?,
        packages: msg
            .packages
            .iter()
            .map(|b| SigningPackage::deserialize(b).map_err(proto))
            .collect::<Result<_, _>>()?,
    })
}

/// The leader's own serialized shares, kept until broadcast (`<id>.own`).
pub fn encode_own_shares(shares: &[Vec<u8>]) -> Result<Vec<u8>, NodeError> {
    Ok(version::encode(Format::OwnShares, shares)?)
}

pub fn decode_own_shares(bytes: &[u8]) -> Result<Vec<Vec<u8>>, NodeError> {
    Ok(version::decode(Format::OwnShares, bytes)?)
}

/// The leader's set of commitment sets already put in requests (`used_commitments.bin`).
pub fn encode_used_commitments(used: &BTreeSet<[u8; 32]>) -> Result<Vec<u8>, NodeError> {
    Ok(version::encode(Format::UsedCommitments, used)?)
}

/// Fails on an unreadable file instead of starting empty: forgetting used commitment
/// sets could put one in a second request.
pub fn decode_used_commitments(bytes: &[u8]) -> Result<BTreeSet<[u8; 32]>, NodeError> {
    Ok(version::decode(Format::UsedCommitments, bytes)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn pool_covers_sixteen_proposals_within_one_batch() {
        use super::pool_target_for;
        assert_eq!(
            pool_target_for(3, 2),
            32,
            "2-of-3: 2 per proposal, 16 proposals"
        );
        assert_eq!(pool_target_for(2, 2), 32, "the floor");
        assert_eq!(pool_target_for(5, 3), 96);
        assert_eq!(pool_target_for(7, 5), 240);
        assert_eq!(
            pool_target_for(8, 3),
            256,
            "capped at one batch (12 proposals)"
        );
    }

    #[test]
    fn sent_transactions_are_kept_until_removed() {
        let dir = std::env::temp_dir().join(format!("zafe-sent-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sent = super::SentTxs::in_dir(&dir);
        let tx = super::Sent {
            txid: [7; 32],
            raw: vec![1, 2, 3],
        };
        assert_eq!(sent.get(&tx.txid), None);
        sent.put(&tx);
        assert_eq!(sent.get(&tx.txid), Some(vec![1, 2, 3]));
        sent.remove(&tx.txid);
        assert_eq!(sent.get(&tx.txid), None);
        // Keeping nothing is allowed (CLI tests, callers without a directory).
        let none = super::SentTxs::none();
        none.put(&tx);
        assert_eq!(none.get(&tx.txid), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
    use super::*;

    /// The app embeds invites in `zafe://join?invite=...` links and QR codes unescaped,
    /// which relies on this character set; it also bounds the QR size.
    #[test]
    fn invite_text_is_url_safe_and_small() {
        let invite = Invite {
            mailbox: [7; 16],
            join_token: [9; 32],
            creator: [3; 32],
            threshold: 2,
            members: 3,
            name: "Grants committee — Ops fund 2026".into(),
        };
        let text = invite.encode();
        assert!(text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-:._~".contains(&b)));
        assert!(text.len() < 300, "{} chars", text.len());
        assert_eq!(Invite::decode(&text).unwrap(), invite);
    }

    #[test]
    fn invites_from_another_version_are_rejected_as_such() {
        let invite = Invite {
            mailbox: [7; 16],
            join_token: [9; 32],
            creator: [3; 32],
            threshold: 2,
            members: 3,
            name: "Ops".into(),
        };
        let text = invite.encode();
        assert!(text.starts_with("zafe-invite-v1:"));
        let newer = text.replacen("zafe-invite-v1:", "zafe-invite-v2:", 1);
        match Invite::decode(&newer) {
            Err(NodeError::UnsupportedVersion(v)) => {
                assert_eq!((v.format, v.found, v.supported), (Format::Invite, 2, 1));
                assert!(v.is_newer());
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            Invite::decode("zafe-invite-vx:00"),
            Err(NodeError::BadInvite)
        ));
    }

    #[test]
    fn leader_state_files_round_trip_and_reject_other_versions() {
        let own = vec![vec![1u8; 32], vec![2u8; 32]];
        let bytes = encode_own_shares(&own).unwrap();
        assert_eq!(decode_own_shares(&bytes).unwrap(), own);
        let used: BTreeSet<[u8; 32]> = [[1u8; 32], [2u8; 32]].into();
        let bytes = encode_used_commitments(&used).unwrap();
        assert_eq!(decode_used_commitments(&bytes).unwrap(), used);
        let mut newer = bytes.clone();
        newer[0] = 9;
        assert!(matches!(
            decode_used_commitments(&newer),
            Err(NodeError::UnsupportedVersion(_))
        ));
    }
}
