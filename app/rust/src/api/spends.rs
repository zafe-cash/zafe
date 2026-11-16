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

#[derive(Clone, Debug)]
pub struct UnapprovedSpendInfo {
    pub txid: String,
    pub mined_height: u32,
}

#[derive(Clone, Debug)]
pub struct UnapprovedSpends {
    pub spends: Vec<UnapprovedSpendInfo>,
    pub from_copy_at: Option<u64>,
}

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
    if wallet.chain_height()?.is_none() {
        return Ok(none());
    }
    let spends = wallet.vault_spends()?;
    Ok(UnapprovedSpends {
        spends: spend_watch::unapproved_spends(&state, &spends)
            .into_iter()
            .map(|s| UnapprovedSpendInfo {
                txid: TxId::from_bytes(s.txid).to_string(),
                mined_height: s.mined_height.unwrap_or(0),
            })
            .collect(),
        from_copy_at,
    })
}
