use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use rand_core::{CryptoRng, RngCore};
use reddsa::frost::redpallas::{
    keys::{
        repairable::{self, Delta, Sigma},
        PublicKeyPackage,
    },
    Identifier, PallasBlake2b512,
};
use serde::{Deserialize, Serialize};
use zafe_proto::{
    relay::{RecoveryPut, RecoveryValueKind, SeatCancel, SeatFinish, SeatMoveApproval, Signed},
    version::{self, Format},
    Chain, Envelope, Identity, IdentityPublic, Kind, LogKey, MailboxId,
};

use crate::{
    node::{
        announce_version, append_event, frost_id_of, load_state, members_by_pk, read_inbox,
        read_log_verified, save_copy, send_fresh, Invite, LogAnchor, NodeError, SeqCounter,
        VaultIds, VaultMaterial, VaultRelay,
    },
    nonce_store::NonceKey,
    relay_client::RelayClient,
    vault::{Replacement, VaultEvent, VaultState, REPAIR_QUORUM_FROM},
    wallet::ZafeNetwork,
};

fn proto(e: impl core::fmt::Debug) -> NodeError {
    NodeError::Protocol(format!("{e:?}"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryRequest {
    pub identity: IdentityPublic,
}

impl RecoveryRequest {
    const PREFIX: &'static str = "zafe-recover-v";

    pub fn encode(&self) -> String {
        format!(
            "{}{}:{}",
            Self::PREFIX,
            version::RECOVERY_REQUEST,
            hex::encode(postcard::to_allocvec(self).expect("encodable"))
        )
    }

    pub fn decode(s: &str) -> Result<Self, NodeError> {
        let bad = || NodeError::Protocol("not a recovery code".into());
        let rest = s.trim().strip_prefix(Self::PREFIX).ok_or_else(bad)?;
        let (found, body) = rest.split_once(':').ok_or_else(bad)?;
        let found: u16 = found.parse().map_err(|_| bad())?;
        version::check(Format::RecoveryRequest, found)?;
        postcard::from_bytes(&hex::decode(body).map_err(|_| bad())?).map_err(|_| bad())
    }
}

fn recovery_secret(me: &Identity, cosigner: &[u8; 32]) -> [u8; 32] {
    signed_value(me, b"Zafe seat recovery secret v1", &[cosigner])
}

fn recovery_challenge(me: &Identity, subject: &[u8; 32], commitment: &[u8; 32]) -> [u8; 32] {
    signed_value(
        me,
        b"Zafe seat recovery challenge v1",
        &[subject, commitment],
    )
}

fn signed_value(me: &Identity, label: &[u8], parts: &[&[u8; 32]]) -> [u8; 32] {
    let mut message = label.to_vec();
    for p in parts {
        message.extend_from_slice(*p);
    }
    let sig = me.sign(&message);
    blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"Zafe_RecovValue_")
        .hash(&sig)
        .as_bytes()
        .try_into()
        .expect("32 bytes")
}

fn value_of(
    values: &[Signed<RecoveryPut>],
    cosigner: &[u8; 32],
    kind: RecoveryValueKind,
) -> Option<[u8; 32]> {
    values
        .iter()
        .find(|v| v.payload.cosigner == *cosigner && v.payload.kind == kind)
        .map(|v| v.payload.value)
}

fn older_relay(e: crate::relay_client::RelayClientError) -> NodeError {
    match e {
        crate::relay_client::RelayClientError::Status { status: 404, .. } => NodeError::NotReady(
            "the relay doesn't support safe seat recovery yet; ask whoever runs it to update"
                .into(),
        ),
        e => e.into(),
    }
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct RecoveryPin {
    commitment: [u8; 32],
    challenge: [u8; 32],
}

impl RecoveryPin {
    fn name(me: &Identity, cosigner: &[u8; 32]) -> String {
        format!(
            "{}-{}.pin",
            hex::encode(me.public().sig_pk),
            hex::encode(cosigner)
        )
    }

    fn key(me: &Identity) -> NonceKey {
        NonceKey::derive(&me.seeds().to_bytes(), b"seat recovery pins")
    }

    fn load(dir: &Path, me: &Identity, cosigner: &[u8; 32]) -> Result<Option<Self>, NodeError> {
        let name = Self::name(me, cosigner);
        let bytes = match std::fs::read(dir.join(&name)) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(NodeError::Io(e.to_string())),
        };
        let plain = Self::key(me).open(&name, &bytes).ok_or_else(|| {
            NodeError::Protocol("this phone's seat recovery record can't be read".into())
        })?;
        Ok(Some(version::decode(Format::RecoveryPin, &plain)?))
    }

    fn create(&self, dir: &Path, me: &Identity, cosigner: &[u8; 32]) -> Result<Self, NodeError> {
        let name = Self::name(me, cosigner);
        let sealed = Self::key(me).seal(&name, &version::encode(Format::RecoveryPin, self)?);
        std::fs::create_dir_all(dir).map_err(|e| NodeError::Io(e.to_string()))?;
        crate::durable::create_new(&dir.join(&name), &sealed)
            .map_err(|e| NodeError::Io(e.to_string()))?;
        Self::load(dir, me, cosigner)?.ok_or_else(|| NodeError::Io("seat recovery record".into()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryCheck {
    pub cosigner: [u8; 32],
    pub words: String,
    pub update_needed: bool,
}

const PERSONAL_SIGMA_SEAL: &[u8] = b"Zafe repair sigma v1";

fn sigma_context(from: &[u8; 32], to: &[u8; 32], replacement: u64, attempt: u32) -> Vec<u8> {
    let mut context = PERSONAL_SIGMA_SEAL.to_vec();
    context.extend_from_slice(from);
    context.extend_from_slice(to);
    context.extend_from_slice(&replacement.to_le_bytes());
    context.extend_from_slice(&attempt.to_le_bytes());
    context
}

async fn ensure_pq_key(
    relay: &RelayClient,
    me: &Identity,
    subject: [u8; 32],
    keys: &BTreeMap<[u8; 32], crate::pq::PqPublic>,
) -> Result<(), NodeError> {
    let mine = crate::pq::PqKeys::derive(me.seeds());
    if keys.get(&me.public().sig_pk) != Some(mine.public()) {
        relay
            .pq_key_put(me, subject, mine.public())
            .await
            .map_err(older_relay)?;
    }
    Ok(())
}

pub async fn recovery_checks(
    relay: &RelayClient,
    me: &Identity,
    dir: &Path,
) -> Result<Vec<RecoveryCheck>, NodeError> {
    use RecoveryValueKind::*;
    let subject = me.public().sig_pk;
    let (values, keys) = tokio::join!(
        relay.recovery_read(me, subject),
        relay.pq_keys_read(me, subject)
    );
    let values = values.map_err(older_relay)?;
    let keys = keys.map_err(older_relay)?;
    ensure_pq_key(relay, me, subject, &keys).await?;
    let mut checks = Vec::new();
    for cosigner in values
        .iter()
        .filter(|v| v.payload.kind == Announce)
        .map(|v| v.payload.cosigner)
    {
        if !keys.contains_key(&cosigner) && value_of(&values, &cosigner, Commit).is_none() {
            checks.push(RecoveryCheck {
                cosigner,
                words: String::new(),
                update_needed: true,
            });
            continue;
        }
        let secret = recovery_secret(me, &cosigner);
        let commitment = crate::sas::recovery_commitment(me.public(), &secret);
        let has = |kind| value_of(&values, &cosigner, kind);
        if has(Commit).is_none() {
            relay
                .recovery_put(me, subject, cosigner, Commit, commitment)
                .await
                .map_err(older_relay)?;
            continue;
        }
        let pin = match (RecoveryPin::load(dir, me, &cosigner)?, has(Challenge)) {
            (Some(pin), served) if served.is_none_or(|c| c == pin.challenge) => pin,
            (Some(_), _) | (None, None) => continue,
            (None, Some(challenge)) => {
                let pin = RecoveryPin {
                    commitment,
                    challenge,
                };
                if pin.create(dir, me, &cosigner)? != pin {
                    continue;
                }
                pin
            }
        };
        if has(Reveal).is_none() {
            relay
                .recovery_put(me, subject, cosigner, Reveal, secret)
                .await
                .map_err(older_relay)?;
        }
        checks.push(RecoveryCheck {
            cosigner,
            words: crate::sas::recovery_words(me.public(), &cosigner, &secret, &pin.challenge),
            update_needed: false,
        });
    }
    Ok(checks)
}

pub async fn new_phone_words(
    relay: &RelayClient,
    me: &Identity,
    new: &IdentityPublic,
) -> Result<Option<String>, NodeError> {
    use RecoveryValueKind::*;
    let subject = new.sig_pk;
    let my_pk = me.public().sig_pk;
    if subject == my_pk {
        return Err(NodeError::Protocol(
            "that recovery code is this phone's own".into(),
        ));
    }
    let values = relay
        .recovery_read(me, subject)
        .await
        .map_err(older_relay)?;
    let mine = |kind| value_of(&values, &my_pk, kind);
    let keys = relay.pq_keys_read(me, subject).await.map_err(older_relay)?;
    ensure_pq_key(relay, me, subject, &keys).await?;
    if mine(Announce).is_none() {
        relay
            .recovery_put(me, subject, my_pk, Announce, [0; 32])
            .await
            .map_err(older_relay)?;
        return Ok(None);
    }
    let Some(commitment) = mine(Commit) else {
        return Ok(None);
    };
    let challenge = recovery_challenge(me, &subject, &commitment);
    match mine(Challenge) {
        None => {
            relay
                .recovery_put(me, subject, my_pk, Challenge, challenge)
                .await
                .map_err(older_relay)?;
            return Ok(None);
        }
        Some(posted) if posted != challenge => {
            return Err(NodeError::Verification(
                "the new phone's safety value changed after it was challenged: don't approve; \
                 ask them to start over"
                    .into(),
            ))
        }
        Some(_) => {}
    }
    let Some(revealed) = mine(Reveal) else {
        return Ok(None);
    };
    if crate::sas::recovery_commitment(new, &revealed) != commitment {
        return Err(NodeError::Verification(
            "the new phone's safety value doesn't match what it committed to: don't approve".into(),
        ));
    }
    Ok(Some(crate::sas::recovery_words(
        new, &my_pk, &revealed, &challenge,
    )))
}

pub async fn approve_replacement<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    old: [u8; 32],
    request: &RecoveryRequest,
    confirmed_words: &str,
    rng: &mut R,
) -> Result<bool, NodeError> {
    let (words, loaded) = tokio::join!(
        new_phone_words(relay, me, &request.identity),
        load_state(relay, me, material)
    );
    let words = words?
        .ok_or_else(|| NodeError::NotReady("the new phone hasn't shown its words yet".into()))?;
    if crate::sas::normalize(confirmed_words) != words {
        return Err(NodeError::Verification(
            "those aren't the words this phone shows for the new phone: don't approve".into(),
        ));
    }
    let (mut chain, mut state) = loaded?;
    let approval = SeatMoveApproval {
        vault_id: state.descriptor.vault_id,
        old,
        new: request.identity,
        round: state.seat_round(&old),
    };
    let event = VaultEvent::ReplaceApproval {
        old,
        new: request.identity,
        signature: approval.sign(me).map_err(proto)?,
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    let pending = state
        .pending_seat_move(&old)
        .is_some_and(|r| r.new == request.identity);
    if pending {
        let _ = sync_seat_moves(relay, me, material, &mut chain, &mut state, rng).await;
    }
    Ok(pending)
}

pub async fn sync_seat_moves<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    chain: &mut Chain,
    state: &mut VaultState,
    rng: &mut R,
) -> Result<(), NodeError> {
    let my_pk = me.public().sig_pk;
    let vault_id = state.descriptor.vault_id;
    let label = material.label();
    let live = |state: &VaultState| -> Vec<(Replacement, [u8; 32])> {
        state
            .replacements
            .iter()
            .filter(|r| !r.done)
            .filter_map(|r| Some((r.clone(), r.move_id(&vault_id)?)))
            .collect()
    };
    if state.descriptor.member(&my_pk).is_none()
        || (live(state).is_empty() && state.seat_keeps.is_empty())
    {
        return Ok(());
    }
    let mut info = relay.mailbox_info(me, label).await?;
    let mut first_error: Option<NodeError> = None;
    let mut note = |e: NodeError| {
        first_error.get_or_insert(e);
    };
    for c in &info.cancelled {
        let Some(index) = state
            .replacements
            .iter()
            .find(|r| r.is_pending() && r.move_id(&vault_id) == Some(c.cancel.move_id))
            .map(|r| r.index)
        else {
            continue;
        };
        let event = VaultEvent::KeepSeat {
            replacement: index,
            at: c.cancel.at,
            mailbox: c.cancel.mailbox,
            signature: c.signature.clone(),
        };
        if state.check(my_pk, &event).is_err() {
            continue;
        }
        if let Err(e) = append_event(relay, me, material, chain, state, &event, rng).await {
            note(e);
        }
    }
    let keeps: Vec<(SeatCancel, Vec<u8>)> = state
        .seat_keeps
        .values()
        .flatten()
        .map(|k| (k.cancel, k.signature.clone()))
        .collect();
    for (cancel, signature) in keeps {
        if cancel.mailbox == label && info.pending.iter().any(|p| p.move_id == cancel.move_id) {
            if let Err(e) = relay.cancel_seat_move(cancel, signature).await {
                note(e.into());
            }
        }
    }
    for (r, move_id) in live(state) {
        if !info.members.iter().any(|m| m.sig_pk == r.old) {
            continue;
        }
        let has_row = info.pending.iter().any(|p| p.move_id == move_id)
            || info.cancelled.iter().any(|c| c.cancel.move_id == move_id);
        let sent = if r.is_pending() {
            if r.old == my_pk || has_row {
                continue;
            }
            let approval = SeatMoveApproval {
                vault_id,
                old: r.old,
                new: r.new,
                round: r.round.unwrap_or_default(),
            };
            relay
                .replace_member_at(me, label, approval, r.approvals.clone())
                .await
        } else if r.switched && r.new.sig_pk != my_pk {
            if !has_row {
                let approval = SeatMoveApproval {
                    vault_id,
                    old: r.old,
                    new: r.new,
                    round: r.round.unwrap_or_default(),
                };
                if let Err(e) = relay
                    .replace_member_at(me, label, approval, r.approvals.clone())
                    .await
                {
                    note(e.into());
                    continue;
                }
            }
            let finish = SeatFinish {
                mailbox: label,
                vault_id,
                move_id,
                old: r.old,
                new: r.new.sig_pk,
            };
            let finished = relay.finish_seat_move(me, finish).await;
            if finished.is_ok() {
                if let Ok(now) = relay.mailbox_info(me, label).await {
                    info = now;
                }
            }
            finished
        } else {
            continue;
        };
        if let Err(e) = sent {
            note(e.into());
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

pub const SEAT_MOVE_WAIT: std::time::Duration = std::time::Duration::from_secs(72 * 3600);

#[derive(Serialize, Deserialize)]
struct SeatMoveSeen {
    at: u64,
}

fn seen_name(move_id: &[u8; 32]) -> String {
    format!("{}.seen", hex::encode(move_id))
}

fn seen_key(me: &Identity, vault_id: &MailboxId) -> NonceKey {
    NonceKey::derive(&me.seeds().to_bytes(), vault_id)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub fn seat_move_seen(
    dir: &Path,
    me: &Identity,
    vault_id: &MailboxId,
    move_id: &[u8; 32],
) -> Option<u64> {
    let name = seen_name(move_id);
    let bytes = std::fs::read(dir.join(&name)).ok()?;
    let plain = seen_key(me, vault_id).open(&name, &bytes)?;
    let seen: SeatMoveSeen = version::decode(Format::SeatMoveSeen, &plain).ok()?;
    Some(seen.at)
}

fn record_seen(
    dir: &Path,
    me: &Identity,
    vault_id: &MailboxId,
    move_id: &[u8; 32],
    now: u64,
) -> Result<u64, NodeError> {
    if let Some(at) = seat_move_seen(dir, me, vault_id, move_id) {
        return Ok(at);
    }
    let path = dir.join(seen_name(move_id));
    if path.exists() {
        crate::durable::remove(&path).map_err(|e| NodeError::Io(e.to_string()))?;
    }
    let sealed = seen_key(me, vault_id).seal(
        &seen_name(move_id),
        &version::encode(Format::SeatMoveSeen, &SeatMoveSeen { at: now })?,
    );
    std::fs::create_dir_all(dir).map_err(|e| NodeError::Io(e.to_string()))?;
    crate::durable::create_new(&path, &sealed).map_err(|e| NodeError::Io(e.to_string()))?;
    seat_move_seen(dir, me, vault_id, move_id)
        .ok_or_else(|| NodeError::Io("seat move record".into()))
}

pub async fn keep_seat<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    rng: &mut R,
) -> Result<bool, NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let my_pk = me.public().sig_pk;
    let vault_id = state.descriptor.vault_id;
    let voided =
        match state.pending_seat_move(&my_pk) {
            Some(r) if r.round == Some(0) => {
                let cancel = SeatCancel {
                    mailbox: material.label(),
                    vault_id,
                    move_id: r.move_id(&vault_id).ok_or_else(|| proto("move id"))?,
                    old: my_pk,
                    new: r.new.sig_pk,
                    at: unix_now(),
                };
                let event = VaultEvent::KeepSeat {
                    replacement: r.index,
                    at: cancel.at,
                    mailbox: cancel.mailbox,
                    signature: cancel.sign(me).map_err(proto)?,
                };
                append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
                true
            }
            Some(_) => return Err(NodeError::NotReady(
                "your co-signers asked again after you kept this seat; it can't be kept twice: \
                 call them"
                    .into(),
            )),
            None => false,
        };
    let _ = sync_seat_moves(relay, me, material, &mut chain, &mut state, rng).await;
    Ok(voided)
}

#[derive(Serialize, Deserialize)]
struct DeltaMsg {
    replacement: u64,
    attempt: u32,
    delta: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct VaultData {
    public_key_package: Vec<u8>,
    vault_secret: [u8; 32],
    log_key_epoch: u32,
    log_key: [u8; 32],
}

#[derive(Serialize, Deserialize)]
struct SigmaMsg {
    replacement: u64,
    attempt: u32,
    sigma: Vec<u8>,
    data: VaultData,
    anchor: Option<LogAnchor>,
    create_token: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SealedSigmaMsg {
    replacement: u64,
    attempt: u32,
    sealed: Vec<u8>,
}

#[derive(Deserialize)]
struct SigmaMsgV2 {
    replacement: u64,
    attempt: u32,
    sigma: Vec<u8>,
    data: VaultData,
}

fn decode_sigma(
    bytes: &[u8],
    me: &Identity,
    from: &[u8; 32],
) -> Result<SigmaMsg, version::DecodeError> {
    let malformed = || version::DecodeError::Malformed(Format::Repair);
    match version::split(Format::Repair, bytes)? {
        (3, body) => postcard::from_bytes(body).map_err(|_| malformed()),
        (2, body) => {
            let v2: SigmaMsgV2 = postcard::from_bytes(body)
                .map_err(|_| version::DecodeError::Malformed(Format::Repair))?;
            Ok(SigmaMsg {
                replacement: v2.replacement,
                attempt: v2.attempt,
                sigma: v2.sigma,
                data: v2.data,
                anchor: None,
                create_token: None,
            })
        }
        _ => {
            let outer: SealedSigmaMsg = version::decode(Format::Repair, bytes)?;
            let context =
                sigma_context(from, &me.public().sig_pk, outer.replacement, outer.attempt);
            let inner = crate::pq::PqKeys::derive(me.seeds())
                .open(&context, &outer.sealed)
                .map_err(|_| malformed())?;
            let msg: SigmaMsg = postcard::from_bytes(&inner).map_err(|_| malformed())?;
            if (msg.replacement, msg.attempt) != (outer.replacement, outer.attempt) {
                return Err(malformed());
            }
            Ok(msg)
        }
    }
}

fn decode_repair<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> Result<T, version::DecodeError> {
    match version::split(Format::Repair, bytes)? {
        (2 | 3, body) => {
            postcard::from_bytes(body).map_err(|_| version::DecodeError::Malformed(Format::Repair))
        }
        _ => version::decode(Format::Repair, bytes),
    }
}

#[derive(Serialize, Deserialize)]
struct HelperState {
    deltas: BTreeMap<[u8; 32], Vec<u8>>,
    sent: bool,
}

type DeltaKey = (u64, u32, [u8; 32]);

#[derive(Debug, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub deltas_sent: usize,
    pub held: usize,
    pub helped: usize,
    pub sigmas_sent: usize,
    pub waiting: usize,
}

const DELTA_EXT: &str = "delta";
const DONE_EXT: &str = "done";

fn helper_path(dir: &Path, r: &Replacement, ext: &str) -> PathBuf {
    dir.join(format!("{}-{}.{ext}", r.index, r.attempt))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), NodeError> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(proto)?;
    std::fs::rename(&tmp, path).map_err(proto)
}

fn repair_ids(
    state: &VaultState,
    r: &Replacement,
) -> Result<(Vec<Identifier>, Identifier), NodeError> {
    let helpers = r
        .helpers()
        .iter()
        .map(|pk| frost_id_of(&state.descriptor, pk))
        .collect::<Result<Vec<_>, _>>()?;
    let participant = Identifier::deserialize(&r.frost_id).map_err(proto)?;
    Ok((helpers, participant))
}

pub async fn help_repairs<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    chain: &Chain,
    state: &VaultState,
    dir: &Path,
    wait: std::time::Duration,
    rng: &mut R,
) -> Result<RepairReport, NodeError> {
    let my_pk = me.public().sig_pk;
    let vault_id = state.descriptor.vault_id;
    let now = unix_now();
    let mut report = RepairReport::default();
    let (mut chain, mut state) = (chain.clone(), state.clone());
    let mailbox = material.label();
    let mut first_error = sync_seat_moves(relay, me, material, &mut chain, &mut state, rng)
        .await
        .err();
    for r in state.replacements.iter().filter(|r| r.is_pending()) {
        if let Some(move_id) = r.move_id(&vault_id) {
            if let Err(e) = record_seen(dir, me, &vault_id, &move_id, now) {
                first_error.get_or_insert(e);
            }
        }
    }
    let mine: Vec<u64> = state
        .replacements
        .iter()
        .filter(|r| !r.done && r.helpers().contains(&my_pk) && !r.stopped.contains(&my_pk))
        .filter(|r| !helper_path(dir, r, DONE_EXT).exists())
        .map(|r| r.index)
        .collect();
    if mine.is_empty() {
        return first_error.map_or(Ok(report), Err);
    }
    std::fs::create_dir_all(dir).map_err(proto)?;
    let key_package = material.key_package()?;
    let mut seq = SeqCounter::default();

    let mut received: BTreeMap<DeltaKey, (u64, Vec<u8>)> = BTreeMap::new();
    for (cursor, from, env) in read_inbox(relay, me, mailbox, &members_by_pk(&state), 0).await? {
        if env.header.kind != Kind::RepairDelta {
            continue;
        }
        let Ok(bytes) = env.open(me, &from) else {
            continue;
        };
        let Ok(msg) = decode_repair::<DeltaMsg>(&bytes) else {
            continue;
        };
        received
            .entry((msg.replacement, msg.attempt, from.sig_pk))
            .or_insert((cursor, msg.delta));
    }

    let mut switched = false;
    for index in mine {
        let step = Box::pin(help_one(
            relay,
            me,
            material,
            &mut chain,
            &mut state,
            index,
            &received,
            &key_package,
            dir,
            HelperClock { now, wait },
            &mut seq,
            &mut report,
            rng,
        ))
        .await;
        match step {
            Ok(s) => switched |= s,
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    if switched {
        if let Err(e) = sync_seat_moves(relay, me, material, &mut chain, &mut state, rng).await {
            first_error.get_or_insert(e);
        }
    }
    match first_error {
        Some(e) => Err(e),
        None => Ok(report),
    }
}

#[derive(Clone, Copy)]
struct HelperClock {
    now: u64,
    wait: std::time::Duration,
}

async fn help_one<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    chain: &mut Chain,
    state: &mut VaultState,
    index: u64,
    received: &BTreeMap<DeltaKey, (u64, Vec<u8>)>,
    key_package: &reddsa::frost::redpallas::keys::KeyPackage,
    dir: &Path,
    clock: HelperClock,
    seq: &mut SeqCounter,
    report: &mut RepairReport,
    rng: &mut R,
) -> Result<bool, NodeError> {
    let my_pk = me.public().sig_pk;
    let vault_id = state.descriptor.vault_id;
    let mailbox = material.label();
    let current = |state: &VaultState| {
        state
            .replacements
            .iter()
            .find(|r| {
                r.index == index
                    && !r.done
                    && r.helpers.contains(&my_pk)
                    && !r.stopped.contains(&my_pk)
            })
            .cloned()
    };
    let Some(mut r) = current(state) else {
        return Ok(false);
    };
    let (helper_ids, participant) = repair_ids(state, &r)?;
    let helpers = r.helpers();
    let state_path = helper_path(dir, &r, DELTA_EXT);
    let mut hs: HelperState = match std::fs::read(&state_path) {
        Ok(bytes) => decode_repair(&bytes)?,
        Err(_) => {
            let deltas = repairable::repair_share_part1::<PallasBlake2b512, _>(
                &helper_ids,
                key_package,
                rng,
                participant,
            )
            .map_err(proto)?;
            let deltas = helpers
                .iter()
                .zip(&helper_ids)
                .map(|(pk, id)| {
                    Ok((
                        *pk,
                        deltas.get(id).ok_or_else(|| proto("delta"))?.serialize(),
                    ))
                })
                .collect::<Result<_, NodeError>>()?;
            let hs = HelperState {
                deltas,
                sent: false,
            };
            write_atomic(&state_path, &version::encode(Format::Repair, &hs)?)?;
            hs
        }
    };
    let delta_for = |pk: &[u8; 32]| {
        hs.deltas.get(pk).cloned().ok_or_else(|| {
            NodeError::Protocol("this repair's saved part is for other helpers".into())
        })
    };
    if !hs.sent {
        let members = members_by_pk(state);
        for pk in helpers.iter().filter(|pk| **pk != my_pk) {
            let to = members
                .get(pk)
                .ok_or_else(|| NodeError::Protocol("a helper is no longer a member".into()))?;
            let msg = DeltaMsg {
                replacement: r.index,
                attempt: r.attempt,
                delta: delta_for(pk)?,
            };
            let payload = version::encode(Format::Repair, &msg)?;
            send_fresh(relay, me, seq, |n| {
                Envelope::sealed(me, to, mailbox, n, Kind::RepairDelta, &payload, rng)
                    .map_err(proto)
            })
            .await?;
        }
        hs.sent = true;
        write_atomic(&state_path, &version::encode(Format::Repair, &hs)?)?;
        report.deltas_sent += 1;
    }

    let mut switched = false;
    if r.is_pending() {
        let move_id = r.move_id(&vault_id).ok_or_else(|| proto("move id"))?;
        let seen = record_seen(dir, me, &vault_id, &move_id, clock.now)?;
        if clock.now < seen.saturating_add(clock.wait.as_secs()) {
            report.held += 1;
            return Ok(false);
        }
        if !r.helped.contains(&my_pk) {
            let event = VaultEvent::RepairHelped {
                replacement: r.index,
                attempt: r.attempt,
            };
            let at = append_event(relay, me, material, chain, state, &event, rng).await?;
            let counted = !state.ignored.iter().any(|(i, _)| *i == at);
            r = current(state)
                .filter(|now| counted && now.attempt == r.attempt && now.helped.contains(&my_pk))
                .ok_or_else(|| {
                    NodeError::Protocol("this helper's part didn't count in the log".into())
                })?;
            switched = r.switched;
            report.helped += 1;
        }
        if !r.switched {
            report.waiting += 1;
            return Ok(switched);
        }
    }

    let mut deltas = vec![Delta::deserialize(&delta_for(&my_pk)?).map_err(proto)?];
    let mut cursors = Vec::new();
    for pk in helpers.iter().filter(|pk| **pk != my_pk) {
        match received.get(&(r.index, r.attempt, *pk)) {
            Some((cursor, bytes)) => {
                deltas.push(Delta::deserialize(bytes).map_err(proto)?);
                cursors.push(*cursor);
            }
            None => break,
        }
    }
    if deltas.len() < helpers.len() {
        report.waiting += 1;
        return Ok(switched);
    }
    let Some(new_key) = relay
        .pq_keys_read(me, r.new.sig_pk)
        .await
        .map_err(older_relay)?
        .remove(&r.new.sig_pk)
    else {
        report.waiting += 1;
        return Ok(switched);
    };
    let sigma = repairable::repair_share_part2(&deltas);
    let msg = SigmaMsg {
        replacement: r.index,
        attempt: r.attempt,
        sigma: sigma.serialize(),
        data: VaultData {
            public_key_package: material.public_key_package.clone(),
            vault_secret: material.vault_secret,
            log_key_epoch: material.log_key_epoch,
            log_key: material.log_key,
        },
        anchor: Some(LogAnchor::of(chain)),
        create_token: material.relay.create_token.clone(),
    };
    let inner = postcard::to_allocvec(&msg).map_err(proto)?;
    let sealed = SealedSigmaMsg {
        replacement: r.index,
        attempt: r.attempt,
        sealed: new_key.seal(
            &sigma_context(&my_pk, &r.new.sig_pk, r.index, r.attempt),
            &inner,
            rng,
        ),
    };
    let payload = version::encode(Format::Repair, &sealed)?;
    send_fresh(relay, me, seq, |n| {
        Envelope::sealed(me, &r.new, mailbox, n, Kind::RepairSigma, &payload, rng).map_err(proto)
    })
    .await?;
    write_atomic(&helper_path(dir, &r, DONE_EXT), b"")?;
    let _ = std::fs::remove_file(&state_path);
    let _ = relay.ack_inbox(me, mailbox, &cursors).await;
    report.sigmas_sent += 1;
    Ok(switched)
}

#[derive(Debug)]
pub enum RecoveryStatus {
    Waiting,
    Pending,
    SeatMoved {
        received: usize,
        needed: usize,
    },
    Done {
        material: Box<VaultMaterial>,
        invite: Invite,
    },
}

pub async fn try_recover(
    relay: &RelayClient,
    me: &Identity,
    label: Option<MailboxId>,
    confirmed: &[[u8; 32]],
) -> Result<RecoveryStatus, NodeError> {
    let mut status = RecoveryStatus::Waiting;
    for mailbox in relay.mailboxes(me).await? {
        if label.is_some_and(|l| l != mailbox) {
            continue;
        }
        match recover_from(relay, me, mailbox, confirmed).await? {
            done @ RecoveryStatus::Done { .. } => return Ok(done),
            s @ RecoveryStatus::SeatMoved { .. } => status = s,
            RecoveryStatus::Pending if matches!(status, RecoveryStatus::Waiting) => {
                status = RecoveryStatus::Pending
            }
            RecoveryStatus::Pending | RecoveryStatus::Waiting => {}
        }
    }
    Ok(status)
}

async fn recover_from(
    relay: &RelayClient,
    me: &Identity,
    label: MailboxId,
    confirmed: &[[u8; 32]],
) -> Result<RecoveryStatus, NodeError> {
    let on_relay: BTreeMap<[u8; 32], IdentityPublic> = relay
        .members(me, label)
        .await?
        .members
        .into_iter()
        .map(|m| (m.sig_pk, m))
        .collect();
    let mut messages: BTreeMap<([u8; 32], u64, u32), (u64, SigmaMsg)> = BTreeMap::new();
    for (cursor, from, env) in read_inbox(relay, me, label, &on_relay, 0).await? {
        if env.header.kind != Kind::RepairSigma {
            continue;
        }
        let Ok(bytes) = env.open(me, &from) else {
            continue;
        };
        if let Ok(msg) = decode_sigma(&bytes, me, &from.sig_pk) {
            messages
                .entry((from.sig_pk, msg.replacement, msg.attempt))
                .or_insert((cursor, msg));
        }
    }
    let Some((voucher, anchor, vouched)) =
        messages.iter().rev().find_map(|((from, _, _), (_, m))| {
            let anchor = m.anchor.filter(|_| confirmed.contains(from))?;
            Some((*from, anchor, m))
        })
    else {
        return Ok(if on_relay.contains_key(&me.public().sig_pk) {
            RecoveryStatus::SeatMoved {
                received: 0,
                needed: 0,
            }
        } else {
            RecoveryStatus::Pending
        });
    };
    let key = LogKey::from_bytes(vouched.data.log_key_epoch, vouched.data.log_key);
    let ids = VaultIds {
        vault: anchor.vault_id,
        label,
    };
    let (chain, state, _) = read_log_verified(relay, me, ids, &key).await?;
    for (_, m) in messages.values() {
        if let Some(anchor) = &m.anchor {
            anchor.check(&chain)?;
        }
    }
    let r = state.replacement_to(&me.public().sig_pk).ok_or_else(|| {
        NodeError::Protocol("the vault log has no seat move to this device".into())
    })?;
    if state.descriptor.member(&voucher).is_none()
        || !r.approvals.iter().any(|(pk, _)| *pk == voucher)
    {
        return Err(NodeError::Verification(
            "the co-signer you compared words with didn't move your seat in this vault".into(),
        ));
    }
    let helpers = r.helpers();
    let from_helpers: Vec<&SigmaMsg> = helpers
        .iter()
        .filter_map(|pk| messages.get(&(*pk, r.index, r.attempt)).map(|(_, m)| m))
        .filter(|m| m.replacement == r.index && m.attempt == r.attempt)
        .collect();
    if from_helpers.len() < helpers.len() {
        return Ok(RecoveryStatus::SeatMoved {
            received: from_helpers.len(),
            needed: helpers.len(),
        });
    }
    let failed = |why: &str| {
        NodeError::Verification(format!(
            "the repaired key doesn't check out ({why}); ask your co-signers to approve the move again"
        ))
    };
    let data = &vouched.data;
    if from_helpers
        .iter()
        .any(|m| &m.data != data || m.create_token != vouched.create_token)
    {
        return Err(failed("helpers sent different vault data"));
    }
    let pkp = PublicKeyPackage::deserialize(&data.public_key_package).map_err(proto)?;
    let group_key = pkp.verifying_key().serialize().map_err(proto)?;
    if group_key[..] != state.descriptor.group_public_key[..] {
        return Err(failed("wrong group key"));
    }
    let id = Identifier::deserialize(&r.frost_id).map_err(proto)?;
    let sigmas = from_helpers
        .iter()
        .map(|m| Sigma::deserialize(&m.sigma).map_err(proto))
        .collect::<Result<Vec<_>, _>>()?;
    let key_package = repairable::repair_share_part3(&sigmas, id, &pkp).map_err(proto)?;
    if pkp.verifying_shares().get(&id) != Some(key_package.verifying_share()) {
        return Err(failed("the share doesn't match this member's public share"));
    }
    let material = VaultMaterial {
        descriptor: state.descriptor.clone(),
        key_package: key_package.serialize().map_err(proto)?,
        public_key_package: data.public_key_package.clone(),
        vault_secret: data.vault_secret,
        log_key_epoch: data.log_key_epoch,
        log_key: data.log_key,
        relay: VaultRelay {
            url: Some(relay.base_url().to_owned()),
            label,
            create_token: vouched.create_token.clone(),
        },
    };
    let network = ZafeNetwork::from_name(&state.descriptor.network)
        .ok_or_else(|| proto("unknown network"))?;
    let ufvk = material
        .vault_keys()?
        .ufvk()
        .map_err(proto)?
        .encode(&network)
        .map_err(proto)?;
    if ufvk != state.descriptor.ufvk {
        return Err(failed(
            "the vault secret doesn't give the vault's viewing key",
        ));
    }
    save_copy(relay, &chain);
    let creator = chain
        .entries()
        .first()
        .map(|e| e.header.author)
        .unwrap_or_default();
    let invite = Invite {
        vault_id: ids.vault,
        label,
        relay_url: Some(relay.base_url().to_owned()),
        join_token: [0; 32],
        creator,
        threshold: state.descriptor.threshold,
        members: state.descriptor.members.len() as u16,
        name: state.descriptor.name.clone(),
    };
    let cursors: Vec<u64> = helpers
        .iter()
        .filter_map(|pk| messages.get(&(*pk, r.index, r.attempt)).map(|(c, _)| *c))
        .collect();
    let _ = relay.ack_inbox(me, label, &cursors).await;
    Ok(RecoveryStatus::Done {
        material: Box::new(material),
        invite,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatLink {
    pub relay_url: String,
    pub label: MailboxId,
}

impl SeatLink {
    const PREFIX: &'static str = "zafe-seat-v";

    pub fn for_vault(material: &VaultMaterial, default_relay: &str) -> Self {
        Self {
            relay_url: material.relay.url_or(default_relay).to_owned(),
            label: material.label(),
        }
    }

    pub fn encode(&self) -> String {
        format!(
            "{}{}:{}",
            Self::PREFIX,
            version::SEAT_LINK,
            hex::encode(postcard::to_allocvec(self).expect("encodable"))
        )
    }

    pub fn decode(text: &str) -> Result<Self, NodeError> {
        let bad = || NodeError::Protocol("not a seat recovery link".into());
        let code = text
            .split(|c: char| c.is_whitespace() || c == '=' || c == '&')
            .find(|w| w.starts_with(Self::PREFIX))
            .unwrap_or(text);
        let rest = code.trim().strip_prefix(Self::PREFIX).ok_or_else(bad)?;
        let (found, body) = rest.split_once(':').ok_or_else(bad)?;
        let found: u16 = found.parse().map_err(|_| bad())?;
        version::check(Format::SeatLink, found)?;
        let link: Self =
            postcard::from_bytes(&hex::decode(body).map_err(|_| bad())?).map_err(|_| bad())?;
        if !(link.relay_url.starts_with("https://") || link.relay_url.starts_with("http://")) {
            return Err(bad());
        }
        Ok(link)
    }
}

pub async fn retry_repair<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    replacement: u64,
    stalled: [u8; 32],
    taker: [u8; 32],
    rng: &mut R,
) -> Result<(), NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let my_pk = me.public().sig_pk;
    let _ = Box::pin(announce_version(
        relay, me, material, &mut chain, &mut state, rng,
    ))
    .await;
    let r = state
        .replacements
        .iter()
        .find(|r| r.index == replacement && !r.done)
        .ok_or_else(|| NodeError::NotReady("that key repair is already finished".into()))?;
    let mut behind = state.members_below(REPAIR_QUORUM_FROM);
    behind.retain(|pk| ![my_pk, r.new.sig_pk, r.old, stalled].contains(pk));
    if !behind.is_empty() {
        return Err(NodeError::MembersMustUpdate(behind));
    }
    let helpers: Vec<[u8; 32]> = r
        .helpers
        .iter()
        .map(|h| if *h == stalled { taker } else { *h })
        .collect();
    if !r.helpers.contains(&stalled) || r.helpers.contains(&taker) || !helpers.contains(&my_pk) {
        return Err(NodeError::NotReady(
            "that signer isn't helping, the other one already is, or you wouldn't be".into(),
        ));
    }
    let keys = relay
        .pq_keys_read(me, r.new.sig_pk)
        .await
        .map_err(older_relay)?;
    if taker == my_pk {
        ensure_pq_key(relay, me, r.new.sig_pk, &keys).await?;
    } else if !keys.contains_key(&taker) {
        return Err(NodeError::NotReady(
            "that signer's Zafe hasn't joined this recovery yet: ask them to open Zafe, \
             updating it if it's old, then try again"
                .into(),
        ));
    }
    let event = VaultEvent::RepairRetry {
        replacement,
        helpers,
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

pub async fn stop_helping<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    replacement: u64,
    rng: &mut R,
) -> Result<(), NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let event = VaultEvent::StopHelping { replacement };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

pub async fn seat_finishing(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    state: &VaultState,
) -> bool {
    let my_pk = me.public().sig_pk;
    let Some(r) = state.replacement_to(&my_pk).filter(|r| !r.done) else {
        return false;
    };
    let Some(move_id) = r.move_id(&state.descriptor.vault_id) else {
        return false;
    };
    state.descriptor.member(&my_pk).is_some()
        && relay
            .mailbox_info(me, material.label())
            .await
            .is_ok_and(|i| {
                i.pending
                    .iter()
                    .any(|p| p.move_id == move_id && p.new.sig_pk == my_pk)
            })
}

pub fn finishing() -> NodeError {
    NodeError::NotReady(
        "your seat is still finishing on the server; this phone retries on its own".into(),
    )
}

pub async fn mark_repair_done<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    rng: &mut R,
) -> Result<(), NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let Some(r) = state.replacement_to(&me.public().sig_pk) else {
        return Ok(());
    };
    if r.done {
        return Ok(());
    }
    if seat_finishing(relay, me, material, &state).await {
        return Err(finishing());
    }
    let event = VaultEvent::RepairDone {
        replacement: r.index,
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

pub fn current_material(material: &VaultMaterial, state: &VaultState) -> Option<VaultMaterial> {
    (material.descriptor.members != state.descriptor.members).then(|| VaultMaterial {
        descriptor: state.descriptor.clone(),
        ..material.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> VaultData {
        VaultData {
            public_key_package: vec![1, 2],
            vault_secret: [3; 32],
            log_key_epoch: 0,
            log_key: [4; 32],
        }
    }

    #[test]
    fn version_2_repair_payloads_still_read() {
        #[derive(Serialize)]
        struct V2 {
            replacement: u64,
            attempt: u32,
            sigma: Vec<u8>,
            data: VaultData,
        }
        let mut bytes = 2u16.to_le_bytes().to_vec();
        bytes.extend(
            postcard::to_allocvec(&V2 {
                replacement: 7,
                attempt: 1,
                sigma: vec![9],
                data: data(),
            })
            .unwrap(),
        );
        let mut rng = rand::rngs::OsRng;
        let me = Identity::generate(&mut rng);
        let other = Identity::generate(&mut rng);
        let from = [3u8; 32];
        let m = decode_sigma(&bytes, &me, &from).unwrap();
        assert_eq!((m.replacement, m.attempt, m.sigma), (7, 1, vec![9]));
        assert!(m.data == data() && m.anchor.is_none() && m.create_token.is_none());

        let delta = DeltaMsg {
            replacement: 7,
            attempt: 1,
            delta: vec![5],
        };
        let mut old = 2u16.to_le_bytes().to_vec();
        old.extend(postcard::to_allocvec(&delta).unwrap());
        let back: DeltaMsg = decode_repair(&old).unwrap();
        assert_eq!(back.delta, vec![5]);
        let encoded = version::encode(Format::Repair, &delta).unwrap();
        assert_eq!(
            version::split(Format::Repair, &encoded).unwrap().0,
            4,
            "deltas share REPAIR 4 on purpose: a 0.1.29 helper (tags 2-3) can't finish an \
             attempt with an updated helper, so it can't send its classical sigma (spec 7.6)"
        );
        let now: DeltaMsg = decode_repair(&encoded).unwrap();
        assert_eq!(now.replacement, 7);

        let current = SigmaMsg {
            replacement: 7,
            attempt: 1,
            sigma: vec![9],
            data: data(),
            anchor: Some(LogAnchor {
                vault_id: [1; 16],
                len: 4,
                head: [2; 32],
            }),
            create_token: Some("t".into()),
        };
        let mut v3 = 3u16.to_le_bytes().to_vec();
        v3.extend(postcard::to_allocvec(&current).unwrap());
        let back = decode_sigma(&v3, &me, &from).unwrap();
        assert_eq!(back.anchor, current.anchor);
        assert_eq!(back.create_token.as_deref(), Some("t"));

        let mut seal = |to: &Identity, attempt: u32| {
            let sealed = SealedSigmaMsg {
                replacement: 7,
                attempt,
                sealed: crate::pq::PqKeys::derive(to.seeds()).public().seal(
                    &sigma_context(&from, &to.public().sig_pk, 7, 1),
                    &postcard::to_allocvec(&current).unwrap(),
                    &mut rng,
                ),
            };
            version::encode(Format::Repair, &sealed).unwrap()
        };
        let back = decode_sigma(&seal(&me, 1), &me, &from).unwrap();
        assert_eq!(back.anchor, current.anchor);
        assert!(
            decode_sigma(&seal(&other, 1), &me, &from).is_err(),
            "sealed to another phone"
        );
        assert!(
            decode_sigma(&seal(&me, 2), &me, &from).is_err(),
            "outer attempt doesn't match the sealed context"
        );
        assert!(
            decode_sigma(&seal(&me, 1), &me, &[4u8; 32]).is_err(),
            "from another helper"
        );
        let mut newer = seal(&me, 1);
        newer[..2].copy_from_slice(&(version::REPAIR + 1).to_le_bytes());
        assert!(decode_sigma(&newer, &me, &from).is_err());
    }

    #[test]
    fn seat_links_round_trip_and_reject_other_versions() {
        let link = SeatLink {
            relay_url: "https://relay.dao.org".into(),
            label: [6; 16],
        };
        let text = link.encode();
        assert!(text.starts_with("zafe-seat-v1:"));
        assert_eq!(SeatLink::decode(&text).unwrap(), link);
        assert_eq!(
            SeatLink::decode(&format!("zafe://seat?link={text}&x=1")).unwrap(),
            link
        );
        let newer = text.replacen("zafe-seat-v1:", "zafe-seat-v2:", 1);
        assert!(matches!(
            SeatLink::decode(&newer),
            Err(NodeError::UnsupportedVersion(v)) if v.is_newer()
        ));
        let not_a_url = SeatLink {
            relay_url: "javascript:alert(1)".into(),
            label: [6; 16],
        };
        assert!(SeatLink::decode(&not_a_url.encode()).is_err());
    }
}
