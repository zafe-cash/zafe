use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use tokio::sync::watch;
use zafe_proto::MailboxId;

pub const DEFAULT_WAITS_PER_KEY: usize = 2;
pub const DEFAULT_WAITS_TOTAL: usize = 4096;
pub const WAIT_RETRY_SECS: u64 = 5;

#[derive(Default)]
struct Slots {
    per_key: HashMap<[u8; 32], usize>,
    total: usize,
}

pub(crate) struct Waiters {
    mailboxes: Mutex<HashMap<MailboxId, watch::Sender<u64>>>,
    slots: Mutex<Slots>,
    per_key: usize,
    total: usize,
}

impl Waiters {
    pub(crate) fn new(per_key: usize, total: usize) -> Self {
        Self {
            mailboxes: Mutex::new(HashMap::new()),
            slots: Mutex::new(Slots::default()),
            per_key,
            total,
        }
    }

    pub(crate) fn signal(&self, mailbox: &MailboxId) {
        let mut map = self.mailboxes.lock().expect("lock");
        if let Some(tx) = map.get(mailbox) {
            if tx.receiver_count() == 0 {
                map.remove(mailbox);
            } else {
                tx.send_modify(|n| *n = n.wrapping_add(1));
            }
        }
    }

    pub(crate) fn subscribe(self: &Arc<Self>, mailbox: MailboxId) -> Subscription {
        let rx = self
            .mailboxes
            .lock()
            .expect("lock")
            .entry(mailbox)
            .or_insert_with(|| watch::channel(0).0)
            .subscribe();
        Subscription {
            rx: Some(rx),
            waiters: self.clone(),
            mailbox,
        }
    }

    pub(crate) fn acquire(self: &Arc<Self>, key: [u8; 32]) -> Option<Slot> {
        let mut slots = self.slots.lock().expect("lock");
        let mine = slots.per_key.get(&key).copied().unwrap_or(0);
        if mine >= self.per_key || slots.total >= self.total {
            return None;
        }
        slots.per_key.insert(key, mine + 1);
        slots.total += 1;
        Some(Slot {
            waiters: self.clone(),
            key,
        })
    }

    #[cfg(test)]
    fn watched(&self) -> usize {
        self.mailboxes.lock().expect("lock").len()
    }
}

pub(crate) struct Subscription {
    rx: Option<watch::Receiver<u64>>,
    waiters: Arc<Waiters>,
    mailbox: MailboxId,
}

impl Subscription {
    pub(crate) async fn changed(&mut self) {
        if let Some(rx) = &mut self.rx {
            if rx.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        drop(self.rx.take());
        let mut map = self.waiters.mailboxes.lock().expect("lock");
        if map
            .get(&self.mailbox)
            .is_some_and(|tx| tx.receiver_count() == 0)
        {
            map.remove(&self.mailbox);
        }
    }
}

pub(crate) struct Slot {
    waiters: Arc<Waiters>,
    key: [u8; 32],
}

impl Drop for Slot {
    fn drop(&mut self) {
        let mut slots = self.waiters.slots.lock().expect("lock");
        slots.total = slots.total.saturating_sub(1);
        if let Some(n) = slots.per_key.get_mut(&self.key) {
            *n -= 1;
            if *n == 0 {
                slots.per_key.remove(&self.key);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn a_signal_after_subscribing_wakes_the_waiter() {
        let w = Arc::new(Waiters::new(2, 10));
        let mut sub = w.subscribe([1; 16]);
        w.signal(&[2; 16]);
        w.signal(&[1; 16]);
        tokio::time::timeout(std::time::Duration::from_secs(1), sub.changed())
            .await
            .expect("woken");
        drop(sub);
        assert_eq!(w.watched(), 0, "mailbox forgotten with its last waiter");
    }

    #[test]
    fn slots_are_capped_per_key_and_in_total() {
        let w = Arc::new(Waiters::new(2, 3));
        let a1 = w.acquire([1; 32]).unwrap();
        let _a2 = w.acquire([1; 32]).unwrap();
        assert!(w.acquire([1; 32]).is_none(), "per key");
        let _b = w.acquire([2; 32]).unwrap();
        assert!(w.acquire([3; 32]).is_none(), "total");
        drop(a1);
        assert!(w.acquire([1; 32]).is_some(), "given back on drop");
    }
}
