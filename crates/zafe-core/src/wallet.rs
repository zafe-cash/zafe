use std::{
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use pczt::Pczt;
use rand::rngs::OsRng;
use tonic::transport::{Channel, ClientTlsConfig};
use zcash_address::ZcashAddress;
use zcash_client_backend::{
    data_api::{
        chain::{error as chain_error, BlockCache, BlockSource},
        locking::LockError,
        locking::LockFilter,
        scanning::ScanRange,
        wallet::TargetHeight,
        wallet::{
            create_pczt_from_proposal, decrypt_and_store_transaction,
            input_selection::{
                GreedyInputSelector, GreedyInputSelectorError, LockedInputPolicy, SpendPolicy,
            },
            propose_send_max_transfer, ConfirmationsPolicy,
        },
        Account as _, AccountBirthday, AccountPurpose, InputSource, MaxSpendMode, OutputLockStore,
        ReceivedNotes, TargetValue, WalletRead, WalletWrite,
    },
    fees::{standard::MultiOutputChangeStrategy, DustOutputPolicy, SplitPolicy, StandardFeeRule},
    proto::{
        compact_formats::CompactBlock,
        service::{self, compact_tx_streamer_client::CompactTxStreamerClient},
    },
    sync,
    wallet::{LockOwner, OutputRef, OvkPolicy, ReceivedNote},
};
use zcash_client_sqlite::{util::SystemClock, wallet::init::init_wallet_db, AccountUuid, WalletDb};
use zcash_keys::keys::UnifiedFullViewingKey;
use zcash_primitives::transaction::{builder::BundlePadding, TxId};
use zcash_protocol::{
    consensus::{BlockHeight, Parameters},
    local_consensus::LocalNetwork,
    memo::MemoBytes,
    value::Zatoshis,
    PoolType, ShieldedPool,
};
use zip321::{Payment, TransactionRequest};

pub type Client = CompactTxStreamerClient<Channel>;

pub const BUILD_LAG_BLOCKS: u32 = 3;

#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("wallet database: {0}")]
    Db(String),
    #[error("lightwalletd: {message}")]
    Remote {
        failure: crate::net::NetFailure,
        message: String,
    },
    #[error("lightwalletd serves {server}, the vault is on {expected}")]
    WrongNetwork { server: String, expected: String },
    #[error("lightwalletd is at block {server_tip}, behind this wallet ({wallet_tip})")]
    ServerBehind { server_tip: u32, wallet_tip: u32 },
    #[error("this version of Zafe doesn't support the network's current rules: {0}")]
    AppOutdated(String),
    #[error("sync: {0}")]
    Sync(String),
    #[error("proposal: {0}")]
    Proposal(String),
    #[error("invalid payment: {0}")]
    Payment(String),
    #[error("not enough unreserved funds: part of the balance is held by open proposals")]
    FundsReserved,
    #[error("a cancelled payment's funds were just moved: try again in a few minutes")]
    NotesSpent,
    #[error("wallet database: wrong key or not encrypted")]
    WrongKey,
}

fn db_err(e: impl core::fmt::Debug) -> WalletError {
    WalletError::Db(format!("{e:?}"))
}

#[derive(Clone)]
pub struct WalletKey(zeroize::Zeroizing<[u8; 32]>);

impl WalletKey {
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(zeroize::Zeroizing::new(bytes))
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, WalletError> {
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| WalletError::Db("wallet key must be 32 bytes".into()))?;
        Ok(Self::from_bytes(bytes))
    }

    pub fn derive(identity_seeds: &[u8], vault_id: &str) -> Self {
        let mut hasher = blake3::Hasher::new_derive_key("Zafe wallet database key v1");
        hasher.update(&(identity_seeds.len() as u64).to_le_bytes());
        hasher.update(identity_seeds);
        hasher.update(vault_id.as_bytes());
        Self::from_bytes(*hasher.finalize().as_bytes())
    }

    pub fn random() -> Self {
        let mut bytes = [0u8; 32];
        rand::RngCore::fill_bytes(&mut OsRng, &mut bytes);
        Self::from_bytes(bytes)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl core::fmt::Debug for WalletKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("WalletKey(..)")
    }
}

fn open_connection(
    path: &Path,
    key: &WalletKey,
    read_only: bool,
) -> Result<rusqlite::Connection, WalletError> {
    use rusqlite::OpenFlags;
    let flags = if read_only {
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX
    } else {
        OpenFlags::default()
    };
    let conn = rusqlite::Connection::open_with_flags(path, flags).map_err(db_err)?;
    let pragma = zeroize::Zeroizing::new(format!(
        "PRAGMA key = \"x'{}'\";",
        hex::encode(key.as_bytes())
    ));
    conn.execute_batch(&pragma).map_err(db_err)?;
    let cipher: Option<String> = conn
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .ok();
    if cipher.is_none() {
        return Err(WalletError::Db("SQLCipher is not available".into()));
    }
    match conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
        r.get::<_, i64>(0)
    }) {
        Ok(_) => Ok(conn),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::NotADatabase =>
        {
            Err(WalletError::WrongKey)
        }
        Err(e) => Err(db_err(e)),
    }
}

fn open_wallet_db<P: Parameters>(
    path: &Path,
    key: &WalletKey,
    params: P,
) -> Result<WalletDb<rusqlite::Connection, P, SystemClock, crate::zrng::ZRng>, WalletError> {
    let conn = open_connection(path, key, false)?;
    rusqlite::vtab::array::load_module(&conn).map_err(db_err)?;
    Ok(WalletDb::from_connection(
        conn,
        params,
        SystemClock,
        crate::zrng::zrng(),
    ))
}

pub fn regtest_network() -> LocalNetwork {
    let one = Some(BlockHeight::from_u32(1));
    LocalNetwork {
        overwinter: one,
        sapling: one,
        blossom: one,
        heartwood: one,
        canopy: one,
        nu5: one,
        nu6: one,
        nu6_1: one,
        nu6_2: one,
        nu6_3: one,
        nu7: one,
    }
}

pub const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);
const KEEPALIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub async fn connect(endpoint: &str) -> Result<Client, WalletError> {
    use crate::tor::{self, Purpose, Route};
    let route = tor::route(Purpose::Lightwalletd, tor::ROUTE_WAIT)
        .await
        .map_err(|b| WalletError::Remote {
            failure: b.into(),
            message: b.to_string(),
        })?;
    let lease = match route {
        Route::Tor(tor) => {
            let uri = endpoint
                .parse::<tonic::transport::Uri>()
                .map_err(|e| remote_error(&e))?;
            return tokio::time::timeout(
                TOR_CONNECT_TIMEOUT,
                tor.connect_to_lightwalletd(uri, false),
            )
            .await
            .map_err(|_| WalletError::Remote {
                failure: crate::net::NetFailure::Timeout,
                message: "connecting through Tor took too long".into(),
            })?
            .map_err(|e| remote_error(&e));
        }
        Route::Direct(lease) => lease,
    };
    connect_direct(endpoint, lease).await
}

async fn connect_direct(
    endpoint: &str,
    lease: crate::tor::DirectLease,
) -> Result<Client, WalletError> {
    let mut channel = Channel::from_shared(endpoint.to_owned()).map_err(|e| remote_error(&e))?;
    if endpoint.starts_with("https://") {
        channel = channel
            .tls_config(ClientTlsConfig::new().with_webpki_roots())
            .map_err(|e| remote_error(&e))?;
    }
    let channel = channel
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .http2_keep_alive_interval(KEEPALIVE_INTERVAL)
        .keep_alive_timeout(KEEPALIVE_TIMEOUT)
        .keep_alive_while_idle(true)
        .connect_with_connector(crate::tor::DirectConnector(lease))
        .await
        .map_err(|e| remote_error(&e))?;
    Ok(CompactTxStreamerClient::new(channel))
}

