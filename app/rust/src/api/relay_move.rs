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

pub struct RelayMoveStarted {
    pub link: String,
    pub host: String,
    pub relay_url: String,
}

pub struct RelayMoveStatus {
    pub relay_url: String,
    pub host: String,
    pub link: String,
    pub mover_hex: String,
    pub mine: bool,
    pub arrived: Vec<String>,
    pub needed: u32,
    pub members: u32,
    pub this_phone_arrived: bool,
    pub expired: bool,
    pub cancelled: bool,
    pub switched: bool,
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

pub struct RelayMoveNotice {
    pub relay_url: String,
    pub host: String,
    pub link: String,
    pub mover_hex: String,
    pub arrived: u32,
    pub needed: u32,
    pub members: u32,
    pub moved: bool,
}

pub struct MoveLinkInfo {
    pub vault_id: String,
    pub relay_url: String,
    pub host: String,
    pub link: String,
}

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

pub fn cancel_relay_move(relay_url: String, seeds: Vec<u8>, material: Vec<u8>) -> Result<()> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    runtime().block_on(Box::pin(node::cancel_move(&relay, &me, &m, &mut OsRng)))?;
    Ok(())
}

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

#[flutter_rust_bridge::frb(sync)]
pub fn move_share_text(vault_name: String, host: String, link: String) -> String {
    format!("I'm moving our Zafe vault {vault_name} to {host}. Open this to move too: {link}")
}
