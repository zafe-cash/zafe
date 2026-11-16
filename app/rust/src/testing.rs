use std::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static SEAT_MOVE_WAIT_SECS: AtomicU64 = AtomicU64::new(u64::MAX);

pub fn set_seat_move_wait(wait: Duration) {
    SEAT_MOVE_WAIT_SECS.store(wait.as_secs(), Ordering::Relaxed);
}

pub(crate) fn seat_move_wait() -> Duration {
    match SEAT_MOVE_WAIT_SECS.load(Ordering::Relaxed) {
        u64::MAX => zafe_core::repair::SEAT_MOVE_WAIT,
        secs => Duration::from_secs(secs),
    }
}
