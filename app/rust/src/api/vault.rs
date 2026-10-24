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
    wallet::{
        check_server, connect_pooled as connect, latest_height, VaultWallet, WalletKey, ZafeNetwork,
    },
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

/// The client for a vault's own relay, with its create token. `default_url` is the
/// app-wide relay setting, used only by vaults from before per-vault relays.
pub(crate) fn vault_relay(default_url: &str, m: &VaultMaterial) -> RelayClient {
    RelayClient::pooled(m.relay.url_or(default_url))
        .with_create_token(m.relay.create_token.as_deref())
}

/// The client for the relay a setup runs on: the invite's (`default_url`, the app-wide
/// setting, for an invite from before invites carried it).
pub(crate) fn invite_relay(
    default_url: &str,
    invite: &Invite,
    create_token: Option<&str>,
) -> RelayClient {
    RelayClient::pooled(invite.relay_url.as_deref().unwrap_or(default_url))
        .with_create_token(create_token)
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
    /// The vault's relay (shown as a fact on the join screen). `None` for an invite from
    /// an older app: the vault is on this phone's default relay.
    pub relay_url: Option<String>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn parse_invite(invite: String) -> Result<InviteInfo, ZafeError> {
    let i = Invite::decode(&invite)?;
    Ok(InviteInfo {
        vault_id: hex::encode(i.vault_id),
        name: i.name,
        threshold: i.threshold,
        members: i.members,
        creator_hex: hex::encode(i.creator),
        relay_url: i.relay_url,
    })
}

/// Creates a vault mailbox on the relay at `relay_url` (the one picked for this vault) and
/// returns the invite string to share. A private relay needs its `create_token`; keep it
/// with the invite and pass it to every [`setup_step`]: the creator seals it to the members
/// with the log key. Fails with `RelayOutdated` on a relay without labelled mailboxes and
/// `CreateTokenRefused` when the token is missing or wrong.
pub fn create_vault(
    relay_url: String,
    seeds: Vec<u8>,
    name: String,
    threshold: u16,
    members: u16,
    create_token: Option<String>,
) -> Result<String, ZafeError> {
    let me = identity(&seeds)?;
    let relay = RelayClient::pooled(relay_url).with_create_token(create_token.as_deref());
    let mut watch = crate::diag::Stopwatch::new("create");
    let invite = runtime().block_on(node::create_vault(
        &relay, &me, &name, threshold, members, &mut OsRng,
    ));
    watch.lap("relay");
    watch.finish();
    Ok(invite?.encode())
}

/// Joins the vault on the invite's relay (`relay_url`, the default relay, only for an
/// invite from an older app).
pub fn join_vault(relay_url: String, seeds: Vec<u8>, invite: String) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let invite = Invite::decode(&invite)?;
    let mut watch = crate::diag::Stopwatch::new("join");
    let joined = runtime().block_on(node::join_vault(
        &invite_relay(&relay_url, &invite, None),
        &me,
        &invite,
    ));
    watch.lap("relay");
    watch.finish();
    joined?;
    Ok(())
}

/// Where a setup stands for this member, after one step of it ([`setup_step`]).
pub struct SetupProgress {
    pub members: Vec<String>,
    pub sealed: bool,
    pub is_creator: bool,
    /// The safety words to compare (4 words), once membership is locked and every
    /// member's part of the check arrived. `None` until then.
    pub safety_number: Option<String>,
    /// Members (key hex) whose part of the safety check hasn't arrived yet.
    pub waiting_for: Vec<String>,
    /// This member confirmed the words: its keys are being made (steps go on in the
    /// background).
    pub started: bool,
    /// What key generation waits for (e.g. "round-1 packages"), and from whom (key hex):
    /// before every member confirmed, the members who haven't.
    pub keygen_stage: Option<String>,
    pub keygen_waiting_for: Vec<String>,
    /// Set once the vault exists: save it in secure storage, then call [`forget_keygen`].
    /// Until then every step returns it again.
    pub material: Option<Vec<u8>>,
}

