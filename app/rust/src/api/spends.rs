//! Unapproved-spend alert (spec §10.4.4): vault spends the log doesn't account for.

use zafe_core::{
    node,
    relay_client::RelayClient,
    spend_watch,
    wallet::{VaultWallet, WalletError},
};
use zcash_protocol::TxId;

use super::{
    error::ZafeError,
    vault::{identity, material, network, runtime, wallet_key, wallet_lock, wallet_path},
};

/// A transaction that spent vault notes with no matching approved proposal in the log:
/// keys were used outside Zafe (old shares, or compromised phones).
pub struct UnapprovedSpendInfo {
    /// Transaction id (hex, display order).
    pub txid: String,
    /// 0 while unmined.
    pub mined_height: u32,
}

/// The vault's spends the log doesn't account for (empty when all is well, or before the
/// first sync). Reads the wallet database `sync_vault` keeps and the vault log; call it
/// after a sync, so the wallet has seen the chain.
pub fn unapproved_spends(
    relay_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<Vec<UnapprovedSpendInfo>, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let key = wallet_key(&db_key)?;
    let path = wallet_path(&db_dir, &m);
    let (_, state) = runtime().block_on(node::load_state(&RelayClient::new(relay_url), &me, &m))?;
    let _guard = wallet_lock();
    if !path.exists() {
        return Ok(vec![]);
    }
    let wallet = match VaultWallet::open(&path, &key, net) {
        Ok(w) => w,
        Err(WalletError::WrongKey) => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let Some(tip) = wallet.chain_height()? else {
        return Ok(vec![]);
    };
    let spends = wallet.vault_spends()?;
    Ok(spend_watch::unapproved_spends(&state, &spends, tip)
        .into_iter()
        .map(|s| UnapprovedSpendInfo {
            txid: TxId::from_bytes(s.txid).to_string(),
            mined_height: s.mined_height.unwrap_or(0),
        })
        .collect())
}