pub async fn connect_pooled(endpoint: &str) -> Result<Client, WalletError> {
    use crate::tor::{self, Purpose, Route};
    use std::collections::HashMap;
    static POOL: std::sync::Mutex<Option<HashMap<String, (tor::DirectLease, Client)>>> =
        std::sync::Mutex::new(None);
    if let Ok(pool) = POOL.lock() {
        if let Some((lease, client)) = pool.as_ref().and_then(|p| p.get(endpoint)) {
            if !lease.is_revoked() {
                return Ok(client.clone());
            }
        }
    }
    let route = tor::route(Purpose::Lightwalletd, tor::ROUTE_WAIT)
        .await
        .map_err(|b| WalletError::Remote {
            failure: b.into(),
            message: b.to_string(),
        })?;
    let Route::Direct(lease) = route else {
        return connect(endpoint).await;
    };
    let client = connect_direct(endpoint, lease.clone()).await?;
    if let Ok(mut pool) = POOL.lock() {
        pool.get_or_insert_with(HashMap::new)
            .insert(endpoint.to_owned(), (lease, client.clone()));
    }
    Ok(client)
}

pub const TOR_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

fn remote_error(e: &(dyn std::error::Error + 'static)) -> WalletError {
    WalletError::Remote {
        failure: crate::net::NetFailure::of(e),
        message: crate::net::error_chain(e),
    }
}

pub(crate) fn status_error(s: &tonic::Status) -> WalletError {
    WalletError::Remote {
        failure: crate::net::NetFailure::of_status(s),
        message: crate::net::error_chain(s),
    }
}

fn server_error(message: impl Into<String>) -> WalletError {
    WalletError::Remote {
        failure: crate::net::NetFailure::Server,
        message: message.into(),
    }
}

pub async fn check_server(
    client: &mut Client,
    network: &str,
    known_height: Option<u32>,
) -> Result<u32, WalletError> {
    let info = client
        .get_lightd_info(service::Empty {})
        .await
        .map_err(|e| status_error(&e))?
        .into_inner();
    let is_main = |n: &str| matches!(n, "main" | "mainnet");
    if !info.chain_name.is_empty() && is_main(&info.chain_name) != is_main(network) {
        return Err(WalletError::WrongNetwork {
            server: info.chain_name,
            expected: network.to_owned(),
        });
    }
    let tip = u32::try_from(info.block_height).map_err(|_| server_error("height out of range"))?;
    check_server_tip(tip, known_height)?;
    Ok(tip)
}

pub fn check_server_tip(server_tip: u32, known_height: Option<u32>) -> Result<(), WalletError> {
    if let Some(known) = known_height {
        if server_tip.saturating_add(SERVER_LAG_SLACK) < known {
            return Err(WalletError::ServerBehind {
                server_tip,
                wallet_tip: known,
            });
        }
    }
    Ok(())
}

pub fn block_scale<P: Parameters>(params: &P, height: u32) -> u32 {
    use zcash_protocol::consensus::NetworkUpgrade;
    if params.is_nu_active(NetworkUpgrade::Nu7, BlockHeight::from_u32(height)) {
        3
    } else {
        1
    }
}

pub fn proposal_expiry<P: Parameters>(params: &P, target: u32, window: u32) -> u32 {
    use zcash_protocol::consensus::NetworkUpgrade;
    let expiry = crate::vault::expiry_height_scaled(target, window, block_scale(params, target));
    match params.activation_height(NetworkUpgrade::Nu7).map(u32::from) {
        Some(nu7) if target < nu7 && expiry >= nu7 => nu7 - 1,
        _ => expiry,
    }
}

pub async fn ensure_current_branch<P: Parameters>(
    client: &mut Client,
    params: &P,
) -> Result<(), WalletError> {
    let info = client
        .get_lightd_info(service::Empty {})
        .await
        .map_err(|e| status_error(&e))?
        .into_inner();
    let tip = u32::try_from(info.block_height).map_err(|_| server_error("height out of range"))?;
    check_branch(&info.consensus_branch_id, tip, params)
}

pub async fn tip_on_current_branch<P: Parameters>(
    client: &mut Client,
    params: &P,
) -> Result<u32, WalletError> {
    let info = client
        .get_lightd_info(service::Empty {})
        .await
        .map_err(|e| status_error(&e))?
        .into_inner();
    let tip = u32::try_from(info.block_height).map_err(|_| server_error("height out of range"))?;
    check_branch(&info.consensus_branch_id, tip, params)?;
    Ok(tip)
}

pub fn check_branch<P: Parameters>(
    server_branch: &str,
    tip: u32,
    params: &P,
) -> Result<(), WalletError> {
    use zcash_protocol::consensus::BranchId;
    let hex = server_branch.trim().trim_start_matches("0x");
    let Ok(server) = u32::from_str_radix(hex, 16) else {
        return Ok(());
    };
    let ours = u32::from(BranchId::for_height(params, BlockHeight::from_u32(tip)));
    if server != ours {
        return Err(WalletError::AppOutdated(format!(
            "the network is on branch {server:08x}, this app builds for {ours:08x}"
        )));
    }
    Ok(())
}

const SERVER_LAG_SLACK: u32 = 3;

pub async fn latest_height(client: &mut Client) -> Result<u32, WalletError> {
    client
        .get_latest_block(service::ChainSpec::default())
        .await
        .map_err(|e| status_error(&e))?
        .into_inner()
        .height
        .try_into()
        .map_err(|_| server_error("height out of range"))
}

pub async fn mempool_txids(
    client: &mut Client,
) -> Result<std::collections::BTreeSet<[u8; 32]>, WalletError> {
    let mut stream = client
        .get_mempool_tx(service::GetMempoolTxRequest::default())
        .await
        .map_err(|e| status_error(&e))?
        .into_inner();
    let mut txids = std::collections::BTreeSet::new();
    while let Some(tx) = stream.message().await.map_err(|e| status_error(&e))? {
        if let Ok(txid) = <[u8; 32]>::try_from(tx.txid.as_slice()) {
            txids.insert(txid);
        }
    }
    Ok(txids)
}

#[derive(Clone, Debug)]
pub struct PaymentRequest {
    pub address: String,
    pub amount_zat: u64,
    pub memo: Option<MemoBytes>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedPayment {
    pub txid: String,
    pub amount_zat: u64,
    pub mined_height: Option<u32>,
    pub block_time: Option<u32>,
    pub memos: Vec<String>,
    pub coinbase: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSpend {
    pub txid: [u8; 32],
    pub mined_height: Option<u32>,
    pub nullifiers: Vec<[u8; 32]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VaultBalance {
    pub ironwood_spendable: u64,
    pub ironwood_locked: u64,
    pub ironwood_change_pending: u64,
    pub ironwood_pending: u64,
    pub ironwood_total: u64,
    pub total: u64,
}

pub const CHANGE_NOTES_TARGET: usize = 4;

pub const CHANGE_NOTE_MIN_ZAT: u64 = 1_000_000;

fn change_split_policy() -> SplitPolicy {
    SplitPolicy::with_min_output_value(
        std::num::NonZeroUsize::new(CHANGE_NOTES_TARGET).expect("non-zero"),
        Zatoshis::const_from_u64(CHANGE_NOTE_MIN_ZAT),
    )
}

pub struct VaultWallet<P: Parameters + Clone + Send + 'static> {
    db: WalletDb<rusqlite::Connection, P, SystemClock, crate::zrng::ZRng>,
    params: P,
    account: AccountUuid,
    cache: MemBlockCache,
    path: PathBuf,
    key: WalletKey,
}

#[derive(Clone, Debug)]
pub struct NoteHold {
    pub owner: [u8; 32],
    pub nullifiers: Vec<[u8; 32]>,
    pub expiry_height: u32,
}

impl<P: Parameters + Clone + Send + Sync + 'static> VaultWallet<P> {
    pub async fn create(
        path: &Path,
        key: &WalletKey,
        params: P,
        name: &str,
        ufvk: &UnifiedFullViewingKey,
        birthday_height: u32,
        client: &mut Client,
    ) -> Result<Self, WalletError> {
        let tip: u32 = client
            .get_latest_block(service::ChainSpec::default())
            .await
            .map_err(|e| status_error(&e))?
            .into_inner()
            .height
            .try_into()
            .map_err(|_| server_error("height out of range"))?;
        if birthday_height < 2 {
            return Err(WalletError::Sync(
                "birthday height must be at least 2".into(),
            ));
        }
        if birthday_height > tip.saturating_add(1) {
            return Err(WalletError::ServerBehind {
                server_tip: tip,
                wallet_tip: birthday_height - 1,
            });
        }
        let treestate = client
            .get_tree_state(service::BlockId {
                height: u64::from(birthday_height - 1),
                ..Default::default()
            })
            .await
            .map_err(|e| status_error(&e))?
            .into_inner();
        let birthday = AccountBirthday::from_treestate(treestate, Some(BlockHeight::from_u32(tip)))
            .map_err(|e| server_error(format!("birthday: {e:?}")))?;

        let created = (|| {
            let mut db = open_wallet_db(path, key, params.clone())?;
            init_wallet_db(&mut db, None).map_err(db_err)?;
            let account = db
                .import_account_ufvk(
                    name,
                    ufvk,
                    &birthday,
                    AccountPurpose::Spending { derivation: None },
                    None,
                )
                .map_err(db_err)?
                .id();
            Ok::<_, WalletError>((db, account))
        })();
        let (db, account) = created.inspect_err(|_| {
            let _ = std::fs::remove_file(path);
        })?;
        Ok(Self {
            db,
            params,
            account,
            cache: MemBlockCache::default(),
            path: path.to_path_buf(),
            key: key.clone(),
        })
    }

    pub fn exists(path: &Path, key: &WalletKey, params: P) -> bool {
        path.exists() && Self::open(path, key, params).is_ok()
    }

    pub fn open(path: &Path, key: &WalletKey, params: P) -> Result<Self, WalletError> {
        if !path.exists() {
            return Err(WalletError::Db("no wallet database".into()));
        }
        let mut db = open_wallet_db(path, key, params.clone())?;
        init_wallet_db(&mut db, None).map_err(db_err)?;
        let ids = db.get_account_ids().map_err(db_err)?;
        let [account] = ids.as_slice() else {
            return Err(WalletError::Db(format!(
                "expected one account, found {}",
                ids.len()
            )));
        };
        Ok(Self {
            db,
            params,
            account: *account,
            cache: MemBlockCache::default(),
            path: path.to_path_buf(),
            key: key.clone(),
        })
    }

    pub async fn sync(&mut self, client: &mut Client) -> Result<(), WalletError> {
        sync::run(client, &self.params, &self.cache, &mut self.db, 1000)
            .await
            .map_err(|e| match e {
                sync::Error::Server(s) => status_error(&s),
                sync::Error::MisbehavingServer => server_error("lightwalletd sent invalid data"),
                sync::Error::Wallet(e) => db_err(e),
                e => WalletError::Sync(format!("{e:?}")),
            })
    }

    pub fn store_mempool_tx(
        &mut self,
        tx: &zcash_primitives::transaction::Transaction,
    ) -> Result<(), WalletError> {
        decrypt_and_store_transaction(&self.params, &mut self.db, tx, None).map_err(db_err)
    }

    pub fn params(&self) -> &P {
        &self.params
    }

    pub fn needs_scan(&self, server_tip: u32) -> Result<bool, WalletError> {
        let Some(known) = self.chain_height()? else {
            return Ok(true);
        };
        if server_tip != known {
            return Ok(true);
        }
        Ok(!self.db.suggest_scan_ranges().map_err(db_err)?.is_empty())
    }

    pub fn can_build_without_scan(&self, server_tip: u32) -> Result<bool, WalletError> {
        use zcash_protocol::consensus::BranchId;
        let Some(known) = self.chain_height()? else {
            return Ok(false);
        };
        if server_tip < known || server_tip - known > BUILD_LAG_BLOCKS {
            return Ok(false);
        }
        let branch = |h: u32| BranchId::for_height(&self.params, BlockHeight::from_u32(h + 1));
        if branch(known) != branch(server_tip) {
            return Ok(false);
        }
        Ok(self.db.suggest_scan_ranges().map_err(db_err)?.is_empty())
    }

    pub fn chain_height(&self) -> Result<Option<u32>, WalletError> {
        Ok(self.db.chain_height().map_err(db_err)?.map(u32::from))
    }

    pub fn balance(&self) -> Result<VaultBalance, WalletError> {
        let Some(summary) = self
            .db
            .get_wallet_summary(ConfirmationsPolicy::default())
            .map_err(db_err)?
        else {
            return Ok(VaultBalance {
                ironwood_spendable: 0,
                ironwood_locked: 0,
                ironwood_change_pending: 0,
                ironwood_pending: 0,
                ironwood_total: 0,
                total: 0,
            });
        };
        let balance = summary
            .account_balances()
            .get(&self.account)
            .ok_or_else(|| WalletError::Db("account missing from summary".into()))?;
        Ok(VaultBalance {
            ironwood_spendable: balance.ironwood_balance().spendable_value().into_u64(),
            ironwood_locked: balance.ironwood_balance().locked_value().into_u64(),
            ironwood_change_pending: balance
                .ironwood_balance()
                .change_pending_confirmation()
                .into_u64(),
            ironwood_pending: balance
                .ironwood_balance()
                .value_pending_spendability()
                .into_u64(),
            ironwood_total: balance.ironwood_balance().total().into_u64(),
            total: balance.total().into_u64(),
        })
    }

    pub fn unspent_note_count(&self) -> Result<usize, WalletError> {
        unspent_note_count_at(&self.path, &self.key, self.account.expose_uuid().as_bytes())
    }

    pub fn received_payments(&self) -> Result<Vec<ReceivedPayment>, WalletError> {
        received_payments_at(&self.path, &self.key, self.account.expose_uuid().as_bytes())
    }

    pub fn vault_spends(&self) -> Result<Vec<VaultSpend>, WalletError> {
        let conn = open_connection(&self.path, &self.key, true)?;
        let mut stmt = conn
            .prepare(
                "SELECT t.txid, t.mined_height, rn.nf
                 FROM ironwood_received_note_spends s
                 JOIN ironwood_received_notes rn ON rn.id = s.ironwood_received_note_id
                 JOIN transactions t ON t.id_tx = s.transaction_id
                 JOIN accounts a ON a.id = rn.account_id
                 WHERE a.uuid = ?1
                   AND rn.nf IS NOT NULL
                   AND NOT (t.mined_height IS NULL AND t.expiry_height BETWEEN 1
                            AND COALESCE((SELECT MAX(height) FROM blocks), 0))
                 ORDER BY t.id_tx, rn.id",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([self.account.expose_uuid().as_bytes().as_slice()], |r| {
                Ok((
                    r.get::<_, [u8; 32]>(0)?,
                    r.get::<_, Option<u32>>(1)?,
                    r.get::<_, [u8; 32]>(2)?,
                ))
            })
            .map_err(db_err)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(db_err)?;
        let mut out: Vec<VaultSpend> = Vec::new();
        for (txid, mined_height, nf) in rows {
            match out.last_mut() {
                Some(last) if last.txid == txid => last.nullifiers.push(nf),
                _ => out.push(VaultSpend {
                    txid,
                    mined_height,
                    nullifiers: vec![nf],
                }),
            }
        }
        Ok(out)
    }

    pub fn mined_time(&self, txid: &[u8; 32]) -> Result<Option<u64>, WalletError> {
        let conn = open_connection(&self.path, &self.key, true)?;
        conn.query_row(
            "SELECT b.time FROM transactions t JOIN blocks b ON b.height = t.mined_height
             WHERE t.txid = ?1",
            [txid.as_slice()],
            |r| r.get::<_, i64>(0),
        )
        .map(|t| Some(t as u64))
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(db_err(e)),
        })
    }

    pub fn tx_mined(&self, txid: &[u8; 32]) -> Result<bool, WalletError> {
        let conn = open_connection(&self.path, &self.key, true)?;
        let mined: Option<Option<u32>> = conn
            .query_row(
                "SELECT mined_height FROM transactions WHERE txid = ?1",
                [txid.as_slice()],
                |r| r.get(0),
            )
            .map(Some)
            .or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                e => Err(db_err(e)),
            })?;
        Ok(matches!(mined, Some(Some(_))))
    }

    pub fn reserve(&mut self, holds: &[NoteHold]) -> Result<usize, WalletError> {
        let refs = {
            let conn = open_connection(&self.path, &self.key, true)?;
            let mut stmt = conn
                .prepare(
                    "SELECT t.txid, rn.action_index
                     FROM ironwood_received_notes rn
                     JOIN transactions t ON t.id_tx = rn.transaction_id
                     JOIN accounts a ON a.id = rn.account_id
                     WHERE rn.nf = ?1 AND a.uuid = ?2",
                )
                .map_err(db_err)?;
            let account = self.account.expose_uuid();
            let mut refs = Vec::new();
            for hold in holds {
                let mut outputs = Vec::new();
                for nf in &hold.nullifiers {
                    let row = stmt.query_row(
                        rusqlite::params![nf.as_slice(), account.as_bytes().as_slice()],
                        |r| Ok((r.get::<_, [u8; 32]>(0)?, r.get::<_, u32>(1)?)),
                    );
                    match row {
                        Ok((txid, index)) => outputs.push(OutputRef::new(
                            TxId::from_bytes(txid),
                            PoolType::IRONWOOD,
                            index,
                        )),
                        Err(rusqlite::Error::QueryReturnedNoRows) => {}
                        Err(e) => return Err(db_err(e)),
                    }
                }
                refs.push((hold, outputs));
            }
            refs
        };

        self.db.clear_locked_outputs(self.account).map_err(db_err)?;
        let mut held = 0;
        for (hold, outputs) in refs {
            let owner = LockOwner::new(hold.owner);
            let expiry = BlockHeight::from(hold.expiry_height);
            for output in outputs {
                match self.db.lock_outputs(&[output], owner, expiry) {
                    Ok(_) => held += 1,
                    Err(LockError::LockFailure(_)) => {}
                    Err(e) => return Err(db_err(e)),
                }
            }
        }
        Ok(held)
    }

    fn ovk_policy(&self, payments_external: bool) -> Result<OvkPolicy, WalletError> {
        let account = self
            .db
            .get_account(self.account)
            .map_err(db_err)?
            .ok_or_else(|| WalletError::Db("vault account missing".into()))?;
        let fvk = account
            .ufvk()
            .and_then(|u| u.orchard())
            .ok_or_else(|| WalletError::Db("vault account has no Orchard key".into()))?;
        let internal =
            zcash_keys::keys::OutgoingViewingKey::from(fvk.to_ovk(orchard::keys::Scope::Internal));
        Ok(OvkPolicy::Custom {
            external_ovk: if payments_external {
                zcash_keys::keys::OutgoingViewingKey::from(
                    fvk.to_ovk(orchard::keys::Scope::External),
                )
            } else {
                internal
            },
            internal_ovk: Some(internal),
        })
    }

    pub fn propose(
        &mut self,
        payments: &[PaymentRequest],
        expiry_blocks: u32,
        must_spend: &[Vec<[u8; 32]>],
    ) -> Result<Pczt, WalletError> {
        let proposal = self.transfer_proposal(payments, must_spend)?;
        let ovk_policy = self.ovk_policy(true)?;
        create_pczt_from_proposal::<
            _,
            _,
            GreedyInputSelectorError,
            _,
            zcash_primitives::transaction::fees::zip317::FeeError,
            _,
        >(
            &mut self.db,
            &self.params,
            &SystemClock,
            &mut crate::zrng::zrng(),
            self.account,
            ovk_policy,
            &proposal,
            Some(BlockHeight::from(proposal_expiry(
                &self.params,
                u32::from(BlockHeight::from(proposal.min_target_height())),
                expiry_blocks,
            ))),
            BundlePadding::DEFAULT,
        )
        .map_err(|e| WalletError::Proposal(format!("{e:?}")))
    }

    pub fn fee_for(&mut self, payments: &[PaymentRequest]) -> Result<u64, WalletError> {
        let proposal = self.transfer_proposal(payments, &[])?;
        Ok(proposal
            .steps()
            .iter()
            .map(|step| u64::from(step.balance().fee_required()))
            .sum())
    }

    fn transfer_proposal(
        &mut self,
        payments: &[PaymentRequest],
        must_spend: &[Vec<[u8; 32]>],
    ) -> Result<
        zcash_client_backend::proposal::Proposal<
            StandardFeeRule,
            zcash_client_sqlite::ReceivedNoteId,
        >,
        WalletError,
    > {
        use zcash_client_backend::data_api::wallet::input_selection::InputSelector;

        let request = TransactionRequest::new(
            payments
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    Payment::new(
                        ZcashAddress::from_str(&p.address)
                            .map_err(|e| WalletError::Payment(format!("#{i}: {e}")))?,
                        Some(
                            Zatoshis::from_u64(p.amount_zat)
                                .map_err(|e| WalletError::Payment(format!("#{i}: {e:?}")))?,
                        ),
                        p.memo.clone(),
                        None,
                        None,
                        vec![],
                    )
                    .map_err(|e| WalletError::Payment(format!("#{i}: {e:?}")))
                })
                .collect::<Result<Vec<_>, _>>()?,
        )
        .map_err(|e| WalletError::Payment(format!("{e:?}")))?;

        let confirmations = ConfirmationsPolicy::default();
        let (target_height, anchor_height) = self
            .db
            .get_target_and_anchor_heights(confirmations.trusted())
            .map_err(db_err)?
            .ok_or_else(|| WalletError::Proposal("sync required".into()))?;
        let inputs = ForcedInputs {
            forced: self.forced_notes(must_spend, target_height)?,
            db: &self.db,
        };
        let change = MultiOutputChangeStrategy::new(
            StandardFeeRule::Zip317,
            None,
            ShieldedPool::Ironwood,
            DustOutputPolicy::default(),
            change_split_policy(),
        );
        let proposal = GreedyInputSelector::new()
            .propose_transaction(
                &self.params,
                &inputs,
                target_height,
                anchor_height,
                &self.db.pool_migration_params(),
                confirmations,
                self.account,
                request,
                &change,
                &SpendPolicy::default(),
                None,
            )
            .map_err(|e| {
                let e = format!("{e:?}");
                let reserved = self
                    .db
                    .get_locked_outputs(self.account)
                    .is_ok_and(|l| !l.is_empty());
                if reserved && e.contains("InsufficientFunds") {
                    WalletError::FundsReserved
                } else {
                    WalletError::Proposal(e)
                }
            })?;
        proposal
            .check_transaction_size()
            .map_err(|e| WalletError::Proposal(format!("{e:?}")))?;
        Ok(proposal.with_proposed_version(None))
    }

    fn forced_notes(
        &self,
        must_spend: &[Vec<[u8; 32]>],
        target_height: TargetHeight,
    ) -> Result<Vec<ReceivedNote<zcash_client_sqlite::ReceivedNoteId, orchard::Note>>, WalletError>
    {
        let conn = open_connection(&self.path, &self.key, true)?;
        let mut stmt = conn
            .prepare(
                "SELECT t.txid, rn.action_index
                 FROM ironwood_received_notes rn
                 JOIN transactions t ON t.id_tx = rn.transaction_id
                 JOIN accounts a ON a.id = rn.account_id
                 WHERE rn.nf = ?1 AND a.uuid = ?2",
            )
            .map_err(db_err)?;
        let account = self.account.expose_uuid();
        let mut forced = Vec::with_capacity(must_spend.len());
        for nullifiers in must_spend {
            let mut known = false;
            let mut found = None;
            for nf in nullifiers {
                let row = stmt.query_row(
                    rusqlite::params![nf.as_slice(), account.as_bytes().as_slice()],
                    |r| Ok((r.get::<_, [u8; 32]>(0)?, r.get::<_, u32>(1)?)),
                );
                let (txid, index) = match row {
                    Ok(row) => row,
                    Err(rusqlite::Error::QueryReturnedNoRows) => continue,
                    Err(e) => return Err(db_err(e)),
                };
                known = true;
                let note = self
                    .db
                    .get_spendable_note(
                        &TxId::from_bytes(txid),
                        ShieldedPool::Ironwood,
                        index,
                        target_height,
                        LockFilter::Unfiltered,
                    )
                    .map_err(db_err)?;
                if let Some(received) = note {
                    if let zcash_client_backend::wallet::Note::Orchard { note, .. } =
                        *received.note()
                    {
                        found = Some(received.map_note(|_| note));
                        break;
                    }
                }
            }
            match found {
                Some(note)
                    if !forced.iter().any(|f: &ReceivedNote<_, _>| {
                        f.internal_note_id() == note.internal_note_id()
                    }) =>
                {
                    forced.push(note)
                }
                Some(_) => {}
                None if known => return Err(WalletError::NotesSpent),
                None => {
                    return Err(WalletError::Proposal(
                        "a cancelled payment's funds aren't in this wallet yet; sync and try again"
                            .into(),
                    ))
                }
            }
        }
        Ok(forced)
    }

    pub fn propose_sweep(
        &mut self,
        nullifiers: &[[u8; 32]],
        owners: &[[u8; 32]],
        recipient: ZcashAddress,
        expiry_blocks: u32,
    ) -> Result<Pczt, WalletError> {
        const SWEEP: LockOwner = LockOwner::new(*b"zafe-sweep-temporary-lock-owner!");
        let mut targets: Vec<[u8; 32]> = Vec::new();
        let others: Vec<OutputRef> = {
            let conn = open_connection(&self.path, &self.key, true)?;
            let mut stmt = conn
                .prepare(
                    "SELECT t.txid, rn.action_index, rn.nf
                     FROM ironwood_received_notes rn
                     JOIN transactions t ON t.id_tx = rn.transaction_id
                     JOIN accounts a ON a.id = rn.account_id
                     WHERE a.uuid = ?1",
                )
                .map_err(db_err)?;
            let rows = stmt
                .query_map([self.account.expose_uuid().as_bytes().as_slice()], |r| {
                    Ok((
                        r.get::<_, [u8; 32]>(0)?,
                        r.get::<_, u32>(1)?,
                        r.get::<_, Option<Vec<u8>>>(2)?,
                    ))
                })
                .map_err(db_err)?;
            let mut others = Vec::new();
            for row in rows {
                let (txid, index, nf) = row.map_err(db_err)?;
                let target = nf
                    .as_deref()
                    .and_then(|nf| nullifiers.iter().find(|t| t[..] == *nf));
                if let Some(t) = target {
                    targets.push(*t);
                } else {
                    others.push(OutputRef::new(
                        TxId::from_bytes(txid),
                        PoolType::IRONWOOD,
                        index,
                    ));
                }
            }
            others
        };
        if targets.is_empty() {
            return Err(WalletError::Proposal(
                "none of those notes are in this wallet yet; sync and try again".into(),
            ));
        }
        let tip = self.chain_height()?.unwrap_or(0);
        for output in &others {
            match self.db.lock_outputs(
                std::slice::from_ref(output),
                SWEEP,
                BlockHeight::from(tip + 10_000),
            ) {
                Ok(_) | Err(LockError::LockFailure(_)) => {}
                Err(e) => return Err(db_err(e)),
            }
        }
        let proposal = propose_send_max_transfer::<
            _,
            _,
            _,
            zcash_client_sqlite::wallet::commitment_tree::Error,
        >(
            &mut self.db,
            &self.params,
            self.account,
            &[ShieldedPool::Ironwood],
            &StandardFeeRule::Zip317,
            recipient,
            None,
            MaxSpendMode::MaxSpendable,
            ConfirmationsPolicy::default(),
            &locked_policy(owners).unwrap_or_default(),
            None,
        )
        .map_err(|e| WalletError::Proposal(format!("{e:?}")));
        for output in &others {
            let _ = self.db.unlock_output(output, SWEEP);
        }
        let proposal = proposal?;
        let ovk_policy = self.ovk_policy(false)?;
        let pczt = create_pczt_from_proposal::<
            _,
            _,
            GreedyInputSelectorError,
            _,
            zcash_primitives::transaction::fees::zip317::FeeError,
            _,
        >(
            &mut self.db,
            &self.params,
            &SystemClock,
            &mut crate::zrng::zrng(),
            self.account,
            ovk_policy,
            &proposal,
            Some(BlockHeight::from(proposal_expiry(
                &self.params,
                u32::from(BlockHeight::from(proposal.min_target_height())),
                expiry_blocks,
            ))),
            BundlePadding::DEFAULT,
        )
        .map_err(|e| WalletError::Proposal(format!("{e:?}")))?;
        let spent = crate::tx::spent_nullifiers(&pczt)
            .map_err(|e| WalletError::Proposal(format!("{e:?}")))?;
        if !targets.iter().all(|nf| spent.contains(nf)) {
            return Err(WalletError::Proposal(
                "some of those funds can't be moved yet (unconfirmed or already spent)".into(),
            ));
        }
        Ok(pczt)
    }
}

