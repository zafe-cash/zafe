//! Moving a vault to another relay (`docs/relay-move.md` §4.2-4.7): start a move, follow
//! one, count arrivals, switch, cancel, and the write rules `append_event` applies.
//!
//! Nothing here connects to a new relay unless the member asked for it: the mover picked
//! it, a follower tapped Move. A phone that sent its arrival checks the new relay before
//! each write to the old one (`write_target`).

use super::*;
use crate::moves::{
    normalize_relay_url, unix_now, MoveLink, MoveMessage, MoveState, MoveTarget, MOVE_LIFETIME_SECS,
};

/// What [`move_vault`] started.
#[derive(Clone, Debug)]
pub struct MoveStarted {
    pub target: MoveTarget,
    /// `https://zafe.cash/move#…` for the group's chat.
    pub link: String,
    /// Members (signing keys) whose move notice reached the old relay (the rest learn from
    /// the link, or from a retry: [`move_progress`] sends what's missing).
    pub notified: Vec<[u8; 32]>,
}

/// Where a move stands on this phone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveStatus {
    pub target: MoveTarget,
    pub mover: [u8; 32],
    /// This phone started it.
    pub mine: bool,
    /// Members known to have moved (this phone's view: arrivals it holds, itself included).
    pub arrived: Vec<[u8; 32]>,
    /// m: arrivals that switch the vault.
    pub needed: usize,
    pub members: usize,
    /// This phone sent its arrival.
    pub this_phone_arrived: bool,
    pub expired: bool,
    pub cancelled: bool,
    /// The vault is on the new relay and this phone follows it.
    pub switched: bool,
    /// The vault switched (a verified `VaultMoved`) but this phone hasn't followed yet:
    /// "Grants moved to relay.dao.org. Move to continue."
    pub moved_elsewhere: bool,
}

/// A move notice found on the vault's current relay ([`move_notice`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveNotice {
    pub target: MoveTarget,
    pub mover: [u8; 32],
    /// The notice carried a `VaultMoved` entry that verified: the vault already switched.
    pub moved: bool,
}

fn now() -> u64 {
    unix_now()
}

fn move_dir(relay: &RelayClient) -> Result<std::path::PathBuf, NodeError> {
    relay
        .log_cache()
        .map(|c| c.dir().to_path_buf())
        .ok_or_else(|| NodeError::Protocol("a relay move needs this phone's log copy".into()))
}

/// This phone's move of the vault, if any (`<vault id>.move` next to the log copy).
pub fn move_state(relay: &RelayClient, material: &VaultMaterial) -> Option<MoveState> {
    let dir = relay.log_cache()?.dir().to_path_buf();
    MoveState::load(&dir, &material.descriptor.vault_id, &material.vault_secret)
}

fn save_move(
    relay: &RelayClient,
    material: &VaultMaterial,
    ms: &MoveState,
) -> Result<(), NodeError> {
    ms.save(&move_dir(relay)?, &material.vault_secret)
        .map_err(|e| NodeError::Io(format!("move state: {e}")))
}

/// The relay this phone should use for the vault: the new one once this phone verified the
/// switch itself ([`MoveState::switched`]), with the new relay's create token. `None`:
/// keep `material.relay`. The app applies this to the stored material and saves it.
pub fn followed_relay(material: &VaultMaterial) -> Option<VaultRelay> {
    let cache = crate::log_cache::configured()?;
    followed_relay_in(cache.dir(), material)
}

/// [`followed_relay`] with the log copy's directory given.
pub fn followed_relay_in(dir: &std::path::Path, material: &VaultMaterial) -> Option<VaultRelay> {
    let ms = MoveState::load(dir, &material.descriptor.vault_id, &material.vault_secret)?;
    let relay = if ms.switched {
        VaultRelay {
            url: Some(ms.target.relay.clone()),
            label: ms.target.label,
            create_token: ms.create_token.clone(),
        }
    } else {
        ms.from
    };
    (relay.label != material.relay.label).then_some(relay)
}

/// `material` with [`followed_relay`] applied.
pub fn with_followed_relay(material: &VaultMaterial) -> VaultMaterial {
    let mut m = material.clone();
    if let Some(relay) = followed_relay(material) {
        m.relay = relay;
    }
    m
}

/// The vault's state from the relay, or from this phone's copy when the relay is down,
/// lost the vault or can't be trusted (a move away from a hostile relay starts from the
/// copy). Errors of this phone's own copy still stop it.
async fn state_here(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
) -> Result<(Chain, VaultState), NodeError> {
    match Box::pin(load_state(relay, me, material)).await {
        Ok(loaded) => Ok(loaded),
        Err(
            NodeError::Relay(_)
            | NodeError::RelayRolledBack { .. }
            | NodeError::RelayForked { .. }
            | NodeError::RelayLostVault { .. }
            | NodeError::RelayMembership(_),
        ) => load_copy(relay, material.descriptor.vault_id, &material.log_key())?
            .ok_or_else(|| NodeError::Protocol("this phone has no copy of the vault log".into())),
        Err(e) => Err(e),
    }
}

/// Logs that this member's app writes the current event version, once (an idempotent
/// `Name` event re-stating its current name). Moving the vault waits until every member's
/// app has done this ([`VaultState::members_needing_update`]). Returns whether it wrote.
pub async fn announce_version<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    chain: &mut Chain,
    state: &mut VaultState,
    rng: &mut R,
) -> Result<bool, NodeError> {
    let pk = me.public().sig_pk;
    if state.descriptor.member(&pk).is_none()
        || state.member_versions.get(&pk).copied().unwrap_or(0) >= version::VAULT_EVENT
    {
        return Ok(false);
    }
    let name = state.names.get(&pk).cloned().unwrap_or_default();
    Box::pin(append_event(
        relay,
        me,
        material,
        chain,
        state,
        &VaultEvent::Name { name },
        rng,
    ))
    .await?;
    Ok(true)
}

