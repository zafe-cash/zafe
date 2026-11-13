//! Test hooks (outside `api`, so not part of the Dart bridge).

use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

/// `u64::MAX`: no override (the real wait).
static SEAT_MOVE_WAIT_SECS: AtomicU64 = AtomicU64::new(u64::MAX);

/// Makes this process's helpers wait `wait` instead of `repair::SEAT_MOVE_WAIT` before their
/// part can complete a pending seat move (tests pass zero).
pub fn set_seat_move_wait(wait: Duration) {
    SEAT_MOVE_WAIT_SECS.store(wait.as_secs(), Ordering::Relaxed);
}

/// The wait helpers use ([`zafe_core::repair::SEAT_MOVE_WAIT`] unless a test set another).
pub(crate) fn seat_move_wait() -> Duration {
    match SEAT_MOVE_WAIT_SECS.load(Ordering::Relaxed) {
        u64::MAX => zafe_core::repair::SEAT_MOVE_WAIT,
        secs => Duration::from_secs(secs),
    }
}
