use std::sync::atomic::{AtomicU32, Ordering};

use zafe_core::{
    mempool::{self, MempoolEvent as CoreEvent},
    wallet::VaultWallet,
};

use super::{
    error::{ZafeEndpoint, ZafeError, ZafeErrorKind},
    vault::{material, network, runtime, wallet_key, wallet_lock, wallet_path},
};
use crate::frb_generated::StreamSink;

static CURRENT_WATCH: AtomicU32 = AtomicU32::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MempoolStatus {
    Connected,
    Stored,
    StoreFailed,
    Disconnected,
    Failed,
}

pub struct MempoolEvent {
    pub status: MempoolStatus,
    pub txid: Option<String>,
    pub error: Option<ZafeError>,
    pub retry_in_secs: u32,
}

impl MempoolEvent {
    fn status(status: MempoolStatus) -> Self {
        Self {
            status,
            txid: None,
            error: None,
            retry_in_secs: 0,
        }
    }
}

impl From<CoreEvent> for MempoolEvent {
    fn from(e: CoreEvent) -> Self {
        match e {
            CoreEvent::Connected => Self::status(MempoolStatus::Connected),
            CoreEvent::Stored { txid } => Self {
                txid: Some(txid.to_string()),
                ..Self::status(MempoolStatus::Stored)
            },
            CoreEvent::StoreFailed { txid, error } => Self {
                txid: Some(txid.to_string()),
                error: Some(error.into()),
                ..Self::status(MempoolStatus::StoreFailed)
            },
            CoreEvent::Disconnected { error, retry_in } => Self {
                error: Some(ZafeError::from(error).at(ZafeEndpoint::Lightwalletd)),
                retry_in_secs: u32::try_from(retry_in.as_secs()).unwrap_or(u32::MAX),
                ..Self::status(MempoolStatus::Disconnected)
            },
        }
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn begin_mempool_watch() -> u32 {
    CURRENT_WATCH.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
}

#[flutter_rust_bridge::frb(sync)]
pub fn stop_mempool_watch() {
    CURRENT_WATCH.fetch_add(1, Ordering::SeqCst);
}

pub fn watch_mempool(
    watch_id: u32,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    material: Vec<u8>,
    sink: StreamSink<MempoolEvent>,
) -> Result<(), ZafeError> {
    runtime().spawn(async move {
        let result =
            watch_mempool_async(watch_id, lightwalletd_url, db_dir, db_key, material, |e| {
                let _ = sink.add(e);
            })
            .await;
        if let Err(e) = result {
            let _ = sink.add(MempoolEvent {
                error: Some(e),
                ..MempoolEvent::status(MempoolStatus::Failed)
            });
        }
    });
    Ok(())
}

#[flutter_rust_bridge::frb(ignore)]
pub fn watch_mempool_with(
    watch_id: u32,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    material: Vec<u8>,
    on_event: impl FnMut(MempoolEvent) + Send,
) -> Result<(), ZafeError> {
    runtime().block_on(watch_mempool_async(
        watch_id,
        lightwalletd_url,
        db_dir,
        db_key,
        material,
        on_event,
    ))
}

async fn watch_mempool_async(
    watch_id: u32,
    lightwalletd_url: String,
    db_dir: String,
    db_key: Vec<u8>,
    material: Vec<u8>,
    mut on_event: impl FnMut(MempoolEvent) + Send,
) -> Result<(), ZafeError> {
    let m = self::material(&material)?;
    let net = network(&m.descriptor.network)?;
    let key = wallet_key(&db_key)?;
    let ufvk = m
        .vault_keys()?
        .ufvk()
        .map_err(|e| ZafeError::new(ZafeErrorKind::Other, e.to_string()))?;
    let path = wallet_path(&db_dir, &m);
    let store = move |tx: mempool::Transaction| {
        let _guard = wallet_lock();
        VaultWallet::open(&path, &key, net)?.store_mempool_tx(&tx)
    };
    mempool::watch(
        &lightwalletd_url,
        &net,
        &ufvk,
        || CURRENT_WATCH.load(Ordering::SeqCst) != watch_id,
        store,
        |e| on_event(e.into()),
    )
    .await;
    Ok(())
}
