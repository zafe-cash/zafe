//! Times the first sync of a brand-new vault wallet on a live lightwalletd (testnet by
//! default), phase by phase: connect, create (tree state + encrypted DB + migrations +
//! account import), then sync. A dev tool, not part of the product:
//!   cargo run -p zafe-core --example first_sync_timing [lightwalletd url]

use std::time::Instant;

use orchard::keys::{FullViewingKey, SpendingKey};
use zafe_core::{
    keys::{VaultKeys, VaultSecret},
    wallet::{connect, VaultWallet, WalletKey, ZafeNetwork},
};
use zcash_client_backend::proto::service;

#[tokio::main]
async fn main() {
    let url = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "https://testnet.zec.rocks:443".into());
    let t0 = Instant::now();
    let lap = |what: &str| println!("{:>7.1}s  {what}", t0.elapsed().as_secs_f32());

    let mut client = connect(&url).await.unwrap();
    lap("connected");
    let tip = client
        .get_latest_block(service::ChainSpec::default())
        .await
        .unwrap()
        .into_inner()
        .height as u32;
    lap(&format!("tip {tip}"));

    let ak: [u8; 32] = FullViewingKey::from(&SpendingKey::from_bytes([42; 32]).unwrap()).to_bytes()
        [..32]
        .try_into()
        .unwrap();
    let keys = VaultKeys::derive(&VaultSecret::from_bytes([1; 32]), &ak).unwrap();
    let ufvk = keys.ufvk().unwrap();

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("wallet.sqlite");
    let key = WalletKey::random();
    let mut wallet = VaultWallet::create(
        &path,
        &key,
        ZafeNetwork::Test,
        "timing",
        &ufvk,
        tip + 1,
        &mut client,
    )
    .await
    .unwrap();
    lap("create (tree state, encrypted db, migrations, account import)");

    wallet.sync(&mut client).await.unwrap();
    lap(&format!(
        "first sync done, chain height {:?}",
        wallet.chain_height()
    ));
    wallet.sync(&mut client).await.unwrap();
    lap("second sync done");
    println!(
        "db size: {} KiB",
        std::fs::metadata(&path).unwrap().len() / 1024
    );
}
