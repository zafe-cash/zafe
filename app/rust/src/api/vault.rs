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

pub(crate) fn vault_relay(default_url: &str, m: &VaultMaterial) -> RelayClient {
    RelayClient::pooled(m.relay.url_or(default_url))
        .with_create_token(m.relay.create_token.as_deref())
}

pub(crate) fn invite_relay(
    default_url: &str,
    invite: &Invite,
    create_token: Option<&str>,
) -> RelayClient {
    RelayClient::pooled(invite.relay_url.as_deref().unwrap_or(default_url))
        .with_create_token(create_token)
}

pub(crate) fn material(bytes: &[u8]) -> Result<VaultMaterial, ZafeError> {
    match VaultMaterial::from_bytes(bytes) {
        Ok(m) => Ok(node::with_followed_relay(&m)),
        Err(e @ node::NodeError::UnsupportedVersion(_)) => Err(e.into()),
        Err(_) => Err(ZafeError::invalid("invalid vault material")),
    }
}

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

#[flutter_rust_bridge::frb(sync)]
pub fn identity_public_key(seeds: Vec<u8>) -> Result<String, ZafeError> {
    Ok(hex::encode(identity(&seeds)?.public().sig_pk))
}

pub struct InviteInfo {
    pub vault_id: String,
    pub name: String,
    pub threshold: u16,
    pub members: u16,
    pub creator_hex: String,
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

pub struct SetupProgress {
    pub members: Vec<String>,
    pub sealed: bool,
    pub is_creator: bool,
    pub safety_number: Option<String>,
    pub waiting_for: Vec<String>,
    pub started: bool,
    pub keygen_stage: Option<String>,
    pub keygen_waiting_for: Vec<String>,
    pub material: Option<Vec<u8>>,
}

type SealedKey = (String, zafe_proto::MailboxId, [u8; 32]);
static SEALED: std::sync::Mutex<std::collections::BTreeMap<SealedKey, node::Membership>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

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
            let sealed = (
                relay.base_url().to_owned(),
                invite.label,
                me.public().sig_pk,
            );
            if confirmed_safety_number.is_some() || node::keygen_started(&dir) {
                if let Some(m) = SEALED.lock().ok().and_then(|c| c.get(&sealed).cloned()) {
                    return Ok(m);
                }
            }
            let m = node::membership(&relay, &me, &invite, &dir, &key).await?;
            let m = node::seal_when_full(&relay, &me, &invite, m, &dir, &key).await?;
            if m.sealed && m.safety_words.is_some() {
                if let Ok(mut cache) = SEALED.lock() {
                    cache.insert(sealed, m.clone());
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
                options.creator_expiry_blocks = expiry_days.map(|d| d * 1152);
            } else {
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

pub fn forget_keygen(state_dir: String) -> Result<(), ZafeError> {
    node::forget_keygen(std::path::Path::new(&state_dir))
        .map_err(|e| ZafeError::invalid(format!("setup state: {e}")))
}

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
        address: node::ufvk_address(d)?,
        threshold: d.threshold,
        members: d
            .members
            .iter()
            .map(|x| hex::encode(x.identity.sig_pk))
            .collect(),
        birthday_height: d.birthday_height,
    })
}

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
    pub locked_zat: u64,
    pub change_pending_zat: u64,
    pub incoming_pending_zat: u64,
    pub total_zat: u64,
}

static NOTE_COUNTS: std::sync::Mutex<std::collections::BTreeMap<[u8; 16], usize>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

pub(crate) fn note_count(vault_id: &[u8; 16]) -> Option<usize> {
    NOTE_COUNTS.lock().ok()?.get(vault_id).copied()
}

static SYNCED: std::sync::Mutex<std::collections::BTreeMap<[u8; 16], node::ChainView>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

pub(crate) fn synced_tip(vault_id: &[u8; 16]) -> Option<u32> {
    Some(SYNCED.lock().ok()?.get(vault_id)?.tip)
}

pub(crate) fn synced_view(vault_id: &[u8; 16]) -> Option<node::ChainView> {
    SYNCED.lock().ok()?.get(vault_id).cloned()
}

pub(crate) fn record_synced(vault_id: [u8; 16], view: node::ChainView) {
    if let Ok(mut views) = SYNCED.lock() {
        views.insert(vault_id, view);
    }
}

pub(crate) fn wallet_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

#[flutter_rust_bridge::frb(sync)]
pub fn wallet_db_key(seeds: Vec<u8>, vault_id: String) -> Vec<u8> {
    WalletKey::derive(&seeds, &vault_id).as_bytes().to_vec()
}

pub(crate) fn wallet_key(db_key: &[u8]) -> Result<WalletKey, ZafeError> {
    WalletKey::from_slice(db_key).map_err(|_| ZafeError::invalid("wallet key must be 32 bytes"))
}

fn remove_wallet_files(path: &std::path::Path) {
    let _ = std::fs::remove_file(path);
    for suffix in ["-journal", "-wal", "-shm"] {
        let mut side = path.as_os_str().to_owned();
        side.push(suffix);
        let _ = std::fs::remove_file(side);
    }
}

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

pub fn chain_tip(lightwalletd_url: String) -> Result<u32, ZafeError> {
    runtime().block_on(async { Ok(latest_height(&mut connect(&lightwalletd_url).await?).await?) })
}

const SYNC_PASS_TIMEOUT: Duration = Duration::from_secs(300);

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
    let early = runtime().block_on(async {
        let server = async {
            let mut client = connect(&lightwalletd_url).await?;
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
        if wallet.needs_scan(server_tip)? {
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
        let state = loaded.ok().map(|(_, state)| state);
        if wallet.chain_height()?.is_some() {
            if let Some(state) = &state {
                let sent = sent_txs(&db_dir, &m);
                let _ = node::reserve_notes(
                    state,
                    &me.public().sig_pk,
                    &mut wallet,
                    &mut client,
                    &sent,
                )
                .await;
                watch.lap("holds");
            }
        }
        let b = wallet.balance()?;
        if let (Ok(n), Ok(mut counts)) = (wallet.unspent_note_count(), NOTE_COUNTS.lock()) {
            counts.insert(m.descriptor.vault_id, n);
        }
        if let (Ok(view), Some(state)) = (node::ChainView::of(&wallet), &state) {
            if let Ok(view) = view.with_mempool(state, &mut client).await {
                record_synced(m.descriptor.vault_id, view);
            }
        }
        watch.lap("balance");
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

pub struct VaultRelaySettings {
    pub url: String,
    pub uses_default: bool,
    pub label: String,
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

#[flutter_rust_bridge::frb(sync)]
pub fn set_vault_create_token(
    material: Vec<u8>,
    token: Option<String>,
) -> Result<Vec<u8>, ZafeError> {
    let mut m = self::material(&material)?;
    m.relay.create_token = token.map(|t| t.trim().to_owned()).filter(|t| !t.is_empty());
    Ok(m.to_bytes()?)
}

#[flutter_rust_bridge::frb(sync)]
pub fn pin_vault_relay(material: Vec<u8>, relay_url: String) -> Result<Option<Vec<u8>>, ZafeError> {
    let mut m = self::material(&material)?;
    if m.relay.url.is_some() {
        return Ok(None);
    }
    let url = zafe_core::moves::normalize_relay_url(&relay_url)
        .ok_or_else(|| ZafeError::invalid("not a relay address"))?;
    m.relay.url = Some(url);
    Ok(Some(m.to_bytes()?))
}

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
