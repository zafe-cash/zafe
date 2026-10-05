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
/// Extension of a leader's saved signing request.
pub const REQUEST_EXT: &str = "req";
/// Extension of a leader's saved own signature shares.
pub const OWN_SHARES_EXT: &str = "own";
