//! The vault wallet on a member's device (spec §4.1, §8, §9.2).
//!
//! Like a hardware-signer (Keystone) account: the device imports the vault UFVK, syncs from
//! lightwalletd, shows balance and history, and builds PCZTs. Spend authorization comes
//! from FROST, never from a local key. The account is imported as `Spending` with no ZIP 32
//! derivation, so the wallet tracks note witnesses (a `ViewOnly` account would not).

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
        scanning::ScanRange,
        wallet::{
            create_pczt_from_proposal, decrypt_and_store_transaction,
            input_selection::{
                GreedyInputSelector, GreedyInputSelectorError, LockedInputPolicy, SpendPolicy,
            },
            propose_send_max_transfer, propose_transfer, ConfirmationsPolicy,
        },
        Account as _, AccountBirthday, AccountPurpose, MaxSpendMode, OutputLockStore, WalletRead,
        WalletWrite,
    },
    fees::{standard::MultiOutputChangeStrategy, DustOutputPolicy, SplitPolicy, StandardFeeRule},
    proto::{
        compact_formats::CompactBlock,
        service::{self, compact_tx_streamer_client::CompactTxStreamerClient},
    },
    sync,
    wallet::{LockOwner, OutputRef, OvkPolicy},
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

#[derive(Debug, thiserror::Error)]
pub enum WalletError {
    #[error("wallet database: {0}")]
    Db(String),
    /// lightwalletd could not be reached or answered with an error.
    #[error("lightwalletd: {message}")]
    Remote {
        failure: crate::net::NetFailure,
        message: String,
    },
    /// lightwalletd serves another network than the vault's.
    #[error("lightwalletd serves {server}, the vault is on {expected}")]
    WrongNetwork { server: String, expected: String },
    /// lightwalletd's chain tip is below blocks this wallet already has (it is still
    /// catching up, or follows another chain).
    #[error("lightwalletd is at block {server_tip}, behind this wallet ({wallet_tip})")]
    ServerBehind { server_tip: u32, wallet_tip: u32 },
    #[error("sync: {0}")]
    Sync(String),
    #[error("proposal: {0}")]
    Proposal(String),
    #[error("invalid payment: {0}")]
    Payment(String),
    /// The vault could cover the payment, but some of its notes are held by open proposals.
    #[error("not enough unreserved funds: part of the balance is held by open proposals")]
    FundsReserved,
    /// The database can't be read with this key: a wrong key, or a plain (unencrypted)
    /// database from before encryption. The wallet is a cache of chain data, so callers
    /// delete it and resync.
    #[error("wallet database: wrong key or not encrypted")]
    WrongKey,
}

fn db_err(e: impl core::fmt::Debug) -> WalletError {
    WalletError::Db(format!("{e:?}"))
}

/// The key of a vault's wallet database (SQLCipher, raw 256-bit key). The app generates
/// one per vault and keeps it in platform secure storage; it is never backed up (a restored
/// vault creates a fresh database and resyncs).
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

/// Opens a connection to the wallet database at `path` (created if missing, unless
/// `read_only`) and unlocks it with `key` before anything else touches it. Fails with
/// [`WalletError::WrongKey`] if the file is not a database encrypted with this key.
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
    // SQLCipher's raw key form: the 32 bytes are the AES key (no passphrase KDF).
    let pragma = zeroize::Zeroizing::new(format!(
        "PRAGMA key = \"x'{}'\";",
        hex::encode(key.as_bytes())
    ));
    conn.execute_batch(&pragma).map_err(db_err)?;
    // Without SQLCipher compiled in, `PRAGMA key` is silently ignored: refuse to go on.
    let cipher: Option<String> = conn
        .query_row("PRAGMA cipher_version", [], |r| r.get(0))
        .ok();
    if cipher.is_none() {
        return Err(WalletError::Db("SQLCipher is not available".into()));
    }
    // The key is only checked when the first page is read.
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

/// A `WalletDb` over an encrypted connection (what `WalletDb::for_path` does, plus the key).
fn open_wallet_db<P: Parameters>(
    path: &Path,
    key: &WalletKey,
    params: P,
) -> Result<WalletDb<rusqlite::Connection, P, SystemClock, OsRng>, WalletError> {
    let conn = open_connection(path, key, false)?;
    rusqlite::vtab::array::load_module(&conn).map_err(db_err)?;
    Ok(WalletDb::from_connection(conn, params, SystemClock, OsRng))
}

/// A regtest network with every upgrade through NU6.3 active at height 1, matching
/// `infra/regtest/zakurad.toml.template`.
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
    }
}

