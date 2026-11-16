use std::time::Duration;

use crate::tor::{self, Purpose, Route};

pub const COINGECKO_URL: &str =
    "https://api.coingecko.com/api/v3/simple/price?ids=zcash&vs_currencies=usd";
pub const DIRECT_TIMEOUT: Duration = Duration::from_secs(15);
pub const TOR_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum PriceError {
    #[error("network: {message}")]
    Network {
        failure: crate::net::NetFailure,
        message: String,
    },
    #[error("unexpected price response: {0}")]
    BadResponse(String),
}

fn network(e: impl std::error::Error + 'static) -> PriceError {
    PriceError::Network {
        failure: crate::net::NetFailure::of(&e),
        message: crate::net::error_chain(&e),
    }
}

pub async fn zec_usd() -> Result<f64, PriceError> {
    let route = tor::route(Purpose::Price, tor::ROUTE_WAIT)
        .await
        .map_err(|b| PriceError::Network {
            failure: b.into(),
            message: b.to_string(),
        })?;
    match route {
        Route::Direct(lease) => {
            lease
                .guard(direct(), || PriceError::Network {
                    failure: crate::net::NetFailure::TorConnecting,
                    message: tor::revoked_error().to_string(),
                })
                .await
        }
        Route::Tor(client) => {
            use zcash_client_backend::tor::http::cryptex::Exchanges;
            let exchanges = Exchanges::unauthenticated_known_with_gemini_trusted();
            let rate = tokio::time::timeout(
                TOR_TIMEOUT,
                client.get_latest_zec_to_usd_rate(&mut crate::zrng::zrng(), &exchanges),
            )
            .await
            .map_err(|_| PriceError::Network {
                failure: crate::net::NetFailure::Timeout,
                message: "no price in time (through Tor)".into(),
            })?
            .map_err(network)?;
            parse_rate(&rate.to_string())
        }
    }
}

async fn direct() -> Result<f64, PriceError> {
    let body = reqwest::Client::builder()
        .timeout(DIRECT_TIMEOUT)
        .user_agent("Zafe")
        .build()
        .map_err(network)?
        .get(COINGECKO_URL)
        .send()
        .await
        .map_err(network)?
        .error_for_status()
        .map_err(network)?
        .bytes()
        .await
        .map_err(network)?;
    coingecko_price(&body)
}

fn coingecko_price(body: &[u8]) -> Result<f64, PriceError> {
    let bad = || PriceError::BadResponse(String::from_utf8_lossy(body).chars().take(120).collect());
    let json: serde_json::Value = serde_json::from_slice(body).map_err(|_| bad())?;
    let usd = json["zcash"]["usd"].as_f64().ok_or_else(bad)?;
    valid(usd).ok_or_else(bad)
}

fn parse_rate(text: &str) -> Result<f64, PriceError> {
    text.parse::<f64>()
        .ok()
        .and_then(valid)
        .ok_or_else(|| PriceError::BadResponse(text.to_owned()))
}

fn valid(usd: f64) -> Option<f64> {
    (usd.is_finite() && usd > 0.0).then_some(usd)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_coingecko() {
        assert_eq!(
            coingecko_price(br#"{"zcash":{"usd":41.27}}"#).unwrap(),
            41.27
        );
        for bad in [
            &br#"{}"#[..],
            br#"{"zcash":{"usd":0}}"#,
            br#"{"zcash":{"usd":"x"}}"#,
            b"<html>",
        ] {
            assert!(
                coingecko_price(bad).is_err(),
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
    }

    #[test]
    fn reads_exchange_rate() {
        assert_eq!(parse_rate("41.2650").unwrap(), 41.265);
        assert!(parse_rate("-1").is_err());
        assert!(parse_rate("NaN").is_err());
    }

    #[tokio::test]
    #[ignore]
    async fn live_direct_price() {
        let usd = zec_usd().await.unwrap();
        println!("ZEC = ${usd}");
        assert!(usd > 1.0 && usd < 100_000.0);
    }
}
