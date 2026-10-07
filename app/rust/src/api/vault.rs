//! Vault setup and wallet API for the Flutter app.
//!
//! Keep this surface to primitives and flat structs; all Zcash and FROST
//! types stay inside `zafe-core`. Secrets (identity seeds, vault material) cross as bytes so
//! the Dart side can keep them in platform secure storage; nothing here persists secrets.
//! Calls run on FRB's worker threads; async work uses one shared tokio runtime.

use std::{path::PathBuf, sync::OnceLock, time::Duration};

use rand::rngs::OsRng;
use zafe_core::{
    node::{self, Invite, VaultMaterial},
    nonce_store::FilePoolStore,
    relay_client::RelayClient,
    state_dir,
    wallet::{check_server, connect, latest_height, VaultWallet, WalletKey, ZafeNetwork},
};
use zafe_proto::{Identity, IdentitySeeds, ProtoError};

use super::error::{ZafeEndpoint, ZafeError, ZafeErrorKind};

type Result<T, E = ZafeError> = std::result::Result<T, E>;

pub(crate) fn runtime() -> &'static tokio::runtime::Runtime {
    static RT: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(2)
            .build()
            .expect("tokio runtime")
    })
}

pub(crate) fn network(name: &str) -> Result<ZafeNetwork, ZafeError> {
    ZafeNetwork::from_name(name)
        .ok_or_else(|| ZafeError::invalid(format!("unknown network {name}")))
}

/// Identity seeds as stored by the app (`IdentitySeeds::to_bytes`, versioned).
///
/// Every call that talks to the relay starts here, so this is also where a missing log
/// copy fails closed: without `init_log_cache` the relay would be trusted to remember each
/// vault's log (spec §6.3), and no call may run that way.
pub(crate) fn identity(seeds: &[u8]) -> Result<Identity, ZafeError> {
    if zafe_core::log_cache::configured().is_none() {
        return Err(ZafeError::new(
            ZafeErrorKind::Other,
            "the log copy is not set up (init_log_cache was not called)",
        ));
    }
    match IdentitySeeds::from_bytes(seeds) {
        Ok(seeds) => Ok(Identity::from_seeds(seeds)),
        Err(ProtoError::UnsupportedVersion(v)) => Err(v.into()),
        Err(_) => Err(ZafeError::invalid("invalid identity")),
    }
}

/// Vault material as stored by the app (`VaultMaterial::to_bytes`, versioned).
pub(crate) fn material(bytes: &[u8]) -> Result<VaultMaterial, ZafeError> {
    match VaultMaterial::from_bytes(bytes) {
        Ok(m) => Ok(m),
        Err(e @ node::NodeError::UnsupportedVersion(_)) => Err(e.into()),
        Err(_) => Err(ZafeError::invalid("invalid vault material")),
    }
}