/// Time to establish the lightwalletd connection.
pub const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Per request, until the response starts (a block stream may then run longer).
pub const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const KEEPALIVE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(15);
const KEEPALIVE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Connects to a lightwalletd gRPC endpoint (e.g. `http://127.0.0.1:9067`, or
/// `https://testnet.zec.rocks:443`). `https` endpoints use rustls with the bundled Mozilla
/// roots (webpki-roots). TLS must be configured explicitly: `Channel::from_shared` with an
/// `https` URI otherwise fails with "Connecting to HTTPS without TLS enabled".
///
/// Follows the Tor route policy ([`crate::tor`]): through Tor when it is on (failing with
/// `NetFailure::TorConnecting`/`TorFailed` while it can't be used, never direct), else a
/// direct connection that Tor cuts when it is turned on.
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
            // The Tor client applies its own connect and request timeouts; this bounds a
            // re-bootstrap it may start first.
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
    let mut channel = Channel::from_shared(endpoint.to_owned()).map_err(|e| remote_error(&e))?;
    if endpoint.starts_with("https://") {
        channel = channel
            .tls_config(ClientTlsConfig::new().with_webpki_roots())
            .map_err(|e| remote_error(&e))?;
    }
    // Without these a lightwalletd that accepts the connection but never answers (seen
    // while it was still starting) hangs the sync forever, holding the wallet lock.
    let channel = channel
        .connect_timeout(CONNECT_TIMEOUT)
        .timeout(REQUEST_TIMEOUT)
        .http2_keep_alive_interval(KEEPALIVE_INTERVAL)
        .keep_alive_timeout(KEEPALIVE_TIMEOUT)
        .keep_alive_while_idle(true)
        // Plain TCP from our connector (tonic adds TLS): the connection is cut, and never
        // re-established, once Tor is turned on.
        .connect_with_connector(crate::tor::DirectConnector(lease))
        .await
        .map_err(|e| remote_error(&e))?;
    Ok(CompactTxStreamerClient::new(channel))
}

/// Upper bound for opening a lightwalletd channel through Tor.
pub const TOR_CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(90);

/// tonic's errors say only "transport error"; keep the causes (e.g. a certificate error).
fn remote_error(e: &(dyn std::error::Error + 'static)) -> WalletError {
    WalletError::Remote {
        failure: crate::net::NetFailure::of(e),
        message: crate::net::error_chain(e),
    }
}

/// A gRPC call's error status: the server's own answer, or the transport's failure.
pub(crate) fn status_error(s: &tonic::Status) -> WalletError {
    WalletError::Remote {
        failure: crate::net::NetFailure::of_status(s),
        message: crate::net::error_chain(s),
    }
}

/// lightwalletd answered with something unusable.
fn server_error(message: impl Into<String>) -> WalletError {
    WalletError::Remote {
        failure: crate::net::NetFailure::Server,
        message: message.into(),
    }
}

/// Checks that lightwalletd serves the vault's network and is not behind `known_height`
/// (the highest block the wallet has). Returns its tip. Names are compared as mainnet or
/// not: test networks (testnet, regtest) don't report names consistently.
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
    if let Some(known) = known_height {
        // A few blocks of slack: a server a moment behind another one is normal.
        if tip.saturating_add(SERVER_LAG_SLACK) < known {
            return Err(WalletError::ServerBehind {
                server_tip: tip,
                wallet_tip: known,
            });
        }
    }
    Ok(tip)
}

/// How far lightwalletd may trail the wallet before `check_server` calls it behind.
const SERVER_LAG_SLACK: u32 = 3;

/// The chain tip height lightwalletd reports.
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

/// Txids (protocol byte order) of every transaction in lightwalletd's mempool with
/// shielded data. The whole mempool is read, so the server learns nothing about which
/// transaction the caller is looking for.
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

/// A payment in a proposal.
#[derive(Clone, Debug)]
pub struct PaymentRequest {
    /// Encoded Zcash address (unified, or Orchard-receiver-bearing).
    pub address: String,
    pub amount_zat: u64,
    pub memo: Option<MemoBytes>,
}

/// Money the vault received from outside: one per transaction that pays the vault and
/// spends none of its notes (so never the change of the vault's own payments).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReceivedPayment {
    /// Transaction id, in the usual (byte-reversed) hex display order.
    pub txid: String,
    /// Sum of the vault's non-change outputs in the transaction.
    pub amount_zat: u64,
    /// `None` while unmined (the wallet only learns of unmined transactions it created).
    pub mined_height: Option<u32>,
    /// Unix seconds of the mining block, when the wallet has scanned it.
    pub block_time: Option<u32>,
    /// Text memos of the received outputs (empty and non-text memos are left out).
    pub memos: Vec<String>,
    /// Whether this is a coinbase transaction (mining reward).
    pub coinbase: bool,
}

