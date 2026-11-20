use serde::{de::DeserializeOwned, Serialize};

pub const ENVELOPE: u16 = 1;
pub const LOG_ENTRY: u16 = 1;
pub const VAULT_EVENT: u16 = 12;
pub const DESCRIPTOR: u16 = 1;
pub const RELAY_API: u16 = 2;
pub const INVITE: u16 = 3;
pub const IDENTITY_SEEDS: u16 = 1;
pub const VAULT_MATERIAL: u16 = 2;
pub const DKG_ROUND1: u16 = 4;
pub const SIGNING_REQUEST: u16 = 1;
pub const SIGNATURE_SHARES: u16 = 1;
pub const NONCES: u16 = 2;
pub const POOL_NONCE: u16 = 2;
pub const OWN_SHARES: u16 = 1;
pub const USED_COMMITMENTS: u16 = 1;
pub const SILENT_SIGNERS: u16 = 1;
pub const KEYGEN_STATE: u16 = 1;
pub const SAFETY_PIN: u16 = 1;
pub const SAFETY_SECRET: u16 = 1;
pub const RECOVERY_PIN: u16 = 1;
pub const SEAT_MOVE_SEEN: u16 = 1;
pub const LOG_CACHE: u16 = 1;
pub const BACKUP: u16 = 3;
pub const RELAY_DB: u16 = 8;
pub const REPAIR: u16 = 3;
pub const RECOVERY_REQUEST: u16 = 1;
pub const LOG_KEY_MESSAGE: u16 = 1;
pub const SEAT_LINK: u16 = 1;
pub const RELAY_STATUS: u16 = 1;
pub const MOVE_MESSAGE: u16 = 1;
pub const MOVE_STATE: u16 = 1;
pub const MOVE_LINK: u16 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Envelope,
    LogEntry,
    VaultEvent,
    Descriptor,
    RelayApi,
    Invite,
    IdentitySeeds,
    VaultMaterial,
    DkgRound1,
    SigningRequest,
    SignatureShares,
    Nonces,
    PoolNonce,
    OwnShares,
    UsedCommitments,
    SilentSigners,
    KeygenState,
    SafetyPin,
    SafetySecret,
    RecoveryPin,
    SeatMoveSeen,
    LogCache,
    Backup,
    RelayDb,
    Repair,
    RecoveryRequest,
    LogKeyMessage,
    SeatLink,
    RelayStatus,
    MoveMessage,
    MoveState,
    MoveLink,
}

impl Format {
    pub const fn current(self) -> u16 {
        match self {
            Format::Envelope => ENVELOPE,
            Format::LogEntry => LOG_ENTRY,
            Format::VaultEvent => VAULT_EVENT,
            Format::Descriptor => DESCRIPTOR,
            Format::RelayApi => RELAY_API,
            Format::Invite => INVITE,
            Format::IdentitySeeds => IDENTITY_SEEDS,
            Format::VaultMaterial => VAULT_MATERIAL,
            Format::DkgRound1 => DKG_ROUND1,
            Format::SigningRequest => SIGNING_REQUEST,
            Format::SignatureShares => SIGNATURE_SHARES,
            Format::Nonces => NONCES,
            Format::PoolNonce => POOL_NONCE,
            Format::OwnShares => OWN_SHARES,
            Format::UsedCommitments => USED_COMMITMENTS,
            Format::SilentSigners => SILENT_SIGNERS,
            Format::KeygenState => KEYGEN_STATE,
            Format::SafetyPin => SAFETY_PIN,
            Format::SafetySecret => SAFETY_SECRET,
            Format::RecoveryPin => RECOVERY_PIN,
            Format::SeatMoveSeen => SEAT_MOVE_SEEN,
            Format::LogCache => LOG_CACHE,
            Format::Backup => BACKUP,
            Format::RelayDb => RELAY_DB,
            Format::Repair => REPAIR,
            Format::RecoveryRequest => RECOVERY_REQUEST,
            Format::LogKeyMessage => LOG_KEY_MESSAGE,
            Format::SeatLink => SEAT_LINK,
            Format::RelayStatus => RELAY_STATUS,
            Format::MoveMessage => MOVE_MESSAGE,
            Format::MoveState => MOVE_STATE,
            Format::MoveLink => MOVE_LINK,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Format::Envelope => "envelope",
            Format::LogEntry => "log entry",
            Format::VaultEvent => "vault event",
            Format::Descriptor => "vault descriptor",
            Format::RelayApi => "relay API",
            Format::Invite => "invite",
            Format::IdentitySeeds => "identity",
            Format::VaultMaterial => "vault material",
            Format::DkgRound1 => "key generation message",
            Format::SigningRequest => "signing request",
            Format::SignatureShares => "signature shares",
            Format::Nonces => "nonce file",
            Format::PoolNonce => "pool nonce file",
            Format::OwnShares => "own signature shares",
            Format::UsedCommitments => "used commitments",
            Format::SilentSigners => "silent signers",
            Format::KeygenState => "key generation state",
            Format::SafetyPin => "safety word pin",
            Format::SafetySecret => "safety value",
            Format::RecoveryPin => "seat recovery pin",
            Format::SeatMoveSeen => "seat move first seen",
            Format::LogCache => "log copy",
            Format::Backup => "backup",
            Format::RelayDb => "relay database",
            Format::Repair => "share repair message",
            Format::RecoveryRequest => "recovery request",
            Format::LogKeyMessage => "log key message",
            Format::SeatLink => "seat recovery link",
            Format::RelayStatus => "relay status",
            Format::MoveMessage => "relay move message",
            Format::MoveState => "relay move state",
            Format::MoveLink => "move link",
        }
    }
}