struct ForcedInputs<'a, D: InputSource> {
    db: &'a D,
    forced: Vec<ReceivedNote<D::NoteRef, orchard::Note>>,
}

impl<D: InputSource> InputSource for ForcedInputs<'_, D> {
    type Error = D::Error;
    type AccountId = D::AccountId;
    type NoteRef = D::NoteRef;

    fn get_spendable_note(
        &self,
        txid: &TxId,
        protocol: ShieldedPool,
        index: u32,
        target_height: TargetHeight,
        lock_filter: LockFilter<'_>,
    ) -> Result<Option<ReceivedNote<Self::NoteRef, zcash_client_backend::wallet::Note>>, Self::Error>
    {
        self.db
            .get_spendable_note(txid, protocol, index, target_height, lock_filter)
    }

    fn anchor_computable(
        &self,
        protocol: ShieldedPool,
        height: BlockHeight,
    ) -> Result<bool, Self::Error> {
        self.db.anchor_computable(protocol, height)
    }

    fn select_spendable_notes(
        &self,
        account: Self::AccountId,
        target_value: TargetValue,
        sources: &[ShieldedPool],
        target_height: TargetHeight,
        confirmations_policy: ConfirmationsPolicy,
        exclude: &[Self::NoteRef],
        lock_filter: LockFilter<'_>,
    ) -> Result<ReceivedNotes<Self::NoteRef>, Self::Error> {
        let forced: Vec<_> = self
            .forced
            .iter()
            .filter(|n| {
                sources.contains(&ShieldedPool::Ironwood) && !exclude.contains(n.internal_note_id())
            })
            .cloned()
            .collect();
        let forced_value = forced.iter().map(|n| n.note().value().inner()).sum::<u64>();
        let rest = match target_value {
            TargetValue::AtLeast(v) if v.into_u64() <= forced_value => None,
            TargetValue::AtLeast(v) => Some(TargetValue::AtLeast(
                Zatoshis::from_u64(v.into_u64() - forced_value).expect("below the target"),
            )),
            other => Some(other),
        };
        let mut skip = exclude.to_vec();
        skip.extend(forced.iter().map(|n| *n.internal_note_id()));
        let found = match rest {
            Some(rest) => self.db.select_spendable_notes(
                account,
                rest,
                sources,
                target_height,
                confirmations_policy,
                &skip,
                lock_filter,
            )?,
            None => ReceivedNotes::empty(),
        };
        let (sapling, orchard, mut ironwood) = (
            found.sapling().to_vec(),
            found.orchard().to_vec(),
            found.ironwood().to_vec(),
        );
        ironwood.extend(forced);
        Ok(ReceivedNotes::new(sapling, orchard, ironwood))
    }

    fn select_single_spendable_note(
        &self,
        account: Self::AccountId,
        value: Zatoshis,
        sources: &[ShieldedPool],
        target_height: TargetHeight,
        confirmations_policy: ConfirmationsPolicy,
        exclude: &[Self::NoteRef],
        lock_filter: LockFilter<'_>,
    ) -> Result<ReceivedNotes<Self::NoteRef>, Self::Error> {
        if !self.forced.is_empty() {
            return Ok(ReceivedNotes::empty());
        }
        self.db.select_single_spendable_note(
            account,
            value,
            sources,
            target_height,
            confirmations_policy,
            exclude,
            lock_filter,
        )
    }

    fn select_unspent_notes(
        &self,
        account: Self::AccountId,
        sources: &[ShieldedPool],
        target_height: TargetHeight,
        exclude: &[Self::NoteRef],
        lock_filter: LockFilter<'_>,
    ) -> Result<ReceivedNotes<Self::NoteRef>, Self::Error> {
        self.db
            .select_unspent_notes(account, sources, target_height, exclude, lock_filter)
    }

    fn get_account_metadata(
        &self,
        account: Self::AccountId,
        selector: &zcash_client_backend::data_api::NoteFilter,
        target_height: TargetHeight,
        exclude: &[Self::NoteRef],
        lock_filter: LockFilter<'_>,
    ) -> Result<zcash_client_backend::data_api::AccountMeta, Self::Error> {
        self.db
            .get_account_metadata(account, selector, target_height, exclude, lock_filter)
    }
}

