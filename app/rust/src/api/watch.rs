//! Live vault activity while the app is open: long polls on the relay (`/v1/wait`), so
//! other members' proposals, votes and signing requests show up at once instead of on
//! the next poll. The app refreshes when an `Activity` event arrives; background checks
//! stay on push notifications and WorkManager.
//!
//! One watch runs per process. The app numbers its watches with increasing ids: starting
//! one stops every older one, and [`stop_vault_watch`] stops that id and older ones. Ids
//! make the order of calls irrelevant: a stop that overtakes its start (Dart's sync stop
//! can run before the async start reaches Rust) still wins. The loop runs on the
//! bridge's tokio runtime, so it holds no bridge worker thread.

use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};

use tokio::sync::watch;
use zafe_core::relay_client::{RelayClient, RelayClientError};
use zafe_proto::{relay::WaitResponse, Identity, MailboxId};

use super::{
    error::ZafeError,
    vault::{identity, material, runtime},
};
use crate::frb_generated::StreamSink;

/// How long each long poll asks the relay to hold (the relay caps it at 25 s).
const WAIT: Duration = Duration::from_secs(25);
/// Longest pause between attempts after failures.
const MAX_BACKOFF_SECS: u64 = 60;
/// A wait that ends this fast without news is paced, so a misbehaving relay can't make
/// the loop spin.
const MIN_ROUND: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VaultActivityKind {
    /// The watch reaches the relay (first time, or again after failures): activity will
    /// be reported as it happens.
    Connected,
    /// The vault log grew or something was delivered to this member: refresh.
    Activity,
    /// The relay has no long polls (older relay): the watch has ended; keep polling.
    Unsupported,
    /// An attempt failed (`error`); the watch retries in `retry_in_secs`. When
    /// `retry_in_secs` is 0 the watch has ended (e.g. a version mismatch or bad input).
    Failed,
}

#[derive(Clone, Debug)]
pub struct VaultActivity {
    pub kind: VaultActivityKind,
    pub error: Option<ZafeError>,
    pub retry_in_secs: u32,
}

impl VaultActivity {
    fn of(kind: VaultActivityKind) -> Self {
        Self {
            kind,
            error: None,
            retry_in_secs: 0,
        }
    }

    fn failed(error: ZafeError, retry_in_secs: u64) -> Self {
        Self {
            kind: VaultActivityKind::Failed,
            error: Some(error),
            retry_in_secs: u32::try_from(retry_in_secs).unwrap_or(u32::MAX),
        }
    }
}

/// The newest watch id started, and the newest id stopped. The watch `id` runs while
/// `latest == id && stopped < id`.
#[flutter_rust_bridge::frb(ignore)]
#[derive(Clone, Copy, Default)]
struct Watches {
    latest: i64,
    stopped: i64,
}

impl Watches {
    fn runs(&self, id: i64) -> bool {
        self.latest == id && self.stopped < id
    }
}

fn watches() -> &'static watch::Sender<Watches> {
    static WATCHES: OnceLock<watch::Sender<Watches>> = OnceLock::new();
    WATCHES.get_or_init(|| watch::channel(Watches::default()).0)
}

/// Watches the vault for activity until `stop_vault_watch(watch_id)` or a watch with a
/// higher `watch_id` starts. Returns at once; events arrive on the stream, failures
/// included (they are events, not errors). The stream closes when the watch ends.
pub fn watch_vault(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    watch_id: i64,
    sink: StreamSink<VaultActivity>,
) -> Result<(), ZafeError> {
    let events = sink.clone();
    if let Err(e) = watch_vault_with(relay_url, seeds, material, watch_id, move |event| {
        events.add(event).is_ok()
    }) {
        let _ = sink.add(VaultActivity::failed(e, 0));
    }
    Ok(())
}

/// Stops the watch `watch_id` and any older one (also one whose start hasn't arrived
/// yet).
#[flutter_rust_bridge::frb(sync)]
pub fn stop_vault_watch(watch_id: i64) {
    watches().send_modify(|w| w.stopped = w.stopped.max(watch_id));
}

