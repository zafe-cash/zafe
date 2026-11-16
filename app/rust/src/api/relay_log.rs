use zafe_core::node::{self, Reseeded};

use super::{
    error::ZafeError,
    vault::{identity, material, runtime, vault_relay},
};

pub fn restore_relay(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<u32, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    let out = runtime().block_on(node::reseed_relay(&relay, &me, m.ids(), &m.log_key()))?;
    Ok(match out {
        Reseeded::Restored { entries } => u32::try_from(entries).unwrap_or(u32::MAX),
        Reseeded::CaughtUp { appended } => u32::try_from(appended).unwrap_or(u32::MAX),
        Reseeded::Current => 0,
    })
}

pub fn follow_relay(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<u32, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = vault_relay(&relay_url, &m);
    let out = runtime().block_on(node::follow_relay(&relay, &me, m.ids(), &m.log_key()))?;
    Ok(u32::try_from(out.dropped).unwrap_or(u32::MAX))
}
