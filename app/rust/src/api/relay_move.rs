//! Moving a vault to another relay (`docs/relay-move.md` §4.2-4.5): start, follow,
//! status, cancel, and the move link.
//!
//! Privacy rule: nothing here connects to a new relay unless the member asked for it.
//! [`relay_move_notice`] and [`parse_move_link`] read only the vault's current relay (or
//! nothing); [`follow_relay_move`] is called only after the member tapped Move.

use rand::rngs::OsRng;
use zafe_core::{
    moves::{relay_host, MoveLink},
    node::{self, MoveStatus},
};

use super::{
    error::ZafeError,
    vault::{identity, material, runtime, vault_relay},
};

type Result<T, E = ZafeError> = std::result::Result<T, E>;

/// A move this phone started.
pub struct RelayMoveStarted {
    /// `https://zafe.cash/move#…` (share it with the group).
    pub link: String,
    /// `relay.dao.org`.
    pub host: String,
    pub relay_url: String,
}

/// Where a move stands on this phone.
pub struct RelayMoveStatus {
    pub relay_url: String,
    pub host: String,
    pub link: String,
    /// Who started it (signing key, hex).
    pub mover_hex: String,
    /// This phone started it.
    pub mine: bool,
    /// Members known to have moved (hex keys, descriptor order; this phone included once
    /// it moved).
    pub arrived: Vec<String>,
    /// m: "Payments switch when m of n have moved."
    pub needed: u32,
    pub members: u32,
    pub this_phone_arrived: bool,
    pub expired: bool,
    pub cancelled: bool,
    /// The vault is on the new relay and this phone follows it.
    pub switched: bool,
    /// The vault switched but this phone hasn't followed: "Move to continue."
    pub moved_elsewhere: bool,
}

impl From<MoveStatus> for RelayMoveStatus {
    fn from(s: MoveStatus) -> Self {
        Self {
            link: MoveLink::of(&s.target).encode(),
            host: s.target.host(),
            relay_url: s.target.relay.clone(),
            mover_hex: hex::encode(s.mover),
            mine: s.mine,
            arrived: s.arrived.iter().map(hex::encode).collect(),
            needed: s.needed as u32,
            members: s.members as u32,
            this_phone_arrived: s.this_phone_arrived,
            expired: s.expired,
            cancelled: s.cancelled,
            switched: s.switched,
            moved_elsewhere: s.moved_elsewhere,
        }
    }
}

/// A move announced on the vault's current relay (the Home card).
pub struct RelayMoveNotice {
    pub relay_url: String,
    pub host: String,
    pub link: String,
    pub mover_hex: String,
    /// Members known to have moved, as far as the notice tells (the mover; everyone in the
    /// switch once it happened).
    pub arrived: u32,
    pub needed: u32,
    pub members: u32,
    /// The vault already switched there: "Grants moved to relay.dao.org. Move to continue."
    pub moved: bool,
}

/// What a move link says (nothing is checked or contacted yet).
pub struct MoveLinkInfo {
    pub vault_id: String,
    pub relay_url: String,
    pub host: String,
    /// The link, normalized (pass it to [`follow_relay_move`]).
    pub link: String,
}

/// Reads a move link (`https://zafe.cash/move#…`, or a message containing it). Contacts
/// nothing.
#[flutter_rust_bridge::frb(sync)]
pub fn parse_move_link(text: String) -> Result<MoveLinkInfo> {
    let link = MoveLink::decode(&text).map_err(|_| ZafeError::invalid("not a move link"))?;
    Ok(MoveLinkInfo {
        vault_id: hex::encode(link.vault_id),
        host: relay_host(&link.relay),
        relay_url: link.relay.clone(),
        link: link.encode(),
    })
}

