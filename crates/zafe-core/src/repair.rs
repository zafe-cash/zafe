//! Moving a lost member's seat to a new device (spec §10.1, §10.4.2).
//!
//! 1. The new device makes a fresh identity and shows a [`RecoveryRequest`] (a code to send
//!    to a co-signer) plus a short [`RecoveryRequest::safety_code`] to read out.
//! 2. Other members approve the move in the vault log ([`approve_replacement`]); the same
//!    signatures let the relay swap the member's key ([`push_seat_moves`]). At the vault's
//!    threshold of approvals the seat moves (`VaultState::replacements`).
//! 3. The approvers repair the member's FROST share with the repairable threshold scheme
//!    (RTS, ePrint 2017/1155; `frost_core::keys::repairable`) without anyone learning it:
//!    each helper splits its share into deltas for the other helpers, each sums what it
//!    gets into a sigma for the new device, which also receives the vault secret, the log
//!    key and the public key package ([`help_repairs`]).
//! 4. The new device sums the sigmas, checks the result against the member's public share
//!    and every helper's copy of the vault data against each other and the vault's viewing
//!    key, and stores its material ([`try_recover`]).
//!
//! A wrong sigma can't steal anything (it only gives a useless share), and the checks in
//! step 4 catch it; the move is then asked again.

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
    relay::ReplaceApproval,
    version::{self, Format},
    Envelope, Identity, IdentityPublic, Kind, LogKey, MailboxId,
};

use crate::{
    node::{
        append_event, frost_id_of, load_log, load_state, members_by_pk, read_inbox, Invite,
        NodeError, SeqCounter, VaultMaterial,
    },
    relay_client::RelayClient,
    vault::{Replacement, VaultEvent, VaultState},
    wallet::ZafeNetwork,
};

fn proto(e: impl core::fmt::Debug) -> NodeError {
    NodeError::Protocol(format!("{e:?}"))
}

/// A recovering device's request to take over a lost member's seat: its new public keys.
/// Shared like an invite (text `zafe-recover-v1:<hex>`, a link or a QR code).
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

    /// Parses a request; one from another version of Zafe fails with
    /// [`NodeError::UnsupportedVersion`].
    pub fn decode(s: &str) -> Result<Self, NodeError> {
        let bad = || NodeError::Protocol("not a recovery code".into());
        let rest = s.trim().strip_prefix(Self::PREFIX).ok_or_else(bad)?;
        let (found, body) = rest.split_once(':').ok_or_else(bad)?;
        let found: u16 = found.parse().map_err(|_| bad())?;
        version::check(Format::RecoveryRequest, found)?;
        postcard::from_bytes(&hex::decode(body).map_err(|_| bad())?).map_err(|_| bad())
    }

    /// Eight digits ("1234 5678") both phones show, read out to check that the code
    /// wasn't swapped on the way.
    pub fn safety_code(&self) -> String {
        let hash = blake2b_simd::Params::new()
            .hash_length(16)
            .personal(b"Zafe_RecoverCode")
            .to_state()
            .update(&self.identity.sig_pk)
            .update(&self.identity.enc_pk)
            .finalize();
        let n = u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("8 bytes")) % 100_000_000;
        format!("{:04} {:04}", n / 10_000, n % 10_000)
    }
}

/// Member: approves moving `old`'s seat to the device that made `request`. Returns whether
/// the seat has moved (this approval completed it); the relay is then told right away.
pub async fn approve_replacement<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    old: [u8; 32],
    request: &RecoveryRequest,
    rng: &mut R,
) -> Result<bool, NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let approval = ReplaceApproval {
        mailbox: state.descriptor.vault_id,
        old,
        new: request.identity,
    };
    let event = VaultEvent::ReplaceApproval {
        old,
        new: request.identity,
        signature: approval.sign(me).map_err(proto)?,
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    let moved = state.replacement_to(&request.identity.sig_pk).is_some();
    if moved {
        push_seat_moves(relay, me, &state).await?;
    }
    Ok(moved)
}