fn locked_policy(owners: &[[u8; 32]]) -> Option<LockedInputPolicy> {
    zcash_client_backend::data_api::wallet::input_selection::NonEmptyBTreeSet::from_set(
        owners.iter().map(|o| LockOwner::new(*o)).collect(),
    )
    .map(LockedInputPolicy::PreferLocked)
}

fn unspent_note_count_at(
    path: &Path,
    key: &WalletKey,
    account: &[u8; 16],
) -> Result<usize, WalletError> {
    let conn = open_connection(path, key, true)?;
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*)
             FROM ironwood_received_notes rn
             JOIN accounts a ON a.id = rn.account_id
             WHERE a.uuid = ?1 AND rn.value > 0
               AND NOT EXISTS (SELECT 1 FROM ironwood_received_note_spends s
                               WHERE s.ironwood_received_note_id = rn.id)",
            rusqlite::params![account.as_slice()],
            |r| r.get(0),
        )
        .map_err(db_err)?;
    Ok(usize::try_from(n).unwrap_or(0))
}

fn received_payments_at(
    path: &Path,
    key: &WalletKey,
    account: &[u8; 16],
) -> Result<Vec<ReceivedPayment>, WalletError> {
    let conn = open_connection(path, key, true)?;
    let mut stmt = conn
        .prepare(
            "SELECT t.id_tx, t.txid, t.mined_height, b.time, SUM(ro.value),
                    (t.tx_index IS NOT NULL AND t.tx_index = 0)
             FROM v_received_outputs ro
             JOIN accounts a ON a.id = ro.account_id
             JOIN transactions t ON t.id_tx = ro.transaction_id
             LEFT JOIN blocks b ON b.height = t.mined_height
             WHERE a.uuid = ?1
               AND ro.is_change = 0
               AND NOT EXISTS (
                   SELECT 1 FROM v_received_output_spends s
                   WHERE s.transaction_id = t.id_tx AND s.account_id = a.id)
               AND NOT (t.mined_height IS NULL AND t.expiry_height BETWEEN 1
                        AND COALESCE((SELECT MAX(height) FROM blocks), 0))
             GROUP BY t.id_tx
             ORDER BY t.mined_height IS NOT NULL, t.mined_height DESC, t.id_tx DESC",
        )
        .map_err(db_err)?;
    let rows = stmt
        .query_map([account.as_slice()], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Vec<u8>>(1)?,
                r.get::<_, Option<u32>>(2)?,
                r.get::<_, Option<u32>>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, bool>(5)?,
            ))
        })
        .map_err(db_err)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(db_err)?;
    let mut memo_stmt = conn
        .prepare(
            "SELECT ro.memo FROM v_received_outputs ro
             JOIN accounts a ON a.id = ro.account_id
             WHERE a.uuid = ?1 AND ro.transaction_id = ?2 AND ro.is_change = 0
               AND ro.memo IS NOT NULL
             ORDER BY ro.pool, ro.output_index",
        )
        .map_err(db_err)?;
    rows.into_iter()
        .map(|(id_tx, txid, mined_height, block_time, value, coinbase)| {
            let txid: [u8; 32] = txid
                .try_into()
                .map_err(|_| WalletError::Db("txid is not 32 bytes".into()))?;
            let memos = memo_stmt
                .query_map(rusqlite::params![account.as_slice(), id_tx], |r| {
                    r.get::<_, Vec<u8>>(0)
                })
                .map_err(db_err)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(db_err)?
                .iter()
                .filter_map(|m| memo_text(m))
                .collect();
            Ok(ReceivedPayment {
                txid: zcash_protocol::TxId::from_bytes(txid).to_string(),
                amount_zat: u64::try_from(value)
                    .map_err(|_| WalletError::Db("negative received value".into()))?,
                mined_height,
                block_time,
                memos,
                coinbase,
            })
        })
        .collect()
}