/// A new member identity. `seeds` (versioned, see `IdentitySeeds::to_bytes`) is secret:
/// store it in secure storage.
pub struct IdentityInfo {
    pub seeds: Vec<u8>,
    pub public_key_hex: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn generate_identity() -> IdentityInfo {
    let id = Identity::generate(&mut OsRng);
    IdentityInfo {
        seeds: id.seeds().to_bytes(),
        public_key_hex: hex::encode(id.public().sig_pk),
    }
}

/// The public key (hex) of an identity, e.g. to mark "You" in member lists.
#[flutter_rust_bridge::frb(sync)]
pub fn identity_public_key(seeds: Vec<u8>) -> Result<String, ZafeError> {
    Ok(hex::encode(identity(&seeds)?.public().sig_pk))
}

pub struct InviteInfo {
    /// The vault's id (hex), known from the invite onward; keys the vault on the device.
    pub vault_id: String,
    pub name: String,
    pub threshold: u16,
    pub members: u16,
    pub creator_hex: String,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_invite(invite: String) -> Result<InviteInfo, ZafeError> {
    let i = Invite::decode(&invite)?;
    Ok(InviteInfo {
        vault_id: hex::encode(i.mailbox),
        name: i.name,
        threshold: i.threshold,
        members: i.members,
        creator_hex: hex::encode(i.creator),
    })
}

/// Creates a vault mailbox on the relay and returns the invite string to share.
pub fn create_vault(
    relay_url: String,
    seeds: Vec<u8>,
    name: String,
    threshold: u16,
    members: u16,
) -> Result<String, ZafeError> {
    let me = identity(&seeds)?;
    let relay = RelayClient::new(relay_url);
    let invite = runtime().block_on(node::create_vault(
        &relay, &me, &name, threshold, members, &mut OsRng,
    ))?;
    Ok(invite.encode())
}

pub fn join_vault(relay_url: String, seeds: Vec<u8>, invite: String) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let invite = Invite::decode(&invite)?;
    runtime().block_on(node::join_vault(&RelayClient::new(relay_url), &me, &invite))?;
    Ok(())
}

pub struct MembershipInfo {
    pub members: Vec<String>,
    pub sealed: bool,
    pub safety_number: String,
    pub is_creator: bool,
}

pub fn vault_membership(
    relay_url: String,
    seeds: Vec<u8>,
    invite: String,
) -> Result<MembershipInfo, ZafeError> {
    let me = identity(&seeds)?;
    let invite = Invite::decode(&invite)?;
    let (members, sealed, safety_number) =
        runtime().block_on(node::membership(&RelayClient::new(relay_url), &me, &invite))?;
    Ok(MembershipInfo {
        members: members.iter().map(|m| hex::encode(m.sig_pk)).collect(),
        sealed,
        safety_number,
        is_creator: me.public().sig_pk == invite.creator,
    })
}

/// Creator only: freezes membership once everyone has joined.
pub fn seal_vault(relay_url: String, seeds: Vec<u8>, invite: String) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let invite = Invite::decode(&invite)?;
    runtime().block_on(node::seal(&RelayClient::new(relay_url), &me, &invite))?;
    Ok(())
}

/// Runs key generation. Blocks until every member finishes (or `timeout_secs`). Returns the
/// vault material (secret: store it in secure storage). The creator picks the birthday:
/// `birthday_height`, or lightwalletd's tip + 1 when `None` (the app passes `None`).
/// Then publishes this member's one-tap commitment pool (nonces in `state_dir`, the
/// vault's signing state dir): every member is present at keygen, so every pool is in the
/// log before anyone can propose, and the first payment is one tap. `None` skips it (the
/// pool then waits for the first `list_proposals`); the app always passes the dir.
#[allow(clippy::too_many_arguments)]
pub fn run_keygen(
    relay_url: String,
    lightwalletd_url: String,
    network_name: String,
    seeds: Vec<u8>,
    invite: String,
    confirmed_safety_number: String,
    timeout_secs: u32,
    birthday_height: Option<u32>,
    expiry_days: Option<u32>,
    state_dir: Option<String>,
) -> Result<Vec<u8>, ZafeError> {
    let me = identity(&seeds)?;
    let invite = Invite::decode(&invite)?;
    let net = network(&network_name)?;
    let relay = RelayClient::new(relay_url);
    let material = runtime().block_on(async {
        let birthday = if me.public().sig_pk != invite.creator {
            None
        } else if let Some(h) = birthday_height {
            Some(h.max(2))
        } else {
            let tip = latest_height(&mut connect(&lightwalletd_url).await?).await?;
            Some((tip + 1).max(2))
        };
        let material = node::run_keygen(
            &relay,
            &me,
            &invite,
            &confirmed_safety_number,
            &net,
            net.name(),
            birthday,
            // Creator only (others take it from the creator); 1152 blocks = 1 day.
            expiry_days.map(|d| d * 1152),
            &mut OsRng,
            Duration::from_secs(u64::from(timeout_secs)),
        )
        .await
        .map_err(anyhow::Error::from)?;
        // Best effort: list_proposals tops up again on every refresh.
        if let Some(dir) = state_dir {
            let mut pool = FilePoolStore::new(PathBuf::from(dir).join(state_dir::POOL));
            if let Err(e) = node::top_up_pool(&relay, &me, &material, &mut pool, &mut OsRng).await {
                eprintln!("commitment pool after keygen: {e}");
            }
        }
        Ok::<_, anyhow::Error>(material)
    })?;
    Ok(material.to_bytes()?)
}