/// Tells the relay about seat moves it hasn't made yet (idempotent; any member can).
pub async fn push_seat_moves(
    relay: &RelayClient,
    me: &Identity,
    state: &VaultState,
) -> Result<(), NodeError> {
    if state.replacements.is_empty() {
        return Ok(());
    }
    let mailbox = state.descriptor.vault_id;
    let on_relay = relay.members(me, mailbox).await?.members;
    for r in &state.replacements {
        if on_relay.iter().any(|m| m.sig_pk == r.old) {
            let approval = ReplaceApproval {
                mailbox,
                old: r.old,
                new: r.new,
            };
            relay
                .replace_member(me, approval, r.approvals.clone())
                .await?;
        }
    }
    Ok(())
}

#[derive(Serialize, Deserialize)]
struct DeltaMsg {
    /// The seat move ([`Replacement::index`]) and the repair attempt.
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
}

/// A helper's own deltas for one move, kept until its sigma is sent (a second part 1 would
/// produce different, incompatible deltas).
#[derive(Serialize, Deserialize)]
struct HelperState {
    /// Serialized deltas by helper `sig_pk` (this helper's own included).
    deltas: BTreeMap<[u8; 32], Vec<u8>>,
    sent: bool,
}

/// A received delta: (move, attempt, sender).
type DeltaKey = (u64, u32, [u8; 32]);

/// What [`help_repairs`] did this time.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct RepairReport {
    /// Moves this member sent its deltas for.
    pub deltas_sent: usize,
    /// Moves this member finished (sigma sent to the new device).
    pub sigmas_sent: usize,
    /// Moves waiting for other helpers' deltas.
    pub waiting: usize,
}

/// A helper's saved deltas, written before they are sent.
const DELTA_EXT: &str = "delta";
/// Marks a helper's finished attempt (its sigma was sent).
const DONE_EXT: &str = "done";

fn helper_path(dir: &Path, r: &Replacement, ext: &str) -> PathBuf {
    dir.join(format!("{}-{}.{ext}", r.index, r.attempt))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), NodeError> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, bytes).map_err(proto)?;
    std::fs::rename(&tmp, path).map_err(proto)
}

/// The helpers' FROST identifiers and the repaired member's, for a move.
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

