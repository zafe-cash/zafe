//! Typed errors for the Dart side (no substring matching): Dart switches on
//! `kind` for copy and recovery actions, and shows `message` only as detail.

use zafe_core::{
    net::NetFailure, node::NodeError, price::PriceError, relay_client::RelayClientError,
    wallet::WalletError,
};
use zafe_proto::UnsupportedVersion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZafeErrorKind {
    /// The relay or lightwalletd could not be reached (`endpoint` says which).
    Network,
    /// Waiting on other members or on sync; try again later.
    NotReady,
    /// Other members did not answer in time; safe to retry.
    Timeout,
    /// This device's independent check failed: never approve or sign.
    Verification,
    /// The vault cannot cover the amount plus the fee.
    InsufficientFunds,
    /// The vault holds enough, but part of it is held by open proposals.
    FundsReserved,
    /// Bad address, amount, memo or invite.
    InvalidInput,
    /// Data, a message or the relay comes from a newer version of Zafe: update the app.
    UpdateRequired,
    /// The relay is older than this app and must be updated by whoever runs it.
    RelayOutdated,
    /// The relay's storage quota for this vault is full; it frees up as old messages
    /// expire (30 days), or whoever runs the relay raises it.
    RelayStorageFull,
    /// The TLS handshake with `endpoint` failed (certificate untrusted, expired or for
    /// another host, or a server that doesn't speak TLS).
    Tls,
    /// `endpoint` accepted the connection but didn't answer in time.
    NetworkTimeout,
    /// lightwalletd's chain is behind blocks this wallet already has.
    ServerBehind,
    /// lightwalletd serves another network than the vault's.
    WrongNetwork,
    /// This device's wallet database failed (it is a cache: it resyncs if deleted).
    WalletDatabase,
    /// "Use Tor" is on and Tor is still connecting: nothing was sent (never direct).
    TorConnecting,
    /// "Use Tor" is on but Tor couldn't connect: nothing was sent (never direct).
    TorFailed,
    /// The relay answered 404 "unknown mailbox": this vault isn't on the relay used.
    VaultNotOnRelay,
    Other,
}

/// Which server an error came from, when it came from one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZafeEndpoint {
    None,
    Relay,
    Lightwalletd,
}

#[derive(Clone, Debug)]
pub struct ZafeError {
    pub kind: ZafeErrorKind,
    pub message: String,
    pub endpoint: ZafeEndpoint,
}

impl ZafeError {
    pub(crate) fn new(kind: ZafeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            endpoint: ZafeEndpoint::None,
        }
    }

    pub(crate) fn at(mut self, endpoint: ZafeEndpoint) -> Self {
        self.endpoint = endpoint;
        self
    }

    pub(crate) fn invalid(message: impl Into<String>) -> Self {
        Self::new(ZafeErrorKind::InvalidInput, message)
    }
}

impl std::fmt::Display for ZafeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ZafeError {}

fn net_kind(failure: NetFailure) -> ZafeErrorKind {
    match failure {
        NetFailure::Unreachable => ZafeErrorKind::Network,
        NetFailure::Tls => ZafeErrorKind::Tls,
        NetFailure::Timeout => ZafeErrorKind::NetworkTimeout,
        NetFailure::Server => ZafeErrorKind::Other,
        NetFailure::TorConnecting => ZafeErrorKind::TorConnecting,
        NetFailure::TorFailed => ZafeErrorKind::TorFailed,
    }
}

/// Newer data means "update the app"; older data this build no longer reads is `Other`
/// (pre-release formats have no migrations: reset or restore).
fn version_kind(v: &UnsupportedVersion) -> ZafeErrorKind {
    if v.is_newer() {
        ZafeErrorKind::UpdateRequired
    } else {
        ZafeErrorKind::Other
    }
}

impl From<UnsupportedVersion> for ZafeError {
    fn from(v: UnsupportedVersion) -> Self {
        Self::new(version_kind(&v), v.to_string())
    }
}

