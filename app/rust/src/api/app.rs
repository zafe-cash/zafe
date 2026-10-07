//! App lifecycle.

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    flutter_rust_bridge::setup_default_user_utils();
    crate::diag::install();
}

/// Where this device keeps its own copy of every vault's log (spec §6.3, §14). Call it
/// once at startup, in every isolate that talks to the relay, before anything else: the
/// copy is what lets the app notice a relay that lost or rewound the log, so a relay
/// client made before this runs has no such protection.
#[flutter_rust_bridge::frb(sync)]
pub fn init_log_cache(dir: String) {
    zafe_core::log_cache::configure(dir);
}