/// Helper: does this member's part in repairing the share of every moved seat it helps
/// with (spec §10.4.2). Safe to call on every refresh: each step is recorded in `dir`
/// (secret: the deltas are shares of the member's key share; deleted once done).
pub async fn help_repairs<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    state: &VaultState,
    dir: &Path,
    rng: &mut R,
) -> Result<RepairReport, NodeError> {
    let my_pk = me.public().sig_pk;
    let mine: Vec<&Replacement> = state
        .replacements
        .iter()
        .filter(|r| !r.done && r.helpers().contains(&my_pk))
        .filter(|r| !helper_path(dir, r, DONE_EXT).exists())
        .collect();
    let mut report = RepairReport::default();
    if mine.is_empty() {
        return Ok(report);
    }
    std::fs::create_dir_all(dir).map_err(proto)?;
    push_seat_moves(relay, me, state).await?;
    let mailbox = state.descriptor.vault_id;
    let key_package = material.key_package()?;
    let members = members_by_pk(state);
    let mut seq = SeqCounter::default();

    // Deltas other helpers sent us, by move, attempt and sender.
    let mut received: BTreeMap<DeltaKey, (u64, Vec<u8>)> = BTreeMap::new();
    for (cursor, from, env) in read_inbox(relay, me, mailbox, &members, 0).await? {
        if env.header.kind != Kind::RepairDelta {
            continue;
        }
        let Ok(bytes) = env.open(me, &from) else {
            continue;
        };
        let Ok(msg) = version::decode::<DeltaMsg>(Format::Repair, &bytes) else {
            continue;
        };
        received
            .entry((msg.replacement, msg.attempt, from.sig_pk))
            .or_insert((cursor, msg.delta));
    }

    for r in mine {
        let (helper_ids, participant) = repair_ids(state, r)?;
        let helpers = r.helpers();
        let state_path = helper_path(dir, r, DELTA_EXT);
        let mut hs: HelperState = match std::fs::read(&state_path) {
            Ok(bytes) => version::decode(Format::Repair, &bytes)?,
            Err(_) => {
                let deltas = repairable::repair_share_part1::<PallasBlake2b512, _>(
                    &helper_ids,
                    &key_package,
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
                // Saved before anything is sent: a second part 1 would not match.
                write_atomic(&state_path, &version::encode(Format::Repair, &hs)?)?;
                hs
            }
        };
        if !hs.sent {
            for pk in helpers.iter().filter(|pk| **pk != my_pk) {
                let to = members
                    .get(pk)
                    .ok_or_else(|| NodeError::Protocol("a helper is no longer a member".into()))?;
                let msg = DeltaMsg {
                    replacement: r.index,
                    attempt: r.attempt,
                    delta: hs.deltas[pk].clone(),
                };
                let env = Envelope::sealed(
                    me,
                    to,
                    mailbox,
                    seq.next_seq(),
                    Kind::RepairDelta,
                    &version::encode(Format::Repair, &msg)?,
                    rng,
                )
                .map_err(proto)?;
                relay.send(&env).await?;
            }
            hs.sent = true;
            write_atomic(&state_path, &version::encode(Format::Repair, &hs)?)?;
            report.deltas_sent += 1;
        }

        // Part 2 once every other helper's delta is here.
        let mut deltas = vec![Delta::deserialize(&hs.deltas[&my_pk]).map_err(proto)?];
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
            continue;
        }
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
        };
        let env = Envelope::sealed(
            me,
            &r.new,
            mailbox,
            seq.next_seq(),
            Kind::RepairSigma,
            &version::encode(Format::Repair, &msg)?,
            rng,
        )
        .map_err(proto)?;
        relay.send(&env).await?;
        write_atomic(&helper_path(dir, r, DONE_EXT), b"")?;
        let _ = std::fs::remove_file(&state_path);
        let _ = relay.ack_inbox(me, mailbox, &cursors).await;
        report.sigmas_sent += 1;
    }
    Ok(report)
}

/// Where a recovering device is.
#[derive(Debug)]
pub enum RecoveryStatus {
    /// No vault has moved a seat to this device yet (co-signers haven't approved).
    Waiting,
    /// The seat moved; the helpers' repair messages are arriving.
    SeatMoved { received: usize, needed: usize },
    /// Recovered: the device's vault material and an invite describing the vault (its
    /// join token is unused, membership being sealed).
    Done {
        material: Box<VaultMaterial>,
        invite: Invite,
    },
}

/// Recovering device: checks whether its seat has moved and the helpers' repair is
/// complete, and if so builds and verifies its vault material (spec §10.4.2 checks).
pub async fn try_recover(relay: &RelayClient, me: &Identity) -> Result<RecoveryStatus, NodeError> {
    let mut status = RecoveryStatus::Waiting;
    for mailbox in relay.mailboxes(me).await? {
        match recover_from(relay, me, mailbox).await? {
            done @ RecoveryStatus::Done { .. } => return Ok(done),
            s @ RecoveryStatus::SeatMoved { .. } => status = s,
            RecoveryStatus::Waiting => {}
        }
    }
    Ok(status)
}