/// A transaction this wallet saw spending vault notes (mined, or in the mempool).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VaultSpend {
    /// Protocol byte order, like `ProposalState::txid`.
    pub txid: [u8; 32],
    pub mined_height: Option<u32>,
    /// Nullifiers of the vault notes it spends (the ones this wallet knows).
    pub nullifiers: Vec<[u8; 32]>,
}

/// Balances of the vault account, in zatoshis.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VaultBalance {
    pub ironwood_spendable: u64,
    /// Held for open or sent proposals (note reservation); counted in the total.
    pub ironwood_locked: u64,
    /// The vault's own change waiting for confirmations.
    pub ironwood_change_pending: u64,
    /// Other received notes waiting for confirmations (or for scanning).
    pub ironwood_pending: u64,
    pub ironwood_total: u64,
    pub total: u64,
}

pub struct VaultWallet<P: Parameters + Clone + Send + 'static> {
    db: WalletDb<rusqlite::Connection, P, SystemClock, OsRng>,
    params: P,
    account: AccountUuid,
    cache: MemBlockCache,
    path: PathBuf,
    key: WalletKey,
}

/// Notes an open vault proposal spends, held back from new proposals until the proposal's
/// transaction expires (spec §9.1, `reservedNotes`).
#[derive(Clone, Debug)]
pub struct NoteHold {
    /// Stable per proposal (its PCZT hash), so re-reserving is idempotent.
    pub owner: [u8; 32],
    pub nullifiers: Vec<[u8; 32]>,
    /// The transaction's expiry height: after it the notes can't be spent by it any more.
    pub expiry_height: u32,
}

