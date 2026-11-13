//! Names inside a member's signing state directory, one per vault (the app's
//! `ZafePaths.stateDir(id)`, the CLI's home). The app bridge and the CLI both use these,
//! so a name is spelled once.

/// Interactive signing nonces (`nonce_store::FileNonceStore`).
pub const NONCES: &str = "nonces";
/// One-tap commitment pool nonces (`nonce_store::FilePoolStore`).
pub const POOL: &str = "pool";
/// The leader's signing requests (`<id>.req`), own shares (`<id>.own`) and used commitments.
pub const LEADER: &str = "leader";
/// Share-repair helper state (`repair::help_repairs`).
pub const REPAIR: &str = "repair";
/// Raw transactions this member broadcast (`node::SentTxs`).
pub const SENT: &str = "sent";
/// Commitment sets a leader already used (`node::encode_used_commitments`).
pub const USED_COMMITMENTS: &str = "used_commitments.bin";
/// Signers this leader saw leave a round unanswered (`node::SilentSigners`).
pub const SILENT_SIGNERS: &str = "silent.bin";
/// Marker file: this device was restored from a backup, so its next refresh retires the
/// commitments the previous device published and publishes a fresh pool
/// (`node::top_up_pool` with `reset`). Written by the restore, removed after the reset.
pub const POOL_RESET: &str = "pool-reset";
/// This member's key-generation state while a setup is in progress (`node::keygen_step`);
/// deleted once the vault's keys are saved.
pub const KEYGEN: &str = "keygen.state";
/// The member list and commitments this member revealed its safety value against
/// (`node::safety_check`); deleted with [`KEYGEN`].
pub const SAFETY_PIN: &str = "safety.pin";
/// This member's random safety value for the setup (`node::safety_check`), written before
/// its commitment is sent; deleted with [`KEYGEN`].
pub const SAFETY_SECRET: &str = "safety.secret";
/// A recovering phone's seat-recovery pins (`repair::recovery_checks`), one file per
/// (new identity, co-signer); not inside any vault's directory (there is none yet).
pub const RECOVERY: &str = "recovery";
/// Extension of a leader's saved signing request.
pub const REQUEST_EXT: &str = "req";
/// Extension of a leader's saved own signature shares.
pub const OWN_SHARES_EXT: &str = "own";
