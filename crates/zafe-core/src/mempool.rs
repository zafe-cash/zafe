use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

use zcash_client_backend::{decrypt_transaction, proto::service};
use zcash_keys::keys::UnifiedFullViewingKey;
pub use zcash_primitives::transaction::{Transaction, TxId};
use zcash_protocol::consensus::{BlockHeight, BranchId, Parameters};

use crate::wallet::{connect, latest_height, WalletError};

pub const BLOCK_RECONNECT: Duration = Duration::from_secs(1);
pub const CANCEL_POLL: Duration = Duration::from_millis(100);
const SEEN_CAPACITY: usize = 4096;

#[derive(Debug)]
pub enum MempoolEvent {
    Connected,
    Stored {
        txid: TxId,
    },
    StoreFailed {
        txid: TxId,
        error: WalletError,
    },
    Disconnected {
        error: WalletError,
        retry_in: Duration,
    },
}

#[derive(Debug, Default)]
pub struct Backoff {
    failures: u32,
}

impl Backoff {
    pub const FIRST: Duration = Duration::from_secs(1);
    pub const MAX: Duration = Duration::from_secs(30);

    pub fn failed(&mut self) -> Duration {
        let delay = Self::FIRST
            .saturating_mul(1u32 << self.failures.min(16))
            .min(Self::MAX);
        self.failures = self.failures.saturating_add(1);
        delay
    }

    pub fn reset(&mut self) {
        self.failures = 0;
    }
}

#[derive(Debug)]
struct SeenTxids {
    capacity: usize,
    order: VecDeque<TxId>,
    set: HashSet<TxId>,
}

impl SeenTxids {
    fn new(capacity: usize) -> Self {
        Self {
            capacity,
            order: VecDeque::new(),
            set: HashSet::new(),
        }
    }

    fn contains(&self, txid: &TxId) -> bool {
        self.set.contains(txid)
    }

    fn insert(&mut self, txid: TxId) {
        if self.capacity == 0 || !self.set.insert(txid) {
            return;
        }
        self.order.push_back(txid);
        while self.order.len() > self.capacity {
            if let Some(old) = self.order.pop_front() {
                self.set.remove(&old);
            }
        }
    }
}

pub fn parse_mempool_tx<P: Parameters>(params: &P, tip: u32, data: &[u8]) -> Option<Transaction> {
    let branch = BranchId::for_height(params, BlockHeight::from_u32(tip.saturating_add(1)));
    Transaction::read(data, branch).ok()
}

pub fn is_vault_tx<P: Parameters>(
    params: &P,
    tip: u32,
    tx: &Transaction,
    ufvk: &UnifiedFullViewingKey,
) -> bool {
    let ufvks = HashMap::from([((), ufvk.clone())]);
    decrypt_transaction(params, None, Some(BlockHeight::from_u32(tip)), tx, &ufvks)
        .has_decrypted_outputs()
}

async fn until_cancelled(cancelled: &impl Fn() -> bool) {
    while !cancelled() {
        tokio::time::sleep(CANCEL_POLL).await;
    }
}

async fn sleep_unless_cancelled(delay: Duration, cancelled: &impl Fn() -> bool) -> bool {
    tokio::select! {
        biased;
        _ = until_cancelled(cancelled) => false,
        _ = tokio::time::sleep(delay) => true,
    }
}

