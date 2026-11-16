use super::*;
use crate::moves::{
    normalize_relay_url, unix_now, MoveLink, MoveMessage, MoveState, MoveTarget, MOVE_LIFETIME_SECS,
};

#[derive(Clone, Debug)]
pub struct MoveStarted {
    pub target: MoveTarget,
    pub link: String,
    pub notified: Vec<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveStatus {
    pub target: MoveTarget,
    pub mover: [u8; 32],
    pub mine: bool,
    pub arrived: Vec<[u8; 32]>,
    pub needed: usize,
    pub members: usize,
    pub this_phone_arrived: bool,
    pub expired: bool,
    pub cancelled: bool,
    pub switched: bool,
    pub moved_elsewhere: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveNotice {
    pub target: MoveTarget,
    pub mover: [u8; 32],
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

pub fn followed_relay(material: &VaultMaterial) -> Option<VaultRelay> {
    let cache = crate::log_cache::configured()?;
    followed_relay_in(cache.dir(), material)
}

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

pub fn with_followed_relay(material: &VaultMaterial) -> VaultMaterial {
    let mut m = material.clone();
    if let Some(relay) = followed_relay(material) {
        m.relay = relay;
    }
    m
}

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
    send_fresh(relay, me, seq, |n| {
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
    let mut label = [0u8; 16];
    rng.fill_bytes(&mut label);
    let move_id = crate::moves::move_id_for(&me.public().sig_pk, &label);
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

#[derive(Default)]
struct Arrivals {
    target: Option<MoveTarget>,
    by: BTreeMap<[u8; 32], Vec<u8>>,
    mover: Option<[u8; 32]>,
    token: Option<String>,
    cancelled: BTreeSet<[u8; 32]>,
}

impl Arrivals {
    fn mover_arrived(&self) -> bool {
        self.mover.is_some_and(|m| self.by.contains_key(&m))
    }

    fn withdraw(&mut self, cancelled: impl IntoIterator<Item = [u8; 32]>) {
        self.cancelled.extend(cancelled);
        let gone = &self.cancelled;
        self.by.retain(|pk, _| !gone.contains(pk));
    }
}

fn mover_of(state: &VaultState, target: &MoveTarget) -> Option<[u8; 32]> {
    let members: Vec<[u8; 32]> = state.member_identities().iter().map(|m| m.sig_pk).collect();
    target.mover_among(&members)
}

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
        Err(NodeError::Relay(RelayClientError::Status {
            status: 403 | 404, ..
        })) => return Ok(out),
        Err(e) => return Err(e),
    };
    let mut cancelled = BTreeSet::new();
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
                if out.target.is_none() {
                    out.mover = mover_of(state, &target);
                    out.target = Some(target);
                }
                if out.mover == Some(sender.sig_pk) && create_token.is_some() {
                    out.token = create_token;
                }
                out.by.insert(sender.sig_pk, signature);
            }
            MoveMessage::Cancelled { move_id } if move_id == link.move_id => {
                cancelled.insert(sender.sig_pk);
            }
            _ => {}
        }
    }
    out.withdraw(cancelled);
    Ok(out)
}

async fn cancels_on_old(
    old: &RelayClient,
    me: &Identity,
    material: &VaultMaterial,
    state: &VaultState,
    move_id: &[u8; 16],
) -> BTreeSet<[u8; 32]> {
    let members = members_by_pk(state);
    let inbox = match read_inbox(old, me, material.label(), &members, 0).await {
        Ok(inbox) => inbox,
        Err(e) => {
            tracing::warn!("move cancels on the old relay not read: {e}");
            return BTreeSet::new();
        }
    };
    inbox
        .into_iter()
        .filter(|(_, _, env)| env.header.kind == Kind::MoveCancelled)
        .filter_map(|(_, sender, env)| {
            let msg = MoveMessage::from_padded(&env.open(me, &sender).ok()?).ok()?;
            matches!(msg, MoveMessage::Cancelled { move_id: id } if &id == move_id)
                .then_some(sender.sig_pk)
        })
        .collect()
}

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
    let switched = switched_by(&their_state, &link.move_id);
    let mut replace = false;
    let (chain, state) = match load_copy(old, ids.vault, &key)? {
        None => (theirs, their_state),
        Some((ours, our_state)) => {
            let mut part = None;
            for (i, (a, b)) in ours.entries().iter().zip(theirs.entries()).enumerate() {
                if a.hash().map_err(proto)? != b.hash().map_err(proto)? {
                    part = Some(i as u64);
                    break;
                }
            }
            match part {
                None if ours.len() > theirs.len() => (ours, our_state),
                None => (theirs, their_state),
                Some(i) => {
                    let voids = their_state.moves.iter().any(|m| {
                        m.target.move_id == link.move_id && m.index >= i && m.target.base_index < i
                    });
                    if !voids {
                        return Err(if switched {
                            NodeError::RelayForked { index: i }
                        } else {
                            NodeError::MoveConflict
                        });
                    }
                    if !droppable(
                        &ours.entries()[i as usize..],
                        &key,
                        &our_state,
                        &their_state,
                    ) {
                        return Err(NodeError::RelayForked { index: i });
                    }
                    replace = true;
                    (theirs, their_state)
                }
            }
        }
    };
    Box::pin(check_relay_membership(&bare, me, link.label, &state)).await?;
    if replace {
        if let Some(cache) = old.log_cache() {
            cache.replace(&ids.vault, chain.entries())?;
        }
    } else if switched && chain.len() == relay_len {
        save_copy(old, &chain);
    }
    Ok((chain, state, relay_len, new))
}

