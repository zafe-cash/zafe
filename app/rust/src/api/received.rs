use zafe_core::wallet::{VaultWallet, WalletError};

use super::{
    error::ZafeError,
    vault::{material, network, wallet_key, wallet_lock, wallet_path},
};

pub struct ReceivedInfo {
    pub txid: String,
    pub amount_zat: u64,
    pub mined_height: u32,
    pub block_time_secs: u32,
    pub confirmations: u32,
    pub memo: String,
    pub is_coinbase: bool,
}

pub fn list_received(
    db_dir: String,
    db_key: Vec<u8>,
    material: Vec<u8>,
) -> Result<Vec<ReceivedInfo>, ZafeError> {
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let key = wallet_key(&db_key)?;
    let path = wallet_path(&db_dir, &m);
    let _guard = wallet_lock();
    if !path.exists() {
        return Ok(vec![]);
    }
    let wallet = match VaultWallet::open(&path, &key, net) {
        Ok(w) => w,
        Err(WalletError::WrongKey) => return Ok(vec![]),
        Err(e) => return Err(e.into()),
    };
    let tip = wallet.chain_height()?.unwrap_or(0);
    Ok(wallet
        .received_payments()?
        .into_iter()
        .map(|p| {
            let mined = p.mined_height.unwrap_or(0);
            ReceivedInfo {
                txid: p.txid,
                amount_zat: p.amount_zat,
                mined_height: mined,
                block_time_secs: p.block_time.unwrap_or(0),
                confirmations: if mined == 0 || tip < mined {
                    0
                } else {
                    tip - mined + 1
                },
                memo: super::proposals::visible_text(&p.memos.join("\n\n")),
                is_coinbase: p.coinbase,
            }
        })
        .collect())
}