/// One sealed move message to `to` under `label` on `relay`.
#[allow(clippy::too_many_arguments)]
async fn send_move_message<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    label: MailboxId,
    to: &IdentityPublic,
    kind: Kind,
    msg: &MoveMessage,
    seq: &mut SeqCounter,
    rng: &mut R,
) -> Result<(), NodeError> {
    let payload = msg.to_padded();
    send_fresh(relay, seq, |n| {
        Envelope::sealed(me, to, label, n, kind, &payload, rng).map_err(proto)
    })
    .await
}

fn others(state: &VaultState, me: &Identity) -> Vec<IdentityPublic> {
    state
        .member_identities()
        .into_iter()
        .filter(|m| m.sig_pk != me.public().sig_pk)
        .collect()
}

/// Sends this phone's arrival to every other member on the new relay.
async fn send_arrival<R: RngCore + CryptoRng>(
    new: &RelayClient,
    me: &Identity,
    state: &VaultState,
    ms: &MoveState,
    rng: &mut R,
) -> Result<(), NodeError> {
    let mine = ms.mover == me.public().sig_pk;
    let msg = MoveMessage::Arrival {
        target: ms.target.clone(),
        signature: ms.target.sign(me),
        // The mover's arrival carries the token, sealed; nobody else's needs to.
        create_token: if mine { ms.create_token.clone() } else { None },
    };
    let mut seq = SeqCounter::default();
    for to in others(state, me) {
        send_move_message(
            new,
            me,
            ms.target.label,
            &to,
            Kind::Arrival,
            &msg,
            &mut seq,
            rng,
        )
        .await?;
    }
    Ok(())
}

/// [`send_arrival`], remembering whether it reached everyone (else the next
/// [`move_progress`] sends it again; a repeat is harmless).
async fn deliver_arrival<R: RngCore + CryptoRng>(
    new: &RelayClient,
    me: &Identity,
    state: &VaultState,
    ms: &mut MoveState,
    rng: &mut R,
) {
    match Box::pin(send_arrival(new, me, state, ms, rng)).await {
        Ok(()) => ms.arrival_delivered = true,
        Err(e) => tracing::warn!("arrival not sent to every member yet: {e}"),
    }
}

/// Leaves move notices on the old relay for the members not notified yet (best effort:
/// an old relay that is down or hostile is the reason for many moves). With `moved`, the
/// `VaultMoved` entry goes to everyone.
#[allow(clippy::too_many_arguments)]
async fn send_notices<R: RngCore + CryptoRng>(
    old: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    state: &VaultState,
    ms: &mut MoveState,
    moved: Option<Vec<u8>>,
    signature: Vec<u8>,
    rng: &mut R,
) -> bool {
    let msg = MoveMessage::Notice {
        target: ms.target.clone(),
        mover: ms.mover,
        signature,
        create_token: ms.create_token.clone(),
        moved: moved.clone(),
    };
    let mut seq = SeqCounter::default();
    let mut all = true;
    for to in others(state, me) {
        if moved.is_none() && ms.notified.contains(&to.sig_pk) {
            continue;
        }
        let sent = send_move_message(
            old,
            me,
            material.label(),
            &to,
            Kind::MoveNotice,
            &msg,
            &mut seq,
            rng,
        )
        .await;
        match sent {
            Ok(()) if moved.is_none() => ms.notified.push(to.sig_pk),
            Ok(()) => {}
            Err(e) => {
                all = false;
                tracing::warn!("move notice not left on the old relay: {e}");
                if relay_unreachable(&e) {
                    break;
                }
            }
        }
    }
    all
}

