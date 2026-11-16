use zafe_core::{
    net::NetFailure, node::NodeError, price::PriceError, relay_client::RelayClientError,
    wallet::WalletError,
};
use zafe_proto::UnsupportedVersion;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZafeErrorKind {
    Network,
    NotReady,
    Timeout,
    Verification,
    InsufficientFunds,
    FundsReserved,
    InvalidInput,
    UpdateRequired,
    RelayOutdated,
    RelayStorageFull,
    RelayLogFull,
    RelayFull,
    TooManySeatMoves,
    RelayRolledBack,
    RelayForked,
    LogCopyInvalid,
    RelayMembership,
    RelayLostVault,
    Tls,
    NetworkTimeout,
    ServerBehind,
    WrongNetwork,
    WalletDatabase,
    TorConnecting,
    TorFailed,
    VaultNotOnRelay,
    Other,
    KeygenIncomplete,
    InviteExpired,
    CreateTokenRefused,
    VaultMoved,
    MoveInProgress,
    MembersMustUpdate,
    MoveNotFound,
    MoveExpired,
    MoveStale,
    TwoMoves,
    AlreadySigned,
    MoveCancelled,
    MoveUnconfirmed,
    MoveConflict,
}

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
    pub members: Vec<String>,
}

impl ZafeError {
    pub(crate) fn new(kind: ZafeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            endpoint: ZafeEndpoint::None,
            members: Vec::new(),
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
            NodeError::Relay(_)
            | NodeError::RelayRolledBack { .. }
            | NodeError::RelayForked { .. }
            | NodeError::RelayMembership(_)
            | NodeError::RelayLostVault { .. } => ZafeEndpoint::Relay,
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
            NodeError::Relay(RelayClientError::RelayTooOld { .. }) => ZafeErrorKind::RelayOutdated,
            NodeError::Relay(RelayClientError::CreateTokenRefused) => {
                ZafeErrorKind::CreateTokenRefused
            }
            NodeError::Relay(RelayClientError::Status { status: 404, body })
                if body.contains("unknown mailbox") =>
            {
                ZafeErrorKind::VaultNotOnRelay
            }
            NodeError::Relay(RelayClientError::Status { status: 410, .. }) => {
                ZafeErrorKind::InviteExpired
            }
            NodeError::Relay(RelayClientError::RateLimited { .. }) => ZafeErrorKind::NotReady,
            NodeError::Relay(RelayClientError::StorageFull { detail })
                if detail == zafe_proto::relay::LOG_FULL =>
            {
                ZafeErrorKind::RelayLogFull
            }
            NodeError::Relay(RelayClientError::StorageFull { detail })
                if detail == zafe_proto::relay::RELAY_FULL =>
            {
                ZafeErrorKind::RelayFull
            }
            NodeError::Relay(RelayClientError::StorageFull { .. }) => {
                ZafeErrorKind::RelayStorageFull
            }
            NodeError::Relay(RelayClientError::TooManySeatMoves) => ZafeErrorKind::TooManySeatMoves,
            NodeError::RelayRolledBack { .. } => ZafeErrorKind::RelayRolledBack,
            NodeError::RelayForked { .. } => ZafeErrorKind::RelayForked,
            NodeError::LogCopyInvalid(_) => ZafeErrorKind::LogCopyInvalid,
            NodeError::RelayMembership(_) => ZafeErrorKind::RelayMembership,
            NodeError::RelayLostVault { .. } => ZafeErrorKind::RelayLostVault,
            NodeError::NotReady(_) => ZafeErrorKind::NotReady,
            NodeError::Timeout(_) => ZafeErrorKind::Timeout,
            NodeError::KeygenStalled { .. } => ZafeErrorKind::KeygenIncomplete,
            NodeError::Verification(_)
            | NodeError::SafetyNumberMismatch { .. }
            | NodeError::EchoMismatch => ZafeErrorKind::Verification,
            NodeError::BadInvite => ZafeErrorKind::InvalidInput,
            NodeError::VaultMoved { .. } => ZafeErrorKind::VaultMoved,
            NodeError::MoveInProgress => ZafeErrorKind::MoveInProgress,
            NodeError::MembersMustUpdate(_) => ZafeErrorKind::MembersMustUpdate,
            NodeError::MoveNotFound => ZafeErrorKind::MoveNotFound,
            NodeError::MoveExpired => ZafeErrorKind::MoveExpired,
            NodeError::MoveStale => ZafeErrorKind::MoveStale,
            NodeError::TwoMoves => ZafeErrorKind::TwoMoves,
            NodeError::AlreadySigned => ZafeErrorKind::AlreadySigned,
            NodeError::MoveCancelled => ZafeErrorKind::MoveCancelled,
            NodeError::MoveUnconfirmed { .. } => ZafeErrorKind::MoveUnconfirmed,
            NodeError::MoveConflict => ZafeErrorKind::MoveConflict,
            _ => ZafeErrorKind::Other,
        };
        let members = match &e {
            NodeError::KeygenStalled { missing, .. } => missing.iter().map(hex::encode).collect(),
            NodeError::MembersMustUpdate(behind) => behind.iter().map(hex::encode).collect(),
            _ => Vec::new(),
        };
        let message = match &e {
            NodeError::VaultMoved { url } | NodeError::MoveUnconfirmed { url } => url.clone(),
            e => e.to_string(),
        };
        Self {
            members,
            ..Self::new(kind, message).at(endpoint)
        }
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
            WalletError::AppOutdated(_) => ZafeErrorKind::UpdateRequired,
            WalletError::Db(_) | WalletError::WrongKey => ZafeErrorKind::WalletDatabase,
            WalletError::Payment(_) => ZafeErrorKind::InvalidInput,
            WalletError::FundsReserved => ZafeErrorKind::FundsReserved,
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