pub fn memo_text(bytes: &[u8]) -> Option<String> {
    let memo = zcash_protocol::memo::Memo::try_from(MemoBytes::from_bytes(bytes).ok()?).ok()?;
    match memo {
        zcash_protocol::memo::Memo::Text(t) if !t.is_empty() => Some(t.to_string()),
        _ => None,
    }
}

#[derive(Clone, Default)]
pub struct MemBlockCache(Arc<Mutex<Vec<CompactBlock>>>);

fn height(block: &CompactBlock) -> BlockHeight {
    BlockHeight::from_u32(u32::try_from(block.height).unwrap_or(u32::MAX))
}

#[derive(Debug, thiserror::Error)]
#[error("block cache: {0}")]
pub struct CacheError(String);

impl BlockSource for MemBlockCache {
    type Error = CacheError;

    fn with_blocks<F, WalletErrT>(
        &self,
        from_height: Option<BlockHeight>,
        limit: Option<usize>,
        mut with_block: F,
    ) -> Result<(), chain_error::Error<WalletErrT, Self::Error>>
    where
        F: FnMut(CompactBlock) -> Result<(), chain_error::Error<WalletErrT, Self::Error>>,
    {
        let mut blocks: Vec<CompactBlock> = self
            .0
            .lock()
            .expect("lock")
            .iter()
            .filter(|b| from_height.is_none_or(|h| height(b) >= h))
            .cloned()
            .collect();
        blocks.sort_by_key(|b| b.height);
        for block in blocks.into_iter().take(limit.unwrap_or(usize::MAX)) {
            with_block(block)?;
        }
        Ok(())
    }
}