impl core::fmt::Display for Format {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "{format} version {found} is not supported (this version of Zafe reads version {supported})"
)]
pub struct UnsupportedVersion {
    pub format: Format,
    pub found: u16,
    pub supported: u16,
}

impl UnsupportedVersion {
    pub fn is_newer(&self) -> bool {
        self.found > self.supported
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error(transparent)]
    Unsupported(#[from] UnsupportedVersion),
    #[error("malformed {0}")]
    Malformed(Format),
}

pub fn check(format: Format, found: u16) -> Result<(), UnsupportedVersion> {
    let supported = format.current();
    if found == supported {
        Ok(())
    } else {
        Err(UnsupportedVersion {
            format,
            found,
            supported,
        })
    }
}

pub fn split(format: Format, bytes: &[u8]) -> Result<(u16, &[u8]), DecodeError> {
    match bytes {
        [lo, hi, rest @ ..] => Ok((u16::from_le_bytes([*lo, *hi]), rest)),
        _ => Err(DecodeError::Malformed(format)),
    }
}

pub fn frame(format: Format, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&format.current().to_le_bytes());
    out.extend_from_slice(body);
    out
}

pub fn unframe(format: Format, bytes: &[u8]) -> Result<&[u8], DecodeError> {
    let (version, body) = split(format, bytes)?;
    check(format, version)?;
    Ok(body)
}

pub fn encode<T: Serialize + ?Sized>(format: Format, value: &T) -> Result<Vec<u8>, DecodeError> {
    let body = postcard::to_allocvec(value).map_err(|_| DecodeError::Malformed(format))?;
    Ok(frame(format, &body))
}

pub fn decode<T: DeserializeOwned>(format: Format, bytes: &[u8]) -> Result<T, DecodeError> {
    postcard::from_bytes(unframe(format, bytes)?).map_err(|_| DecodeError::Malformed(format))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_rejection() {
        let bytes = encode(Format::OwnShares, &vec![vec![1u8, 2], vec![3]]).unwrap();
        assert_eq!(&bytes[..2], &OWN_SHARES.to_le_bytes());
        let back: Vec<Vec<u8>> = decode(Format::OwnShares, &bytes).unwrap();
        assert_eq!(back, vec![vec![1, 2], vec![3]]);

        let mut newer = bytes.clone();
        newer[..2].copy_from_slice(&(OWN_SHARES + 1).to_le_bytes());
        let err = decode::<Vec<Vec<u8>>>(Format::OwnShares, &newer).unwrap_err();
        let DecodeError::Unsupported(v) = err else {
            panic!("{err:?}")
        };
        assert_eq!(
            v,
            UnsupportedVersion {
                format: Format::OwnShares,
                found: OWN_SHARES + 1,
                supported: OWN_SHARES
            }
        );
        assert!(v.is_newer());
        assert_eq!(
            decode::<Vec<Vec<u8>>>(Format::OwnShares, &[1]),
            Err(DecodeError::Malformed(Format::OwnShares))
        );
    }
}