/// Sealed setups' membership by (relay, mailbox label, member): see `setup_step`.
type SealedKey = (String, zafe_proto::MailboxId, [u8; 32]);
static SEALED: std::sync::Mutex<std::collections::BTreeMap<SealedKey, node::Membership>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// One step of a vault setup for this member; the setup screen and background checks
/// call it repeatedly, so nobody has to be online at the same time. Locks membership when
/// this member is the creator and every seat is taken. With `confirmed_safety_number`
/// (the words the user compared), starts this member's part of key generation; later
/// steps continue it from `state_dir` without it. The creator's birthday
/// (`birthday_height`, else lightwalletd's tip + 1; the app passes `None`) and a joiner's
/// birthday ceiling are fixed once, when key generation starts. The setup runs on the
/// invite's relay (`relay_url` only for an invite from an older app). The creator passes
/// the relay's `create_token` it created the vault with (`None` on an open relay).
#[allow(clippy::too_many_arguments)]
pub fn setup_step(
    relay_url: String,
    lightwalletd_url: String,
    network_name: String,
    seeds: Vec<u8>,
    invite: String,
    state_dir: String,
    confirmed_safety_number: Option<String>,
    birthday_height: Option<u32>,
    expiry_days: Option<u32>,
    create_token: Option<String>,
) -> Result<SetupProgress, ZafeError> {
    let me = identity(&seeds)?;
    let invite = Invite::decode(&invite)?;
    let net = network(&network_name)?;
    let relay = invite_relay(&relay_url, &invite, create_token.as_deref());
    let dir = PathBuf::from(state_dir);
    let key = zafe_core::nonce_store::NonceKey::derive(&seeds, &invite.vault_id);
    let is_creator = me.public().sig_pk == invite.creator;
    let mut watch = crate::diag::Stopwatch::new("setup");
    // "They match": key generation starts and needs the chain tip (the creator's birthday,
    // a joiner's ceiling). Read it while the relay steps run (each ~1 s over Tor).
    let starting = confirmed_safety_number.is_some()
        && !node::keygen_started(&dir)
        && !(is_creator && birthday_height.is_some());
    let result = runtime().block_on(async {
        let tip = async {
            if !starting {
                return None;
            }
            Some(
                async {
                    let mut client = connect(&lightwalletd_url).await?;
                    Ok::<_, ZafeError>(latest_height(&mut client).await?)
                }
                .await,
            )
        };
        let members = async {
            // Sealed with the words known: members and words can't change any more, so
            // "They match" and key generation steps reuse them (the membership step is
            // ~4 s over Tor). Steps before the tap still ask the relay (a cancelled setup).
            let key = (
                relay.base_url().to_owned(),
                invite.label,
                me.public().sig_pk,
            );
            if confirmed_safety_number.is_some() || node::keygen_started(&dir) {
                if let Some(m) = SEALED.lock().ok().and_then(|c| c.get(&key).cloned()) {
                    return Ok(m);
                }
            }
            let m = node::membership(&relay, &me, &invite).await?;
            let m = node::seal_when_full(&relay, &me, &invite, m).await?;
            if m.sealed && m.safety_words.is_some() {
                if let Ok(mut cache) = SEALED.lock() {
                    cache.insert(key, m.clone());
                }
            }
            Ok::<_, node::NodeError>(m)
        };
        let (m, mut tip) = tokio::join!(members, tip);
        let m = m?;
        watch.lap("members");
        let mut out = SetupProgress {
            members: m.members.iter().map(|m| hex::encode(m.sig_pk)).collect(),
            sealed: m.sealed,
            is_creator,
            safety_number: m.safety_words.clone(),
            waiting_for: m.waiting_for.iter().map(hex::encode).collect(),
            started: node::keygen_started(&dir),
            keygen_stage: None,
            keygen_waiting_for: Vec::new(),
            material: None,
        };
        if m.safety_words.is_none() || !(out.started || confirmed_safety_number.is_some()) {
            return Ok::<_, ZafeError>(out);
        }
        let mut options = node::KeygenOptions::default();
        if !out.started {
            if is_creator {
                let birthday = match (birthday_height, tip.take()) {
                    (Some(h), _) => h,
                    (None, Some(tip)) => tip? + 1,
                    (None, None) => {
                        latest_height(&mut connect(&lightwalletd_url).await?).await? + 1
                    }
                };
                options.creator_birthday_height = Some(birthday.max(2));
                // 1152 75-second blocks = 1 day (scaled for 25 s blocks after NU7).
                options.creator_expiry_blocks = expiry_days.map(|d| d * 1152);
            } else {
                // Joiners refuse a birthday ahead of the chain; best effort, as a server
                // that can't be reached here shouldn't block the setup.
                let tip = match tip.take() {
                    Some(tip) => tip.ok(),
                    None => match connect(&lightwalletd_url).await {
                        Ok(mut client) => latest_height(&mut client).await.ok(),
                        Err(_) => None,
                    },
                };
                options.max_birthday = tip.map(|tip| tip + 1 + node::BIRTHDAY_LEEWAY_BLOCKS);
            }
        }
        let progress = node::keygen_step(
            &relay,
            &me,
            &invite,
            &m,
            &dir,
            &key,
            confirmed_safety_number.as_deref(),
            &net,
            net.name(),
            options,
            &mut OsRng,
        )
        .await?;
        watch.lap("keygen");
        out.started = node::keygen_started(&dir);
        match progress {
            node::KeygenProgress::Waiting { stage, missing } => {
                out.keygen_stage = Some(stage.to_owned());
                out.keygen_waiting_for = missing.iter().map(hex::encode).collect();
            }
            node::KeygenProgress::Done(material) => {
                // Best effort: list_proposals tops up again on every refresh. Published
                // now so the first payment can be one tap.
                let mut pool = FilePoolStore::new(dir.join(state_dir::POOL), key.clone());
                if let Err(e) =
                    node::top_up_pool(&relay, &me, &material, &mut pool, false, None, &mut OsRng)
                        .await
                {
                    eprintln!("commitment pool after keygen: {e}");
                }
                watch.lap("pool");
                out.material = Some(material.to_bytes()?);
            }
        }
        Ok(out)
    });
    watch.finish();
    result
}