/// Starts moving the vault to the relay at `new_url` (after the member's unlock): picks a
/// random mailbox label and move id, restores the vault there from this phone's copy of
/// the log (with the relay's `create_token`, for a private relay), sends this phone's
/// arrival there, then leaves move notices on the current relay if it answers. This phone
/// keeps using the current relay until the switch. Fails with
/// [`NodeError::MembersMustUpdate`] while a member's app is older than moves, and
/// [`NodeError::MoveInProgress`] while this phone takes part in another move.
pub async fn move_vault<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    new_url: &str,
    create_token: Option<&str>,
    rng: &mut R,
) -> Result<MoveStarted, NodeError> {
    let dir = move_dir(relay)?;
    if move_state(relay, material).is_some_and(|ms| ms.open(now())) {
        return Err(NodeError::MoveInProgress);
    }
    let url = normalize_relay_url(new_url)
        .ok_or_else(|| NodeError::Protocol(format!("not a relay address: {new_url}")))?;
    let (mut chain, mut state) = Box::pin(state_here(relay, me, material)).await?;
    if let Some(m) = state.moved() {
        if m.target.label != material.relay.label {
            return Err(NodeError::VaultMoved {
                url: m.target.relay.clone(),
            });
        }
    }
    // The mover's own app announces itself on the way (when the relay answers).
    if state
        .member_versions
        .get(&me.public().sig_pk)
        .copied()
        .unwrap_or(0)
        < version::VAULT_EVENT
    {
        let _ = Box::pin(announce_version(
            relay, me, material, &mut chain, &mut state, rng,
        ))
        .await;
    }
    let mut behind = state.members_needing_update();
    behind.retain(|pk| *pk != me.public().sig_pk);
    if !behind.is_empty() {
        return Err(NodeError::MembersMustUpdate(behind));
    }
    let (mut label, mut move_id) = ([0u8; 16], [0u8; 16]);
    rng.fill_bytes(&mut label);
    rng.fill_bytes(&mut move_id);
    let target = MoveTarget {
        vault_id: material.descriptor.vault_id,
        move_id,
        relay: url.clone(),
        label,
        base_index: chain.len() - 1,
        base_hash: chain.head(),
        expiry: now() + MOVE_LIFETIME_SECS,
    };
    let token = create_token
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned);
    let mut ms = MoveState {
        target: target.clone(),
        mover: me.public().sig_pk,
        arrived: false,
        create_token: token.clone(),
        notified: Vec::new(),
        cancelled: false,
        moved_seen: false,
        switched: false,
        moved_notice_sent: false,
        arrival_delivered: false,
        from: material.relay.clone(),
    };
    // Saved first: the label is the one secret of the new mailbox until it exists.
    ms.save(&dir, &material.vault_secret)
        .map_err(|e| NodeError::Io(format!("move state: {e}")))?;
    let new = relay.at(&url).with_create_token(token.as_deref());
    let ids = VaultIds {
        vault: material.descriptor.vault_id,
        label,
    };
    if let Err(e) = Box::pin(reseed_relay(&new, me, ids, &material.log_key())).await {
        MoveState::remove(&dir, &material.descriptor.vault_id);
        return Err(e);
    }
    // From here on this phone checks the new relay before each write to the old one.
    ms.arrived = true;
    save_move(relay, material, &ms)?;
    Box::pin(deliver_arrival(&new, me, &state, &mut ms, rng)).await;
    let signature = target.sign(me);
    Box::pin(send_notices(
        relay, me, material, &state, &mut ms, None, signature, rng,
    ))
    .await;
    save_move(relay, material, &ms)?;
    Ok(MoveStarted {
        link: MoveLink::of(&target).encode(),
        notified: ms.notified.clone(),
        target,
    })
}

/// Arrivals for one move found in a member's inbox on the new relay.
#[derive(Default)]
struct Arrivals {
    target: Option<MoveTarget>,
    /// Sender → signature, for senders whose arrival stands (not withdrawn).
    by: BTreeMap<[u8; 32], Vec<u8>>,
    /// The first member whose arrival came (the mover, from a link).
    first: Option<[u8; 32]>,
    /// The token the mover's arrival carried.
    token: Option<String>,
    /// Members who sent a cancel for the move.
    cancelled: BTreeSet<[u8; 32]>,
}

/// Reads this member's inbox on the new relay for the move `link` names: arrivals signed by
/// current members (each by its sender) for exactly that target, and cancels.
async fn read_arrivals(
    new: &RelayClient,
    me: &Identity,
    state: &VaultState,
    link: &MoveLink,
) -> Result<Arrivals, NodeError> {
    let members = members_by_pk(state);
    let mut out = Arrivals::default();
    let inbox = match read_inbox(new, me, link.label, &members, 0).await {
        Ok(inbox) => inbox,
        // Not a mailbox this member is in (or none at all): no member's arrival.
        Err(NodeError::Relay(RelayClientError::Status {
            status: 403 | 404, ..
        })) => return Ok(out),
        Err(e) => return Err(e),
    };
    for (_, sender, env) in inbox {
        if !matches!(env.header.kind, Kind::Arrival | Kind::MoveCancelled) {
            continue;
        }
        let Ok(msg) = env
            .open(me, &sender)
            .map_err(|_| ())
            .and_then(|b| MoveMessage::from_padded(&b).map_err(|_| ()))
        else {
            continue;
        };
        match msg {
            MoveMessage::Arrival {
                target,
                signature,
                create_token,
            } => {
                let matches = target.vault_id == link.vault_id
                    && target.move_id == link.move_id
                    && target.label == link.label
                    && target.relay == link.relay
                    && out.target.as_ref().is_none_or(|t| *t == target)
                    && target.verify(&sender, &signature);
                if !matches {
                    continue;
                }
                out.target.get_or_insert(target);
                out.first.get_or_insert(sender.sig_pk);
                if out.first == Some(sender.sig_pk) && create_token.is_some() {
                    out.token = create_token;
                }
                out.cancelled.remove(&sender.sig_pk);
                out.by.insert(sender.sig_pk, signature);
            }
            MoveMessage::Cancelled { move_id } if move_id == link.move_id => {
                out.by.remove(&sender.sig_pk);
                out.cancelled.insert(sender.sig_pk);
            }
            _ => {}
        }
    }
    Ok(out)
}