pub struct VaultSummary {
    pub vault_id: String,
    pub name: String,
    pub network: String,
    pub address: String,
    pub threshold: u16,
    pub members: Vec<String>,
    pub birthday_height: u32,
}

#[flutter_rust_bridge::frb(sync)]
pub fn vault_summary(material: Vec<u8>) -> Result<VaultSummary, ZafeError> {
    let m = self::material(&material)?;
    let d = &m.descriptor;
    Ok(VaultSummary {
        vault_id: hex::encode(d.vault_id),
        name: d.name.clone(),
        network: d.network.clone(),
        address: d.address.clone(),
        threshold: d.threshold,
        members: d
            .members
            .iter()
            .map(|x| hex::encode(x.identity.sig_pk))
            .collect(),
        birthday_height: d.birthday_height,
    })
}

/// The vault's unified full viewing key (for auditors): it reveals every past and future
/// payment of the vault but can't spend. Derived from this member's material and checked
/// against the descriptor every member signed.
#[flutter_rust_bridge::frb(sync)]
pub fn vault_viewing_key(material: Vec<u8>) -> Result<String, ZafeError> {
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let ufvk = m
        .vault_keys()?
        .ufvk()
        .map_err(|e| ZafeError::new(ZafeErrorKind::Other, e.to_string()))?
        .encode(&net);
    if ufvk != m.descriptor.ufvk {
        return Err(ZafeError::new(
            ZafeErrorKind::Verification,
            "the viewing key doesn't match the vault descriptor",
        ));
    }
    Ok(ufvk)
}

pub struct Balance {
    pub height: u32,
    pub spendable_zat: u64,
    /// Held for open or sent payments until they're mined or expire.
    pub locked_zat: u64,
    /// The vault's own change from a payment, waiting for confirmations.
    pub change_pending_zat: u64,
    /// Received money waiting for confirmations.
    pub incoming_pending_zat: u64,
    pub total_zat: u64,
}

/// Serializes use of the wallet database (sync, propose and reads race otherwise).
pub(crate) fn wallet_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// The vault's wallet database key (32 random bytes the app keeps in secure storage).
pub(crate) fn wallet_key(db_key: &[u8]) -> Result<WalletKey, ZafeError> {
    WalletKey::from_slice(db_key).map_err(|_| ZafeError::invalid("wallet key must be 32 bytes"))
}

/// Deletes a wallet database and SQLite's side files.
fn remove_wallet_files(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut side = path.as_os_str().to_owned();
        side.push(suffix);
        let _ = std::fs::remove_file(side);
    }
}

/// Raw transactions this device broadcast for the vault (resent if they drop out of the
/// mempool), next to its wallet database.
pub(crate) fn sent_txs(db_dir: &str, m: &VaultMaterial) -> node::SentTxs {
    node::SentTxs::in_dir(
        PathBuf::from(db_dir).join(format!("sent-{}", hex::encode(m.descriptor.vault_id))),
    )
}

pub(crate) fn wallet_path(db_dir: &str, m: &VaultMaterial) -> PathBuf {
    PathBuf::from(db_dir).join(format!(
        "vault-{}.sqlite",
        hex::encode(m.descriptor.vault_id)
    ))
}

