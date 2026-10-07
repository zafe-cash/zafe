//! Restoring the vault log on a relay that lost it (spec §6.3), from this device's copy.

use zafe_core::{
    node::{self, Reseeded},
    relay_client::RelayClient,
};

use super::{
    error::ZafeError,
    vault::{identity, material, runtime},
};

/// Restores the vault's log on the relay from this phone's copy: recreates the vault
/// after a wiped relay, or appends the entries a rewound relay lost. Returns how many log
/// entries were put back (0: the relay already had everything this phone has). Refuses a
/// relay whose history differs from this phone's copy (`RelayForked`). Messages that were
/// in flight (signing requests) are not restored: members are asked again. Re-register
/// the push token afterwards.
pub fn restore_relay(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<u32, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = RelayClient::new(relay_url);
    let out = runtime().block_on(node::reseed_relay(
        &relay,
        &me,
        m.descriptor.vault_id,
        &m.log_key(),
    ))?;
    Ok(match out {
        Reseeded::Restored { entries } => u32::try_from(entries).unwrap_or(u32::MAX),
        Reseeded::CaughtUp { appended } => u32::try_from(appended).unwrap_or(u32::MAX),
        Reseeded::Current => 0,
    })
}

/// Leaves a fork (spec §6.3): after `RelayForked`, discards this phone's copy of the vault
/// log and follows the relay's history instead. The relay's whole log is verified first;
/// entries that only this phone's copy has are dropped. Returns how many. Refuses when the
/// relay does not actually show another history. Ask the user to confirm first.
pub fn follow_relay(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<u32, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let relay = RelayClient::new(relay_url);
    let out = runtime().block_on(node::follow_relay(
        &relay,
        &me,
        m.descriptor.vault_id,
        &m.log_key(),
    ))?;
    Ok(u32::try_from(out.dropped).unwrap_or(u32::MAX))
}