/// `watch_vault` with a plain callback (tests, non-Flutter callers). The callback
/// returns `false` to stop the watch.
#[flutter_rust_bridge::frb(ignore)]
pub fn watch_vault_with(
    relay_url: String,
    seeds: Vec<u8>,
    material: Vec<u8>,
    watch_id: i64,
    on_event: impl FnMut(VaultActivity) -> bool + Send + 'static,
) -> Result<(), ZafeError> {
    let me = identity(&seeds)?;
    let mailbox = self::material(&material)?.descriptor.vault_id;
    watches().send_modify(|w| w.latest = w.latest.max(watch_id));
    runtime().spawn(watch_loop(
        RelayClient::pooled(relay_url),
        me,
        mailbox,
        watch_id,
        on_event,
    ));
    Ok(())
}

/// Seconds to wait before retrying after `failures` failures in a row, or `None` when
/// retrying can't help.
fn backoff(error: &RelayClientError, failures: u32) -> Option<u64> {
    match error {
        RelayClientError::VersionRejected { .. } | RelayClientError::UnsupportedVersion(_) => None,
        RelayClientError::RateLimited { retry_after_secs } => {
            Some((*retry_after_secs).clamp(1, MAX_BACKOFF_SECS))
        }
        _ => Some((1u64 << failures.min(6)).min(MAX_BACKOFF_SECS)),
    }
}

async fn watch_loop(
    relay: RelayClient,
    me: Identity,
    mailbox: MailboxId,
    id: i64,
    mut on_event: impl FnMut(VaultActivity) -> bool,
) {
    let mut stop = watches().subscribe();
    // Where the vault was at the last answer; `None` until the first one.
    let mut seen: Option<WaitResponse> = None;
    let mut connected = false;
    let mut failures = 0u32;
    loop {
        // The first request only reads where things are (no wait); then each one waits
        // for news past that point, so nothing that happens in between is missed.
        let (log_len, inbox_after, wait) = match seen {
            None => (u64::MAX, u64::MAX, Duration::ZERO),
            Some(s) => (s.log_len, s.inbox_cursor, WAIT),
        };
        let started = Instant::now();
        let result = tokio::select! {
            _ = stop.wait_for(|w| !w.runs(id)) => return,
            r = relay.wait_for_activity(&me, mailbox, log_len, inbox_after, wait) => r,
        };
        let mut pause = Duration::ZERO;
        let event = match result {
            Ok(Some(now)) => {
                failures = 0;
                let news = seen.is_some_and(|s| now.is_news(s.log_len, s.inbox_cursor));
                seen = Some(now);
                let was_connected = std::mem::replace(&mut connected, true);
                if !news && started.elapsed() < MIN_ROUND && wait > Duration::ZERO {
                    pause = MIN_ROUND;
                }
                if news {
                    Some(VaultActivity::of(VaultActivityKind::Activity))
                } else if !was_connected {
                    Some(VaultActivity::of(VaultActivityKind::Connected))
                } else {
                    None
                }
            }
            Ok(None) => {
                on_event(VaultActivity::of(VaultActivityKind::Unsupported));
                return;
            }
            Err(e) => {
                failures = failures.saturating_add(1);
                connected = false;
                let retry = backoff(&e, failures);
                let event = VaultActivity::failed(e.into(), retry.unwrap_or(0));
                match retry {
                    Some(secs) => {
                        pause = Duration::from_secs(secs);
                        Some(event)
                    }
                    None => {
                        on_event(event);
                        return;
                    }
                }
            }
        };
        if let Some(event) = event {
            if !on_event(event) {
                return;
            }
        }
        if !pause.is_zero() {
            tokio::select! {
                _ = stop.wait_for(|w| !w.runs(id)) => return,
                _ = tokio::time::sleep(pause) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_and_respects_the_relay() {
        let transport = || RelayClientError::Encoding;
        assert_eq!(backoff(&transport(), 1), Some(2));
        assert_eq!(backoff(&transport(), 3), Some(8));
        assert_eq!(backoff(&transport(), 30), Some(MAX_BACKOFF_SECS));
        assert_eq!(
            backoff(
                &RelayClientError::RateLimited {
                    retry_after_secs: 7
                },
                1
            ),
            Some(7)
        );
        assert_eq!(
            backoff(
                &RelayClientError::VersionRejected {
                    format: zafe_proto::version::Format::RelayApi,
                    ours: 1,
                    relay_supports: 2
                },
                1
            ),
            None
        );
    }
}