/// Deletes this member's key-generation state, once the vault's material is saved (or
/// the setup was abandoned).
pub fn forget_keygen(state_dir: String) -> Result<(), ZafeError> {
    node::forget_keygen(std::path::Path::new(&state_dir))
        .map_err(|e| ZafeError::invalid(format!("setup state: {e}")))
}

/// Creator only, before membership is locked: deletes the setup on the relay so the
/// invite stops working. `false` when the relay had nothing to delete (an older relay, or
/// the setup is already gone); the app forgets the vault locally in every case.
/// Creator only: the invite works for 72 more hours (until the setup locks). Called when
/// the creator shares it again. `false`: the relay doesn't expire invites.
pub fn renew_invite(relay_url: String, seeds: Vec<u8>, invite: String) -> Result<bool, ZafeError> {
    let me = identity(&seeds)?;
    let invite = Invite::decode(&invite)?;
    Ok(runtime().block_on(node::renew_invite(
        &invite_relay(&relay_url, &invite, None),
        &me,
        &invite,
    ))?)
}

pub fn cancel_setup(relay_url: String, seeds: Vec<u8>, invite: String) -> Result<bool, ZafeError> {
    let me = identity(&seeds)?;
    let invite = Invite::decode(&invite)?;
    Ok(runtime().block_on(node::cancel_setup(
        &invite_relay(&relay_url, &invite, None),
        &me,
        &invite,
    ))?)
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
        .encode(&net)
        .map_err(|e| ZafeError::new(ZafeErrorKind::Other, e.to_string()))?;
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
/// Each vault's unspent note count as of this process's last sync (`sync_vault`), so the
/// next pool top-up (`list_proposals`) can size the pool for proposals that spend many
/// notes (`node::pool_target_for_notes`). Unknown until a sync ran in this process.
static NOTE_COUNTS: std::sync::Mutex<std::collections::BTreeMap<[u8; 16], usize>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

pub(crate) fn note_count(vault_id: &[u8; 16]) -> Option<usize> {
    NOTE_COUNTS.lock().ok()?.get(vault_id).copied()
}

/// Each vault's wallet chain height as of this process's last sync, so approving can
/// check a payment's expiry without opening the wallet: that took the wallet lock and
/// waited for any sync in progress (2.3 s on a phone, right after proposing). A height a
/// few seconds old only narrows what is accepted (the upper expiry bound) or signs a
/// payment a block past expiry, which can't be mined.
static SYNCED_TIPS: std::sync::Mutex<std::collections::BTreeMap<[u8; 16], u32>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

pub(crate) fn synced_tip(vault_id: &[u8; 16]) -> Option<u32> {
    SYNCED_TIPS.lock().ok()?.get(vault_id).copied()
}

pub(crate) fn record_synced_tip(vault_id: [u8; 16], tip: u32) {
    if let Ok(mut tips) = SYNCED_TIPS.lock() {
        tips.insert(vault_id, tip);
    }
}

pub(crate) fn wallet_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// The vault's wallet database key (32 random bytes the app keeps in secure storage).
/// The key that encrypts a vault's wallet database, derived from this device's identity
/// seeds and the vault id. Nothing is stored (see `WalletKey::derive`).
#[flutter_rust_bridge::frb(sync)]
pub fn wallet_db_key(seeds: Vec<u8>, vault_id: String) -> Vec<u8> {
    WalletKey::derive(&seeds, &vault_id).as_bytes().to_vec()
}

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
    crate::diag::remember_support_dir(db_dir);
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
    crate::diag::remember_support_dir(&db_dir);
    let mut watch = crate::diag::Stopwatch::new("sync");
    let relay = vault_relay(&relay_url, &m);
    // The network-only steps first, together and without the wallet lock (each is ~1-2 s
    // over Tor; a payment being proposed waits for this lock): the server's tip and
    // network, and the vault log for the note holds.
    let early = runtime().block_on(async {
        let server = async {
            let mut client = connect(&lightwalletd_url).await?;
            // A server on another network says so here (it would fail the sync with an
            // obscure error, or none); one behind this wallet is checked under the lock.
            let tip = check_server(&mut client, &m.descriptor.network, None).await?;
            Ok::<_, ZafeError>((client, tip))
        };
        tokio::join!(server, node::load_state(&relay, &me, &m))
    });
    watch.lap("network");
    let _guard = wallet_lock();
    watch.lap("lock");
    let result = runtime().block_on(async {
        let (server, loaded) = early;
        let (mut client, server_tip) = server?;
        let mut wallet = open_wallet(&db_dir, &db_key, &lightwalletd_url, &m).await?;
        watch.lap("open");
        zafe_core::wallet::check_server_tip(server_tip, wallet.chain_height()?)?;
        // Nothing new on the server and nothing left to scan: skip the scan's round trips.
        if wallet.needs_scan(server_tip)? {
            // One pass is capped so a stalled stream can't hold the wallet lock forever;
            // sync is incremental, so the next pass continues where this one stopped.
            tokio::time::timeout(SYNC_PASS_TIMEOUT, wallet.sync(&mut client))
                .await
                .map_err(|_| {
                    ZafeError::new(
                        ZafeErrorKind::NetworkTimeout,
                        "sync took too long; it will continue on the next try",
                    )
                    .at(ZafeEndpoint::Lightwalletd)
                })??;
            watch.lap("scan");
        } else {
            watch.lap("noscan");
        }
        if wallet.chain_height()?.is_some() {
            // Locks persist in the wallet database, so a failure keeps the last holds.
            if let Ok((_, state)) = loaded {
                let sent = sent_txs(&db_dir, &m);
                let _ = node::reserve_notes(&state, &mut wallet, &mut client, &sent).await;
                watch.lap("holds");
            }
        }
        let b = wallet.balance()?;
        if let (Ok(n), Ok(mut counts)) = (wallet.unspent_note_count(), NOTE_COUNTS.lock()) {
            counts.insert(m.descriptor.vault_id, n);
        }
        if let Ok(Some(h)) = wallet.chain_height() {
            record_synced_tip(m.descriptor.vault_id, h);
        }
        watch.lap("balance");
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
    });
    watch.finish();
    result
}