/// The new relay's log, verified on its own, and how it stands against this phone's copy:
/// the longer of the two histories (they must agree where both have entries, else
/// [`NodeError::RelayForked`]) and the relay's length. Checks the relay's member list and
/// threshold too. Extends this phone's copy when the relay is ahead.
async fn new_relay_history(
    old: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    link: &MoveLink,
) -> Result<(Chain, VaultState, u64, RelayClient), NodeError> {
    let key = material.log_key();
    let ids = VaultIds {
        vault: material.descriptor.vault_id,
        label: link.label,
    };
    let new = old.at(&link.relay);
    let bare = new.clone().with_log_cache(None);
    let (theirs, their_state, _) = Box::pin(read_log_verified(&bare, me, ids, &key)).await?;
    let d = &their_state.descriptor;
    if d.group_public_key != material.descriptor.group_public_key
        || d.ufvk != material.descriptor.ufvk
    {
        return Err(NodeError::Protocol(
            "the vault log belongs to another vault".into(),
        ));
    }
    let relay_len = theirs.len();
    let (chain, state) = match load_copy(old, ids.vault, &key)? {
        None => (theirs, their_state),
        Some((ours, our_state)) => {
            let common = ours.len().min(theirs.len());
            if common > 0 {
                let i = (common - 1) as usize;
                if ours.entries()[i].hash().map_err(proto)?
                    != theirs.entries()[i].hash().map_err(proto)?
                {
                    return Err(NodeError::RelayForked { index: common - 1 });
                }
            }
            if ours.len() > theirs.len() {
                (ours, our_state)
            } else {
                (theirs, their_state)
            }
        }
    };
    Box::pin(check_relay_membership(&bare, me, link.label, &state)).await?;
    if chain.len() == relay_len {
        save_copy(old, &chain);
    }
    Ok((chain, state, relay_len, new))
}

/// The state's switch for `move_id`, if it has one.
fn switched_by(state: &VaultState, move_id: &[u8; 16]) -> bool {
    state.moves.iter().any(|m| &m.target.move_id == move_id)
}

/// Follows a move (only after the member tapped Move, with unlock): connects to the new
/// relay, requires a current member's signed arrival for this move there (or the switch
/// itself), refuses an expired move or one older than the vault's last switch, runs the
/// checks every load does (history against this phone's copy, members and threshold),
/// adds the entries this phone's copy has that the relay lacks, and sends this phone's
/// arrival. One move per phone. Returns where the move stands (it may have switched).
pub async fn follow_move<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    link: &MoveLink,
    rng: &mut R,
) -> Result<MoveStatus, NodeError> {
    if link.vault_id != material.descriptor.vault_id {
        return Err(NodeError::Protocol(
            "the move link is for another vault".into(),
        ));
    }
    let dir = move_dir(relay)?;
    let existing = move_state(relay, material);
    if let Some(ms) = &existing {
        if ms.target.move_id != link.move_id && ms.open(now()) {
            return Err(NodeError::MoveInProgress);
        }
    }
    // Bring the copy up to date while the old relay still answers (best effort: it may be
    // dead or the reason for the move), and look for a second switch in its notices.
    let _ = Box::pin(load_state(relay, me, material)).await;
    if let Err(NodeError::TwoMoves) = Box::pin(move_notice(relay, me, material)).await {
        return Err(NodeError::TwoMoves);
    }
    let (copy, copy_state) =
        load_copy(relay, material.descriptor.vault_id, &material.log_key())?
            .ok_or_else(|| NodeError::Protocol("this phone has no copy of the vault log".into()))?;
    if copy_state
        .moves
        .iter()
        .any(|m| m.target.move_id == link.move_id)
        && material.relay.label == link.label
    {
        return Box::pin(status_of(relay, me, material)).await;
    }
    let (chain, state, relay_len, new) =
        match Box::pin(new_relay_history(relay, me, material, link)).await {
            Ok(h) => h,
            // No such mailbox there, or not one this member is in: nobody moved there.
            Err(NodeError::Relay(RelayClientError::Status {
                status: 403 | 404, ..
            })) => return Err(NodeError::MoveNotFound),
            Err(e) => return Err(e),
        };
    let ids = VaultIds {
        vault: material.descriptor.vault_id,
        label: link.label,
    };
    // Already switched there (a phone that was off, or a restore): the `VaultMoved` entry
    // carries m members' arrivals, so it is the proof.
    if switched_by(&state, &link.move_id) {
        let m = state
            .moves
            .iter()
            .find(|m| m.target.move_id == link.move_id)
            .expect("checked")
            .clone();
        if state.moved().map(|last| last.index) != Some(m.index) {
            return Err(NodeError::MoveStale);
        }
        let mut ms = existing
            .filter(|ms| ms.target.move_id == link.move_id)
            .unwrap_or(MoveState {
                target: m.target.clone(),
                mover: m.arrived.first().copied().unwrap_or(m.author),
                arrived: false,
                create_token: None,
                notified: Vec::new(),
                cancelled: false,
                moved_seen: false,
                switched: false,
                moved_notice_sent: false,
                arrival_delivered: false,
                from: material.relay.clone(),
            });
        if ms.create_token.is_none() {
            ms.create_token = Box::pin(read_arrivals(&new, me, &state, link)).await?.token;
        }
        ms.moved_seen = true;
        ms.switched = true;
        ms.save(&dir, &material.vault_secret)
            .map_err(|e| NodeError::Io(format!("move state: {e}")))?;
        let _ = relay.forget_push(me, material.label()).await;
        return Box::pin(status_of(relay, me, material)).await;
    }
    let arrivals = Box::pin(read_arrivals(&new, me, &state, link)).await?;
    let Some(target) = arrivals.target.clone().filter(|_| !arrivals.by.is_empty()) else {
        return Err(NodeError::MoveNotFound);
    };
    let mover = arrivals.first.expect("set with the target");
    if arrivals.cancelled.contains(&mover) {
        return Err(NodeError::MoveNotFound);
    }
    if target.expired(now()) {
        return Err(NodeError::MoveExpired);
    }
    if state
        .moved()
        .is_some_and(|last| target.base_index < last.index)
        || switched_by(&state, &target.move_id)
    {
        return Err(NodeError::MoveStale);
    }
    // The mover's history must be this phone's: the entry it started from is ours.
    if let Some(entry) = copy.entries().get(target.base_index as usize) {
        if entry.hash().map_err(proto)? != target.base_hash {
            return Err(NodeError::RelayForked {
                index: target.base_index,
            });
        }
    }
    // Entries this phone has that the new relay lacks (copied, never new: rule 1).
    if chain.len() > relay_len {
        Box::pin(catch_relay_up(&new, me, ids, &chain, relay_len)).await?;
    }
    let mut ms = MoveState {
        target,
        mover,
        arrived: true,
        create_token: arrivals.token.clone(),
        notified: Vec::new(),
        cancelled: false,
        moved_seen: false,
        switched: false,
        moved_notice_sent: false,
        arrival_delivered: false,
        from: material.relay.clone(),
    };
    ms.save(&dir, &material.vault_secret)
        .map_err(|e| NodeError::Io(format!("move state: {e}")))?;
    Box::pin(deliver_arrival(&new, me, &state, &mut ms, rng)).await;
    save_move(relay, material, &ms)?;
    match Box::pin(move_progress(relay, me, material, rng)).await? {
        Some(status) => Ok(status),
        None => Box::pin(status_of(relay, me, material)).await,
    }
}

