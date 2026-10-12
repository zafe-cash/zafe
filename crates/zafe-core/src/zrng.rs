//! The random generator the Zcash libraries want (`rand_core` 0.10). Our own crypto
//! (ed25519-dalek, HPKE, FROST) stays on `rand_core` 0.6 and keeps using `rand`/`OsRng`;
//! only calls into librustzcash and `pczt` take this one.

use rand10::{rand_core::UnwrapErr, rngs::SysRng};

/// The operating system's generator: stateless, `Clone` (the wallet migrator needs that)
/// and a CSPRNG. A failure to read OS entropy panics, as `OsRng` does.
pub type ZRng = UnwrapErr<SysRng>;

/// A handle to the OS generator.
pub fn zrng() -> ZRng {
    UnwrapErr(SysRng)
}