impl<P: Parameters + Clone + Send + Sync + 'static> VaultWallet<P> {
    /// Creates a wallet database at `path`, encrypted with `key`, and imports the vault
    /// UFVK with the given birthday height (the vault's creation height).
    pub async fn create(
        path: &Path,
        key: &WalletKey,
        params: P,
        name: &str,
        ufvk: &UnifiedFullViewingKey,
        birthday_height: u32,
        client: &mut Client,
    ) -> Result<Self, WalletError> {
        // Everything that needs the network happens before the database file exists: a
        // half-created database (no account) would make every later `open` fail.
        let tip: u32 = client
            .get_latest_block(service::ChainSpec::default())
            .await
            .map_err(|e| status_error(&e))?
            .into_inner()
            .height
            .try_into()
            .map_err(|_| server_error("height out of range"))?;
        // Sync downloads the chain state at (range start - 1), and lightwalletd reads a
        // height of 0 as "unspecified", so a wallet cannot be born at height 1.
        if birthday_height < 2 {
            return Err(WalletError::Sync(
                "birthday height must be at least 2".into(),
            ));
        }
        // The creator picks the birthday as its server's tip + 1: a server whose tip is
        // below that is behind (or on another chain) and has no tree state for it.
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

    /// Whether `path` holds a usable vault wallet (a database this key opens, with its
    /// account). Callers use this to decide between `open` and `create`; anything else (a
    /// leftover without an account, a plain database from before encryption, a database
    /// under a lost key) should be deleted and recreated: the wallet is a cache of chain data.
    pub fn exists(path: &Path, key: &WalletKey, params: P) -> bool {
        path.exists() && Self::open(path, key, params).is_ok()
    }

    /// Opens an existing wallet database holding exactly one vault account.
    pub fn open(path: &Path, key: &WalletKey, params: P) -> Result<Self, WalletError> {
        if !path.exists() {
            // Opening would create an empty database.
            return Err(WalletError::Db("no wallet database".into()));
        }
        let db = open_wallet_db(path, key, params.clone())?;
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

    /// Scans until the wallet is at the chain tip.
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

    /// Records a transaction seen in the mempool (unmined) if it concerns the vault: trial
    /// decryption with the vault's viewing key; irrelevant transactions store nothing.
    /// Block sync later sets its mined height (or it expires).
    pub fn store_mempool_tx(
        &mut self,
        tx: &zcash_primitives::transaction::Transaction,
    ) -> Result<(), WalletError> {
        decrypt_and_store_transaction(&self.params, &mut self.db, tx, None).map_err(db_err)
    }

    /// The network this wallet is on.
    pub fn params(&self) -> &P {
        &self.params
    }

    pub fn chain_height(&self) -> Result<Option<u32>, WalletError> {
        Ok(self.db.chain_height().map_err(db_err)?.map(u32::from))
    }

    /// The vault's balance. A vault whose birthday is still ahead of the chain (just
    /// created, no new block yet) has nothing to scan and holds nothing: zero.
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

    /// Payments the vault received, newest first (unmined ones first). Excludes transactions
    /// that spend any vault note: their outputs back to the vault are change (or a self
    /// transfer), not incoming money. Expired unmined transactions are left out.
    pub fn received_payments(&self) -> Result<Vec<ReceivedPayment>, WalletError> {
        received_payments_at(&self.path, &self.key, self.account.expose_uuid().as_bytes())
    }

    /// Every transaction that spent vault notes, as far as this wallet has seen (one row
    /// per spent note, grouped by transaction). Expired unmined ones are left out. Input to
    /// [`crate::spend_watch::unapproved_spends`].
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

    /// Unix time of the block that mined `txid` (protocol byte order), if the wallet has
    /// seen it mined and scanned that block.
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

    /// Whether this wallet has seen transaction `txid` mined (e.g. a vault spend).
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

    /// Makes the wallet's note locks match `holds` exactly, so new proposals skip every note
    /// an open proposal spends, whichever member built it. Holds are applied in order; a
    /// note already held by an earlier one (two proposals raced for it) stays with the
    /// earlier. Nullifiers this wallet doesn't know (not scanned yet, dummy spends) are
    /// skipped. Returns the number of notes held.
    pub fn reserve(&mut self, holds: &[NoteHold]) -> Result<usize, WalletError> {
        let refs = {
            // `WalletDb` doesn't expose its connection: map nullifiers to the outputs that
            // created them on a read-only connection (callers serialize wallet access).
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
            // One note at a time, so a note lost to an earlier hold doesn't release the rest.
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

    /// Selects notes, builds and IO-finalizes a PCZT paying `payments`, with change back to
    /// the vault in the Ironwood pool. Payment outputs are encrypted with the vault's
    /// outgoing viewing key (`OvkPolicy::Sender`), which member verification requires.
    /// The transaction expires `expiry_blocks` after the height it is built for (the vault's
    /// `proposal_expiry_blocks`), so members can approve and sign over days.
    pub fn propose(
        &mut self,
        payments: &[PaymentRequest],
        expiry_blocks: u32,
    ) -> Result<Pczt, WalletError> {
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

        let change = MultiOutputChangeStrategy::new(
            StandardFeeRule::Zip317,
            None,
            ShieldedPool::Ironwood,
            DustOutputPolicy::default(),
            SplitPolicy::single_output(),
        );
        let proposal =
            propose_transfer::<_, _, _, _, zcash_client_sqlite::wallet::commitment_tree::Error>(
                &mut self.db,
                &self.params,
                self.account,
                &GreedyInputSelector::new(),
                &change,
                request,
                ConfirmationsPolicy::default(),
                &SpendPolicy::default(),
                None,
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
            self.account,
            OvkPolicy::Sender,
            &proposal,
            Some(BlockHeight::from(crate::vault::expiry_height(
                u32::from(BlockHeight::from(proposal.min_target_height())),
                expiry_blocks,
            ))),
            BundlePadding::DEFAULT,
        )
        .map_err(|e| WalletError::Proposal(format!("{e:?}")))
    }

    /// Builds and IO-finalizes a PCZT that spends exactly the notes with `nullifiers` (all
    /// of their value minus the ZIP 317 fee) to `recipient`, the vault's own internal
    /// address. Used to make a cancelled proposal unsendable: once this is mined, the
    /// cancelled transaction's notes are spent. Every other note is locked for the
    /// duration of the call, so selection can only pick the target notes.
    pub fn propose_sweep(
        &mut self,
        nullifiers: &[[u8; 32]],
        recipient: ZcashAddress,
        expiry_blocks: u32,
    ) -> Result<Pczt, WalletError> {
        const SWEEP: LockOwner = LockOwner::new(*b"zafe-sweep-temporary-lock-owner!");
        // The proposal's nullifiers include padding (dummy) spends that match no note:
        // only the vault notes among them must be swept.
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
            // Already locked by a live proposal: excluded anyway.
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
            &LockedInputPolicy::Exclude,
            None,
        )
        .map_err(|e| WalletError::Proposal(format!("{e:?}")));
        for output in &others {
            let _ = self.db.unlock_output(output, SWEEP);
        }
        let proposal = proposal?;
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
            self.account,
            OvkPolicy::Sender,
            &proposal,
            Some(BlockHeight::from(crate::vault::expiry_height(
                u32::from(BlockHeight::from(proposal.min_target_height())),
                expiry_blocks,
            ))),
            BundlePadding::DEFAULT,
        )
        .map_err(|e| WalletError::Proposal(format!("{e:?}")))?;
        // Every target note must be spent, or the cancelled transaction stays sendable.
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

/// `VaultWallet::received_payments` for the account with this UUID in the database at `path`.
fn received_payments_at(
    path: &Path,
    key: &WalletKey,
    account: &[u8; 16],
) -> Result<Vec<ReceivedPayment>, WalletError> {
    // `WalletDb` doesn't expose its connection, so read the database's views
    // (`v_received_outputs`, `v_received_output_spends`) on a second, read-only connection.
    // Callers already serialize wallet access.
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

/// The text of a memo, or `None` for an empty, non-text or malformed one.
pub fn memo_text(bytes: &[u8]) -> Option<String> {
    let memo = zcash_protocol::memo::Memo::try_from(MemoBytes::from_bytes(bytes).ok()?).ok()?;
    match memo {
        zcash_protocol::memo::Memo::Text(t) if !t.is_empty() => Some(t.to_string()),
        _ => None,
    }
}

/// In-memory compact block cache for `sync::run`.
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

/// The networks Zafe runs on, selectable at runtime (e.g. from the app's settings).
#[derive(Clone, Copy, Debug)]
pub enum ZafeNetwork {
    Main,
    Test,
    /// Local regtest with every upgrade through NU6.3 at height 1 (`infra/regtest`).
    Regtest(LocalNetwork),
}

impl ZafeNetwork {
    /// "main", "test" or "regtest".
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
            Self::Main => zcash_protocol::consensus::MainNetwork.activation_height(nu),
            Self::Test => zcash_protocol::consensus::TestNetwork.activation_height(nu),
            Self::Regtest(n) => n.activation_height(nu),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::{VaultKeys, VaultSecret};
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

    /// Builds a wallet database with one vault account and hand-written chain data, then
    /// checks which transactions count as received payments.
    #[test]
    #[allow(clippy::type_complexity)]
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
        // (id, txid byte, mined height, tx index, expiry)
        let txs: [(i64, u8, Option<u32>, Option<u32>, u32); 5] = [
            (1, 0xa1, Some(10), Some(1), 50), // external payment, two outputs
            (2, 0xa2, Some(12), Some(0), 0),  // coinbase
            (3, 0xa3, Some(20), Some(1), 60), // the vault's own spend, with change
            (4, 0xa4, None, None, 15),        // unmined and expired
            (5, 0xa5, None, None, 100),       // unmined, still valid
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
        // (note id, tx, action, value, is_change, memo)
        let notes: [(i64, i64, u32, i64, bool, Option<Vec<u8>>); 6] = [
            (1, 1, 0, 100_000, false, Some(text_memo("rent"))),
            (2, 1, 1, 50_000, false, Some(vec![0xf6])),
            (3, 2, 0, 625_000_000, false, None),
            (4, 3, 0, 40_000, true, None),
            (5, 3, 1, 5_000, false, None), // a self-payment inside the vault's own spend
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
        // Transaction 3 spends note 1 (the vault paying someone).
        conn.execute(
            "INSERT INTO ironwood_received_note_spends VALUES (1, 3)",
            [],
        )
        .unwrap();
        drop(conn);

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

    /// The wallet database is encrypted: it reopens with its key, and a wrong key or a plain
    /// database from before encryption fails with `WrongKey` (so callers resync).
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
        // Opening a missing database doesn't create one.
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

        // Nothing readable on disk: no SQLite header, no account name.
        let raw = std::fs::read(&path).unwrap();
        assert!(!raw.starts_with(b"SQLite format 3"));
        assert!(!raw.windows(5).any(|w| w == b"vault"));

        // Reopens with the key (both the wallet and the read-only second connection).
        assert!(VaultWallet::exists(&path, &key, net));
        let wallet = VaultWallet::open(&path, &key, net).unwrap();
        assert_eq!(wallet.balance().unwrap().total, 0);
        assert_eq!(wallet.received_payments().unwrap(), vec![]);
        drop(wallet);

        // A wrong key fails with a typed error, on both kinds of connection.
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

        // A plain database (as written before encryption) doesn't open with a key.
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
