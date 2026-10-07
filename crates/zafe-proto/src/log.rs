//! The vault log (spec §6.3): an append-only, hash-chained, member-signed and encrypted
//! record of vault events. Event contents are opaque bytes at this layer.
//!
//! Each entry commits to its index and the previous entry's hash, and is signed by its
//! author. The relay can only append an entry that extends its current head, and every
//! member re-checks the chain, so a relay that forks, reorders or drops entries is caught.

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand_core::{CryptoRng, RngCore};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{
    envelope::{encode, MailboxId},
    identity::{Identity, IdentityPublic},
    version::{self, Format},
    ProtoError,
};

const PERSONAL_ENTRY_HASH: &[u8; 16] = b"Zafe_LogEntryHsh";
const SIGNATURE_DOMAIN: &[u8] = b"Zafe log entry v1";

/// Symmetric key shared by current members; rotated on every membership change.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct LogKey {
    pub epoch: u32,
    key: [u8; 32],
}

impl LogKey {
    pub fn generate<R: RngCore + CryptoRng>(epoch: u32, rng: &mut R) -> Self {
        let mut key = [0u8; 32];
        rng.fill_bytes(&mut key);
        Self { epoch, key }
    }

    pub fn from_bytes(epoch: u32, key: [u8; 32]) -> Self {
        Self { epoch, key }
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.key
    }
}

/// Everything in an entry except the author's signature.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryHeader {
    /// [`version::LOG_ENTRY`]; signed and part of the AEAD associated data.
    pub version: u16,
    pub mailbox: MailboxId,
    pub index: u64,
    pub prev_hash: [u8; 32],
    pub author: [u8; 32],
    pub epoch: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub header: EntryHeader,
    /// `nonce (24 bytes) || XChaCha20-Poly1305 ciphertext`, AAD = encoded header.
    pub ciphertext: Vec<u8>,
    pub signature: Vec<u8>,
}

/// The `prev_hash` of the first entry.
pub const GENESIS_PREV_HASH: [u8; 32] = [0; 32];

impl LogEntry {
    pub fn create<R: RngCore + CryptoRng>(
        author: &Identity,
        key: &LogKey,
        mailbox: MailboxId,
        index: u64,
        prev_hash: [u8; 32],
        event: &[u8],
        rng: &mut R,
    ) -> Result<Self, ProtoError> {
        let header = EntryHeader {
            version: version::LOG_ENTRY,
            mailbox,
            index,
            prev_hash,
            author: author.public().sig_pk,
            epoch: key.epoch,
        };
        let aad = encode(&header)?;
        let mut nonce = [0u8; 24];
        rng.fill_bytes(&mut nonce);
        let sealed = XChaCha20Poly1305::new(key.as_bytes().into())
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: event,
                    aad: &aad,
                },
            )
            .map_err(|_| ProtoError::Crypto)?;
        let mut ciphertext = nonce.to_vec();
        ciphertext.extend_from_slice(&sealed);
        let signature = author.sign(&signed_bytes(&header, &ciphertext)?).to_vec();
        Ok(Self {
            header,
            ciphertext,
            signature,
        })
    }

    /// `version (u16) || postcard(entry)`; the tag equals `header.version`. This is what
    /// the relay stores and serves.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtoError> {
        Ok(version::frame(Format::LogEntry, &encode(self)?))
    }

    /// Rejects unknown versions with [`ProtoError::UnsupportedVersion`] before parsing.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtoError> {
        let entry: Self = postcard::from_bytes(version::unframe(Format::LogEntry, bytes)?)
            .map_err(|_| ProtoError::Encoding)?;
        if entry.header.version != version::LOG_ENTRY {
            return Err(ProtoError::Encoding);
        }
        Ok(entry)
    }

    pub fn verify_signature(&self, author: &IdentityPublic) -> Result<(), ProtoError> {
        version::check(Format::LogEntry, self.header.version)?;
        if self.header.author != author.sig_pk {
            return Err(ProtoError::WrongSender);
        }
        author.verify(
            &signed_bytes(&self.header, &self.ciphertext)?,
            &self.signature,
        )
    }

    pub fn decrypt(&self, key: &LogKey) -> Result<Vec<u8>, ProtoError> {
        if key.epoch != self.header.epoch || self.ciphertext.len() < 24 {
            return Err(ProtoError::Crypto);
        }
        let (nonce, sealed) = self.ciphertext.split_at(24);
        XChaCha20Poly1305::new(key.as_bytes().into())
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: sealed,
                    aad: &encode(&self.header)?,
                },
            )
            .map_err(|_| ProtoError::Crypto)
    }

    /// The hash the next entry must reference.
    pub fn hash(&self) -> Result<[u8; 32], ProtoError> {
        let hash = blake2b_simd::Params::new()
            .hash_length(32)
            .personal(PERSONAL_ENTRY_HASH)
            .hash(&encode(self)?);
        Ok(hash.as_bytes().try_into().expect("32 bytes"))
    }
}

fn signed_bytes(header: &EntryHeader, ciphertext: &[u8]) -> Result<Vec<u8>, ProtoError> {
    let mut out = SIGNATURE_DOMAIN.to_vec();
    let h = encode(header)?;
    out.extend_from_slice(&(h.len() as u32).to_le_bytes());
    out.extend_from_slice(&h);
    out.extend_from_slice(ciphertext);
    Ok(out)
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ChainError {
    #[error("entry is for another mailbox")]
    WrongMailbox,
    #[error("expected entry index {expected}, got {got}")]
    Gap { expected: u64, got: u64 },
    #[error("entry {0} does not extend the current head (fork)")]
    Fork(u64),
    #[error("entry {0} author is not a member")]
    UnknownAuthor(u64),
    #[error("entry {index}: {source}")]
    Invalid { index: u64, source: ProtoError },
}

/// A verified, hash-chained view of a vault log (members and relay both keep one).
#[derive(Clone, Debug)]
pub struct Chain {
    mailbox: MailboxId,
    entries: Vec<LogEntry>,
    head: [u8; 32],
}

impl Chain {
    pub fn new(mailbox: MailboxId) -> Self {
        Self {
            mailbox,
            entries: Vec::new(),
            head: GENESIS_PREV_HASH,
        }
    }

    pub fn len(&self) -> u64 {
        self.entries.len() as u64
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn head(&self) -> [u8; 32] {
        self.head
    }

    pub fn mailbox(&self) -> MailboxId {
        self.mailbox
    }

    pub fn entries(&self) -> &[LogEntry] {
        &self.entries
    }

    /// Appends an entry if it extends the head and is signed by one of `members`.
    pub fn append(
        &mut self,
        entry: LogEntry,
        members: &[IdentityPublic],
    ) -> Result<(), ChainError> {
        let index = entry.header.index;
        if entry.header.mailbox != self.mailbox {
            return Err(ChainError::WrongMailbox);
        }
        if index != self.len() {
            return Err(ChainError::Gap {
                expected: self.len(),
                got: index,
            });
        }
        if entry.header.prev_hash != self.head {
            return Err(ChainError::Fork(index));
        }
        let author = members
            .iter()
            .find(|m| m.sig_pk == entry.header.author)
            .ok_or(ChainError::UnknownAuthor(index))?;
        entry
            .verify_signature(author)
            .map_err(|source| ChainError::Invalid { index, source })?;
        self.head = entry
            .hash()
            .map_err(|source| ChainError::Invalid { index, source })?;
        self.entries.push(entry);
        Ok(())
    }
}
