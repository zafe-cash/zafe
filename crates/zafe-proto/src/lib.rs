//! Zafe wire protocol: member identities, signed envelopes and HPKE sealing
//! (spec §5). Transport-agnostic and free of Zcash dependencies, so the relay can use it.

pub mod envelope;
pub mod identity;
pub mod log;
pub mod relay;
pub mod version;

pub use envelope::{Envelope, Header, Kind, MailboxId, Recipient, ReplayGuard};
pub use identity::{Identity, IdentityPublic, IdentitySeeds};
pub use log::{Chain, ChainError, LogEntry, LogKey};
pub use version::{Format, UnsupportedVersion};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProtoError {
    #[error(transparent)]
    UnsupportedVersion(#[from] UnsupportedVersion),
    #[error("invalid public key")]
    BadKey,
    #[error("invalid signature")]
    BadSignature,
    #[error("envelope sender does not match the expected identity")]
    WrongSender,
    #[error("envelope is sealed to another member")]
    NotForMe,
    #[error("decryption failed")]
    Crypto,
    #[error("encoding error")]
    Encoding,
    #[error("replayed or reordered envelope (seq {seq}, last seen {last})")]
    Replay { seq: u64, last: u64 },
}

impl From<version::DecodeError> for ProtoError {
    fn from(e: version::DecodeError) -> Self {
        match e {
            version::DecodeError::Unsupported(v) => ProtoError::UnsupportedVersion(v),
            version::DecodeError::Malformed(_) => ProtoError::Encoding,
        }
    }
}