/// This phone's view of its move without contacting any relay beyond what it already
/// knows (for a status right after a step).
async fn status_of(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
) -> Result<MoveStatus, NodeError> {
    let ms = move_state(relay, material)
        .ok_or_else(|| NodeError::Protocol("no move in progress".into()))?;
    let state = load_copy(relay, material.descriptor.vault_id, &material.log_key())?
        .map(|(_, s)| s)
        .ok_or_else(|| NodeError::Protocol("this phone has no copy of the vault log".into()))?;
    let mut arrived = Vec::new();
    if let Some(m) = state
        .moves
        .iter()
        .find(|m| m.target.move_id == ms.target.move_id)
    {
        arrived = m.arrived.clone();
    } else if ms.arrived {
        arrived.push(me.public().sig_pk);
    }
    Ok(status(&ms, &state, me, arrived))
}

fn status(
    ms: &MoveState,
    state: &VaultState,
    me: &Identity,
    mut arrived: Vec<[u8; 32]>,
) -> MoveStatus {
    // Members in descriptor order.
    let order: Vec<[u8; 32]> = state.member_identities().iter().map(|m| m.sig_pk).collect();
    arrived.sort_by_key(|pk| order.iter().position(|o| o == pk).unwrap_or(usize::MAX));
    arrived.dedup();
    MoveStatus {
        target: ms.target.clone(),
        mover: ms.mover,
        mine: ms.mover == me.public().sig_pk,
        arrived,
        needed: state.switch_threshold(),
        members: state.descriptor.members.len(),
        this_phone_arrived: ms.arrived,
        expired: ms.target.expired(now()) && !ms.switched && !ms.moved_seen,
        cancelled: ms.cancelled,
        switched: ms.switched,
        moved_elsewhere: ms.moved_seen && !ms.switched,
    }
}

/// Moves this phone's part of the move on and says where it stands (`None`: no move):
/// counts the arrivals on the new relay, notices a cancel or the switch, and switches the
/// vault when this phone holds m arrivals ([`switch_relay`]). Also resends move notices
/// the old relay didn't take. Called on every refresh while a move is open.
pub async fn move_progress<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    rng: &mut R,
) -> Result<Option<MoveStatus>, NodeError> {
    let Some(mut ms) = move_state(relay, material) else {
        return Ok(None);
    };
    let key = material.log_key();
    let Some((_, copy_state)) = load_copy(relay, material.descriptor.vault_id, &key)? else {
        return Ok(None);
    };
    if ms.switched || ms.cancelled || !ms.arrived || ms.target.expired(now()) {
        return Ok(Some(Box::pin(status_of(relay, me, material)).await?));
    }
    let link = MoveLink::of(&ms.target);
    // The switch may be there already (another phone counted first).
    let (chain, state, relay_len, new) =
        match Box::pin(new_relay_history(relay, me, material, &link)).await {
            Ok(h) => h,
            Err(e) if relay_unreachable(&e) => {
                let mut arrived = vec![me.public().sig_pk];
                if ms.mover != me.public().sig_pk {
                    arrived.push(ms.mover);
                }
                return Ok(Some(status(&ms, &copy_state, me, arrived)));
            }
            Err(e) => return Err(e),
        };
    if switched_by(&state, &ms.target.move_id) {
        ms.moved_seen = true;
        ms.switched = true;
        save_move(relay, material, &ms)?;
        let _ = relay.forget_push(me, material.label()).await;
        return Ok(Some(Box::pin(status_of(relay, me, material)).await?));
    }
    let arrivals = Box::pin(read_arrivals(&new, me, &state, &link)).await?;
    if arrivals.cancelled.contains(&ms.mover) && ms.mover != me.public().sig_pk {
        ms.cancelled = true;
        save_move(relay, material, &ms)?;
        return Ok(Some(Box::pin(status_of(relay, me, material)).await?));
    }
    if !ms.arrival_delivered {
        Box::pin(deliver_arrival(&new, me, &state, &mut ms, rng)).await;
        save_move(relay, material, &ms)?;
    }
    if ms.create_token.is_none() {
        if let Some(token) = arrivals.token.clone() {
            ms.create_token = Some(token);
            save_move(relay, material, &ms)?;
        }
    }
    // The mover's notices that didn't reach the old relay yet.
    if ms.mover == me.public().sig_pk && ms.notified.len() + 1 < state.descriptor.members.len() {
        let signature = ms.target.sign(me);
        Box::pin(send_notices(
            relay, me, material, &state, &mut ms, None, signature, rng,
        ))
        .await;
        save_move(relay, material, &ms)?;
    }
    let mut signatures = arrivals.by.clone();
    signatures.insert(me.public().sig_pk, ms.target.sign(me));
    let arrived: Vec<[u8; 32]> = signatures.keys().copied().collect();
    if arrived.len() >= state.switch_threshold() {
        Box::pin(switch_relay(
            relay, me, material, &mut ms, chain, state, relay_len, &new, signatures, rng,
        ))
        .await?;
        return Ok(Some(Box::pin(status_of(relay, me, material)).await?));
    }
    Ok(Some(status(&ms, &state, me, arrived)))
}

