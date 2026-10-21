//! Connection checks for the endpoint settings: the app tries a relay or lightwalletd URL
//! before saving it. Errors carry the kind (unreachable, TLS, timeout, wrong network) and
//! the endpoint, like sync failures.

use zafe_core::{
    relay_client::RelayClient,
    wallet::{check_server, connect},
};

use super::{
    error::{ZafeEndpoint, ZafeError},
    vault::runtime,
};

/// Checks that a Zafe relay answers at `relay_url` (`GET /health`).
pub fn check_relay(relay_url: String) -> Result<(), ZafeError> {
    runtime()
        .block_on(RelayClient::pooled(relay_url).health())
        .map_err(|e| ZafeError::from(e).at(ZafeEndpoint::Relay))
}

/// Checks that lightwalletd answers at `lightwalletd_url` and serves `network_name`
/// (`main`, `test`, `regtest`). Returns its chain tip height.
pub fn check_lightwalletd(
    lightwalletd_url: String,
    network_name: String,
) -> Result<u32, ZafeError> {
    super::vault::network(&network_name)?;
    runtime().block_on(async {
        let mut client = connect(&lightwalletd_url).await?;
        Ok(check_server(&mut client, &network_name, None).await?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::error::ZafeErrorKind;

    #[test]
    fn unreachable_endpoints_are_typed() {
        // Port 9 (discard) on localhost: nothing listens there in CI or on dev machines.
        let e = check_relay("http://127.0.0.1:9".into()).unwrap_err();
        assert_eq!(e.kind, ZafeErrorKind::Network);
        assert_eq!(e.endpoint, ZafeEndpoint::Relay);
        let e = check_lightwalletd("http://127.0.0.1:9".into(), "regtest".into()).unwrap_err();
        assert_eq!(e.kind, ZafeErrorKind::Network);
        assert_eq!(e.endpoint, ZafeEndpoint::Lightwalletd);
    }
}