#[async_trait]
impl BlockCache for MemBlockCache {
    fn get_tip_height(
        &self,
        range: Option<&ScanRange>,
    ) -> Result<Option<BlockHeight>, Self::Error> {
        Ok(self
            .0
            .lock()
            .expect("lock")
            .iter()
            .map(height)
            .filter(|h| range.is_none_or(|r| r.block_range().contains(h)))
            .max())
    }

    async fn read(&self, range: &ScanRange) -> Result<Vec<CompactBlock>, Self::Error> {
        let mut blocks: Vec<CompactBlock> = self
            .0
            .lock()
            .expect("lock")
            .iter()
            .filter(|b| range.block_range().contains(&height(b)))
            .cloned()
            .collect();
        blocks.sort_by_key(|b| b.height);
        Ok(blocks)
    }

    async fn insert(&self, mut compact_blocks: Vec<CompactBlock>) -> Result<(), Self::Error> {
        self.0.lock().expect("lock").append(&mut compact_blocks);
        Ok(())
    }

    async fn delete(&self, range: ScanRange) -> Result<(), Self::Error> {
        self.0
            .lock()
            .expect("lock")
            .retain(|b| !range.block_range().contains(&height(b)));
        Ok(())
    }
}

pub const MAINNET_NU7_HEIGHT: u32 = 3_543_405;

