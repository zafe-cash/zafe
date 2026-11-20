use std::{
    collections::BTreeMap,
    fs,
    path::Path,
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
    version::{self, DecodeError, Format, UnsupportedVersion},
    Chain, Envelope, Identity, IdentityPublic, Kind, LogEntry, LogKey, MailboxId, ProtoError,
};
use zcash_keys::address::UnifiedAddress;
use zcash_protocol::consensus::Parameters;

use crate::{
    keygen::{member_identifier, KeygenParams, Round1, SkContribution},
    keys::VaultSecret,
    nonce_store::NonceKey,
    relay_client::{RelayClient, RelayClientError},
    state_dir,
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
    #[error("key generation timed out waiting for {stage} from {} member(s)", missing.len())]
    KeygenStalled {
        stage: &'static str,
        missing: Vec<[u8; 32]>,
    },
    #[error("verification failed: {0}")]
    Verification(String),
    #[error("your signature for this payment is already out")]
    AlreadySigned,
    #[error("{} signer(s) sent invalid signatures", .0.len())]
    InvalidShares(Vec<Identifier>),
    #[error("protocol: {0}")]
    Protocol(String),
    #[error("this phone's setup state: {0}")]
    Io(String),
    #[error(transparent)]
    Wallet(#[from] crate::wallet::WalletError),
    #[error("event no longer valid: {0}")]
    Invalid(crate::vault::VaultError),
    #[error("the relay lost part of the vault log: this device has {local_len} entries")]
    RelayRolledBack { local_len: u64 },
    #[error("the relay's vault log differs from this device's copy at entry {index}")]
    RelayForked { index: u64 },
    #[error("this device's copy of the vault history doesn't verify: {0}")]
    LogCopyInvalid(String),
    #[error("the relay no longer has this vault: this device has {local_len} log entries")]
    RelayLostVault { local_len: u64 },
    #[error("the relay's member list doesn't match the vault: {0}")]
    RelayMembership(String),
    #[error(transparent)]
    LogCopy(#[from] crate::log_cache::LogCacheError),
    #[error(transparent)]
    UnsupportedVersion(#[from] UnsupportedVersion),
    #[error("the vault moved to {url}: move to continue")]
    VaultMoved { url: String },
    #[error("this phone is already moving the vault to another relay")]
    MoveInProgress,
    #[error("{} member(s) must update Zafe before the vault can move", .0.len())]
    MembersMustUpdate(Vec<[u8; 32]>),
    #[error("no member has moved to that relay with this link")]
    MoveNotFound,
    #[error("this move expired")]
    MoveExpired,
    #[error("this move is older than the vault's last move")]
    MoveStale,
    #[error("two moves found for the same history: ask your group")]
    TwoMoves,
    #[error("you left this move: ask for a new one")]
    MoveCancelled,
    #[error("can't reach {url} to check the vault's move")]
    MoveUnconfirmed { url: String },
    #[error("the new relay's history doesn't match the vault's: start the move again")]
    MoveConflict,
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

fn vault_err(e: crate::vault::VaultError) -> NodeError {
    match e {
        crate::vault::VaultError::UnsupportedVersion(v) => NodeError::UnsupportedVersion(v),
        e => proto(e),
    }
}

fn chain_err(e: zafe_proto::ChainError) -> NodeError {
    match e {
        zafe_proto::ChainError::Fork(index) => NodeError::RelayForked { index },
        zafe_proto::ChainError::Invalid {
            source: ProtoError::UnsupportedVersion(v),
            ..
        } => NodeError::UnsupportedVersion(v),
        e => proto(e),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct VaultIds {
    pub vault: MailboxId,
    pub label: MailboxId,
}

impl VaultIds {
    pub const fn legacy(id: MailboxId) -> Self {
        Self {
            vault: id,
            label: id,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VaultRelay {
    pub url: Option<String>,
    pub label: MailboxId,
    pub create_token: Option<String>,
}

impl VaultRelay {
    pub fn legacy(vault_id: MailboxId) -> Self {
        Self {
            url: None,
            label: vault_id,
            create_token: None,
        }
    }

    pub fn url_or<'a>(&'a self, default: &'a str) -> &'a str {
        self.url.as_deref().unwrap_or(default)
    }
}

impl core::fmt::Debug for VaultRelay {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("VaultRelay")
            .field("url", &self.url)
            .field("label", &hex::encode(self.label))
            .field("create_token", &self.create_token.as_ref().map(|_| "…"))
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub vault_id: MailboxId,
    pub label: MailboxId,
    pub relay_url: Option<String>,
    pub join_token: [u8; 32],
    pub creator: [u8; 32],
    pub threshold: u16,
    pub members: u16,
    pub name: String,
}

#[derive(Deserialize)]
struct InviteV2 {
    mailbox: MailboxId,
    join_token: [u8; 32],
    creator: [u8; 32],
    threshold: u16,
    members: u16,
    name: String,
}

impl Invite {
    const PREFIX: &'static str = "zafe-invite-v";

    pub fn encode(&self) -> String {
        format!(
            "{}{}:{}",
            Self::PREFIX,
            version::INVITE,
            hex::encode(postcard::to_allocvec(self).expect("encodable"))
        )
    }

    pub fn decode(s: &str) -> Result<Self, NodeError> {
        let rest = s
            .trim()
            .strip_prefix(Self::PREFIX)
            .ok_or(NodeError::BadInvite)?;
        let (found, body) = rest.split_once(':').ok_or(NodeError::BadInvite)?;
        let found: u16 = found.parse().map_err(|_| NodeError::BadInvite)?;
        let body = hex::decode(body).map_err(|_| NodeError::BadInvite)?;
        if found == 2 {
            let v2: InviteV2 = postcard::from_bytes(&body).map_err(|_| NodeError::BadInvite)?;
            return Ok(Self {
                vault_id: v2.mailbox,
                label: v2.mailbox,
                relay_url: None,
                join_token: v2.join_token,
                creator: v2.creator,
                threshold: v2.threshold,
                members: v2.members,
                name: v2.name,
            });
        }
        version::check(Format::Invite, found)?;
        postcard::from_bytes(&body).map_err(|_| NodeError::BadInvite)
    }

    pub fn ids(&self) -> VaultIds {
        VaultIds {
            vault: self.vault_id,
            label: self.label,
        }
    }
}

#[derive(Default)]
pub struct SeqCounter(u64);

static LAST_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl SeqCounter {
    pub fn next_seq(&mut self) -> u64 {
        use std::sync::atomic::Ordering;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_micros() as u64)
            .unwrap_or(0);
        let floor = self.0;
        let mut last = LAST_SEQ.load(Ordering::SeqCst);
        loop {
            let next = last.max(floor).saturating_add(1).max(now);
            match LAST_SEQ.compare_exchange_weak(last, next, Ordering::SeqCst, Ordering::SeqCst) {
                Ok(_) => {
                    self.0 = next;
                    return next;
                }
                Err(current) => last = current,
            }
        }
    }
}

pub async fn send_fresh(
    relay: &RelayClient,
    me: &Identity,
    seq: &mut SeqCounter,
    mut seal: impl FnMut(u64) -> Result<Envelope, NodeError>,
) -> Result<(), NodeError> {
    let mut attempt = 0;
    loop {
        match relay.send(me, &seal(seq.next_seq())?).await {
            Err(RelayClientError::Status { status: 409, .. }) if attempt < 3 => attempt += 1,
            result => return Ok(result?),
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct VaultMaterial {
    pub descriptor: VaultDescriptor,
    pub key_package: Vec<u8>,
    pub public_key_package: Vec<u8>,
    pub vault_secret: [u8; 32],
    pub log_key_epoch: u32,
    pub log_key: [u8; 32],
    pub relay: VaultRelay,
}

#[derive(Deserialize)]
struct VaultMaterialV1 {
    descriptor: VaultDescriptor,
    key_package: Vec<u8>,
    public_key_package: Vec<u8>,
    vault_secret: [u8; 32],
    log_key_epoch: u32,
    log_key: [u8; 32],
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
    pub fn to_bytes(&self) -> Result<Vec<u8>, NodeError> {
        Ok(version::encode(Format::VaultMaterial, self)?)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, NodeError> {
        let material: Self = match version::split(Format::VaultMaterial, bytes)? {
            (1, body) => {
                let v1: VaultMaterialV1 = postcard::from_bytes(body)
                    .map_err(|_| DecodeError::Malformed(Format::VaultMaterial))?;
                Self {
                    relay: VaultRelay::legacy(v1.descriptor.vault_id),
                    descriptor: v1.descriptor,
                    key_package: v1.key_package,
                    public_key_package: v1.public_key_package,
                    vault_secret: v1.vault_secret,
                    log_key_epoch: v1.log_key_epoch,
                    log_key: v1.log_key,
                }
            }
            _ => version::decode(Format::VaultMaterial, bytes)?,
        };
        version::check(Format::Descriptor, material.descriptor.version)?;
        Ok(material)
    }

    pub fn ids(&self) -> VaultIds {
        VaultIds {
            vault: self.descriptor.vault_id,
            label: self.relay.label,
        }
    }

    pub fn label(&self) -> MailboxId {
        self.relay.label
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

mod relay_move;
pub use relay_move::*;

fn vault_address(
    fvk: &orchard::keys::FullViewingKey,
    network: &impl Parameters,
) -> Result<String, NodeError> {
    Ok(UnifiedAddress::from_receivers(
        Some(fvk.address_at(0u32, Scope::External)),
        None,
        None,
        None,
        None,
    )
    .ok_or_else(|| NodeError::Protocol("cannot build unified address".into()))?
    .encode(network))
}

pub fn ufvk_address(descriptor: &VaultDescriptor) -> Result<String, NodeError> {
    let network = crate::wallet::ZafeNetwork::from_name(&descriptor.network)
        .ok_or_else(|| proto("unknown network"))?;
    let ufvk = zcash_keys::keys::UnifiedFullViewingKey::decode(&network, &descriptor.ufvk)
        .map_err(proto)?;
    let fvk = ufvk
        .orchard()
        .ok_or_else(|| proto("the viewing key has no Orchard key"))?;
    vault_address(fvk, &network)
}

pub fn check_address(descriptor: &VaultDescriptor) -> Result<(), NodeError> {
    if ufvk_address(descriptor)? != descriptor.address {
        return Err(NodeError::Verification(
            "the vault's address isn't the one its viewing key gives".into(),
        ));
    }
    Ok(())
}

pub async fn create_vault<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    creator: &Identity,
    name: &str,
    threshold: u16,
    members: u16,
    rng: &mut R,
) -> Result<Invite, NodeError> {
    let mut vault_id = [0u8; 16];
    let mut label = [0u8; 16];
    let mut join_token = [0u8; 32];
    rng.fill_bytes(&mut vault_id);
    rng.fill_bytes(&mut label);
    rng.fill_bytes(&mut join_token);
    KeygenParams::new(vault_id, threshold, members).map_err(proto)?;
    relay.info().await?;
    relay
        .create_mailbox(creator, label, &join_token, members)
        .await?;
    relay.set_threshold(creator, label, threshold).await?;
    Ok(Invite {
        vault_id,
        label,
        relay_url: Some(relay.base_url().to_owned()),
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
    relay.join(member, invite.label, invite.join_token).await?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Membership {
    pub members: Vec<IdentityPublic>,
    pub sealed: bool,
    pub safety_words: Option<String>,
    pub waiting_for: Vec<[u8; 32]>,
}

pub async fn membership(
    relay: &RelayClient,
    who: &Identity,
    invite: &Invite,
    state_dir: &Path,
    key: &NonceKey,
) -> Result<Membership, NodeError> {
    let response = relay.members(who, invite.label).await?;
    let mut out = Membership {
        members: response.members,
        sealed: response.sealed,
        safety_words: None,
        waiting_for: Vec::new(),
    };
    if out.sealed {
        let (words, waiting) =
            safety_check(relay, who, invite.ids(), &out.members, state_dir, key).await?;
        out.safety_words = words;
        out.waiting_for = waiting;
    }
    Ok(out)
}

type SafetySent = (String, MailboxId, [u8; 32], u8, [u8; 32]);

static SAFETY_SENT: std::sync::Mutex<BTreeSet<SafetySent>> = std::sync::Mutex::new(BTreeSet::new());

pub async fn wait_for_safety_words(
    relay: &RelayClient,
    me: &Identity,
    invite: &Invite,
    state_dir: &Path,
    key: &NonceKey,
    timeout: Duration,
) -> Result<String, NodeError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(words) = membership(relay, me, invite, state_dir, key)
            .await?
            .safety_words
        {
            return Ok(words);
        }
        if Instant::now() > deadline {
            return Err(NodeError::Timeout("the other members' safety values"));
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

fn safety_secret(state_dir: &Path, key: &NonceKey, pinned: bool) -> Result<[u8; 32], NodeError> {
    let path = state_dir.join(state_dir::SAFETY_SECRET);
    let load = || -> Result<Option<[u8; 32]>, NodeError> {
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(NodeError::Io(e.to_string())),
        };
        let plain = key
            .open(state_dir::SAFETY_SECRET, &bytes)
            .ok_or_else(|| NodeError::Protocol("this phone's safety value can't be read".into()))?;
        Ok(Some(version::decode(Format::SafetySecret, &plain)?))
    };
    match load()? {
        Some(secret) => Ok(secret),
        None if pinned => Err(NodeError::Protocol(
            "this phone's safety value is gone after it was revealed".into(),
        )),
        None => {
            let mut secret = [0u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut secret);
            let sealed = key.seal(
                state_dir::SAFETY_SECRET,
                &version::encode(Format::SafetySecret, &secret)?,
            );
            fs::create_dir_all(state_dir).map_err(|e| NodeError::Io(e.to_string()))?;
            crate::durable::create_new(&path, &sealed).map_err(|e| NodeError::Io(e.to_string()))?;
            load()?.ok_or_else(|| NodeError::Io("safety value".into()))
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SafetyPin {
    members: Vec<IdentityPublic>,
    commitments: BTreeMap<[u8; 32], [u8; 32]>,
}

impl SafetyPin {
    fn load(path: &Path, key: &NonceKey) -> Result<Option<Self>, NodeError> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(NodeError::Io(e.to_string())),
        };
        let plain = key.open(state_dir::SAFETY_PIN, &bytes).ok_or_else(|| {
            NodeError::Protocol("this phone's safety word record can't be read".into())
        })?;
        Ok(Some(version::decode(Format::SafetyPin, &plain)?))
    }

    fn create(&self, state_dir: &Path, key: &NonceKey) -> Result<Self, NodeError> {
        let path = state_dir.join(state_dir::SAFETY_PIN);
        let sealed = key.seal(
            state_dir::SAFETY_PIN,
            &version::encode(Format::SafetyPin, self)?,
        );
        fs::create_dir_all(state_dir).map_err(|e| NodeError::Io(e.to_string()))?;
        crate::durable::create_new(&path, &sealed).map_err(|e| NodeError::Io(e.to_string()))?;
        Self::load(&path, key)?.ok_or_else(|| NodeError::Io("safety word record".into()))
    }
}

async fn safety_check(
    relay: &RelayClient,
    me: &Identity,
    ids: VaultIds,
    members: &[IdentityPublic],
    state_dir: &Path,
    key: &NonceKey,
) -> Result<(Option<String>, Vec<[u8; 32]>), NodeError> {
    let (vault, label) = (ids.vault, ids.label);
    let my_pk = me.public().sig_pk;
    let changed = |what: &str| {
        NodeError::Verification(format!(
            "{what} changed after this phone revealed its safety value: stop, and don't \
             make keys for this setup"
        ))
    };
    let pinned = SafetyPin::load(&state_dir.join(state_dir::SAFETY_PIN), key)?;
    let secret = safety_secret(state_dir, key, pinned.is_some())?;
    let mut sorted = members.to_vec();
    sorted.sort();
    sorted.dedup();
    if pinned.as_ref().is_some_and(|p| p.members != sorted) {
        return Err(changed("The member list"));
    }
    let by_pk: BTreeMap<[u8; 32], IdentityPublic> = sorted.iter().map(|m| (m.sig_pk, *m)).collect();
    if !by_pk.contains_key(&my_pk) {
        return Err(NodeError::NotReady(
            "this phone isn't a member of the setup".into(),
        ));
    }
    let others: Vec<[u8; 32]> = by_pk.keys().filter(|pk| **pk != my_pk).copied().collect();

    let mut commits = BTreeMap::<[u8; 32], Vec<[u8; 32]>>::new();
    let mut reveals = BTreeMap::<[u8; 32], [u8; 32]>::new();
    for (_, sender, env) in read_inbox(relay, me, label, &by_pk, 0).await? {
        let Some(value) = env
            .open(me, &sender)
            .ok()
            .and_then(|p| <[u8; 32]>::try_from(p.as_slice()).ok())
        else {
            continue;
        };
        match env.header.kind {
            Kind::SafetyCommit => commits.entry(sender.sig_pk).or_default().push(value),
            Kind::SafetyReveal => {
                reveals.entry(sender.sig_pk).or_insert(value);
            }
            _ => {}
        }
    }
    if let Some(pin) = &pinned {
        if commits
            .iter()
            .any(|(pk, seen)| seen.iter().any(|c| pin.commitments.get(pk) != Some(c)))
        {
            return Err(changed("A member's commitment"));
        }
    }

    let send = |kind: Kind, payload: [u8; 32]| async move {
        let key = (
            relay.base_url().to_owned(),
            label,
            my_pk,
            kind as u8,
            payload,
        );
        if SAFETY_SENT.lock().expect("not poisoned").contains(&key) {
            return Ok::<_, NodeError>(());
        }
        send_fresh(relay, me, &mut SeqCounter::default(), |seq| {
            Envelope::public(me, label, seq, kind, &payload).map_err(proto)
        })
        .await?;
        SAFETY_SENT.lock().expect("not poisoned").insert(key);
        Ok(())
    };
    send(
        Kind::SafetyCommit,
        crate::sas::commitment(&vault, &my_pk, &secret),
    )
    .await?;
    let pin = match pinned {
        Some(pin) => pin,
        None => {
            let missing_commits: Vec<_> = others
                .iter()
                .filter(|pk| !commits.contains_key(*pk))
                .copied()
                .collect();
            if !missing_commits.is_empty() {
                return Ok((None, missing_commits));
            }
            let pin = SafetyPin {
                members: sorted,
                commitments: others.iter().map(|pk| (*pk, commits[pk][0])).collect(),
            };
            if pin.create(state_dir, key)? != pin {
                return Err(changed("What the relay shows"));
            }
            pin
        }
    };
    send(Kind::SafetyReveal, secret).await?;
    let missing_reveals: Vec<_> = others
        .iter()
        .filter(|pk| !reveals.contains_key(*pk))
        .copied()
        .collect();
    if !missing_reveals.is_empty() {
        return Ok((None, missing_reveals));
    }
    let mut values = vec![(*me.public(), secret)];
    for pk in &others {
        let value = reveals[pk];
        if crate::sas::commitment(&vault, pk, &value) != pin.commitments[pk] {
            return Err(NodeError::Verification(
                "a member's safety value doesn't match what it committed to: stop, and \
                 don't make keys for this setup"
                    .into(),
            ));
        }
        values.push((by_pk[pk], value));
    }
    Ok((Some(crate::sas::words(&vault, &values)), Vec::new()))
}

pub async fn cancel_setup(
    relay: &RelayClient,
    creator: &Identity,
    invite: &Invite,
) -> Result<bool, NodeError> {
    if creator.public().sig_pk != invite.creator {
        return Err(NodeError::Verification(
            "only the creator can cancel a setup".to_owned(),
        ));
    }
    Ok(relay.cancel_setup(creator, invite.label).await?)
}

pub async fn renew_invite(
    relay: &RelayClient,
    creator: &Identity,
    invite: &Invite,
) -> Result<bool, NodeError> {
    if creator.public().sig_pk != invite.creator {
        return Err(NodeError::Verification(
            "only the creator can renew an invite".to_owned(),
        ));
    }
    Ok(relay.renew_invite(creator, invite.label).await?)
}

pub async fn seal(
    relay: &RelayClient,
    creator: &Identity,
    invite: &Invite,
) -> Result<(), NodeError> {
    let members = relay.members(creator, invite.label).await?.members;
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
            invite.label,
            members.iter().map(|m| m.sig_pk).collect(),
        )
        .await?;
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct Round1Msg {
    package: Vec<u8>,
    birthday_height: Option<u32>,
    proposal_expiry_blocks: Option<u32>,
    sk_commitment: [u8; 32],
    pq_key: Vec<u8>,
}

type Received = BTreeMap<(u8, [u8; 32]), Vec<u8>>;

fn kind_tag(kind: Kind) -> u8 {
    kind as u8
}

async fn read_received(
    relay: &RelayClient,
    me: &Identity,
    mailbox: MailboxId,
    members: &BTreeMap<[u8; 32], IdentityPublic>,
) -> Result<Received, NodeError> {
    let mut received = Received::new();
    for (_, sender, envelope) in read_inbox(relay, me, mailbox, members, 0).await? {
        if let Ok(payload) = envelope.open(me, &sender) {
            received
                .entry((kind_tag(envelope.header.kind), envelope.header.from))
                .or_insert(payload);
        }
    }
    Ok(received)
}

#[derive(Debug)]
pub enum KeygenProgress {
    Waiting {
        stage: &'static str,
        missing: Vec<[u8; 32]>,
    },
    Done(Box<VaultMaterial>),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct KeygenOptions {
    pub creator_birthday_height: Option<u32>,
    pub creator_expiry_blocks: Option<u32>,
    pub max_birthday: Option<u32>,
}

pub const BIRTHDAY_LEEWAY_BLOCKS: u32 = 10;

#[derive(Serialize, Deserialize)]
struct KeygenState {
    words: String,
    round1_secret: Vec<u8>,
    round1_package: Vec<u8>,
    round1_msg: Vec<u8>,
    sk_contribution: [u8; 32],
    max_birthday: Option<u32>,
    log_key: Option<[u8; 32]>,
    sent: u8,
}

const SENT_ROUND1: u8 = 1;
const SENT_ROUND2: u8 = 2;
const SENT_SIGNATURE: u8 = 4;
const SENT_LOG: u8 = 8;

impl KeygenState {
    fn load(path: &Path, key: &NonceKey) -> Result<Option<Self>, NodeError> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(NodeError::Io(e.to_string())),
        };
        let plain = key.open(state_dir::KEYGEN, &bytes).ok_or_else(|| {
            NodeError::Protocol("this phone's key-generation state can't be read".into())
        })?;
        Ok(Some(version::decode(Format::KeygenState, &plain)?))
    }

    fn sealed(&self, key: &NonceKey) -> Result<Vec<u8>, NodeError> {
        Ok(key.seal(
            state_dir::KEYGEN,
            &version::encode(Format::KeygenState, self)?,
        ))
    }

    fn save(&self, path: &Path, key: &NonceKey) -> Result<(), NodeError> {
        crate::durable::write(path, &self.sealed(key)?).map_err(|e| NodeError::Io(e.to_string()))
    }
}

pub fn keygen_started(state_dir: &Path) -> bool {
    state_dir.join(state_dir::KEYGEN).exists()
}

pub fn forget_keygen(state_dir: &Path) -> std::io::Result<()> {
    for name in [
        state_dir::KEYGEN,
        state_dir::SAFETY_PIN,
        state_dir::SAFETY_SECRET,
    ] {
        match crate::durable::remove(&state_dir.join(name)) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            other => other?,
        }
    }
    Ok(())
}

pub async fn keygen_step<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    invite: &Invite,
    membership: &Membership,
    state_dir: &Path,
    key: &NonceKey,
    confirmed: Option<&str>,
    network: &P,
    network_name: &str,
    options: KeygenOptions,
    rng: &mut R,
) -> Result<KeygenProgress, NodeError> {
    let waiting = |stage, missing| Ok(KeygenProgress::Waiting { stage, missing });
    let my_pk = me.public().sig_pk;
    let is_creator = my_pk == invite.creator;
    let members = &membership.members;
    if !membership.sealed || members.len() != usize::from(invite.members) {
        return waiting("members", Vec::new());
    }
    let Some(words) = membership.safety_words.clone() else {
        return waiting("safety words", membership.waiting_for.clone());
    };
    let params =
        KeygenParams::new(invite.vault_id, invite.threshold, invite.members).map_err(proto)?;
    let frost_id = |pk: &[u8; 32]| member_identifier(pk, &invite.vault_id).map_err(proto);

    let path = state_dir.join(state_dir::KEYGEN);
    let mut st = match KeygenState::load(&path, key)? {
        Some(st) => st,
        None => {
            let Some(confirmed) = confirmed else {
                return waiting("your confirmation", Vec::new());
            };
            if words != crate::sas::normalize(confirmed) {
                return Err(NodeError::SafetyNumberMismatch {
                    actual: words,
                    confirmed: confirmed.into(),
                });
            }
            if !relay.read_log(me, invite.label, 0).await?.is_empty() {
                return Err(NodeError::NotReady(
                    "this vault's keys were already made".into(),
                ));
            }
            let round1 = Round1::start(params, frost_id(&my_pk)?, rng).map_err(proto)?;
            let contribution = SkContribution::generate(rng);
            let msg = Round1Msg {
                pq_key: crate::pq::PqKeys::derive(me.seeds()).public().to_bytes(),
                sk_commitment: contribution.commitment(&invite.vault_id, &my_pk),
                package: round1.package().serialize().map_err(proto)?,
                birthday_height: if is_creator {
                    options.creator_birthday_height
                } else {
                    None
                },
                proposal_expiry_blocks: if is_creator {
                    Some(
                        options
                            .creator_expiry_blocks
                            .unwrap_or(crate::vault::DEFAULT_PROPOSAL_EXPIRY_BLOCKS),
                    )
                } else {
                    None
                },
            };
            let st = KeygenState {
                words: words.clone(),
                round1_secret: round1.secret_bytes().map_err(proto)?,
                round1_package: round1.package().serialize().map_err(proto)?,
                round1_msg: version::encode(Format::DkgRound1, &msg)?,
                sk_contribution: *contribution.as_bytes(),
                max_birthday: options.max_birthday,
                log_key: None,
                sent: 0,
            };
            fs::create_dir_all(state_dir).map_err(|e| NodeError::Io(e.to_string()))?;
            crate::durable::create_new(&path, &st.sealed(key)?)
                .map_err(|e| NodeError::Io(e.to_string()))?;
            KeygenState::load(&path, key)?.ok_or_else(|| NodeError::Io("keygen state".into()))?
        }
    };
    if st.words != words {
        return Err(NodeError::SafetyNumberMismatch {
            actual: words,
            confirmed: st.words,
        });
    }

    let by_pk: BTreeMap<[u8; 32], IdentityPublic> =
        members.iter().map(|m| (m.sig_pk, *m)).collect();
    let others: Vec<[u8; 32]> = by_pk.keys().filter(|pk| **pk != my_pk).copied().collect();
    let id_to_pk: BTreeMap<Identifier, [u8; 32]> = by_pk
        .keys()
        .map(|pk| Ok((frost_id(pk)?, *pk)))
        .collect::<Result<_, NodeError>>()?;
    let mut seq = SeqCounter::default();
    let public = |kind: Kind, payload: Vec<u8>| {
        move |n| Envelope::public(me, invite.label, n, kind, &payload).map_err(proto)
    };

    if st.sent & SENT_ROUND1 == 0 {
        send_fresh(
            relay,
            me,
            &mut seq,
            public(Kind::DkgRound1, st.round1_msg.clone()),
        )
        .await?;
        st.sent |= SENT_ROUND1;
        st.save(&path, key)?;
    }

    let received = read_received(relay, me, invite.label, &by_pk).await?;
    let from_all = |kind: Kind, from: &[[u8; 32]]| {
        let got: BTreeMap<[u8; 32], Vec<u8>> = from
            .iter()
            .filter_map(|pk| {
                received
                    .get(&(kind_tag(kind), *pk))
                    .map(|p| (*pk, p.clone()))
            })
            .collect();
        let missing: Vec<[u8; 32]> = from
            .iter()
            .filter(|pk| !got.contains_key(*pk))
            .copied()
            .collect();
        (got, missing)
    };

    let (r1, missing) = from_all(Kind::DkgRound1, &others);
    if !missing.is_empty() {
        return waiting("round-1 packages", missing);
    }
    let my_msg: Round1Msg =
        version::decode(Format::DkgRound1, &st.round1_msg).map_err(|e| match e {
            DecodeError::Unsupported(v) if !v.is_newer() => NodeError::Protocol(
                "this setup was started on an older Zafe: start the setup again".into(),
            ),
            e => e.into(),
        })?;
    let mut birthday_height = my_msg.birthday_height;
    let mut expiry_blocks = my_msg.proposal_expiry_blocks;
    let mut received1 = BTreeMap::new();
    let my_pq = crate::pq::PqKeys::derive(me.seeds());
    let mut commitments = BTreeMap::from([(frost_id(&my_pk)?, my_msg.sk_commitment)]);
    let mut pq_hashes = BTreeMap::from([(frost_id(&my_pk)?, my_pq.public().hash())]);
    let mut pq_keys = BTreeMap::new();
    for (pk, bytes) in &r1 {
        let m: Round1Msg = version::decode(Format::DkgRound1, bytes).map_err(|e| match e {
            DecodeError::Unsupported(v) if !v.is_newer() => NodeError::MembersMustUpdate(vec![*pk]),
            e => e.into(),
        })?;
        commitments.insert(frost_id(pk)?, m.sk_commitment);
        let pq_key = crate::pq::PqPublic::from_bytes(&m.pq_key)
            .map_err(|e| NodeError::Protocol(e.to_string()))?;
        pq_hashes.insert(frost_id(pk)?, pq_key.hash());
        pq_keys.insert(*pk, pq_key);
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
    if st.max_birthday.is_some_and(|max| birthday_height > max) {
        return Err(NodeError::Protocol(format!(
            "the creator chose wallet birthday {birthday_height}, ahead of the chain; ask \
             them to check their Zcash server and start a new setup"
        )));
    }
    let proposal_expiry_blocks = expiry_blocks
        .filter(|b| {
            (crate::vault::MIN_PROPOSAL_EXPIRY_BLOCKS..=crate::vault::MAX_PROPOSAL_EXPIRY_BLOCKS)
                .contains(b)
        })
        .ok_or_else(|| {
            NodeError::Protocol("creator sent no valid proposal expiry window".into())
        })?;

    let round1 = Round1::restore(
        params,
        frost_id(&my_pk)?,
        &st.round1_secret,
        &st.round1_package,
    )
    .map_err(proto)?;
    let contribution = SkContribution::from_bytes(st.sk_contribution);
    let (round2, outgoing) = round1.advance(received1).map_err(proto)?;
    let echo = crate::keygen::echo_with_pq_keys(
        &crate::keygen::echo_with_commitments(&round2.echo(), &commitments),
        &pq_hashes,
    );
    if st.sent & SENT_ROUND2 == 0 {
        send_fresh(relay, me, &mut seq, public(Kind::DkgEcho, echo.to_vec())).await?;
        for (to_id, package) in &outgoing {
            let to = by_pk[&id_to_pk[to_id]];
            let bytes = package.serialize().map_err(proto)?;
            let sealed_contribution = pq_keys[&to.sig_pk].seal(
                &crate::keygen::contribution_context(&invite.vault_id, &my_pk, &to.sig_pk),
                contribution.as_bytes(),
                rng,
            );
            for (kind, payload) in [
                (Kind::DkgRound2, bytes.as_slice()),
                (Kind::SkContribution, sealed_contribution.as_slice()),
            ] {
                send_fresh(relay, me, &mut seq, |n| {
                    Envelope::sealed(me, &to, invite.label, n, kind, payload, rng).map_err(proto)
                })
                .await?;
            }
        }
        st.sent |= SENT_ROUND2;
        st.save(&path, key)?;
    }

    let (echoes, missing) = from_all(Kind::DkgEcho, &others);
    if !missing.is_empty() {
        return waiting("echo hashes", missing);
    }
    if echoes.values().any(|e| e.as_slice() != echo.as_slice()) {
        return Err(NodeError::EchoMismatch);
    }
    let (r2, missing) = from_all(Kind::DkgRound2, &others);
    if !missing.is_empty() {
        return waiting("round-2 packages", missing);
    }
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

    let (sk_msgs, missing) = from_all(Kind::SkContribution, &others);
    if !missing.is_empty() {
        return waiting("sk contributions", missing);
    }
    let mut contributions = BTreeMap::new();
    contributions.insert(frost_id(&my_pk)?, contribution);
    for (pk, bytes) in sk_msgs {
        let opened = my_pq
            .open(
                &crate::keygen::contribution_context(&invite.vault_id, &pk, &my_pk),
                &bytes,
            )
            .map_err(|e| NodeError::Protocol(e.to_string()))?;
        let arr: [u8; 32] = opened.as_slice().try_into().map_err(proto)?;
        let id = frost_id(&pk)?;
        let revealed = SkContribution::from_bytes(arr);
        crate::keygen::check_contribution(&invite.vault_id, &pk, &commitments[&id], &revealed)
            .map_err(|e| NodeError::Protocol(e.to_string()))?;
        contributions.insert(id, revealed);
    }
    let output = dkg_result.finish(&contributions).map_err(proto)?;

    let address = vault_address(output.vault_keys.fvk(), network)?;
    let descriptor = VaultDescriptor {
        vault_id: invite.vault_id,
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
        ufvk: output
            .vault_keys
            .ufvk()
            .map_err(proto)?
            .encode(network)
            .map_err(proto)?,
        address,
        use_qsk: true,
        proposal_expiry_blocks,
        birthday_height,
        epoch: 0,
        transcript_hash: output.transcript_hash,
    };
    let message = descriptor.signing_message().map_err(proto)?;
    let my_sig = me.sign(&message).to_vec();
    if st.sent & SENT_SIGNATURE == 0 {
        send_fresh(
            relay,
            me,
            &mut seq,
            public(Kind::DescriptorSignature, my_sig.clone()),
        )
        .await?;
        st.sent |= SENT_SIGNATURE;
        st.save(&path, key)?;
    }
    let (sigs, missing) = from_all(Kind::DescriptorSignature, &others);
    if !missing.is_empty() {
        return waiting("descriptor signatures", missing);
    }
    let mut signatures = vec![(my_pk, my_sig)];
    for (pk, sig) in sigs {
        by_pk[&pk]
            .verify(&message, &sig)
            .map_err(|_| NodeError::Protocol("bad descriptor signature".into()))?;
        signatures.push((pk, sig));
    }

    let ids = invite.ids();
    let (log_key, create_token) = if is_creator {
        let key_bytes = match st.log_key {
            Some(k) => k,
            None => {
                let k = *LogKey::generate(0, rng).as_bytes();
                st.log_key = Some(k);
                st.save(&path, key)?;
                k
            }
        };
        let log_key = LogKey::from_bytes(0, key_bytes);
        let create_token = relay.create_token().map(str::to_owned);
        if st.sent & SENT_LOG == 0 {
            let bytes = encode_log_key_message(&log_key, create_token.as_deref())?;
            for pk in &others {
                send_fresh(relay, me, &mut seq, |n| {
                    Envelope::sealed(me, &by_pk[pk], invite.label, n, Kind::LogKey, &bytes, rng)
                        .map_err(proto)
                })
                .await?;
            }
            let event = VaultEvent::Created {
                descriptor: descriptor.clone(),
                signatures,
            };
            let entry = LogEntry::create(
                me,
                &log_key,
                invite.vault_id,
                0,
                [0; 32],
                &event.to_bytes().map_err(proto)?,
                rng,
            )
            .map_err(proto)?;
            append_entry(relay, me, ids, &entry).await?;
            st.sent |= SENT_LOG;
            st.save(&path, key)?;
        }
        (log_key, create_token)
    } else {
        let (msgs, missing) = from_all(Kind::LogKey, &[invite.creator]);
        if !missing.is_empty() {
            return waiting("log key", missing);
        }
        decode_log_key_message(&msgs[&invite.creator])?
    };

    let entries = relay.read_log(me, invite.label, 0).await?;
    if entries.is_empty() {
        return waiting("vault log", vec![invite.creator]);
    }
    let mut chain = Chain::new(invite.vault_id);
    for e in entries {
        chain.append(e, members).map_err(chain_err)?;
    }
    let state = VaultState::replay(chain.entries(), &log_key).map_err(vault_err)?;
    if state.descriptor != descriptor {
        return Err(NodeError::Protocol(
            "logged descriptor differs from ours".into(),
        ));
    }

    Ok(KeygenProgress::Done(Box::new(VaultMaterial {
        descriptor,
        key_package: output.key_package.serialize().map_err(proto)?,
        public_key_package: output.public_key_package.serialize().map_err(proto)?,
        vault_secret: *output.vault_secret.as_bytes(),
        log_key_epoch: log_key.epoch,
        log_key: *log_key.as_bytes(),
        relay: VaultRelay {
            url: invite.relay_url.clone(),
            label: invite.label,
            create_token,
        },
    })))
}

#[derive(Serialize, Deserialize)]
struct LogKeyMessage {
    epoch: u32,
    key: [u8; 32],
    create_token: Option<String>,
}

fn encode_log_key_message(key: &LogKey, create_token: Option<&str>) -> Result<Vec<u8>, NodeError> {
    Ok(version::encode(
        Format::LogKeyMessage,
        &LogKeyMessage {
            epoch: key.epoch,
            key: *key.as_bytes(),
            create_token: create_token.map(str::to_owned),
        },
    )?)
}

fn decode_log_key_message(bytes: &[u8]) -> Result<(LogKey, Option<String>), NodeError> {
    if bytes.len() == 36 && bytes[..2] == [0, 0] {
        let epoch = u32::from_le_bytes(bytes[..4].try_into().expect("4 bytes"));
        return Ok((
            LogKey::from_bytes(epoch, bytes[4..].try_into().expect("32 bytes")),
            None,
        ));
    }
    let m: LogKeyMessage = version::decode(Format::LogKeyMessage, bytes)?;
    let token = m.create_token.filter(|t| !t.trim().is_empty());
    Ok((LogKey::from_bytes(m.epoch, m.key), token))
}

async fn append_entry(
    relay: &RelayClient,
    me: &Identity,
    ids: VaultIds,
    entry: &LogEntry,
) -> Result<AppendResult, RelayClientError> {
    relay.append_log_to(me, ids.label, entry).await
}

pub async fn seal_when_full(
    relay: &RelayClient,
    me: &Identity,
    invite: &Invite,
    membership: Membership,
    state_dir: &Path,
    key: &NonceKey,
) -> Result<Membership, NodeError> {
    if membership.sealed
        || me.public().sig_pk != invite.creator
        || membership.members.len() != usize::from(invite.members)
    {
        return Ok(membership);
    }
    seal(relay, me, invite).await?;
    self::membership(relay, me, invite, state_dir, key).await
}

pub async fn run_keygen<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    invite: &Invite,
    confirmed_safety_words: &str,
    network: &P,
    network_name: &str,
    options: KeygenOptions,
    state_dir: std::path::PathBuf,
    key: NonceKey,
    rng: &mut R,
    timeout: Duration,
) -> Result<VaultMaterial, NodeError> {
    let (state_dir, key) = (state_dir.as_path(), &key);
    let deadline = Instant::now() + timeout;
    loop {
        let m = membership(relay, me, invite, state_dir, key).await?;
        let m = seal_when_full(relay, me, invite, m, state_dir, key).await?;
        match keygen_step(
            relay,
            me,
            invite,
            &m,
            state_dir,
            key,
            Some(confirmed_safety_words),
            network,
            network_name,
            options,
            rng,
        )
        .await?
        {
            KeygenProgress::Done(material) => {
                forget_keygen(state_dir).map_err(|e| NodeError::Io(e.to_string()))?;
                return Ok(*material);
            }
            KeygenProgress::Waiting { stage, missing } => {
                if Instant::now() > deadline {
                    return Err(NodeError::KeygenStalled { stage, missing });
                }
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

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

async fn fetch_rest(
    relay: &RelayClient,
    me: &Identity,
    label: MailboxId,
    key: &LogKey,
    chain: &mut Chain,
    state: &mut VaultState,
) -> Result<(), NodeError> {
    loop {
        let batch = relay.read_log(me, label, chain.len()).await?;
        if batch.is_empty() {
            break;
        }
        for entry in batch {
            chain
                .append(entry.clone(), &state.log_authors())
                .map_err(chain_err)?;
            state.apply_entry(&entry, key);
        }
    }
    Ok(())
}

async fn catch_up(
    relay: &RelayClient,
    me: &Identity,
    label: MailboxId,
    key: &LogKey,
    chain: &mut Chain,
    state: &mut VaultState,
) -> Result<(), NodeError> {
    let before = chain.len();
    fetch_rest(relay, me, label, key, chain, state).await?;
    if chain.len() > before {
        save_copy(relay, chain);
    }
    Ok(())
}

pub(crate) fn save_copy(relay: &RelayClient, chain: &Chain) {
    if let Some(cache) = relay.log_cache() {
        if let Err(e) = cache.write(&chain.mailbox(), chain.entries()) {
            eprintln!("log copy: {e}");
        }
    }
}

fn start_chain(
    first: &LogEntry,
    vault_id: MailboxId,
    key: &LogKey,
) -> Result<(Chain, VaultState), NodeError> {
    let state = VaultState::replay(std::slice::from_ref(first), key).map_err(vault_err)?;
    if state.descriptor.vault_id != vault_id {
        return Err(NodeError::Protocol(
            "the vault log belongs to another vault".into(),
        ));
    }
    check_address(&state.descriptor)?;
    let mut chain = Chain::new(vault_id);
    chain
        .append(first.clone(), &state.member_identities())
        .map_err(chain_err)?;
    Ok((chain, state))
}

fn extend_chain(
    chain: &mut Chain,
    state: &mut VaultState,
    entries: impl IntoIterator<Item = LogEntry>,
    key: &LogKey,
) -> Result<(), NodeError> {
    for entry in entries {
        chain
            .append(entry.clone(), &state.log_authors())
            .map_err(chain_err)?;
        state.apply_entry(&entry, key);
    }
    Ok(())
}

fn load_copy(
    relay: &RelayClient,
    vault_id: MailboxId,
    key: &LogKey,
) -> Result<Option<(Chain, VaultState)>, NodeError> {
    match relay.log_cache() {
        Some(cache) => copy_in(cache, vault_id, key),
        None => Ok(None),
    }
}

fn copy_in(
    cache: &crate::log_cache::LogCache,
    vault_id: MailboxId,
    key: &LogKey,
) -> Result<Option<(Chain, VaultState)>, NodeError> {
    let saved = cache.read(&vault_id)?;
    let Some((first, rest)) = saved.split_first() else {
        return Ok(None);
    };
    start_chain(first, vault_id, key)
        .and_then(|(mut chain, mut state)| {
            extend_chain(&mut chain, &mut state, rest.iter().cloned(), key)?;
            Ok((chain, state))
        })
        .map(Some)
        .map_err(|e| NodeError::LogCopyInvalid(e.to_string()))
}

pub fn state_from_copy(
    relay: &RelayClient,
    material: &VaultMaterial,
) -> Result<Option<(VaultState, u64)>, NodeError> {
    let Some(cache) = relay.log_cache() else {
        return Ok(None);
    };
    let vault = material.descriptor.vault_id;
    let Some((_, state)) = copy_in(cache, vault, &material.log_key())? else {
        return Ok(None);
    };
    let as_of = cache
        .relay_status(&vault)
        .last_reached
        .or_else(|| cache.written_at(&vault))
        .unwrap_or(0);
    Ok(Some((state, as_of)))
}

pub fn install_log_copy(
    cache: &crate::log_cache::LogCache,
    material: &VaultMaterial,
    entries: &[Vec<u8>],
) -> Result<u64, NodeError> {
    let parsed = entries
        .iter()
        .map(|b| LogEntry::from_bytes(b).map_err(proto))
        .collect::<Result<Vec<_>, _>>()?;
    let Some((first, rest)) = parsed.split_first() else {
        return Ok(cache.read(&material.descriptor.vault_id)?.len() as u64);
    };
    let key = material.log_key();
    let (mut chain, mut state) = start_chain(first, material.descriptor.vault_id, &key)?;
    extend_chain(&mut chain, &mut state, rest.iter().cloned(), &key)?;
    if state.descriptor.group_public_key != material.descriptor.group_public_key
        || state.descriptor.ufvk != material.descriptor.ufvk
    {
        return Err(NodeError::Protocol(
            "the log copy belongs to another vault".into(),
        ));
    }
    cache.write(&material.descriptor.vault_id, chain.entries())?;
    Ok(cache.read(&material.descriptor.vault_id)?.len() as u64)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogAnchor {
    pub vault_id: MailboxId,
    pub len: u64,
    pub head: [u8; 32],
}

impl LogAnchor {
    pub fn of(chain: &Chain) -> Self {
        Self {
            vault_id: chain.mailbox(),
            len: chain.len(),
            head: chain.head(),
        }
    }

    pub fn check(&self, chain: &Chain) -> Result<(), NodeError> {
        if chain.mailbox() != self.vault_id {
            return Err(NodeError::Protocol(
                "the vault log belongs to another vault".into(),
            ));
        }
        if self.len == 0 {
            return Ok(());
        }
        if chain.len() < self.len {
            return Err(NodeError::RelayRolledBack {
                local_len: self.len,
            });
        }
        let at = chain.entries()[(self.len - 1) as usize]
            .hash()
            .map_err(proto)?;
        if at != self.head {
            return Err(NodeError::RelayForked {
                index: self.len - 1,
            });
        }
        Ok(())
    }
}

const MEMBERSHIP_RECHECK: std::time::Duration = std::time::Duration::from_secs(60);

type MembershipChecks =
    std::sync::Mutex<std::collections::HashMap<(String, MailboxId), (std::time::Instant, usize)>>;

fn membership_checks() -> &'static MembershipChecks {
    static CHECKS: std::sync::OnceLock<MembershipChecks> = std::sync::OnceLock::new();
    CHECKS.get_or_init(Default::default)
}

pub async fn check_relay_membership(
    relay: &RelayClient,
    me: &Identity,
    label: MailboxId,
    state: &VaultState,
) -> Result<(), NodeError> {
    check_relay_membership_with(relay, me, label, state, None).await
}

fn membership_checked_recently(relay: &RelayClient, label: MailboxId) -> bool {
    let cache_key = (relay.base_url().to_string(), label);
    membership_checks()
        .lock()
        .expect("lock")
        .get(&cache_key)
        .is_some_and(|(at, _)| at.elapsed() < MEMBERSHIP_RECHECK)
}

type RelayMembers = (Vec<IdentityPublic>, Option<u16>);

async fn relay_members(
    relay: &RelayClient,
    me: &Identity,
    mailbox: MailboxId,
) -> Result<RelayMembers, NodeError> {
    match relay.mailbox_info(me, mailbox).await {
        Ok(info) => Ok((info.members, Some(info.threshold))),
        Err(RelayClientError::Status { status: 404, .. }) => {
            Ok((relay.members(me, mailbox).await?.members, None))
        }
        Err(e) => Err(e.into()),
    }
}

async fn check_relay_membership_with(
    relay: &RelayClient,
    me: &Identity,
    label: MailboxId,
    state: &VaultState,
    fetched: Option<Result<RelayMembers, NodeError>>,
) -> Result<(), NodeError> {
    let moves = state.seat_changes as usize;
    let cache_key = (relay.base_url().to_string(), label);
    if let Some((at, seen)) = membership_checks().lock().expect("lock").get(&cache_key) {
        if *seen == moves && at.elapsed() < MEMBERSHIP_RECHECK {
            return Ok(());
        }
    }
    let (members, threshold) = match fetched {
        Some(fetched) => fetched?,
        None => relay_members(relay, me, label).await?,
    };
    let mismatch = |why: String| Err(NodeError::RelayMembership(why));
    let mut held: Vec<IdentityPublic> = members;
    for want in &state.descriptor.members {
        let want = want.identity;
        let exact = held.iter().position(|h| *h == want);
        let moved = || {
            let mut key = want.sig_pk;
            for _ in 0..=state.replacements.len() {
                key = state.replacement_to(&key)?.old;
                if let Some(i) = held.iter().position(|h| h.sig_pk == key) {
                    return Some(i);
                }
            }
            None
        };
        match exact.or_else(moved) {
            Some(i) => {
                held.remove(i);
            }
            None => return mismatch("a member is missing or has another key".into()),
        }
    }
    if !held.is_empty() {
        return mismatch(format!("{} extra key(s) on the relay", held.len()));
    }
    if let Some(t) = threshold {
        if t != 0 && t != state.descriptor.threshold {
            return mismatch(format!(
                "the relay lists a threshold of {t}, the vault's is {}",
                state.descriptor.threshold
            ));
        }
    }
    membership_checks()
        .lock()
        .expect("lock")
        .insert(cache_key, (std::time::Instant::now(), moves));
    Ok(())
}

pub async fn load_state(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
) -> Result<(Chain, VaultState), NodeError> {
    let result = Box::pin(load_state_inner(relay, me, material)).await;
    record_reach(relay, material.descriptor.vault_id, &result);
    result
}

fn record_reach<T>(relay: &RelayClient, vault: MailboxId, result: &Result<T, NodeError>) {
    let Some(cache) = relay.log_cache() else {
        return;
    };
    let reached = match result {
        Ok(_) => true,
        Err(NodeError::Relay(e)) => !e.is_unreachable(),
        Err(
            NodeError::RelayRolledBack { .. }
            | NodeError::RelayForked { .. }
            | NodeError::RelayLostVault { .. }
            | NodeError::RelayMembership(_),
        ) => true,
        Err(_) => return,
    };
    cache.record_relay(&vault, reached);
}

pub fn relay_unreachable(e: &NodeError) -> bool {
    matches!(e, NodeError::Relay(r) if r.is_unreachable())
}

async fn load_state_inner(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
) -> Result<(Chain, VaultState), NodeError> {
    let ids = material.ids();
    let members = async {
        match membership_checked_recently(relay, ids.label) {
            true => None,
            false => Some(relay_members(relay, me, ids.label).await),
        }
    };
    let log_key = material.log_key();
    let (log, members) =
        Box::pin(async { tokio::join!(load_log(relay, me, ids, &log_key), members) }).await;
    let (chain, state) = log?;
    let d = &state.descriptor;
    if d.group_public_key != material.descriptor.group_public_key
        || d.ufvk != material.descriptor.ufvk
    {
        return Err(NodeError::Protocol(
            "the vault log belongs to another vault".into(),
        ));
    }
    Box::pin(check_relay_membership_with(
        relay, me, ids.label, &state, members,
    ))
    .await?;
    Ok((chain, state))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Followed {
    pub dropped: u64,
    pub entries: u64,
}

pub async fn follow_relay(
    relay: &RelayClient,
    me: &Identity,
    ids: VaultIds,
    key: &LogKey,
) -> Result<Followed, NodeError> {
    let Some(cache) = relay.log_cache() else {
        return Err(NodeError::Protocol(
            "this device keeps no copy to discard".into(),
        ));
    };
    let ours: Vec<LogEntry> = match load_copy(relay, ids.vault, key) {
        Ok(Some((copy, _))) => {
            match Box::pin(load_log(relay, me, ids, key)).await {
                Err(NodeError::RelayForked { .. }) => {}
                Err(e) => return Err(e),
                Ok(_) => {
                    return Err(NodeError::Protocol(
                        "the relay's log agrees with this device's copy".into(),
                    ))
                }
            }
            copy.entries().to_vec()
        }
        Ok(None) => {
            return Err(NodeError::Protocol(
                "this device has no copy to discard".into(),
            ))
        }
        Err(NodeError::LogCopyInvalid(_)) => cache.read(&ids.vault)?,
        Err(e) => return Err(e),
    };
    let bare = relay.clone().with_log_cache(None);
    let (theirs, state) = Box::pin(load_log(&bare, me, ids, key)).await?;
    Box::pin(check_relay_membership(&bare, me, ids.label, &state)).await?;
    let common = ours
        .iter()
        .zip(theirs.entries())
        .take_while(|(a, b)| matches!((a.hash(), b.hash()), (Ok(x), Ok(y)) if x == y))
        .count() as u64;
    cache.replace(&ids.vault, theirs.entries())?;
    Ok(Followed {
        dropped: ours.len() as u64 - common,
        entries: theirs.len(),
    })
}

pub async fn load_log(
    relay: &RelayClient,
    me: &Identity,
    ids: VaultIds,
    key: &LogKey,
) -> Result<(Chain, VaultState), NodeError> {
    let (chain, state, saved_len) = read_log_verified(relay, me, ids, key).await?;
    if saved_len == 0 || chain.len() > saved_len {
        save_copy(relay, &chain);
    }
    Ok((chain, state))
}

pub(crate) async fn read_log_verified(
    relay: &RelayClient,
    me: &Identity,
    ids: VaultIds,
    key: &LogKey,
) -> Result<(Chain, VaultState, u64), NodeError> {
    let label = ids.label;
    let (mut chain, mut state, saved_len, rest) = match load_copy(relay, ids.vault, key)? {
        Some((mut chain, mut state)) => {
            let saved_len = chain.len();
            let last = chain.entries().last().expect("a saved copy has entries");
            let last_hash = last.hash().map_err(proto)?;
            let from = saved_len - 1;
            let (page, bytes) = match relay.read_log_page(me, label, from).await {
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
            let rest = zafe_proto::relay::page_may_continue(page.len(), bytes);
            extend_chain(&mut chain, &mut state, page.into_iter().skip(1), key)?;
            (chain, state, saved_len, rest)
        }
        None => {
            let (first_page, bytes) = relay.read_log_page(me, label, 0).await?;
            let first = first_page.first().ok_or(NodeError::Invalid(
                crate::vault::VaultError::NotCreatedFirst,
            ))?;
            let rest = zafe_proto::relay::page_may_continue(first_page.len(), bytes);
            let (mut chain, mut state) = start_chain(first, ids.vault, key)?;
            extend_chain(&mut chain, &mut state, first_page.into_iter().skip(1), key)?;
            (chain, state, 0, rest)
        }
    };
    if rest {
        fetch_rest(relay, me, label, key, &mut chain, &mut state).await?;
    }
    Ok((chain, state, saved_len))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reseeded {
    Restored { entries: u64 },
    CaughtUp { appended: u64 },
    Current,
}

const RESEED_BATCH_BYTES: usize = 512 * 1024;

pub async fn reseed_relay(
    relay: &RelayClient,
    me: &Identity,
    ids: VaultIds,
    key: &LogKey,
) -> Result<Reseeded, NodeError> {
    use zafe_proto::relay::Reseed;
    let (chain, state) = load_copy(relay, ids.vault, key)?.ok_or_else(|| {
        NodeError::Protocol("this device has no copy of the vault log to restore from".into())
    })?;
    let total = chain.len();
    let raw: Vec<Vec<u8>> = chain
        .entries()
        .iter()
        .map(|e| e.to_bytes().map_err(proto))
        .collect::<Result<_, _>>()?;
    let members = state.member_identities();
    let threshold = state.descriptor.threshold;

    let mut from = 0usize;
    for _ in 0..(raw.len() + 8) {
        let mut bytes = 0;
        let mut end = from;
        while end < raw.len() && (end == from || bytes + raw[end].len() <= RESEED_BATCH_BYTES) {
            bytes += raw[end].len();
            end += 1;
        }
        let request = Reseed {
            mailbox: ids.label,
            members: members.clone(),
            threshold,
            from: from as u64,
            entries: raw[from..end].to_vec(),
            finish: end == raw.len(),
            timestamp: 0,
        };
        let answer = relay.reseed(me, request).await?;
        if !answer.restored {
            Box::pin(check_relay_membership(relay, me, ids.label, &state)).await?;
            return catch_relay_up(relay, me, ids, &chain, answer.len).await;
        }
        if answer.open {
            return Ok(Reseeded::Restored { entries: total });
        }
        from = usize::try_from(answer.len)
            .ok()
            .filter(|n| *n <= raw.len())
            .ok_or_else(|| NodeError::Protocol("the relay holds more than we sent".into()))?;
    }
    Err(NodeError::Protocol(
        "could not restore the log on the relay".into(),
    ))
}

async fn catch_relay_up(
    relay: &RelayClient,
    me: &Identity,
    ids: VaultIds,
    chain: &Chain,
    relay_len: u64,
) -> Result<Reseeded, NodeError> {
    let ours = chain.entries();
    let shared = relay_len.min(chain.len());
    if shared > 0 {
        let at = shared - 1;
        let page = relay.read_log(me, ids.label, at).await?;
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
        match append_entry(relay, me, ids, entry).await? {
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

pub async fn append_event<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    chain: &mut Chain,
    state: &mut VaultState,
    event: &VaultEvent,
    rng: &mut R,
) -> Result<u64, NodeError> {
    match Box::pin(relay_move::write_target(relay, me, material, chain, state)).await? {
        Some((new, ids)) => {
            Box::pin(append_on(&new, me, material, ids, chain, state, event, rng)).await
        }
        None => {
            Box::pin(append_on(
                relay,
                me,
                material,
                material.ids(),
                chain,
                state,
                event,
                rng,
            ))
            .await
        }
    }
}

async fn append_on<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    ids: VaultIds,
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
        let entry = LogEntry::create(me, &key, ids.vault, chain.len(), chain.head(), &bytes, rng)
            .map_err(proto)?;
        match append_entry(relay, me, ids, &entry).await? {
            AppendResult::Appended { index } => {
                chain
                    .append(entry.clone(), &state.log_authors())
                    .map_err(proto)?;
                state.apply_entry(&entry, &key);
                save_copy(relay, chain);
                return Ok(index);
            }
            AppendResult::Conflict { .. } => {
                catch_up(relay, me, ids.label, &key, chain, state).await?
            }
        }
    }
    Err(NodeError::Protocol(
        "could not append after 10 conflicts".into(),
    ))
}

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
    vault::{GroupShares, ProposalState, ProposalStatus, ProposedPayment},
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

pub fn request_hash(request: &SigningRequest) -> Result<[u8; 32], NodeError> {
    Ok(hash_request_bytes(&encode_request(request)?))
}

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
        max_expiry_delta: 1 + crate::wallet::block_scale(
            network,
            tip_height + 1 + crate::vault::EXPIRY_TIP_SLACK_BLOCKS,
        ) * (proposal_expiry_blocks
            + crate::vault::EXPIRY_TIP_SLACK_BLOCKS
            + crate::vault::EXPIRY_ROUNDING_BLOCKS),
    })
}

pub fn members_by_pk(state: &VaultState) -> BTreeMap<[u8; 32], IdentityPublic> {
    state
        .member_identities()
        .into_iter()
        .map(|m| (m.sig_pk, m))
        .collect()
}

#[derive(Clone, Debug, Default)]
pub struct ChainView {
    pub tip: u32,
    pub spends: Vec<crate::wallet::VaultSpend>,
    pub dropped: Vec<[u8; 32]>,
}

impl ChainView {
    pub fn of<P: Parameters + Clone + Send + Sync + 'static>(
        wallet: &VaultWallet<P>,
    ) -> Result<Self, NodeError> {
        Ok(Self {
            tip: wallet
                .chain_height()?
                .ok_or_else(|| NodeError::NotReady("wallet not synced".into()))?,
            spends: wallet.vault_spends()?,
            dropped: Vec::new(),
        })
    }

    pub async fn with_mempool(
        mut self,
        state: &VaultState,
        lightwalletd: &mut Client,
    ) -> Result<Self, NodeError> {
        let pending: Vec<[u8; 32]> = state
            .proposals
            .values()
            .filter(|p| self.pending_broadcast(state, p))
            .filter_map(|p| p.txid)
            .collect();
        if !pending.is_empty() {
            let mempool = crate::wallet::mempool_txids(lightwalletd).await?;
            self.dropped = pending
                .into_iter()
                .filter(|txid| !mempool.contains(txid))
                .collect();
        }
        Ok(self)
    }

    fn pending_broadcast(&self, state: &VaultState, p: &ProposalState) -> bool {
        p.status == ProposalStatus::Broadcast
            && self.tip < p.expiry_height.saturating_add(state.reorg_margin(self.tip))
            && p.txid.is_some_and(|txid| {
                !self
                    .spends
                    .iter()
                    .any(|s| s.txid == txid && s.mined_height.is_some())
            })
    }
}

pub const TIP_LEAD_BLOCKS: u32 = 10;

pub fn still_sendable<'a>(state: &'a VaultState, chain: &ChainView) -> Vec<&'a ProposalState> {
    let margin = state.reorg_margin(chain.tip);
    let spent_deep = |p: &ProposalState| {
        chain.spends.iter().any(|s| {
            s.mined_height
                .is_some_and(|h| chain.tip.saturating_add(1).saturating_sub(h) >= margin)
                && s.nullifiers.iter().any(|nf| p.nullifiers.contains(nf))
        })
    };
    let mut live: Vec<_> = state
        .proposals
        .values()
        .filter(|p| {
            let withdrawn = matches!(
                p.status,
                ProposalStatus::Cancelled | ProposalStatus::Rejected
            );
            let dropped = chain.pending_broadcast(state, p)
                && p.txid.is_some_and(|txid| chain.dropped.contains(&txid));
            (withdrawn || dropped)
                && p.possibly_signed
                && chain.tip < p.expiry_height.saturating_add(margin)
                && !spent_deep(p)
        })
        .collect();
    live.sort_by_key(|p| p.log_index);
    live
}

pub fn check_vote(
    state: &VaultState,
    proposal: &ProposalId,
    chain: &ChainView,
) -> Result<(), NodeError> {
    let p = state
        .proposals
        .get(proposal)
        .ok_or_else(|| NodeError::Protocol("unknown proposal".into()))?;
    let scale = state.block_scale(chain.tip);
    if p.tip_height > chain.tip.saturating_add(TIP_LEAD_BLOCKS * scale) {
        return Err(NodeError::NotReady(
            "this payment was built on newer blocks than this phone has: sync first".into(),
        ));
    }
    if p.tip_height
        .saturating_add(state.descriptor.proposal_expiry_blocks * scale)
        < chain.tip
    {
        return Err(NodeError::Verification(
            "this payment was built on blocks too old to approve".into(),
        ));
    }
    if still_sendable(state, chain).iter().any(|w| {
        w.log_index < p.log_index && !w.nullifiers.iter().any(|nf| p.nullifiers.contains(nf))
    }) {
        return Err(NodeError::Verification(
            "a cancelled or dropped payment can still be sent, and this one doesn't spend its \
             funds: both could go out"
                .into(),
        ));
    }
    Ok(())
}

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
    check_payees(material, wallet.params(), payments)?;
    let params = wallet.params().clone();
    let (branch, loaded) = tokio::join!(
        crate::wallet::ensure_current_branch(lightwalletd, &params),
        load_state(relay, me, material)
    );
    branch?;
    let (mut chain, mut state) = loaded?;
    propose_loaded(
        relay,
        me,
        material,
        wallet,
        lightwalletd,
        sent,
        payments,
        auto_send,
        &mut chain,
        &mut state,
        rng,
    )
    .await
}

pub fn check_payees<P: Parameters>(
    material: &VaultMaterial,
    params: &P,
    payments: &[PaymentRequest],
) -> Result<(), NodeError> {
    let fvk = material.vault_keys()?.fvk().clone();
    for p in payments {
        if orchard_receiver(params, &p.address).is_ok_and(|a| fvk.scope_for_address(&a).is_some()) {
            return Err(crate::wallet::WalletError::Payment(
                "that is this vault's own address".into(),
            )
            .into());
        }
    }
    Ok(())
}

pub async fn propose_loaded<
    P: Parameters + Clone + Send + Sync + 'static,
    R: RngCore + CryptoRng,
>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    wallet: &mut VaultWallet<P>,
    lightwalletd: &mut Client,
    sent: &SentTxs,
    payments: &[PaymentRequest],
    auto_send: bool,
    chain: &mut Chain,
    state: &mut VaultState,
    rng: &mut R,
) -> Result<ProposalId, NodeError> {
    check_payees(material, wallet.params(), payments)?;
    let fvk = material.vault_keys()?.fvk().clone();
    let me_pk = me.public().sig_pk;
    for _ in 0..3 {
        reserve_notes(state, &me_pk, wallet, lightwalletd, sent).await?;
        let chain_view = ChainView::of(wallet)?
            .with_mempool(state, lightwalletd)
            .await?;
        let must_spend: Vec<Vec<[u8; 32]>> = still_sendable(state, &chain_view)
            .iter()
            .map(|w| w.nullifiers.clone())
            .collect();
        let pczt = match wallet.propose(
            payments,
            material.descriptor.proposal_expiry_blocks,
            &must_spend,
        ) {
            Err(crate::wallet::WalletError::NotesSpent) => {
                return Err(NodeError::NotReady(
                    "a cancelled payment's funds were just moved: try again in a few minutes"
                        .into(),
                ))
            }
            other => other?,
        };
        let tip = chain_view.tip;
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
        match append_event(relay, me, material, chain, state, &event, rng).await {
            Ok(_) => return Ok(id),
            Err(NodeError::Invalid(VaultError::NotesInUse(_))) => continue,
            Err(e) => return Err(e),
        }
    }
    Err(NodeError::NotReady(
        "other proposals keep taking the same notes; try again".into(),
    ))
}

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
    if !matches!(
        target.status,
        ProposalStatus::Cancelled | ProposalStatus::Rejected
    ) {
        return Err(NodeError::NotReady(
            "only a cancelled payment can be made unsendable".into(),
        ));
    }
    let chain_view = ChainView::of(wallet)?
        .with_mempool(&state, lightwalletd)
        .await?;
    let mut swept = vec![target];
    swept.extend(
        still_sendable(&state, &chain_view)
            .into_iter()
            .filter(|w| w.id != cancelled),
    );
    let nullifiers: Vec<[u8; 32]> = swept.iter().flat_map(|w| w.nullifiers.clone()).collect();
    let owners: Vec<[u8; 32]> = swept.iter().map(|w| w.pczt_hash).collect();
    let keys = material.vault_keys()?;
    let fvk = keys.fvk().clone();
    let internal = zcash_keys::address::UnifiedAddress::from_receivers(
        Some(fvk.address_at(0u32, orchard::keys::Scope::Internal)),
        None,
        None,
        None,
        None,
    )
    .ok_or_else(|| NodeError::Protocol("vault internal address".into()))?
    .encode(wallet.params());
    let recipient = zcash_address::ZcashAddress::try_from_encoded(&internal).map_err(proto)?;

    reserve_notes(&state, &me.public().sig_pk, wallet, lightwalletd, sent).await?;
    let pczt = wallet.propose_sweep(
        &nullifiers,
        &owners,
        recipient,
        material.descriptor.proposal_expiry_blocks,
    )?;
    let tip = chain_view.tip;
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

pub async fn propose_again<
    P: Parameters + Clone + Send + Sync + 'static,
    R: RngCore + CryptoRng,
>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    wallet: &mut VaultWallet<P>,
    lightwalletd: &mut Client,
    sent: &SentTxs,
    proposal: ProposalId,
    rng: &mut R,
) -> Result<ProposalId, NodeError> {
    let (_, state) = load_state(relay, me, material).await?;
    let p = state
        .proposals
        .get(&proposal)
        .ok_or_else(|| NodeError::Protocol("unknown proposal".into()))?;
    if p.author != me.public().sig_pk {
        return Err(NodeError::Verification(
            "only the member who proposed it can propose it again".into(),
        ));
    }
    if !matches!(p.status, ProposalStatus::Open | ProposalStatus::Approved) {
        return Err(NodeError::NotReady("this payment is no longer open".into()));
    }
    let auto_send = p.auto_send;
    let payments = p
        .payments
        .iter()
        .map(|x| {
            Ok(PaymentRequest {
                address: x.address.clone(),
                amount_zat: x.amount_zat,
                memo: Some(zcash_protocol::memo::MemoBytes::from_bytes(&x.memo).map_err(proto)?),
            })
        })
        .collect::<Result<Vec<_>, NodeError>>()?;
    let swept = payments
        .is_empty()
        .then(|| {
            state.proposals.values().find(|q| {
                matches!(
                    q.status,
                    ProposalStatus::Cancelled | ProposalStatus::Rejected
                ) && q.nullifiers.iter().any(|nf| p.nullifiers.contains(nf))
            })
        })
        .flatten()
        .map(|q| q.id);
    if payments.is_empty() && swept.is_none() {
        return Err(NodeError::Protocol("nothing to propose again".into()));
    }
    cancel(relay, me, material, proposal, rng).await?;
    match swept {
        Some(target) => {
            invalidate(relay, me, material, wallet, lightwalletd, sent, target, rng).await
        }
        None => {
            propose(
                relay,
                me,
                material,
                wallet,
                lightwalletd,
                sent,
                &payments,
                auto_send,
                rng,
            )
            .await
        }
    }
}

pub fn one_tap_stuck(
    state: &VaultState,
    proposal: &ProposalId,
    me: &[u8; 32],
    pool: &impl PoolStore,
) -> bool {
    let Some(p) = state.proposals.get(proposal) else {
        return false;
    };
    let Some(pre) = p.preprocessed.as_ref() else {
        return false;
    };
    if p.ready_group.is_some()
        || !matches!(p.status, ProposalStatus::Open | ProposalStatus::Approved)
    {
        return false;
    }
    let can_still_sign = |m: &[u8; 32], g: usize| match p.shares.get(m) {
        Some(posted) => posted.iter().any(|s| usize::from(s.group) == g),
        None if m == me => (0..usize::from(p.signing_spends))
            .all(|j| pre.commitment(g, j, me).is_some_and(|c| pool.contains(c))),
        None => true,
    };
    !(0..pre.subsets.len()).any(|g| pre.subsets[g].iter().all(|m| can_still_sign(m, g)))
}

pub async fn reserve_notes<P: Parameters + Clone + Send + Sync + 'static>(
    state: &VaultState,
    me: &[u8; 32],
    wallet: &mut VaultWallet<P>,
    lightwalletd: &mut Client,
    sent: &SentTxs,
) -> Result<usize, NodeError> {
    let tip = wallet.chain_height()?.unwrap_or(0);
    let mut pending = Vec::new();
    for p in state.proposals.values() {
        if let (ProposalStatus::Broadcast, Some(txid)) = (p.status, p.txid) {
            if p.expiry_height > tip && !wallet.tx_mined(&txid)? {
                pending.push((p.id, txid, p.signed_tx.as_deref()));
            } else {
                sent.remove(&txid);
            }
        }
    }
    let mut dropped = BTreeSet::new();
    if !pending.is_empty() {
        let mempool = crate::wallet::mempool_txids(lightwalletd).await?;
        let chain_tip = crate::wallet::latest_height(lightwalletd).await?;
        if chain_tip == tip {
            for (id, txid, logged) in pending {
                if mempool.contains(&txid) {
                    continue;
                }
                let raw = sent.get(&txid).or_else(|| logged.map(<[u8]>::to_vec));
                let resent = match raw {
                    Some(raw) => send_raw(&raw, lightwalletd).await?.is_ok(),
                    None => false,
                };
                if !resent {
                    dropped.insert(id);
                }
            }
        }
    }
    Ok(wallet.reserve(&note_holds(state, &dropped, me))?)
}

pub fn note_holds(
    state: &VaultState,
    dropped: &BTreeSet<ProposalId>,
    me: &[u8; 32],
) -> Vec<crate::wallet::NoteHold> {
    let mut proposals: Vec<_> = state
        .proposals
        .values()
        .filter_map(|p| {
            let until = match p.status {
                ProposalStatus::Open | ProposalStatus::Approved if p.possibly_signed => {
                    p.expiry_height
                }
                ProposalStatus::Open | ProposalStatus::Approved
                    if p.reserves(state.descriptor.threshold) || p.author == *me =>
                {
                    state.reservation_end(p)
                }
                ProposalStatus::Broadcast if !dropped.contains(&p.id) => p.expiry_height,
                ProposalStatus::Broadcast => p
                    .expiry_height
                    .saturating_add(state.reorg_margin(p.expiry_height)),
                ProposalStatus::Cancelled | ProposalStatus::Rejected if p.possibly_signed => p
                    .expiry_height
                    .saturating_add(state.reorg_margin(p.expiry_height)),
                _ => return None,
            };
            Some((p, until))
        })
        .collect();
    proposals.sort_by_key(|(p, _)| p.log_index);
    proposals
        .into_iter()
        .map(|(p, until)| crate::wallet::NoteHold {
            owner: p.pczt_hash,
            nullifiers: p.nullifiers.clone(),
            expiry_height: until,
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

pub fn review<P: Parameters>(
    state: &VaultState,
    material: &VaultMaterial,
    network: &P,
    chain: &ChainView,
    proposal: ProposalId,
) -> Result<VerifiedTx, NodeError> {
    let tip_height = chain.tip;
    let (pczt, payments) = proposal_pczt(state, &proposal)?;
    check_vote(state, &proposal, chain)?;
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

#[derive(Debug)]
pub struct Approved {
    pub verified: VerifiedTx,
    pub signed: bool,
    pub completed: bool,
    pub auto_send: bool,
    pub timings: Vec<(&'static str, u128)>,
}

struct Laps(Instant, Vec<(&'static str, u128)>);

impl Laps {
    fn new() -> Self {
        Self(Instant::now(), Vec::new())
    }
    fn lap(&mut self, step: &'static str) {
        let now = Instant::now();
        self.1.push((step, now.duration_since(self.0).as_millis()));
        self.0 = now;
    }
}

pub async fn approve<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    view: &ChainView,
    proposal: ProposalId,
    store: &mut impl NonceStore,
    pool: &mut impl PoolStore,
    rng: &mut R,
) -> Result<Approved, NodeError> {
    let mut laps = Laps::new();
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    laps.lap("history");
    approve_loaded(
        relay, me, material, network, view, proposal, store, pool, rng, &mut chain, &mut state,
        laps,
    )
    .await
}

pub async fn approve_after_propose<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    view: &ChainView,
    proposal: ProposalId,
    store: &mut impl NonceStore,
    pool: &mut impl PoolStore,
    rng: &mut R,
    chain: &mut Chain,
    state: &mut VaultState,
) -> Result<Approved, NodeError> {
    approve_loaded(
        relay,
        me,
        material,
        network,
        view,
        proposal,
        store,
        pool,
        rng,
        chain,
        state,
        Laps::new(),
    )
    .await
}

async fn approve_loaded<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    view: &ChainView,
    proposal: ProposalId,
    store: &mut impl NonceStore,
    pool: &mut impl PoolStore,
    rng: &mut R,
    chain: &mut Chain,
    state: &mut VaultState,
    mut laps: Laps,
) -> Result<Approved, NodeError> {
    let (pczt, payments) = proposal_pczt(state, &proposal)?;
    let expected = expectations(
        network,
        &payments,
        view.tip,
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

    let add_commitments = p.shares.contains_key(&my_pk);
    if add_commitments {
        pool.drop_vote(&proposal, &pczt_hash);
        let needed = p.one_tap_blocked()
            && (p.approvals.get(&my_pk).is_none_or(Vec::is_empty)
                || !store.contains(&proposal, &pczt_hash));
        if !needed {
            let verified = verify_pczt(&pczt, keys.fvk(), &expected)
                .map_err(|e| NodeError::Verification(e.to_string()))?;
            laps.lap("sign");
            return Ok(Approved {
                verified,
                signed: true,
                completed: p.completed_by == Some(my_pk),
                auto_send,
                timings: laps.1,
            });
        }
    }
    check_vote(state, &proposal, view)?;
    if let Some(bytes) = pool.kept_vote(&proposal, &pczt_hash) {
        let event = VaultEvent::from_bytes(&bytes).map_err(proto)?;
        let kept_ok = matches!(
            &event,
            VaultEvent::Vote { proposal: id, pczt_hash: h, approve: true, shares, .. }
                if *id == proposal && *h == pczt_hash && !shares.is_empty()
        );
        if kept_ok {
            let verified = verify_pczt(&pczt, keys.fvk(), &expected)
                .map_err(|e| NodeError::Verification(e.to_string()))?;
            check_signing_spends(&verified, signing_spends)?;
            laps.lap("sign");
            append_event(relay, me, material, chain, state, &event, rng).await?;
            laps.lap("post");
            pool.drop_vote(&proposal, &pczt_hash);
            let p = &state.proposals[&proposal];
            return Ok(Approved {
                verified,
                signed: true,
                completed: p.completed_by == Some(my_pk),
                auto_send,
                timings: laps.1,
            });
        }
        pool.drop_vote(&proposal, &pczt_hash);
    }

    let one_tap = match &p.preprocessed {
        Some(pre) if !add_commitments => {
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
        _ => None,
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
        if let Err(e) = pool.keep_vote(&proposal, &pczt_hash, &event.to_bytes().map_err(proto)?) {
            tracing::warn!("could not keep the one-tap vote: {e}");
        }
        (event, verified, true)
    } else {
        let unlogged = store
            .commitments(&proposal, &pczt_hash)
            .map(|c| {
                c.iter()
                    .map(|c| c.serialize().map_err(proto))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .filter(|c: &Vec<Vec<u8>>| p.approvals.get(&my_pk) != Some(c));
        let (commitments, verified) = match unlogged {
            Some(commitments) => {
                let verified = verify_pczt(&pczt, keys.fvk(), &expected)
                    .map_err(|e| NodeError::Verification(e.to_string()))?;
                (commitments, verified)
            }
            None => {
                let (approval, verified) = member
                    .approve(proposal, &pczt, &expected, store, rng)
                    .map_err(approve_error)?;
                let commitments = approval
                    .commitments
                    .iter()
                    .map(|c| c.serialize().map_err(proto))
                    .collect::<Result<Vec<_>, _>>()?;
                (commitments, verified)
            }
        };
        check_signing_spends(&verified, signing_spends)?;
        let event = VaultEvent::Vote {
            proposal,
            pczt_hash,
            approve: true,
            commitments,
            shares: vec![],
        };
        (event, verified, add_commitments)
    };
    laps.lap("sign");
    append_event(relay, me, material, chain, state, &event, rng).await?;
    laps.lap("post");
    if signed {
        pool.drop_vote(&proposal, &pczt_hash);
    }
    let p = &state.proposals[&proposal];
    Ok(Approved {
        verified,
        signed,
        completed: p.completed_by == Some(my_pk),
        auto_send,
        timings: laps.1,
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

pub const POOL_PROPOSALS: usize = 16;

pub const MIN_POOL_TARGET: usize = 32;

pub fn pool_target(descriptor: &crate::vault::VaultDescriptor) -> usize {
    pool_target_for(descriptor.members.len(), usize::from(descriptor.threshold))
}

pub const MAX_DEEP_POOL: usize = 4 * crate::vault::MAX_COMMITMENT_BATCH;

pub fn pool_target_for_notes(
    descriptor: &crate::vault::VaultDescriptor,
    notes: Option<usize>,
) -> usize {
    deep_pool_target_for(
        descriptor.members.len(),
        usize::from(descriptor.threshold),
        notes,
    )
}

fn deep_pool_target_for(n: usize, t: usize, notes: Option<usize>) -> usize {
    let per_spend = binomial(n.saturating_sub(1), t.saturating_sub(1));
    let deep = notes
        .unwrap_or(0)
        .saturating_mul(per_spend)
        .min(MAX_DEEP_POOL);
    pool_target_for(n, t).max(deep)
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

pub async fn top_up_pool<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    pool: &mut impl PoolStore,
    reset: bool,
    notes: Option<usize>,
    rng: &mut R,
) -> Result<usize, NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    top_up_pool_loaded(
        relay, me, material, pool, reset, notes, &mut chain, &mut state, rng,
    )
    .await
}

pub async fn top_up_pool_loaded<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    pool: &mut impl PoolStore,
    reset: bool,
    notes: Option<usize>,
    chain: &mut Chain,
    state: &mut VaultState,
    rng: &mut R,
) -> Result<usize, NodeError> {
    if reset
        && state
            .pools
            .get(&me.public().sig_pk)
            .is_some_and(|p| p.available() > 0)
    {
        append_event(
            relay,
            me,
            material,
            chain,
            state,
            &VaultEvent::PoolReset,
            rng,
        )
        .await?;
    }
    let available = state
        .pools
        .get(&me.public().sig_pk)
        .map_or(0, crate::vault::Pool::available);
    let target = pool_target_for_notes(&material.descriptor, notes);
    if available * 2 >= target {
        return Ok(0);
    }
    let key_package = material.key_package()?;
    let mut count = 0;
    while available + count < target {
        let size = (target - available - count).min(crate::vault::MAX_COMMITMENT_BATCH);
        let batch =
            crate::session::new_pool_commitments(&key_package, size, pool, rng).map_err(proto)?;
        count += batch.len();
        append_event(
            relay,
            me,
            material,
            chain,
            state,
            &VaultEvent::Commitments { batch },
            rng,
        )
        .await?;
    }
    Ok(count)
}

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
        pool.drop_vote(&p.id, &p.pczt_hash);
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
        .filter(|p| store.take(&p.id, &p.pczt_hash).is_some())
        .count()
}

pub fn is_ready(state: &VaultState, proposal: &ProposalId) -> bool {
    state
        .proposals
        .get(proposal)
        .is_some_and(|p| p.ready_group.is_some() && p.status == ProposalStatus::Approved)
}

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
    let (Some(ready), Some(pre)) = (p.ready_group, p.preprocessed.as_ref()) else {
        return Err(NodeError::NotReady(
            "no signer group is complete yet".into(),
        ));
    };
    let complete: Vec<usize> = std::iter::once(usize::from(ready))
        .chain((0..pre.subsets.len()).filter(|g| {
            *g != usize::from(ready)
                && pre.subsets[*g].iter().all(|m| {
                    p.shares
                        .get(m)
                        .is_some_and(|gs| gs.iter().any(|s| usize::from(s.group) == *g))
                })
        }))
        .collect();
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
    .map_err(|e| NodeError::Verification(e.to_string()))?;
    let public_key_package = material.public_key_package()?;
    let aggregate = |group: usize| -> Result<Vec<(usize, [u8; 64])>, NodeError> {
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
        crate::session::aggregate_group(
            &ids,
            &pre.commitments[group],
            &shares,
            &verified,
            &public_key_package,
        )
        .map_err(proto)
    };
    let mut first_error = None;
    let mut signatures = None;
    for group in complete {
        match aggregate(group) {
            Ok(s) => {
                signatures = Some(s);
                break;
            }
            Err(e) => {
                tracing::warn!("signer group {group} did not aggregate: {e}");
                first_error.get_or_insert(e);
            }
        }
    }
    let Some(signatures) = signatures else {
        return Err(first_error.expect("at least the ready group was tried"));
    };

    let to_prove = pczt.clone();
    let proved = tokio::task::spawn_blocking(move || {
        Prover::new(to_prove)
            .create_ironwood_proof(crate::zrng::zrng(), proving_key())
            .map(|p| p.finish())
            .map_err(proto)
    })
    .await
    .map_err(|e| NodeError::Protocol(format!("proving task: {e}")))??;
    let signed = tx::apply_signatures(proved, &signatures).map_err(proto)?;
    let sent = broadcast(signed, lightwalletd).await?;
    let txid = sent.txid;
    let event = VaultEvent::SignedBroadcast {
        proposal,
        tx: sent.raw.clone(),
    };
    if let Err(e) = append_event(relay, me, material, &mut chain, &mut state, &event, rng).await {
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

#[derive(Clone, Debug)]
pub struct Sent {
    pub txid: [u8; 32],
    pub raw: Vec<u8>,
}

pub async fn broadcast(signed: Pczt, lightwalletd: &mut Client) -> Result<Sent, NodeError> {
    let transaction = TransactionExtractor::new(signed)
        .with_orchard(verifying_key())
        .extract(crate::zrng::zrng())
        .map_err(proto)?;
    let mut raw = Vec::new();
    transaction.write(&mut raw).map_err(proto)?;
    let txid = *transaction.txid().as_ref();
    if let Err(m) = send_raw(&raw, lightwalletd).await? {
        if m.to_ascii_lowercase().contains("consensus branch id") {
            return Err(NodeError::Wallet(crate::wallet::WalletError::AppOutdated(
                m,
            )));
        }
        let already = crate::wallet::mempool_txids(lightwalletd)
            .await
            .is_ok_and(|txids| txids.contains(&txid));
        if !already {
            return Err(NodeError::Protocol(format!("broadcast rejected: {m}")));
        }
    }
    Ok(Sent { txid, raw })
}

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

pub fn order_signers(
    available: Vec<Identifier>,
    mine: Identifier,
    silent: &BTreeSet<Identifier>,
) -> Vec<Identifier> {
    let mut ordered = available;
    ordered.sort_by_key(|id| (*id != mine, silent.contains(id), *id));
    ordered
}

pub struct SilentSigners(std::path::PathBuf);

impl SilentSigners {
    pub fn at(path: impl Into<std::path::PathBuf>) -> Self {
        Self(path.into())
    }

    pub fn load(&self) -> BTreeSet<Identifier> {
        std::fs::read(&self.0)
            .ok()
            .and_then(|b| version::decode::<BTreeSet<Vec<u8>>>(Format::SilentSigners, &b).ok())
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| Identifier::deserialize(id).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn update(&self, silent: &[Identifier], answered: &[Identifier]) {
        let mut set = self.load();
        set.extend(silent.iter().copied());
        for id in answered {
            set.remove(id);
        }
        let ids: BTreeSet<Vec<u8>> = set.iter().map(|id| id.serialize()).collect();
        let Ok(bytes) = version::encode(Format::SilentSigners, &ids) else {
            return;
        };
        if let Some(dir) = self.0.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = crate::durable::write(&self.0, &bytes);
    }
}

pub async fn unanswered_signers(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    request: &SigningRequest,
) -> Result<Vec<Identifier>, NodeError> {
    let (_, state) = load_state(relay, me, material).await?;
    let shares = read_shares(relay, me, material, &state, request).await?;
    let mine = *material.key_package()?.identifier();
    Ok(request
        .signers
        .iter()
        .filter(|id| **id != mine && !shares.contains_key(*id))
        .copied()
        .collect())
}

pub struct SentTxs(Option<std::path::PathBuf>);

impl SentTxs {
    pub fn in_dir(dir: impl Into<std::path::PathBuf>) -> Self {
        Self(Some(dir.into()))
    }

    pub fn none() -> Self {
        Self(None)
    }

    fn file(&self, txid: &[u8; 32]) -> Option<std::path::PathBuf> {
        self.0
            .as_ref()
            .map(|d| d.join(format!("{}.tx", hex::encode(txid))))
    }

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

pub async fn reject<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    proposal: ProposalId,
    store: &mut impl NonceStore,
    pool: &impl PoolStore,
    rng: &mut R,
) -> Result<(), NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let my_pk = me.public().sig_pk;
    let p = state
        .proposals
        .get(&proposal)
        .ok_or_else(|| NodeError::Protocol("unknown proposal".into()))?;
    let pczt_hash = p.pczt_hash;
    if p.shares.contains_key(&my_pk) || pool.kept_vote(&proposal, &pczt_hash).is_some() {
        return Err(NodeError::AlreadySigned);
    }
    let held = store.take(&proposal, &pczt_hash).is_some();
    let approved_here = p.possibly_signed && p.approvals.get(&my_pk).is_some_and(|c| !c.is_empty());
    let replied = store.kept_replies().iter().any(|bytes| {
        version::decode::<SharesMsg>(Format::SignatureShares, bytes)
            .is_ok_and(|m| m.proposal == proposal)
    });
    if (approved_here && !held) || store.claimed(&proposal, &pczt_hash) || replied {
        return Err(NodeError::AlreadySigned);
    }
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

pub struct SentRequest {
    pub request: SigningRequest,
    pub used_commitments: Vec<[u8; 32]>,
}

fn undeliverable(e: &RelayClientError) -> bool {
    match e {
        RelayClientError::StorageFull { detail } => detail == zafe_proto::relay::RECIPIENT_FULL,
        RelayClientError::Status { status: 403, .. } => true,
        _ => false,
    }
}

pub async fn request_signatures<P: Parameters, R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    network: &P,
    tip_height: u32,
    proposal: ProposalId,
    used: &BTreeSet<[u8; 32]>,
    silent: &SilentSigners,
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
    let mut unusable = Vec::new();
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
        if commitments.is_empty() {
            continue;
        }
        let parsed = commitments
            .iter()
            .map(|c| SigningCommitments::deserialize(c))
            .collect::<Result<Vec<_>, _>>();
        let added = parsed.is_ok_and(|commitments| {
            leader.add_approval(crate::session::Approval {
                member,
                proposal,
                pczt_hash: p.pczt_hash,
                commitments,
            })
        });
        if !added {
            unusable.push(member);
            continue;
        }
        hash_of.insert(member, hash);
    }
    if !unusable.is_empty() {
        silent.update(&unusable, &[]);
    }
    let threshold = usize::from(material.descriptor.threshold);
    let mine = *material.key_package()?.identifier();
    let available = leader.available_signers();
    let mut refused: BTreeSet<Identifier> = BTreeSet::new();
    let mut reached_before: Vec<Identifier> = Vec::new();
    let mut last_refusal = None;
    loop {
        let silent_now = silent.load();
        let chosen: Vec<Identifier> = order_signers(
            available
                .iter()
                .copied()
                .filter(|id| !refused.contains(id) && !reached_before.contains(id))
                .collect(),
            mine,
            &silent_now,
        )
        .into_iter()
        .take(threshold)
        .collect();
        if chosen.len() < threshold {
            if let Some(e) = last_refusal {
                return Err(NodeError::Relay(e));
            }
            return Err(NodeError::NotReady(format!(
                "{} of {threshold} signers are ready. Signers from the unfinished round need to \
                 approve again, or more signers need to approve",
                chosen.len()
            )));
        }
        let request = leader.request(&chosen).map_err(proto)?;
        let bytes = encode_request(&request)?;
        let mut order: Vec<Identifier> = request.signers.iter().copied().collect();
        order.sort_by_key(|id| !silent_now.contains(id));
        let mut seq = SeqCounter::default();
        let mut undelivered = None;
        let mut reached = Vec::new();
        for id in &order {
            let to = member_by_frost_id(&state.descriptor, id)?;
            if to.sig_pk == me.public().sig_pk {
                continue;
            }
            let env = Envelope::sealed(
                me,
                &to,
                material.label(),
                seq.next_seq(),
                Kind::SigningRequest,
                &bytes,
                rng,
            )
            .map_err(proto)?;
            match relay.send(me, &env).await {
                Ok(()) => reached.push(*id),
                Err(e) if undeliverable(&e) => {
                    eprintln!("signing request to a signer not delivered: {e}");
                    silent.update(&[*id], &[]);
                    undelivered = Some((*id, e));
                    break;
                }
                Err(e) => return Err(e.into()),
            }
        }
        match undelivered {
            Some((id, e)) => {
                let unsent: Vec<Identifier> = chosen
                    .iter()
                    .copied()
                    .filter(|c| !reached.contains(c))
                    .collect();
                leader.release(&unsent);
                refused.insert(id);
                reached_before.extend(reached);
                last_refusal = Some(e);
            }
            None => {
                let used_commitments = chosen
                    .iter()
                    .chain(&reached_before)
                    .map(|id| hash_of[id])
                    .collect();
                return Ok(SentRequest {
                    request,
                    used_commitments,
                });
            }
        }
    }
}

fn check_request_logged(state: &VaultState, request: &SigningRequest) -> Result<(), NodeError> {
    let unlogged =
        || NodeError::NotReady("the signing request isn't from approvals in the log".into());
    let p = state
        .proposals
        .get(&request.proposal)
        .filter(|p| p.pczt_hash == request.pczt_hash)
        .ok_or_else(unlogged)?;
    for package in &request.packages {
        if package
            .signing_commitments()
            .keys()
            .ne(request.signers.iter())
        {
            return Err(unlogged());
        }
    }
    for id in &request.signers {
        let member = member_by_frost_id(&state.descriptor, id)?;
        let logged = p
            .approvals
            .get(&member.sig_pk)
            .filter(|c| c.len() == request.packages.len())
            .ok_or_else(unlogged)?;
        for (package, c) in request.packages.iter().zip(logged) {
            let c = SigningCommitments::deserialize(c).map_err(|_| unlogged())?;
            if package.signing_commitment(id) != Some(c) {
                return Err(unlogged());
            }
        }
    }
    Ok(())
}

#[derive(Debug, Default)]
pub struct RespondReport {
    pub answered: Vec<ProposalId>,
    pub skipped: Vec<(ProposalId, String)>,
    pub acknowledged: u64,
}

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

    for (cursor, leader, env) in read_inbox(relay, me, material.label(), &members, 0).await? {
        match env.header.kind {
            Kind::Join
            | Kind::DkgRound1
            | Kind::DkgEcho
            | Kind::DkgRound2
            | Kind::SkContribution
            | Kind::DescriptorSignature
            | Kind::SafetyCommit
            | Kind::SafetyReveal
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
        let request_hash = hash_request_bytes(&bytes);
        if closed(&msg.proposal) {
            store.drop_reply(&request_hash);
            done.push(cursor);
            continue;
        }
        let kept = store.kept_reply(&request_hash);
        if kept.is_none() && !store.contains(&msg.proposal, &msg.pczt_hash) {
            done.push(cursor);
            continue;
        }
        let result: Result<Vec<u8>, NodeError> = kept.map(Ok).unwrap_or_else(|| {
            let (pczt, payments) = proposal_pczt(&state, &msg.proposal)?;
            let request = decode_request(&bytes)?;
            check_request_logged(&state, &request)?;
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
                request_hash,
                shares: shares.iter().map(|s| s.serialize()).collect(),
            };
            let reply = version::encode(Format::SignatureShares, &reply)?;
            let _ = store.keep_reply(&request_hash, &reply);
            Ok(reply)
        });
        match result {
            Ok(reply) => {
                send_fresh(relay, me, &mut seq, |n| {
                    Envelope::sealed(
                        me,
                        &leader,
                        material.label(),
                        n,
                        Kind::SignatureShares,
                        &reply,
                        rng,
                    )
                    .map_err(proto)
                })
                .await?;
                store.drop_reply(&request_hash);
                report.answered.push(msg.proposal);
                done.push(cursor);
            }
            Err(e) => report.skipped.push((msg.proposal, e.to_string())),
        }
    }
    if !done.is_empty() {
        report.acknowledged = relay
            .ack_inbox(me, material.label(), &done)
            .await
            .unwrap_or(0);
    }
    Ok(report)
}

pub fn proving_key() -> &'static orchard::circuit::ProvingKey {
    static PK: std::sync::OnceLock<orchard::circuit::ProvingKey> = std::sync::OnceLock::new();
    PK.get_or_init(|| {
        orchard::circuit::ProvingKey::build(orchard::circuit::OrchardCircuitVersion::PostNu6_3)
    })
}

pub fn verifying_key() -> &'static orchard::circuit::VerifyingKey {
    static VK: std::sync::OnceLock<orchard::circuit::VerifyingKey> = std::sync::OnceLock::new();
    VK.get_or_init(|| {
        orchard::circuit::VerifyingKey::build(orchard::circuit::OrchardCircuitVersion::PostNu6_3)
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShareProgress {
    pub received: usize,
    pub needed: usize,
}

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
    for (_, from, env) in read_inbox(relay, me, material.label(), &members, 0).await? {
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
            continue;
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
    check_request_logged(&state, request)?;
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
            .create_ironwood_proof(crate::zrng::zrng(), proving_key())
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
        .map_err(|e| match &e {
            crate::session::SessionError::Signing(crate::signing::SigningError::Frost(f))
                if !f.culprits().is_empty() =>
            {
                NodeError::InvalidShares(f.culprits())
            }
            _ => proto(e),
        })?;

    let proved = proving
        .await
        .map_err(|e| NodeError::Protocol(format!("proving task: {e}")))??;
    let signed = tx::apply_signatures(proved, &signatures).map_err(proto)?;
    let sent = broadcast(signed, lightwalletd).await?;
    let txid = sent.txid;
    let event = VaultEvent::SignedBroadcast {
        proposal: request.proposal,
        tx: sent.raw.clone(),
    };
    if let Err(e) = append_event(relay, me, material, &mut chain, &mut state, &event, rng).await {
        return Err(NodeError::Protocol(format!(
            "broadcast {} but could not log it: {e}",
            hex::encode(txid)
        )));
    }
    Ok(sent)
}

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

pub fn encode_own_shares(shares: &[Vec<u8>]) -> Result<Vec<u8>, NodeError> {
    Ok(version::encode(Format::OwnShares, shares)?)
}

pub fn decode_own_shares(bytes: &[u8]) -> Result<Vec<Vec<u8>>, NodeError> {
    Ok(version::decode(Format::OwnShares, bytes)?)
}

pub fn encode_used_commitments(used: &BTreeSet<[u8; 32]>) -> Result<Vec<u8>, NodeError> {
    Ok(version::encode(Format::UsedCommitments, used)?)
}

pub fn decode_used_commitments(bytes: &[u8]) -> Result<BTreeSet<[u8; 32]>, NodeError> {
    Ok(version::decode(Format::UsedCommitments, bytes)?)
}

#[cfg(test)]
mod tests {
    #[test]
    fn sequence_numbers_never_go_back_across_counters() {
        use super::SeqCounter;
        let mut last = 0;
        for _ in 0..1000 {
            let seq = SeqCounter::default().next_seq();
            assert!(seq > last);
            last = seq;
        }
    }

    #[test]
    fn the_leader_asks_itself_first_and_silent_signers_last() {
        use super::{order_signers, Identifier};
        let id = |n: u16| Identifier::try_from(n).unwrap();
        let all = vec![id(1), id(2), id(3), id(4)];
        assert_eq!(
            order_signers(all.clone(), id(3), &Default::default()),
            vec![id(3), id(1), id(2), id(4)]
        );
        let silent = [id(1)].into_iter().collect();
        assert_eq!(order_signers(all, id(3), &silent)[..2], [id(3), id(2)]);
        assert_eq!(
            order_signers(vec![id(1), id(2)], id(9), &silent),
            vec![id(2), id(1)]
        );
    }

    #[test]
    fn pools_deepen_for_vaults_with_many_notes() {
        use super::{deep_pool_target_for, pool_target_for, MAX_DEEP_POOL};
        let base = pool_target_for(3, 2);
        assert_eq!(deep_pool_target_for(3, 2, None), base);
        assert_eq!(deep_pool_target_for(3, 2, Some(3)), base);
        assert_eq!(deep_pool_target_for(3, 2, Some(100)), 200);
        assert_eq!(deep_pool_target_for(3, 2, Some(100_000)), MAX_DEEP_POOL);
    }

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
        let none = super::SentTxs::none();
        none.put(&tx);
        assert_eq!(none.get(&tx.txid), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
    use super::*;

    fn invite(name: &str) -> Invite {
        Invite {
            vault_id: [7; 16],
            label: [8; 16],
            relay_url: Some("https://testnet.relay.zafe.cash".into()),
            join_token: [9; 32],
            creator: [3; 32],
            threshold: 2,
            members: 3,
            name: name.into(),
        }
    }

    #[test]
    fn invite_text_is_url_safe_and_small() {
        let invite = invite("Grants committee — Ops fund 2026");
        let text = invite.encode();
        assert!(text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"-:._~".contains(&b)));
        assert!(text.len() < 400, "{} chars", text.len());
        assert_eq!(Invite::decode(&text).unwrap(), invite);
    }

    #[test]
    fn version_2_invites_still_decode() {
        #[derive(Serialize)]
        struct V2 {
            mailbox: MailboxId,
            join_token: [u8; 32],
            creator: [u8; 32],
            threshold: u16,
            members: u16,
            name: String,
        }
        let body = postcard::to_allocvec(&V2 {
            mailbox: [7; 16],
            join_token: [9; 32],
            creator: [3; 32],
            threshold: 2,
            members: 3,
            name: "Ops".into(),
        })
        .unwrap();
        let old = Invite::decode(&format!("zafe-invite-v2:{}", hex::encode(body))).unwrap();
        assert_eq!(
            old,
            Invite {
                label: [7; 16],
                relay_url: None,
                ..invite("Ops")
            }
        );
        assert_eq!(old.ids(), VaultIds::legacy([7; 16]));
    }

    #[test]
    fn invites_from_another_version_are_rejected_as_such() {
        let text = invite("Ops").encode();
        let tag = format!("zafe-invite-v{}:", version::INVITE);
        assert!(text.starts_with(&tag));
        let newer = text.replacen(&tag, &format!("zafe-invite-v{}:", version::INVITE + 1), 1);
        match Invite::decode(&newer) {
            Err(NodeError::UnsupportedVersion(v)) => {
                assert_eq!(
                    (v.format, v.found, v.supported),
                    (Format::Invite, version::INVITE + 1, version::INVITE)
                );
                assert!(v.is_newer());
            }
            other => panic!("{other:?}"),
        }
        let older = text.replacen(&tag, "zafe-invite-v1:", 1);
        match Invite::decode(&older) {
            Err(NodeError::UnsupportedVersion(v)) => assert!(!v.is_newer()),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            Invite::decode("zafe-invite-vx:00"),
            Err(NodeError::BadInvite)
        ));
    }

    #[test]
    fn log_key_messages_old_and_new() {
        let key = LogKey::from_bytes(0, [5; 32]);
        let bytes = encode_log_key_message(&key, Some("s3cret")).unwrap();
        let (back, token) = decode_log_key_message(&bytes).unwrap();
        assert_eq!((back.epoch, *back.as_bytes()), (0, [5; 32]));
        assert_eq!(token.as_deref(), Some("s3cret"));
        let bytes = encode_log_key_message(&key, None).unwrap();
        assert_eq!(decode_log_key_message(&bytes).unwrap().1, None);
        let mut legacy = 0u32.to_le_bytes().to_vec();
        legacy.extend_from_slice(&[5; 32]);
        let (back, token) = decode_log_key_message(&legacy).unwrap();
        assert_eq!((back.epoch, *back.as_bytes(), token), (0, [5; 32], None));
    }

    #[test]
    fn version_1_material_reads_as_legacy_relay() {
        let descriptor = crate::vault::VaultDescriptor {
            vault_id: [7; 16],
            version: version::DESCRIPTOR,
            name: "Grants".into(),
            network: "regtest".into(),
            threshold: 1,
            members: vec![],
            group_public_key: [1; 32],
            ufvk: "uview".into(),
            address: "u".into(),
            use_qsk: true,
            birthday_height: 2,
            proposal_expiry_blocks: 8064,
            epoch: 0,
            transcript_hash: [2; 32],
        };
        #[derive(Serialize)]
        struct V1 {
            descriptor: crate::vault::VaultDescriptor,
            key_package: Vec<u8>,
            public_key_package: Vec<u8>,
            vault_secret: [u8; 32],
            log_key_epoch: u32,
            log_key: [u8; 32],
        }
        let mut bytes = 1u16.to_le_bytes().to_vec();
        bytes.extend(
            postcard::to_allocvec(&V1 {
                descriptor,
                key_package: vec![3],
                public_key_package: vec![4],
                vault_secret: [5; 32],
                log_key_epoch: 0,
                log_key: [6; 32],
            })
            .unwrap(),
        );
        let m = VaultMaterial::from_bytes(&bytes).unwrap();
        assert_eq!(m.relay, VaultRelay::legacy([7; 16]));
        assert_eq!(m.ids(), VaultIds::legacy([7; 16]));
        assert_eq!(m.relay.url_or("https://r"), "https://r");
        let again = VaultMaterial::from_bytes(&m.to_bytes().unwrap()).unwrap();
        assert_eq!(again.relay, m.relay);
        assert_eq!(
            &m.to_bytes().unwrap()[..2],
            &version::VAULT_MATERIAL.to_le_bytes()
        );
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