/// The switch (this phone holds m arrivals): a last catch-up from the old relay if it
/// answers, then the new relay is caught up from this phone's history and gets
/// `VaultMoved` as its first new entry. Every phone that sees it follows. The entry also
/// goes into move notices on the old relay (so a straggler, and a second move from the
/// same point, can be found), and this phone's push registration there is dropped.
#[allow(clippy::too_many_arguments)]
async fn switch_relay<R: RngCore + CryptoRng>(
    old: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    ms: &mut MoveState,
    mut chain: Chain,
    mut state: VaultState,
    mut relay_len: u64,
    new: &RelayClient,
    signatures: BTreeMap<[u8; 32], Vec<u8>>,
    rng: &mut R,
) -> Result<(), NodeError> {
    let key = material.log_key();
    let ids = VaultIds {
        vault: material.descriptor.vault_id,
        label: ms.target.label,
    };
    // Last entries from the old relay, if it still answers and agrees with our copy.
    if let Ok((old_chain, old_state)) = Box::pin(load_state(old, me, material)).await {
        if old_chain.len() > chain.len() {
            chain = old_chain;
            state = old_state;
        }
    }
    if chain.len() > relay_len {
        Box::pin(catch_relay_up(new, me, ids, &chain, relay_len)).await?;
        relay_len = chain.len();
    }
    let _ = relay_len;
    // The notice on the old relay carries the mover's arrival, or this phone's own when
    // the mover's isn't among them.
    let (notice_mover, notice_sig) = match signatures.get(&ms.mover) {
        Some(sig) => (ms.mover, sig.clone()),
        None => (me.public().sig_pk, ms.target.sign(me)),
    };
    let arrivals: Vec<([u8; 32], Vec<u8>)> = signatures
        .into_iter()
        .filter(|(pk, _)| state.descriptor.member(pk).is_some())
        .collect();
    let event = VaultEvent::VaultMoved {
        target: ms.target.clone(),
        arrivals,
        at: now(),
    };
    let bytes = event.to_bytes().map_err(proto)?;
    let mut entry_bytes = None;
    for _ in 0..10 {
        if switched_by(&state, &ms.target.move_id) {
            break;
        }
        state
            .check(me.public().sig_pk, &event)
            .map_err(NodeError::Invalid)?;
        let entry = LogEntry::create(me, &key, ids.vault, chain.len(), chain.head(), &bytes, rng)
            .map_err(proto)?;
        match Box::pin(append_entry(new, me, ids, &entry)).await? {
            AppendResult::Appended { .. } => {
                chain
                    .append(entry.clone(), &state.log_authors())
                    .map_err(proto)?;
                state.apply_entry(&entry, &key);
                save_copy(new, &chain);
                entry_bytes = Some(entry.to_bytes().map_err(proto)?);
                break;
            }
            AppendResult::Conflict { .. } => {
                Box::pin(fetch_rest(new, me, ids.label, &key, &mut chain, &mut state)).await?;
                save_copy(new, &chain);
            }
        }
    }
    if !switched_by(&state, &ms.target.move_id) {
        return Err(NodeError::Protocol("could not log the switch".into()));
    }
    ms.moved_seen = true;
    ms.switched = true;
    save_move(old, material, ms)?;
    if let Some(entry) = entry_bytes {
        let mut notice = ms.clone();
        notice.mover = notice_mover;
        ms.moved_notice_sent = Box::pin(send_notices(
            old,
            me,
            material,
            &state,
            &mut notice,
            Some(entry),
            notice_sig,
            rng,
        ))
        .await;
        save_move(old, material, ms)?;
    }
    let _ = old.forget_push(me, material.label()).await;
    Ok(())
}

