//! The vault's history as CSV (spec §11.3), generated on this device.

use zafe_core::{
    history, node,
    relay_client::RelayClient,
    wallet::{VaultWallet, WalletError},
};

use super::{
    error::ZafeError,
    names::{to_map, SignerName},
    vault::{identity, material, network, runtime, wallet_key, wallet_lock, wallet_path},
};

/// CSV of every payment the vault sent (from the vault log) and received (from this
/// device's wallet database), oldest first. Before the first sync the wallet part is empty.
/// Proposers and approvers this device named show as "Name (hexkey)".
pub fn export_history_csv(
    relay_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    seeds: Vec<u8>,
    material: Vec<u8>,
    names: Vec<SignerName>,
) -> Result<String, ZafeError> {
    let names = to_map(names);
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let (_, state) =
        runtime().block_on(node::load_state(&RelayClient::pooled(relay_url), &me, &m))?;

    let key = wallet_key(&db_key)?;
    let path = wallet_path(&db_dir, &m);
    let _guard = wallet_lock();
    let wallet = if path.exists() {
        match VaultWallet::open(&path, &key, net) {
            Ok(w) => Some(w),
            Err(WalletError::WrongKey) => None,
            Err(e) => return Err(e.into()),
        }
    } else {
        None
    };
    let mut rows = history::sent_rows(&state, &names, |txid| {
        wallet
            .as_ref()
            .and_then(|w| w.mined_time(txid).ok().flatten())
    });
    if let Some(w) = &wallet {
        rows.extend(history::received_rows(&w.received_payments()?));
    }
    Ok(history::to_csv(rows))
}
