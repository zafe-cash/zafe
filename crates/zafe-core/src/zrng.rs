use rand10::{rand_core::UnwrapErr, rngs::SysRng};

pub type ZRng = UnwrapErr<SysRng>;

pub fn zrng() -> ZRng {
    UnwrapErr(SysRng)
}