/// Withdraws this phone from its open move: a sealed cancel to every other member on the
/// new relay (and, from the mover, on the old relay so the Home cards go). The mover's
/// cancel ends the move; anyone else's only voids their arrival. Can't undo a switch.
pub async fn cancel_move<R: RngCore + CryptoRng>(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    rng: &mut R,
) -> Result<(), NodeError> {
    let Some(mut ms) = move_state(relay, material) else {
        return Ok(());
    };
    if ms.switched || ms.moved_seen {
        return Err(NodeError::Protocol("the vault already moved".into()));
    }
    let state = load_copy(relay, material.descriptor.vault_id, &material.log_key())?
        .map(|(_, s)| s)
        .ok_or_else(|| NodeError::Protocol("this phone has no copy of the vault log".into()))?;
    // Saved first: from now on this phone counts and follows nothing for the move.
    ms.cancelled = true;
    save_move(relay, material, &ms)?;
    let msg = MoveMessage::Cancelled {
        move_id: ms.target.move_id,
    };
    let mut seq = SeqCounter::default();
    if ms.arrived {
        let new = relay.at(&ms.target.relay);
        for to in others(&state, me) {
            if let Err(e) = send_move_message(
                &new,
                me,
                ms.target.label,
                &to,
                Kind::MoveCancelled,
                &msg,
                &mut seq,
                rng,
            )
            .await
            {
                tracing::warn!("move cancel not sent on the new relay: {e}");
                break;
            }
        }
    }
    if ms.mover == me.public().sig_pk {
        let mut seq = SeqCounter::default();
        for to in others(&state, me) {
            if let Err(e) = send_move_message(
                relay,
                me,
                material.label(),
                &to,
                Kind::MoveCancelled,
                &msg,
                &mut seq,
                rng,
            )
            .await
            {
                tracing::warn!("move cancel not sent on the old relay: {e}");
                break;
            }
        }
    }
    Ok(())
}

/// Verifies a `VaultMoved` entry from a move notice against this phone's copy of the
/// log, without contacting the new relay: a current member wrote it, it decrypts to a
/// switch for `target` with m distinct current members' valid arrivals, and the point it
/// starts from is in this phone's history where the copy reaches it.
fn verify_moved_entry(
    entry: &[u8],
    target: &MoveTarget,
    state: &VaultState,
    copy: &Chain,
    key: &LogKey,
) -> bool {
    let Ok(entry) = LogEntry::from_bytes(entry) else {
        return false;
    };
    let Some(author) = state.descriptor.member(&entry.header.author) else {
        return false;
    };
    if entry.header.mailbox != state.descriptor.vault_id
        || entry.verify_signature(&author.identity).is_err()
    {
        return false;
    }
    let Ok((
        VaultEvent::VaultMoved {
            target: t,
            arrivals,
            ..
        },
        _,
    )) = entry
        .decrypt(key)
        .map_err(|_| ())
        .and_then(|b| VaultEvent::from_bytes_versioned(&b).map_err(|_| ()))
    else {
        return false;
    };
    if &t != target {
        return false;
    }
    if let Some(e) = copy.entries().get(target.base_index as usize) {
        if e.hash().ok() != Some(target.base_hash) {
            return false;
        }
    }
    let mut seen = BTreeSet::new();
    let valid = arrivals.iter().all(|(pk, sig)| {
        state
            .descriptor
            .member(pk)
            .is_some_and(|m| seen.insert(*pk) && t.verify(&m.identity, sig))
    });
    valid && seen.len() >= state.switch_threshold()
}

/// The move notices on the vault's current relay (no other relay is contacted): the
/// newest open move a member started, if any, and whether the vault already switched. A
/// notice whose `VaultMoved` verifies makes this phone stop writing to its relay (§4.5
/// rule 3) until it follows. Two switches from the same point of the history are
/// [`NodeError::TwoMoves`]. Notices are never acknowledged.
pub async fn move_notice(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
) -> Result<Option<MoveNotice>, NodeError> {
    let key = material.log_key();
    let Some((copy, state)) = load_copy(relay, material.descriptor.vault_id, &key)? else {
        return Ok(None);
    };
    let members = members_by_pk(&state);
    let mut notices: Vec<MoveNotice> = Vec::new();
    let mut cancelled: BTreeSet<[u8; 16]> = BTreeSet::new();
    let mut tokens: BTreeMap<[u8; 16], String> = BTreeMap::new();
    for (_, sender, env) in read_inbox(relay, me, material.label(), &members, 0).await? {
        if !matches!(env.header.kind, Kind::MoveNotice | Kind::MoveCancelled) {
            continue;
        }
        let Ok(msg) = env
            .open(me, &sender)
            .map_err(|_| ())
            .and_then(|b| MoveMessage::from_padded(&b).map_err(|_| ()))
        else {
            continue;
        };
        match msg {
            MoveMessage::Notice {
                target,
                mover,
                signature,
                create_token,
                moved,
            } => {
                let Some(m) = members.get(&mover) else {
                    continue;
                };
                if target.vault_id != material.descriptor.vault_id || !target.verify(m, &signature)
                {
                    continue;
                }
                let moved =
                    moved.is_some_and(|e| verify_moved_entry(&e, &target, &state, &copy, &key));
                if let Some(token) = create_token {
                    tokens.insert(target.move_id, token);
                }
                match notices.iter_mut().find(|n| n.target == target) {
                    Some(n) => n.moved |= moved,
                    None => notices.push(MoveNotice {
                        target,
                        mover,
                        moved,
                    }),
                }
            }
            MoveMessage::Cancelled { move_id } => {
                // Only the mover can call a move off for everyone.
                if notices
                    .iter()
                    .any(|n| n.target.move_id == move_id && n.mover == sender.sig_pk)
                {
                    cancelled.insert(move_id);
                }
            }
            MoveMessage::Arrival { .. } => {}
        }
    }
    // A switch from the same point as one this vault made (or as another notice's) means
    // a member signed two moves: follow neither.
    let same_base = |a: &MoveTarget, b: &MoveTarget| {
        a.move_id != b.move_id && a.base_index == b.base_index && a.base_hash == b.base_hash
    };
    if notices
        .iter()
        .filter(|n| n.moved)
        .any(|n| state.moves.iter().any(|m| same_base(&m.target, &n.target)))
    {
        return Err(NodeError::TwoMoves);
    }
    // Moves this vault already made, or older than its last switch, are history.
    let last = state.moved().map(|m| m.index);
    notices.retain(|n| {
        !switched_by(&state, &n.target.move_id)
            && last.is_none_or(|i| n.target.base_index >= i)
            && n.target.label != material.relay.label
    });
    let switched: Vec<&MoveNotice> = notices.iter().filter(|n| n.moved).collect();
    for (i, a) in switched.iter().enumerate() {
        if switched[i + 1..]
            .iter()
            .any(|b| same_base(&a.target, &b.target))
        {
            return Err(NodeError::TwoMoves);
        }
    }
    let pick = notices
        .iter()
        .filter(|n| n.moved || (!cancelled.contains(&n.target.move_id) && !n.target.expired(now())))
        .max_by_key(|n| (n.moved, n.target.expiry))
        .cloned();
    if let Some(n) = &pick {
        if n.moved {
            // Rule 3: this phone stops writing to the old relay until it follows.
            let ms = move_state(relay, material);
            let mut ms = match ms {
                Some(ms) if ms.target.move_id == n.target.move_id => ms,
                _ => MoveState {
                    target: n.target.clone(),
                    mover: n.mover,
                    arrived: false,
                    create_token: None,
                    notified: Vec::new(),
                    cancelled: false,
                    moved_seen: false,
                    switched: false,
                    moved_notice_sent: false,
                    arrival_delivered: false,
                    from: material.relay.clone(),
                },
            };
            if !ms.moved_seen {
                ms.moved_seen = true;
                ms.create_token = ms
                    .create_token
                    .or_else(|| tokens.get(&n.target.move_id).cloned());
                save_move(relay, material, &ms)?;
            }
        }
    }
    Ok(pick)
}

