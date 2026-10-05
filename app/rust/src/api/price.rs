//! The ZEC/USD price for showing balances in dollars (`zafe_core::price`: through Tor
//! when it's on, else CoinGecko directly). The app asks for it on mainnet only.

use super::{error::ZafeError, vault::runtime};

/// The latest price of 1 ZEC in USD.
pub fn zec_usd_price() -> Result<f64, ZafeError> {
    Ok(runtime().block_on(zafe_core::price::zec_usd())?)
}
