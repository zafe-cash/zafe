#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
    crate::diag::install();
}

#[flutter_rust_bridge::frb(sync)]
pub fn init_log_cache(dir: String) {
    zafe_core::log_cache::configure(dir);
}

#[flutter_rust_bridge::frb(sync)]
pub fn debug_seat_move_wait(secs: u64) {
    if cfg!(debug_assertions) {
        crate::testing::set_seat_move_wait(std::time::Duration::from_secs(secs));
    }
}