/// Registers this device's push token (FCM on Android, APNs on iOS) for the vault, so the
/// relay can wake it on vault activity. Pushes carry no content. On the vault's own relay
/// and mailbox: from its `material`, else (during setup, so setup steps can be woken by
/// pushes) from its `invite`; with neither, `vault_id` on `relay_url` (a vault from before
/// labels).
pub fn register_push(
    relay_url: String,
    seeds: Vec<u8>,
    vault_id: String,
    material: Option<Vec<u8>>,
    invite: Option<String>,
    platform: String,
    token: String,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let vault_id: [u8; 16] = hex::decode(&vault_id)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| ZafeError::invalid("bad vault id"))?;
    let (relay, mailbox) = match (material, invite) {
        (Some(bytes), _) => {
            let m = self::material(&bytes)?;
            (vault_relay(&relay_url, &m), m.label())
        }
        (None, Some(text)) => {
            let i = Invite::decode(&text)?;
            (invite_relay(&relay_url, &i, None), i.label)
        }
        (None, None) => (RelayClient::pooled(relay_url), vault_id),
    };
    let platform = match platform.as_str() {
        "fcm" => zafe_proto::relay::PushPlatform::Fcm,
        "fcm-fid" => zafe_proto::relay::PushPlatform::FcmFid,
        "apns" => zafe_proto::relay::PushPlatform::Apns,
        other => return Err(ZafeError::invalid(format!("unknown push platform {other}"))),
    };
    runtime().block_on(relay.register_push(&me, mailbox, platform, token))?;
    Ok(())
}