pub async fn watch<P, C, S, E>(
    endpoint: &str,
    params: &P,
    ufvk: &UnifiedFullViewingKey,
    cancelled: C,
    store: S,
    mut emit: E,
) where
    P: Parameters,
    C: Fn() -> bool,
    S: Fn(Transaction) -> Result<(), WalletError> + Send + Sync + 'static,
    E: FnMut(MempoolEvent),
{
    let store = Arc::new(store);
    let mut backoff = Backoff::default();
    let mut seen = SeenTxids::new(SEEN_CAPACITY);
    loop {
        if cancelled() {
            return;
        }
        let open = async {
            let mut client = connect(endpoint).await?;
            let tip = latest_height(&mut client).await?;
            let stream = client
                .get_mempool_stream(service::Empty {})
                .await
                .map_err(|s| crate::wallet::status_error(&s))?
                .into_inner();
            Ok::<_, WalletError>((tip, stream))
        };
        let opened = tokio::select! {
            biased;
            _ = until_cancelled(&cancelled) => return,
            r = open => r,
        };
        let (tip, mut stream) = match opened {
            Ok(x) => x,
            Err(error) => {
                let retry_in = backoff.failed();
                emit(MempoolEvent::Disconnected { error, retry_in });
                if !sleep_unless_cancelled(retry_in, &cancelled).await {
                    return;
                }
                continue;
            }
        };
        emit(MempoolEvent::Connected);

        let ended = loop {
            let message = tokio::select! {
                biased;
                _ = until_cancelled(&cancelled) => return,
                m = stream.message() => m,
            };
            let raw = match message {
                Ok(Some(raw)) => raw,
                Ok(None) => break Ok(()),
                Err(s) => break Err(crate::wallet::status_error(&s)),
            };
            backoff.reset();
            let Some(tx) = parse_mempool_tx(params, tip, &raw.data) else {
                continue;
            };
            let txid = tx.txid();
            if seen.contains(&txid) {
                continue;
            }
            if !is_vault_tx(params, tip, &tx, ufvk) {
                seen.insert(txid);
                continue;
            }
            let store = store.clone();
            let stored = tokio::select! {
                biased;
                _ = until_cancelled(&cancelled) => return,
                r = tokio::task::spawn_blocking(move || store(tx)) => r,
            };
            match stored {
                Ok(Ok(())) => {
                    seen.insert(txid);
                    emit(MempoolEvent::Stored { txid });
                }
                Ok(Err(error)) => emit(MempoolEvent::StoreFailed { txid, error }),
                Err(e) => emit(MempoolEvent::StoreFailed {
                    txid,
                    error: WalletError::Db(e.to_string()),
                }),
            }
        };
        drop(stream);
        let delay = match ended {
            Ok(()) => {
                backoff.reset();
                BLOCK_RECONNECT
            }
            Err(error) => {
                let retry_in = backoff.failed();
                emit(MempoolEvent::Disconnected { error, retry_in });
                retry_in
            }
        };
        if !sleep_unless_cancelled(delay, &cancelled).await {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Instant,
    };

    use super::*;
    use crate::keys::{VaultKeys, VaultSecret};

    #[test]
    fn backoff_doubles_to_thirty_seconds_and_resets() {
        let mut b = Backoff::default();
        let secs: Vec<u64> = (0..8).map(|_| b.failed().as_secs()).collect();
        assert_eq!(secs, [1, 2, 4, 8, 16, 30, 30, 30]);
        b.reset();
        assert_eq!(b.failed(), Backoff::FIRST);
        for _ in 0..100 {
            assert!(b.failed() <= Backoff::MAX);
        }
    }

    #[test]
    fn seen_txids_forget_the_oldest() {
        let id = |b: u8| TxId::from_bytes([b; 32]);
        let mut seen = SeenTxids::new(2);
        seen.insert(id(1));
        seen.insert(id(2));
        seen.insert(id(2));
        assert!(seen.contains(&id(1)) && seen.contains(&id(2)));
        seen.insert(id(3));
        assert!(!seen.contains(&id(1)));
        assert!(seen.contains(&id(2)) && seen.contains(&id(3)));
    }

    fn ufvk() -> UnifiedFullViewingKey {
        use orchard::keys::{FullViewingKey, SpendingKey};
        let ak: [u8; 32] = FullViewingKey::from(&SpendingKey::from_bytes([3u8; 32]).unwrap())
            .to_bytes()[..32]
            .try_into()
            .unwrap();
        VaultKeys::derive(&VaultSecret::from_bytes([9u8; 32]), &ak)
            .unwrap()
            .ufvk()
            .unwrap()
    }

    #[tokio::test]
    async fn cancelled_before_start_returns_without_events() {
        let mut events = 0;
        watch(
            "http://127.0.0.1:1",
            &zcash_protocol::consensus::Network::TestNetwork,
            &ufvk(),
            || true,
            |_| Ok(()),
            |_| events += 1,
        )
        .await;
        assert_eq!(events, 0);
    }

    #[tokio::test]
    async fn unreachable_server_backs_off_and_stops_on_cancel() {
        let cancel = AtomicBool::new(false);
        let mut retries = Vec::new();
        let started = Instant::now();
        watch(
            "http://127.0.0.1:1",
            &zcash_protocol::consensus::Network::TestNetwork,
            &ufvk(),
            || cancel.load(Ordering::Relaxed),
            |_| Ok(()),
            |e| match e {
                MempoolEvent::Disconnected { retry_in, .. } => {
                    retries.push(retry_in);
                    if retries.len() == 2 {
                        cancel.store(true, Ordering::Relaxed);
                    }
                }
                other => panic!("unexpected {other:?}"),
            },
        )
        .await;
        assert_eq!(retries, [Duration::from_secs(1), Duration::from_secs(2)]);
        assert!(started.elapsed() < Duration::from_millis(2500));
    }
}
