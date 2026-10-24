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

/// What a relay says about itself (`GET /v1/info`), for the relay picker.
#[derive(Debug)]
pub struct RelayCheck {
    /// The relay is older than labelled mailboxes (no `/v1/info`): it can't hold a new
    /// vault (grey it out). The other fields are then unknown (zero / false / `None`).
    pub too_old: bool,
    pub relay_api_version: u16,
    /// It sends push notifications ("No notifications" when false).
    pub push: bool,
    /// Creating or restoring a vault there needs its create token.
    pub create_token_required: bool,
    /// Most members one vault may have.
    pub max_members: u16,
    /// Requests per minute per signing key (`None`: unlimited).
    pub requests_per_minute: Option<u32>,
    /// Bytes of vault history one vault may keep (`None`: unlimited).
    pub log_bytes: Option<u64>,
    /// Days an undelivered message is kept.
    pub delivery_retention_days: u32,
    /// Days without use after which the relay deletes a vault (`None`: never).
    pub idle_days: Option<u32>,
}

/// Checks the one relay the user selected (never a list of them in the background: each
/// would learn this phone's address): that a Zafe relay answers at `relay_url` (`GET
/// /health`), and what it says about itself. Through Tor when Tor is on.
pub fn check_relay(relay_url: String) -> Result<RelayCheck, ZafeError> {
    let relay = RelayClient::pooled(relay_url);
    let at_relay =
        |e: zafe_core::relay_client::RelayClientError| ZafeError::from(e).at(ZafeEndpoint::Relay);
    runtime().block_on(async {
        relay.health().await.map_err(at_relay)?;
        match relay.info().await {
            Ok(info) => Ok(RelayCheck {
                too_old: false,
                relay_api_version: info.versions.relay_api,
                push: info.push,
                create_token_required: info.create_token,
                max_members: info.limits.max_members,
                requests_per_minute: info.limits.requests_per_minute,
                log_bytes: info.limits.log_bytes,
                delivery_retention_days: info.limits.delivery_retention_days,
                idle_days: info.expiry.idle_days,
            }),
            Err(zafe_core::relay_client::RelayClientError::RelayTooOld { .. }) => Ok(RelayCheck {
                too_old: true,
                relay_api_version: 0,
                push: false,
                create_token_required: false,
                max_members: 0,
                requests_per_minute: None,
                log_bytes: None,
                delivery_retention_days: 0,
                idle_days: None,
            }),
            Err(e) => Err(at_relay(e)),
        }
    })
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