#[derive(Clone, Copy, Debug)]
pub enum ZafeNetwork {
    Main,
    Test,
    Regtest(LocalNetwork),
}

impl ZafeNetwork {
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "main" => Some(Self::Main),
            "test" => Some(Self::Test),
            "regtest" => Some(Self::Regtest(regtest_network())),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Test => "test",
            Self::Regtest(_) => "regtest",
        }
    }
}

impl Parameters for ZafeNetwork {
    fn network_type(&self) -> zcash_protocol::consensus::NetworkType {
        match self {
            Self::Main => zcash_protocol::consensus::MainNetwork.network_type(),
            Self::Test => zcash_protocol::consensus::TestNetwork.network_type(),
            Self::Regtest(n) => n.network_type(),
        }
    }

    fn activation_height(
        &self,
        nu: zcash_protocol::consensus::NetworkUpgrade,
    ) -> Option<BlockHeight> {
        match self {
            Self::Main => zcash_protocol::consensus::MainNetwork
                .activation_height(nu)
                .or_else(|| {
                    (nu == zcash_protocol::consensus::NetworkUpgrade::Nu7)
                        .then(|| BlockHeight::from_u32(MAINNET_NU7_HEIGHT))
                }),
            Self::Test => zcash_protocol::consensus::TestNetwork.activation_height(nu),
            Self::Regtest(n) => n.activation_height(nu),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{VaultKeys, VaultSecret};

    #[test]
    fn an_unknown_network_upgrade_is_reported_as_an_outdated_app() {
        use zcash_protocol::consensus::{MAIN_NETWORK, TEST_NETWORK};
        assert!(check_branch("37a5165b", 3_515_000, &MAIN_NETWORK).is_ok());
        assert!(check_branch("0x37A5165B", 3_515_000, &MAIN_NETWORK).is_ok());
        assert!(check_branch("77190ad9", 4_493_000, &TEST_NETWORK).is_ok());
        let before_nu7 = LocalNetwork {
            nu7: None,
            ..regtest_network()
        };
        let err = check_branch("77190ad9", 10, &before_nu7).unwrap_err();
        assert!(matches!(err, WalletError::AppOutdated(_)), "{err}");
        assert!(check_branch("", 10, &before_nu7).is_ok());
        assert!(check_branch("nonsense", 10, &before_nu7).is_ok());
    }

    #[test]
    fn mainnet_knows_the_nu7_height() {
        use zcash_protocol::consensus::{BranchId, NetworkUpgrade};
        let main = ZafeNetwork::Main;
        assert_eq!(
            main.activation_height(NetworkUpgrade::Nu7),
            Some(BlockHeight::from_u32(MAINNET_NU7_HEIGHT))
        );
        let at = |h| BranchId::for_height(&main, BlockHeight::from_u32(h));
        assert_eq!(at(MAINNET_NU7_HEIGHT - 1), BranchId::Nu6_3);
        assert_eq!(at(MAINNET_NU7_HEIGHT), BranchId::Nu7);
        assert!(check_branch("37a5165b", MAINNET_NU7_HEIGHT - 1, &main).is_ok());
        assert!(check_branch("77190ad9", MAINNET_NU7_HEIGHT, &main).is_ok());
        assert_eq!(
            proposal_expiry(&main, MAINNET_NU7_HEIGHT - 500, 1_152),
            MAINNET_NU7_HEIGHT - 1
        );
    }

    #[test]
    fn a_proposal_does_not_outlive_a_known_network_upgrade() {
        let ahead = LocalNetwork {
            nu7: Some(BlockHeight::from_u32(2_000)),
            ..regtest_network()
        };
        assert_eq!(proposal_expiry(&ahead, 1_500, 1_152), 1_999);
        assert_eq!(
            proposal_expiry(&ahead, 2_500, 1_152),
            crate::vault::expiry_height_scaled(2_500, 1_152, 3)
        );
        assert_eq!(
            proposal_expiry(&ahead, 100, 1_152),
            crate::vault::expiry_height(100, 1_152)
        );
        let unknown = LocalNetwork {
            nu7: None,
            ..regtest_network()
        };
        assert_eq!(
            proposal_expiry(&unknown, 1_500, 1_152),
            crate::vault::expiry_height(1_500, 1_152)
        );
    }

    #[test]
    fn expiry_windows_keep_their_duration_after_nu7() {
        let main = ZafeNetwork::Main;
        let week = crate::vault::DEFAULT_PROPOSAL_EXPIRY_BLOCKS;
        assert_eq!(block_scale(&main, MAINNET_NU7_HEIGHT - 1), 1);
        assert_eq!(block_scale(&main, MAINNET_NU7_HEIGHT), 3);
        let before = proposal_expiry(&main, 3_000_001, week);
        assert!((3_000_001 + week..3_000_001 + week + 144).contains(&before));
        let target = MAINNET_NU7_HEIGHT + 10;
        let after = proposal_expiry(&main, target, week);
        assert!((target + 3 * week..target + 3 * week + 3 * 144).contains(&after));
        assert_eq!(after % (3 * 144), 0);

        let check = |tip: u32, expiry: u32| {
            let e = crate::node::expectations(&main, &[], tip, week).unwrap();
            expiry > tip && expiry - tip <= e.max_expiry_delta
        };
        assert!(check(target - 1, after));
        assert!(check(MAINNET_NU7_HEIGHT - 50, after));
        assert!(!check(target - 1, after + 3 * 144 + 3 * 96 + 1));
        let e = crate::node::expectations(&main, &[], 3_000_000, week).unwrap();
        assert_eq!(e.max_expiry_delta, week + 1 + 96 + 144);
    }

    #[test]
    fn the_wallet_key_is_derived_not_stored() {
        let seeds = [7u8; 66];
        let a = WalletKey::derive(&seeds, "vault-a");
        assert_eq!(
            a.as_bytes(),
            WalletKey::derive(&seeds, "vault-a").as_bytes()
        );
        assert_ne!(
            a.as_bytes(),
            WalletKey::derive(&seeds, "vault-b").as_bytes()
        );
        assert_ne!(
            a.as_bytes(),
            WalletKey::derive(&[8u8; 66], "vault-a").as_bytes()
        );
        assert_ne!(
            WalletKey::derive(&[1, 2, 3], "x").as_bytes(),
            WalletKey::derive(&[1, 2], "\x03x").as_bytes()
        );
    }

    use orchard::keys::{FullViewingKey, SpendingKey};
    use zcash_client_backend::data_api::chain::ChainState;
    use zcash_primitives::block::BlockHash;

    fn text_memo(text: &str) -> Vec<u8> {
        zcash_protocol::memo::Memo::from_bytes(text.as_bytes())
            .unwrap()
            .encode()
            .as_array()
            .to_vec()
    }

    #[test]
    fn received_payments_exclude_own_spends_and_expired() {
        let dir = std::env::temp_dir().join(format!("zafe-recv-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wallet.sqlite");
        let _ = std::fs::remove_file(&path);
        let net = regtest_network();

        let ak: [u8; 32] = FullViewingKey::from(&SpendingKey::from_bytes([3u8; 32]).unwrap())
            .to_bytes()[..32]
            .try_into()
            .unwrap();
        let ufvk = VaultKeys::derive(&VaultSecret::from_bytes([9u8; 32]), &ak)
            .unwrap()
            .ufvk()
            .unwrap();
        let key = WalletKey::random();
        let mut db = open_wallet_db(&path, &key, net).unwrap();
        init_wallet_db(&mut db, None).unwrap();
        let birthday = AccountBirthday::from_parts(
            ChainState::empty(BlockHeight::from_u32(1), BlockHash([0; 32])),
            None,
        );
        let account = db
            .import_account_ufvk(
                "vault",
                &ufvk,
                &birthday,
                AccountPurpose::Spending { derivation: None },
                None,
            )
            .unwrap()
            .id();

        let conn = open_connection(&path, &key, false).unwrap();
        let account_id: i64 = conn
            .query_row("SELECT id FROM accounts", [], |r| r.get(0))
            .unwrap();
        for (height, time) in [
            (10u32, 1_700_000_000u32),
            (12, 1_700_000_300),
            (20, 1_700_001_000),
        ] {
            conn.execute(
                "INSERT INTO blocks (height, hash, time, sapling_tree) VALUES (?1, ?2, ?3, x'00')",
                rusqlite::params![height, vec![height as u8; 32], time],
            )
            .unwrap();
        }
        type TxRow = (i64, u8, Option<u32>, Option<u32>, u32);
        let txs: [TxRow; 5] = [
            (1, 0xa1, Some(10), Some(1), 50),
            (2, 0xa2, Some(12), Some(0), 0),
            (3, 0xa3, Some(20), Some(1), 60),
            (4, 0xa4, None, None, 15),
            (5, 0xa5, None, None, 100),
        ];
        for (id, byte, mined, index, expiry) in txs {
            conn.execute(
                "INSERT INTO transactions (id_tx, txid, block, mined_height, tx_index,
                     expiry_height, min_observed_height)
                 VALUES (?1, ?2, ?3, ?3, ?4, ?5, 1)",
                rusqlite::params![id, vec![byte; 32], mined, index, expiry],
            )
            .unwrap();
        }
        type NoteRow = (i64, i64, u32, i64, bool, Option<Vec<u8>>);
        let notes: [NoteRow; 6] = [
            (1, 1, 0, 100_000, false, Some(text_memo("rent"))),
            (2, 1, 1, 50_000, false, Some(vec![0xf6])),
            (3, 2, 0, 625_000_000, false, None),
            (4, 3, 0, 40_000, true, None),
            (5, 3, 1, 5_000, false, None),
            (6, 5, 0, 7_000, false, None),
        ];
        for (id, tx, action, value, change, memo) in notes {
            conn.execute(
                "INSERT INTO ironwood_received_notes (id, transaction_id, action_index,
                     account_id, diversifier, value, rho, rseed, is_change, memo, note_version)
                 VALUES (?1, ?2, ?3, ?4, x'00', ?5, x'00', x'00', ?6, ?7, 0)",
                rusqlite::params![id, tx, action, account_id, value, change, memo],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO ironwood_received_note_spends VALUES (1, 3)",
            [],
        )
        .unwrap();
        drop(conn);

        assert_eq!(
            unspent_note_count_at(&path, &key, account.expose_uuid().as_bytes()).unwrap(),
            5
        );
        let got = received_payments_at(&path, &key, account.expose_uuid().as_bytes()).unwrap();
        let txid = |b: u8| zcash_protocol::TxId::from_bytes([b; 32]).to_string();
        assert_eq!(
            got,
            vec![
                ReceivedPayment {
                    txid: txid(0xa5),
                    amount_zat: 7_000,
                    mined_height: None,
                    block_time: None,
                    memos: vec![],
                    coinbase: false,
                },
                ReceivedPayment {
                    txid: txid(0xa2),
                    amount_zat: 625_000_000,
                    mined_height: Some(12),
                    block_time: Some(1_700_000_300),
                    memos: vec![],
                    coinbase: true,
                },
                ReceivedPayment {
                    txid: txid(0xa1),
                    amount_zat: 150_000,
                    mined_height: Some(10),
                    block_time: Some(1_700_000_000),
                    memos: vec!["rent".into()],
                    coinbase: false,
                },
            ]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn memo_text_skips_empty_and_binary() {
        assert_eq!(memo_text(&text_memo("hi")), Some("hi".into()));
        assert_eq!(memo_text(&[0xf6]), None);
        assert_eq!(memo_text(&[0xff; 512]), None);
    }

    #[test]
    fn wallet_database_is_encrypted() {
        let dir = std::env::temp_dir().join(format!("zafe-enc-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wallet.sqlite");
        let _ = std::fs::remove_file(&path);
        let net = regtest_network();
        let key = WalletKey::random();

        let ak: [u8; 32] = FullViewingKey::from(&SpendingKey::from_bytes([3u8; 32]).unwrap())
            .to_bytes()[..32]
            .try_into()
            .unwrap();
        let ufvk = VaultKeys::derive(&VaultSecret::from_bytes([9u8; 32]), &ak)
            .unwrap()
            .ufvk()
            .unwrap();
        assert!(VaultWallet::open(&path, &key, net).is_err());
        assert!(!path.exists());

        let mut db = open_wallet_db(&path, &key, net).unwrap();
        init_wallet_db(&mut db, None).unwrap();
        let birthday = AccountBirthday::from_parts(
            ChainState::empty(BlockHeight::from_u32(1), BlockHash([0; 32])),
            None,
        );
        db.import_account_ufvk(
            "vault",
            &ufvk,
            &birthday,
            AccountPurpose::Spending { derivation: None },
            None,
        )
        .unwrap();
        drop(db);

        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.starts_with(b"SQLite format 3"));
        assert!(!raw.windows(5).any(|w| w == b"vault"));

        assert!(VaultWallet::exists(&path, &key, net));
        let wallet = VaultWallet::open(&path, &key, net).unwrap();
        assert_eq!(wallet.balance().unwrap().total, 0);
        assert_eq!(wallet.received_payments().unwrap(), vec![]);
        drop(wallet);

        let wrong = WalletKey::random();
        assert!(matches!(
            VaultWallet::open(&path, &wrong, net),
            Err(WalletError::WrongKey)
        ));
        assert!(matches!(
            open_connection(&path, &wrong, true),
            Err(WalletError::WrongKey)
        ));
        assert!(!VaultWallet::exists(&path, &wrong, net));

        let plain = dir.join("plain.sqlite");
        let _ = std::fs::remove_file(&plain);
        let conn = rusqlite::Connection::open(&plain).unwrap();
        conn.execute_batch("CREATE TABLE t (x); INSERT INTO t VALUES (1);")
            .unwrap();
        drop(conn);
        assert!(std::fs::read(&plain)
            .unwrap()
            .starts_with(b"SQLite format 3"));
        assert!(matches!(
            VaultWallet::open(&plain, &key, net),
            Err(WalletError::WrongKey)
        ));

        assert!(WalletKey::from_slice(&[0u8; 31]).is_err());
        assert_eq!(
            WalletKey::from_slice(key.as_bytes()).unwrap().as_bytes(),
            key.as_bytes()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