fn droppable(dropped: &[LogEntry], key: &LogKey, ours: &VaultState, theirs: &VaultState) -> bool {
    let intent_only = dropped.iter().all(|e| {
        let Ok(bytes) = e.decrypt(key) else {
            return false;
        };
        match VaultEvent::from_bytes_versioned(&bytes) {
            Ok((
                VaultEvent::Name { .. }
                | VaultEvent::BackupVerified { .. }
                | VaultEvent::Commitments { .. }
                | VaultEvent::Proposal { .. }
                | VaultEvent::Cancelled { .. },
                _,
            )) => true,
            Ok((VaultEvent::Vote { shares, .. }, _)) => shares.is_empty(),
            _ => false,
        }
    });
    let signed_kept = ours
        .proposals
        .iter()
        .filter(|(_, p)| p.possibly_signed)
        .all(|(id, _)| theirs.proposals.get(id).is_some_and(|p| p.possibly_signed));
    let members = |s: &VaultState| -> BTreeSet<[u8; 32]> {
        s.member_identities().iter().map(|m| m.sig_pk).collect()
    };
    let moves_kept = ours
        .moves
        .iter()
        .all(|m| switched_by(theirs, &m.target.move_id));
    intent_only && signed_kept && moves_kept && members(ours) == members(theirs)
}

fn switched_by(state: &VaultState, move_id: &[u8; 16]) -> bool {
    state.moves.iter().any(|m| &m.target.move_id == move_id)
}

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
            Err(NodeError::Relay(RelayClientError::Status {
                status: 403 | 404, ..
            })) => return Err(NodeError::MoveNotFound),
            Err(e) => return Err(e),
        };
    let ids = VaultIds {
        vault: material.descriptor.vault_id,
        label: link.label,
    };
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
                mover: mover_of(&state, &m.target).unwrap_or(m.author),
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
    if existing
        .as_ref()
        .is_some_and(|ms| ms.target.move_id == link.move_id && ms.cancelled)
    {
        return Err(NodeError::MoveCancelled);
    }
    let mut arrivals = Box::pin(read_arrivals(&new, me, &state, link)).await?;
    arrivals.withdraw(Box::pin(cancels_on_old(relay, me, material, &state, &link.move_id)).await);
    let (Some(target), Some(mover)) = (arrivals.target.clone(), arrivals.mover) else {
        return Err(NodeError::MoveNotFound);
    };
    if !arrivals.mover_arrived() {
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
    if let Some(entry) = copy.entries().get(target.base_index as usize) {
        if entry.hash().map_err(proto)? != target.base_hash {
            return Err(NodeError::RelayForked {
                index: target.base_index,
            });
        }
    }
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
    let mut arrivals = Box::pin(read_arrivals(&new, me, &state, &link)).await?;
    arrivals.withdraw(
        Box::pin(cancels_on_old(
            relay,
            me,
            material,
            &state,
            &ms.target.move_id,
        ))
        .await,
    );
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
    if let Ok((old_chain, old_state)) = Box::pin(load_state(old, me, material)).await {
        for (a, b) in old_chain.entries().iter().zip(chain.entries()) {
            if a.hash().map_err(proto)? != b.hash().map_err(proto)? {
                return Err(NodeError::MoveConflict);
            }
        }
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
    let notice_sig = match signatures.get(&ms.mover) {
        Some(sig) => sig.clone(),
        None => ms.target.sign(me),
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
    {
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
    let mut cancels: Vec<([u8; 16], [u8; 32])> = Vec::new();
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
                if target.vault_id != material.descriptor.vault_id {
                    continue;
                }
                let moved =
                    moved.is_some_and(|e| verify_moved_entry(&e, &target, &state, &copy, &key));
                let named = mover_of(&state, &target);
                let signed_by_mover = named == Some(mover)
                    && members
                        .get(&mover)
                        .is_some_and(|m| target.verify(m, &signature));
                if !moved && !signed_by_mover {
                    continue;
                }
                let Some(mover) = named.or(moved.then_some(mover)) else {
                    continue;
                };
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
            MoveMessage::Cancelled { move_id } => cancels.push((move_id, sender.sig_pk)),
            MoveMessage::Arrival { .. } => {}
        }
    }
    let cancelled: BTreeSet<[u8; 16]> = cancels
        .into_iter()
        .filter(|(id, from)| {
            notices
                .iter()
                .any(|n| &n.target.move_id == id && &n.mover == from)
        })
        .map(|(id, _)| id)
        .collect();
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
        Ok(_) | Err(NodeError::TwoMoves) => Ok(None),
        Err(e) if relay_unreachable(&e) => Err(e),
        Err(e) => {
            tracing::warn!("could not read the move notices before writing: {e}");
            Err(NodeError::MoveUnconfirmed {
                url: relay.base_url().to_owned(),
            })
        }
    }
}

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
    let standing = ms.arrived && !ms.cancelled;
    if !ms.switched {
        if ms.moved_seen && !standing {
            return Err(moved());
        }
        if !ms.moved_seen && (!standing || ms.target.expired(now())) {
            return Ok(None);
        }
    }
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
        Ok(_) if ms.moved_seen || ms.switched => Err(moved()),
        Ok(_) => Ok(None),
        Err(e) if ms.switched || ms.moved_seen => {
            tracing::warn!("the vault moved, and its new relay can't be read: {e}");
            Err(moved())
        }
        Err(NodeError::MoveConflict) => Ok(None),
        Err(e) => {
            tracing::warn!("could not check the new relay before writing: {e}");
            Err(NodeError::MoveUnconfirmed {
                url: ms.target.relay.clone(),
            })
        }
    }
}
