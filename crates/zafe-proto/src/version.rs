//! Format versions for everything Zafe encodes, on the wire and on disk.
//!
//! Every binary encoding starts with a little-endian `u16` version tag ([`encode`] /
//! [`decode`], or [`frame`] / [`unframe`] for bytes that are not postcard). Decoding a tag
//! other than the current one fails with [`UnsupportedVersion`] instead of misreading the
//! bytes. Where a downgrade matters, the version is also inside what gets signed: the
//! envelope and log-entry headers carry it, relay requests sign it, and the descriptor's
//! `version` field is part of the hash every member signs.
//!
//! Bumping a format: change its constant below, keep a decoder for the old version where
//! stored data must survive (match on [`split`] and migrate), and record the change in
//! `AGENTS.md`. A format that has no older decoder simply rejects old data.

use serde::{de::DeserializeOwned, Serialize};

/// Signed/sealed envelopes (`Header::version`), including their raw payloads (DKG echo
/// hashes, round-2 packages, `sk` contributions, descriptor signatures, the log key).
pub const ENVELOPE: u16 = 1;
/// Vault log entries (`EntryHeader::version`; signed, and the AEAD associated data).
pub const LOG_ENTRY: u16 = 1;
/// Vault log events (the decrypted plaintext of a log entry).
/// 2 (2026-10-01): members' display names (`VaultEvent::Name`); version 1 events still
/// decode (their variants keep their postcard index).
/// 3 (2026-10-01): backup attestations (`VaultEvent::BackupVerified`), appended the same way.
/// 4 (2026-10-01): moving a lost member's seat (`VaultEvent::ReplaceApproval`), appended.
/// 5 (2026-10-01): retrying a stalled key repair and marking it done (`RepairRetry`,
/// `RepairDone`), appended.
/// 6 (2026-10-07): no new variant. A proposal written with version 6 gets one-tap
/// commitments for every signer group whose members all have pool commitments, instead
/// of all-or-nothing. An older app skips such events (unsupported version) rather than
/// computing another assignment; proposals written with versions 1-5 replay as before.
/// 7 (2026-10-07): no new variant. A `Broadcast` written with version 7 must carry the
/// txid the proposal's PCZT determines (the shielded sighash equals the v6 txid, which
/// excludes signatures and proofs), else it is ignored. Version 1-6 broadcasts replay as
/// before; an older app skips version 7 events.
pub const VAULT_EVENT: u16 = 7;
/// The vault descriptor (`VaultDescriptor::version`, covered by every member's signature).
pub const DESCRIPTOR: u16 = 1;
/// Relay API request and response bodies (signed into every request).
pub const RELAY_API: u16 = 1;
/// Invites (`zafe-invite-v1:` text prefix).
pub const INVITE: u16 = 1;
/// A member's identity seeds as stored on a device and in backups.
pub const IDENTITY_SEEDS: u16 = 1;
/// A member's vault material (key package, vault secret, descriptor, log key).
pub const VAULT_MATERIAL: u16 = 1;
/// DKG round-1 envelope payload.
/// 2 (2026-09-30): the creator's message also carries the proposal expiry window.
/// 3 (2026-10-01): every member's message carries a commitment to its `sk` contribution.
pub const DKG_ROUND1: u16 = 3;
/// Signing request (envelope payload, and the leader's `<id>.req` file).
pub const SIGNING_REQUEST: u16 = 1;
/// Signature shares answering a signing request (envelope payload).
pub const SIGNATURE_SHARES: u16 = 1;
/// Interactive-signing nonce files (`FileNonceStore`).
pub const NONCES: u16 = 1;
/// Pre-published pool nonce files (`FilePoolStore`).
pub const POOL_NONCE: u16 = 1;
/// The leader's own signature shares kept until broadcast (`<id>.own`).
pub const OWN_SHARES: u16 = 1;
/// The leader's set of commitment sets already used in requests (`used_commitments.bin`).
pub const USED_COMMITMENTS: u16 = 1;
/// This device's copy of a vault's log (`log_cache::LogCache`, `<mailbox>.log`).
pub const LOG_CACHE: u16 = 1;
/// Encrypted vault backups (`ZAFEBAK`; the tag is a single byte there). 2 added signer
/// names (2026-09-30); version 1 still decrypts and is migrated.
pub const BACKUP: u16 = 2;
/// The relay's SQLite schema (`PRAGMA user_version`). 2: storage quota counters on
/// `mailboxes` (migrated from 1 at startup). 3: `mailboxes.threshold` for moving a lost
/// member's seat (migrated from 1 and 2).
pub const RELAY_DB: u16 = 3;
/// Share repair envelope payloads (RTS deltas and sigmas). 2 (2026-10-01): both carry the
/// repair attempt.
pub const REPAIR: u16 = 2;
/// A recovering device's request to take over a lost member's seat (`zafe-recover-v1:`).
pub const RECOVERY_REQUEST: u16 = 1;

/// Each versioned format. [`Format::current`] is the version this build writes and reads.
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
    LogCache,
    Backup,
    RelayDb,
    Repair,
    RecoveryRequest,
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
            Format::LogCache => LOG_CACHE,
            Format::Backup => BACKUP,
            Format::RelayDb => RELAY_DB,
            Format::Repair => REPAIR,
            Format::RecoveryRequest => RECOVERY_REQUEST,
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
            Format::LogCache => "log copy",
            Format::Backup => "backup",
            Format::RelayDb => "relay database",
            Format::Repair => "share repair message",
            Format::RecoveryRequest => "recovery request",
        }
    }
}

impl core::fmt::Display for Format {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.name())
    }
}

/// Data (or a peer) uses a version of `format` this build does not read.
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
    /// The data comes from a newer version of Zafe: updating the app fixes it.
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

/// Checks a version read from a signed header or field.
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

/// Splits the version tag off `bytes` without checking it (for migrations).
pub fn split(format: Format, bytes: &[u8]) -> Result<(u16, &[u8]), DecodeError> {
    match bytes {
        [lo, hi, rest @ ..] => Ok((u16::from_le_bytes([*lo, *hi]), rest)),
        _ => Err(DecodeError::Malformed(format)),
    }
}

/// `version (u16 LE) || body` with the current version of `format`.
pub fn frame(format: Format, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + body.len());
    out.extend_from_slice(&format.current().to_le_bytes());
    out.extend_from_slice(body);
    out
}

/// The body of a [`frame`]d value, if its version is the current one.
pub fn unframe(format: Format, bytes: &[u8]) -> Result<&[u8], DecodeError> {
    let (version, body) = split(format, bytes)?;
    check(format, version)?;
    Ok(body)
}

/// Postcard-encodes `value` behind the current version tag of `format`.
pub fn encode<T: Serialize + ?Sized>(format: Format, value: &T) -> Result<Vec<u8>, DecodeError> {
    let body = postcard::to_allocvec(value).map_err(|_| DecodeError::Malformed(format))?;
    Ok(frame(format, &body))
}

/// Decodes an [`encode`]d value, rejecting any version but the current one.
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
