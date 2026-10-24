//! Unapproved-spend alert (spec §10.4.4): vault spends the log doesn't account for.

use zafe_core::{
    node, spend_watch,
    wallet::{VaultWallet, WalletError},
};
use zcash_protocol::TxId;

use super::{
    error::ZafeError,
    vault::{
        identity, material, network, runtime, vault_relay, wallet_key, wallet_lock, wallet_path,
    },
};

/// A transaction that spent vault notes with no matching approved proposal in the log:
/// keys were used outside Zafe (old shares, or compromised phones).
#[derive(Clone, Debug)]
pub struct UnapprovedSpendInfo {
    /// Transaction id (hex, display order).
    pub txid: String,
    /// 0 while unmined.
    pub mined_height: u32,
}

/// What the alert found, and what it was based on.
#[derive(Clone, Debug)]
pub struct UnapprovedSpends {
    pub spends: Vec<UnapprovedSpendInfo>,
    /// Set when the relay couldn't be reached and the check ran on this phone's copy of
    /// the vault history: when that copy last matched the relay (Unix seconds), for "based
    /// on this phone's history from 09:14". A spend a newer entry would explain may show
    /// until the relay is back.
    pub from_copy_at: Option<u64>,
}

/// The vault's spends the log doesn't account for (empty when all is well, or before the
/// first sync). Reads the wallet database `sync_vault` keeps and the vault log; call it
/// after a sync, so the wallet has seen the chain. Keeps working while the relay can't be
/// reached, from this phone's log copy (`from_copy_at`).
pub fn unapproved_spends(
    relay_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<UnapprovedSpends, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let key = wallet_key(&db_key)?;
    let path = wallet_path(&db_dir, &m);
    let relay = vault_relay(&relay_url, &m);
    let (state, from_copy_at) = match runtime().block_on(node::load_state(&relay, &me, &m)) {
        Ok((_, state)) => (state, None),
        Err(e) if node::relay_unreachable(&e) => match node::state_from_copy(&relay, &m)? {
            Some((state, at)) => (state, Some(at)),
            None => return Err(e.into()),
        },
        Err(e) => return Err(e.into()),
    };
    let none = || UnapprovedSpends {
        spends: vec![],
        from_copy_at,
    };
    let _guard = wallet_lock();
    if !path.exists() {
        return Ok(none());
    }
    let wallet = match VaultWallet::open(&path, &key, net) {
        Ok(w) => w,
        Err(WalletError::WrongKey) => return Ok(none()),
        Err(e) => return Err(e.into()),
    };
    let Some(tip) = wallet.chain_height()? else {
        return Ok(none());
    };
    let spends = wallet.vault_spends()?;
    Ok(UnapprovedSpends {
        spends: spend_watch::unapproved_spends(&state, &spends, tip)
            .into_iter()
            .map(|s| UnapprovedSpendInfo {
                txid: TxId::from_bytes(s.txid).to_string(),
                mined_height: s.mined_height.unwrap_or(0),
            })
            .collect(),
        from_copy_at,
    })
}
