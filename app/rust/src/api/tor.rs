use std::{path::PathBuf, time::Duration};

use zafe_core::tor::{self, TorError, TorStatus};

use super::{
    error::{ZafeError, ZafeErrorKind},
    vault::runtime,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TorState {
    Off,
    Connecting,
    Connected,
    Failed,
}

impl From<TorStatus> for TorState {
    fn from(s: TorStatus) -> Self {
        match s {
            TorStatus::Off => TorState::Off,
            TorStatus::Connecting => TorState::Connecting,
            TorStatus::Ready => TorState::Connected,
            TorStatus::Failed => TorState::Failed,
        }
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn tor_request() {
    tor::request();
}

pub async fn tor_enable(tor_dir: String, timeout_secs: u32) -> Result<TorState, ZafeError> {
    let budget = Duration::from_secs(timeout_secs.into());
    runtime()
        .spawn(async move { tor::enable(&PathBuf::from(tor_dir), budget).await })
        .await
        .map_err(|e| ZafeError::new(ZafeErrorKind::Other, e.to_string()))?
        .map(TorState::from)
        .map_err(tor_error)
}

#[flutter_rust_bridge::frb(sync)]
pub fn tor_disable() {
    tor::disable();
}

#[flutter_rust_bridge::frb(sync)]
pub fn tor_state() -> TorState {
    tor::status().into()
}

#[flutter_rust_bridge::frb(sync)]
pub fn tor_set_dormant(dormant: bool) {
    tor::set_dormant(dormant);
}

fn tor_error(e: TorError) -> ZafeError {
    let kind = match e {
        TorError::Abandoned => ZafeErrorKind::Other,
        _ => ZafeErrorKind::TorFailed,
    };
    ZafeError::new(kind, e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn states_map_one_to_one() {
        assert_eq!(TorState::from(TorStatus::Off), TorState::Off);
        assert_eq!(TorState::from(TorStatus::Connecting), TorState::Connecting);
        assert_eq!(TorState::from(TorStatus::Ready), TorState::Connected);
        assert_eq!(TorState::from(TorStatus::Failed), TorState::Failed);
    }

    #[test]
    fn bootstrap_failures_are_typed() {
        assert_eq!(tor_error(TorError::Timeout).kind, ZafeErrorKind::TorFailed);
        assert_eq!(
            tor_error(TorError::Bootstrap("x".into())).kind,
            ZafeErrorKind::TorFailed
        );
    }
}
