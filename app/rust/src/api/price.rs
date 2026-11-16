use super::{error::ZafeError, vault::runtime};

pub fn zec_usd_price() -> Result<f64, ZafeError> {
    Ok(runtime().block_on(zafe_core::price::zec_usd())?)
}