/// Starts moving the vault to `new_relay_url` (call after unlock): the vault is restored
/// there from this phone's history, this phone's arrival goes there, and move notices go
/// to the others on the current relay. Fails with `MembersMustUpdate` (listing them) while
/// a member's app is older than moves, `MoveInProgress` while this phone is in another
/// move, `CreateTokenRefused` for a private relay without the right token.
pub fn start_relay_move(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    new_relay_url: String,
    create_token: Option<String>,
) -> Result<RelayMoveStarted> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    let started = runtime().block_on(Box::pin(node::move_vault(
        &relay,
        &me,
        &m,
        &new_relay_url,
        create_token.as_deref(),
        &mut OsRng,
    )))?;
    Ok(RelayMoveStarted {
        link: started.link,
        host: started.target.host(),
        relay_url: started.target.relay,
    })
}

/// Follows a move from its link: **only after the member tapped Move** (with unlock). This
/// is the first time the new relay is contacted. Refuses (`MoveNotFound`, `MoveExpired`,
/// `MoveStale`, `RelayForked`, `RelayMembership`, `MoveInProgress`, `TwoMoves`) unless a
/// current member's arrival for this move is there and the relay's history checks out.
pub fn follow_relay_move(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    link: String,
) -> Result<RelayMoveStatus> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let link = MoveLink::decode(&link).map_err(|_| ZafeError::invalid("not a move link"))?;
    let relay = vault_relay(&relay_url, &m);
    let status = runtime().block_on(Box::pin(node::follow_move(
        &relay, &me, &m, &link, &mut OsRng,
    )))?;
    Ok(status.into())
}

/// This phone's move, moved on: arrivals counted on the new relay (only when this phone
/// moved there itself), the switch made at m. `None`: no move.
pub fn relay_move_status(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<Option<RelayMoveStatus>> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    let status = runtime().block_on(Box::pin(node::move_progress(&relay, &me, &m, &mut OsRng)))?;
    Ok(status.map(Into::into))
}

/// Withdraws this phone from its move (the mover: calls the move off).
pub fn cancel_relay_move(relay_url: String, seeds: Vec<u8>, material: Vec<u8>) -> Result<()> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    runtime().block_on(Box::pin(node::cancel_move(&relay, &me, &m, &mut OsRng)))?;
    Ok(())
}

/// A move notice on the vault's current relay, for the Home card (reads only that relay).
/// `None` when there is none, it was cancelled or expired, or this phone is already in it.
pub fn relay_move_notice(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<Option<RelayMoveNotice>> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    let notice = runtime().block_on(Box::pin(node::move_notice(&relay, &me, &m)))?;
    let mine = node::move_state(&relay, &m);
    let Some(n) = notice else { return Ok(None) };
    if mine
        .as_ref()
        .is_some_and(|ms| ms.target.move_id == n.target.move_id && (ms.arrived || ms.cancelled))
        && !n.moved
    {
        return Ok(None);
    }
    let state = node::state_from_copy(&relay, &m)?.map(|(s, _)| s);
    let (needed, members) = state.as_ref().map_or((0, 0), |s| {
        (
            s.switch_threshold() as u32,
            s.descriptor.members.len() as u32,
        )
    });
    Ok(Some(RelayMoveNotice {
        link: MoveLink::of(&n.target).encode(),
        host: n.target.host(),
        relay_url: n.target.relay.clone(),
        mover_hex: hex::encode(n.mover),
        arrived: if n.moved { needed } else { 1 },
        needed,
        members,
        moved: n.moved,
    }))
}

/// Members (hex keys) whose app must update before the vault can move: from the vault's
/// relay, or this phone's copy when it can't be reached.
pub fn relay_move_blockers(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<Vec<String>> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    let state = match runtime().block_on(node::load_state(&relay, &me, &m)) {
        Ok((_, state)) => state,
        Err(_) => match node::state_from_copy(&relay, &m)? {
            Some((state, _)) => state,
            None => return Ok(Vec::new()),
        },
    };
    let mine = me.public().sig_pk;
    Ok(state
        .members_needing_update()
        .into_iter()
        .filter(|pk| *pk != mine)
        .map(hex::encode)
        .collect())
}

/// The move link text the mover shares (§4.2).
#[flutter_rust_bridge::frb(sync)]
pub fn move_share_text(vault_name: String, host: String, link: String) -> String {
    format!("I'm moving our Zafe vault {vault_name} to {host}. Open this to move too: {link}")
}