/// Where a vault lives (Settings: "Relay · everyone in Grants").
pub struct VaultRelaySettings {
    /// The vault's relay (the app-wide one for a vault from before per-vault relays).
    pub url: String,
    /// The vault has no relay of its own and follows the app-wide setting.
    pub uses_default: bool,
    /// The relay's mailbox id for the vault (hex); the vault id for older vaults.
    pub label: String,
    /// A create token is stored for the vault's relay (the token itself stays here).
    pub has_create_token: bool,
}

#[flutter_rust_bridge::frb(sync)]
pub fn vault_relay_settings(
    material: Vec<u8>,
    default_relay_url: String,
) -> Result<VaultRelaySettings, ZafeError> {
    let m = self::material(&material)?;
    Ok(VaultRelaySettings {
        url: m.relay.url_or(&default_relay_url).to_owned(),
        uses_default: m.relay.url.is_none(),
        label: hex::encode(m.label()),
        has_create_token: m.relay.create_token.is_some(),
    })
}

/// The vault's material with `token` stored as its relay's create token (`None` or blank
/// clears it), e.g. typed in before restoring the vault on a private relay. Save the
/// result in place of the material.
#[flutter_rust_bridge::frb(sync)]
pub fn set_vault_create_token(
    material: Vec<u8>,
    token: Option<String>,
) -> Result<Vec<u8>, ZafeError> {
    let mut m = self::material(&material)?;
    m.relay.create_token = token.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    Ok(m.to_bytes()?)
}

/// When this phone last reached the vault's relay (Unix seconds), and since when it
/// can't: for "Can't reach relay · since 09:14". Both `None` before the first request.
pub struct RelayStatusInfo {
    pub last_reached: Option<u64>,
    pub unreachable_since: Option<u64>,
}

#[flutter_rust_bridge::frb(sync)]
pub fn vault_relay_status(vault_id: String) -> Result<RelayStatusInfo, ZafeError> {
    let vault: [u8; 16] = hex::decode(&vault_id)
        .ok()
        .and_then(|b| b.try_into().ok())
        .ok_or_else(|| ZafeError::invalid("bad vault id"))?;
    let status = zafe_core::log_cache::configured()
        .map(|c| c.relay_status(&vault))
        .unwrap_or_default();
    Ok(RelayStatusInfo {
        last_reached: status.last_reached,
        unreachable_since: status.unreachable_since,
    })
}