async fn recover_from(
    relay: &RelayClient,
    me: &Identity,
    mailbox: MailboxId,
) -> Result<RecoveryStatus, NodeError> {
    let on_relay: BTreeMap<[u8; 32], IdentityPublic> = relay
        .members(me, mailbox)
        .await?
        .members
        .into_iter()
        .map(|m| (m.sig_pk, m))
        .collect();
    let mut messages: BTreeMap<([u8; 32], u32), (u64, SigmaMsg)> = BTreeMap::new();
    for (cursor, from, env) in read_inbox(relay, me, mailbox, &on_relay, 0).await? {
        if env.header.kind != Kind::RepairSigma {
            continue;
        }
        let Ok(bytes) = env.open(me, &from) else {
            continue;
        };
        if let Ok(msg) = version::decode::<SigmaMsg>(Format::Repair, &bytes) {
            messages
                .entry((from.sig_pk, msg.attempt))
                .or_insert((cursor, msg));
        }
    }
    let Some((_, first)) = messages.values().last() else {
        return Ok(RecoveryStatus::SeatMoved {
            received: 0,
            needed: 0,
        });
    };
    // The log key from a helper lets this device read the log, and the log decides
    // which move is ours and who the helpers are.
    let key = LogKey::from_bytes(first.data.log_key_epoch, first.data.log_key);
    let (chain, state) = load_log(relay, me, mailbox, &key).await?;
    let r = state.replacement_to(&me.public().sig_pk).ok_or_else(|| {
        NodeError::Protocol("the vault log has no seat move to this device".into())
    })?;
    let helpers = r.helpers();
    let from_helpers: Vec<&SigmaMsg> = helpers
        .iter()
        .filter_map(|pk| messages.get(&(*pk, r.attempt)).map(|(_, m)| m))
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
    let data = &from_helpers[0].data;
    if from_helpers.iter().any(|m| &m.data != data) {
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
    };
    let network = ZafeNetwork::from_name(&state.descriptor.network)
        .ok_or_else(|| proto("unknown network"))?;
    let ufvk = material
        .vault_keys()?
        .ufvk()
        .map_err(proto)?
        .encode(&network);
    if ufvk != state.descriptor.ufvk {
        return Err(failed(
            "the vault secret doesn't give the vault's viewing key",
        ));
    }
    let creator = chain
        .entries()
        .first()
        .map(|e| e.header.author)
        .unwrap_or_default();
    let invite = Invite {
        mailbox,
        join_token: [0; 32],
        creator,
        threshold: state.descriptor.threshold,
        members: state.descriptor.members.len() as u16,
        name: state.descriptor.name.clone(),
    };
    let cursors: Vec<u64> = helpers
        .iter()
        .filter_map(|pk| messages.get(&(*pk, r.attempt)).map(|(c, _)| *c))
        .collect();
    let _ = relay.ack_inbox(me, mailbox, &cursors).await;
    Ok(RecoveryStatus::Done {
        material: Box::new(material),
        invite,
    })
}

/// Member: takes over from `stalled`, a helper who isn't doing its part in repairing the
/// key of the seat moved at `replacement`, starting a new attempt with this member in its
/// place (spec §10.4.2).
pub async fn retry_repair<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    replacement: u64,
    stalled: [u8; 32],
    rng: &mut R,
) -> Result<(), NodeError> {
    let (mut chain, mut state) = load_state(relay, me, material).await?;
    let r = state
        .replacements
        .iter()
        .find(|r| r.index == replacement && !r.done)
        .ok_or_else(|| NodeError::NotReady("that key repair is already finished".into()))?;
    let my_pk = me.public().sig_pk;
    if !r.helpers.contains(&stalled) || r.helpers.contains(&my_pk) {
        return Err(NodeError::NotReady(
            "you are already helping, or that signer isn't".into(),
        ));
    }
    let helpers = r
        .helpers
        .iter()
        .map(|h| if *h == stalled { my_pk } else { *h })
        .collect();
    let event = VaultEvent::RepairRetry {
        replacement,
        helpers,
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

/// New device, once its key checked out: tells the others the repair is finished.
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
    let event = VaultEvent::RepairDone {
        replacement: r.index,
    };
    append_event(relay, me, material, &mut chain, &mut state, &event, rng).await?;
    Ok(())
}

/// The member's material with the vault's current membership, when seats moved since it
/// was stored (the app saves it so summaries and checks see the new keys).
pub fn current_material(material: &VaultMaterial, state: &VaultState) -> Option<VaultMaterial> {
    (material.descriptor.members != state.descriptor.members).then(|| VaultMaterial {
        descriptor: state.descriptor.clone(),
        ..material.clone()
    })
}