impl From<NodeError> for ZafeError {
    fn from(e: NodeError) -> Self {
        if let NodeError::Wallet(w) = e {
            return w.into();
        }
        let endpoint = match &e {
            NodeError::Relay(_) => ZafeEndpoint::Relay,
            _ => ZafeEndpoint::None,
        };
        let kind = match &e {
            NodeError::UnsupportedVersion(v)
            | NodeError::Relay(RelayClientError::UnsupportedVersion(v)) => version_kind(v),
            NodeError::Relay(r @ RelayClientError::VersionRejected { .. }) => {
                if r.app_outdated() {
                    ZafeErrorKind::UpdateRequired
                } else {
                    ZafeErrorKind::RelayOutdated
                }
            }
            NodeError::Relay(RelayClientError::Transport { failure, .. }) => net_kind(*failure),
            NodeError::Relay(RelayClientError::Status { status: 404, body })
                if body.contains("unknown mailbox") =>
            {
                ZafeErrorKind::VaultNotOnRelay
            }
            NodeError::Relay(RelayClientError::RateLimited { .. }) => ZafeErrorKind::NotReady,
            NodeError::Relay(RelayClientError::StorageFull { .. }) => {
                ZafeErrorKind::RelayStorageFull
            }
            NodeError::NotReady(_) => ZafeErrorKind::NotReady,
            NodeError::Timeout(_) => ZafeErrorKind::Timeout,
            NodeError::Verification(_)
            | NodeError::SafetyNumberMismatch { .. }
            | NodeError::EchoMismatch => ZafeErrorKind::Verification,
            NodeError::BadInvite => ZafeErrorKind::InvalidInput,
            _ => ZafeErrorKind::Other,
        };
        Self::new(kind, e.to_string()).at(endpoint)
    }
}

impl From<RelayClientError> for ZafeError {
    fn from(e: RelayClientError) -> Self {
        NodeError::Relay(e).into()
    }
}

impl From<PriceError> for ZafeError {
    fn from(e: PriceError) -> Self {
        let kind = match &e {
            PriceError::Network { failure, .. } => net_kind(*failure),
            PriceError::BadResponse(_) => ZafeErrorKind::Other,
        };
        Self::new(kind, e.to_string())
    }
}

impl From<WalletError> for ZafeError {
    fn from(e: WalletError) -> Self {
        let endpoint = match &e {
            WalletError::Remote { .. }
            | WalletError::WrongNetwork { .. }
            | WalletError::ServerBehind { .. } => ZafeEndpoint::Lightwalletd,
            _ => ZafeEndpoint::None,
        };
        let kind = match &e {
            WalletError::Remote { failure, .. } => net_kind(*failure),
            WalletError::WrongNetwork { .. } => ZafeErrorKind::WrongNetwork,
            WalletError::ServerBehind { .. } => ZafeErrorKind::ServerBehind,
            WalletError::Db(_) | WalletError::WrongKey => ZafeErrorKind::WalletDatabase,
            WalletError::Payment(_) => ZafeErrorKind::InvalidInput,
            WalletError::FundsReserved => ZafeErrorKind::FundsReserved,
            // zcash_client_backend's error is only available as text here.
            WalletError::Proposal(m) if m.contains("InsufficientFunds") => {
                ZafeErrorKind::InsufficientFunds
            }
            _ => ZafeErrorKind::Other,
        };
        Self::new(kind, e.to_string()).at(endpoint)
    }
}

impl From<anyhow::Error> for ZafeError {
    fn from(e: anyhow::Error) -> Self {
        let e = match e.downcast::<NodeError>() {
            Ok(n) => return n.into(),
            Err(e) => e,
        };
        match e.downcast::<WalletError>() {
            Ok(w) => w.into(),
            Err(e) => Self::new(ZafeErrorKind::Other, format!("{e:#}")),
        }
    }
}