/// Where the next write of `material`'s vault goes (`append_event`'s move rules): `None`
/// for the relay the caller passed; `Some` for the new relay once this phone holds proof of
/// the switch, with `chain` and `state` replaced by the new relay's history. Refuses a new
/// entry on a new relay before its switch, and a write by a phone that knows the vault
/// moved but hasn't followed. Before a write to the relay the vault is on, the move notices
/// there are read: a phone that took no part in a switch (off while the others moved)
/// learns of it only from them, and a write after it would fork the history it has to
/// follow from.
pub(super) async fn write_target(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    chain: &mut Chain,
    state: &mut VaultState,
) -> Result<Option<(RelayClient, VaultIds)>, NodeError> {
    if let Some(target) = Box::pin(own_move_target(relay, me, material, chain, state)).await? {
        return Ok(Some(target));
    }
    match Box::pin(move_notice(relay, me, material)).await {
        Ok(Some(n)) if n.moved => Err(NodeError::VaultMoved {
            url: n.target.relay,
        }),
        // Two switches from one point: follow neither, so the vault stays here.
        Ok(_) | Err(NodeError::TwoMoves) => Ok(None),
        Err(e) => {
            tracing::warn!("could not read the move notices before writing: {e}");
            Ok(None)
        }
    }
}

/// [`write_target`] from this phone's own move: the rules for a phone that sent an
/// arrival or saw a verified switch.
async fn own_move_target(
    relay: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    chain: &mut Chain,
    state: &mut VaultState,
) -> Result<Option<(RelayClient, VaultIds)>, NodeError> {
    let Some(ms) = move_state(relay, material) else {
        return Ok(None);
    };
    let on_new = material.relay.label == ms.target.label;
    if on_new {
        // Rule 1: nothing new on the new relay before `VaultMoved` (catch-up copies existing
        // entries through another path).
        if !switched_by(state, &ms.target.move_id) && !ms.switched {
            return Err(NodeError::Protocol(
                "no new entries on the new relay before the vault switches".into(),
            ));
        }
        return Ok(None);
    }
    let moved = || NodeError::VaultMoved {
        url: ms.target.relay.clone(),
    };
    if !ms.switched {
        if ms.moved_seen && !ms.arrived {
            // Rule 3: it moved, and this phone hasn't followed.
            return Err(moved());
        }
        if !ms.moved_seen && (ms.cancelled || !ms.arrived || ms.target.expired(now())) {
            return Ok(None);
        }
    }
    // Rule 2: has the vault switched on the new relay?
    let link = MoveLink::of(&ms.target);
    match Box::pin(new_relay_history(relay, me, material, &link)).await {
        Ok((new_chain, new_state, _, new)) if switched_by(&new_state, &ms.target.move_id) => {
            let mut ms = ms;
            if !ms.switched {
                ms.moved_seen = true;
                ms.switched = true;
                save_move(relay, material, &ms)?;
                let _ = relay.forget_push(me, material.label()).await;
            }
            *chain = new_chain;
            *state = new_state;
            let new = new.with_create_token(ms.create_token.as_deref());
            Ok(Some((
                new,
                VaultIds {
                    vault: material.descriptor.vault_id,
                    label: ms.target.label,
                },
            )))
        }
        // Proof of the switch was seen, but the new relay doesn't show it: write nowhere.
        Ok(_) if ms.moved_seen || ms.switched => Err(moved()),
        Ok(_) => Ok(None),
        Err(e) if ms.switched || ms.moved_seen => {
            tracing::warn!("the vault moved, and its new relay can't be read: {e}");
            Err(moved())
        }
        // The old relay can't complete a payment after a switch (too few members are left
        // there), so a write that can't check the new relay goes ahead.
        Err(e) => {
            tracing::warn!("could not check the new relay before writing: {e}");
            Ok(None)
        }
    }
}