/// Opens the vault wallet, creating it on first use. Hold `wallet_lock()` while using it.
pub(crate) async fn open_wallet(
    db_dir: &str,
    db_key: &[u8],
    lightwalletd_url: &str,
    m: &VaultMaterial,
) -> Result<VaultWallet<ZafeNetwork>, ZafeError> {
    let net = network(&m.descriptor.network)?;
    let key = wallet_key(db_key)?;
    let path = wallet_path(db_dir, m);
    if VaultWallet::exists(&path, &key, net) {
        return Ok(VaultWallet::open(&path, &key, net)?);
    }
    // A leftover without its account, a plain database from before encryption, or one
    // under a lost key: the wallet only caches chain data, so resync from the birthday.
    remove_wallet_files(&path);
    let mut client = connect(lightwalletd_url).await?;
    let ufvk = m.vault_keys()?.ufvk().map_err(anyhow::Error::from)?;
    Ok(VaultWallet::create(
        &path,
        &key,
        net,
        &m.descriptor.name,
        &ufvk,
        m.descriptor.birthday_height,
        &mut client,
    )
    .await?)
}

/// The chain tip lightwalletd reports: one cheap call. The app runs `sync_vault` only
/// when it moved since the last successful sync (or when something else changed).
pub fn chain_tip(lightwalletd_url: String) -> Result<u32, ZafeError> {
    runtime().block_on(async { Ok(latest_height(&mut connect(&lightwalletd_url).await?).await?) })
}

/// Upper bound for one sync pass (see `sync_vault`).
const SYNC_PASS_TIMEOUT: Duration = Duration::from_secs(300);

/// Syncs the vault wallet (creating its database under `db_dir` on first use), then holds
/// back the notes that live proposals spend, so `spendable_zat` is what a new proposal can
/// use. Holds are best effort: if the relay can't be reached, the previous ones stay.
pub fn sync_vault(
    db_dir: String,
    db_key: Vec<u8>,
    lightwalletd_url: String,
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
) -> Result<Balance, ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let _guard = wallet_lock();
    runtime().block_on(async {
        let mut wallet = open_wallet(&db_dir, &db_key, &lightwalletd_url, &m).await?;
        let mut client = connect(&lightwalletd_url).await?;
        // A server on another network or behind this wallet would fail the sync with an
        // obscure error (or none): say so instead.
        check_server(&mut client, &m.descriptor.network, wallet.chain_height()?).await?;
        // One pass is capped so a stalled stream can't hold the wallet lock forever; sync is
        // incremental, so the next pass continues where this one stopped.
        tokio::time::timeout(SYNC_PASS_TIMEOUT, wallet.sync(&mut client))
            .await
            .map_err(|_| {
                ZafeError::new(
                    ZafeErrorKind::NetworkTimeout,
                    "sync took too long; it will continue on the next try",
                )
                .at(ZafeEndpoint::Lightwalletd)
            })??;
        if wallet.chain_height()?.is_some() {
            // Locks persist in the wallet database, so a failure keeps the last holds.
            if let Ok((_, state)) = node::load_state(&RelayClient::new(relay_url), &me, &m).await {
                let sent = sent_txs(&db_dir, &m);
                let _ = node::reserve_notes(&state, &mut wallet, &mut client, &sent).await;
            }
        }
        let b = wallet.balance()?;
        // Before the vault's birthday block exists the wallet has no chain height yet.
        let height = match wallet.chain_height()? {
            Some(h) => h,
            None => latest_height(&mut client).await?,
        };
        Ok(Balance {
            height,
            spendable_zat: b.ironwood_spendable,
            locked_zat: b.ironwood_locked,
            change_pending_zat: b.ironwood_change_pending,
            incoming_pending_zat: b.ironwood_pending,
            total_zat: b.total,
        })
    })
}

/// Registers this device's push token (FCM on Android, APNs on iOS) for the vault, so the
/// relay can wake it on vault activity. Pushes carry no content.
pub fn register_push(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    platform: String,
    token: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let m = self::material(&material)?;
    let platform = match platform.as_str() {
        "fcm" => zafe_proto::relay::PushPlatform::Fcm,
        "apns" => zafe_proto::relay::PushPlatform::Apns,
        other => return Err(ZafeError::invalid(format!("unknown push platform {other}"))),
    };
    runtime().block_on(RelayClient::new(relay_url).register_push(
        &me,
        m.descriptor.vault_id,
        platform,
        token,
    ))?;
    Ok(())
}
