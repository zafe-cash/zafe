//! File-backed FROST nonce store (one file per proposal and PCZT).
//!
//! Nonces are secret and single-use. On devices the directory must be app-private and
//! excluded from backups and device transfer (spec §9.4): Android `allowBackup="false"`,
//! iOS `isExcludedFromBackup`. Restoring an old copy could make a member sign twice with the
//! same nonces, which leaks their key share.
//!
//! Files are versioned (`zafe_proto::version`). A file in a version this build can't read
//! counts as missing: its nonces are never used, which is always safe.

use std::{fs, path::PathBuf};

use reddsa::frost::redpallas::round1::{SigningCommitments, SigningNonces};
use zafe_proto::version::{self, Format};

use crate::session::{NonceStore, PoolStore, ProposalId, SessionError};

pub struct FileNonceStore(PathBuf);

impl FileNonceStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self(dir.into())
    }

    fn file(&self, proposal: &ProposalId, hash: &[u8; 32]) -> PathBuf {
        self.0.join(format!(
            "{}-{}.bin",
            hex::encode(proposal),
            hex::encode(hash)
        ))
    }

    fn read(&self, proposal: &ProposalId, hash: &[u8; 32]) -> Option<Vec<SigningNonces>> {
        let encoded: Vec<Vec<u8>> =
            version::decode(Format::Nonces, &fs::read(self.file(proposal, hash)).ok()?).ok()?;
        encoded
            .iter()
            .map(|b| SigningNonces::deserialize(b).ok())
            .collect()
    }
}

impl NonceStore for FileNonceStore {
    fn contains(&self, proposal: &ProposalId, hash: &[u8; 32]) -> bool {
        self.file(proposal, hash).exists()
    }

    fn commitments(
        &self,
        proposal: &ProposalId,
        hash: &[u8; 32],
    ) -> Option<Vec<SigningCommitments>> {
        Some(
            self.read(proposal, hash)?
                .iter()
                .map(|n| *n.commitments())
                .collect(),
        )
    }

    fn put(
        &mut self,
        proposal: ProposalId,
        hash: [u8; 32],
        nonces: Vec<SigningNonces>,
    ) -> Result<(), SessionError> {
        let storage = |e: &dyn std::fmt::Debug| SessionError::Storage(format!("{e:?}"));
        let encoded = nonces
            .iter()
            .map(|n| n.serialize().map_err(|e| storage(&e)))
            .collect::<Result<Vec<_>, _>>()?;
        let bytes = version::encode(Format::Nonces, &encoded).map_err(|e| storage(&e))?;
        fs::create_dir_all(&self.0).map_err(|e| storage(&e))?;
        // Write then rename, so a crash never leaves a truncated nonce file.
        let path = self.file(&proposal, &hash);
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, bytes).map_err(|e| storage(&e))?;
        fs::rename(&tmp, &path).map_err(|e| storage(&e))
    }

    fn take(&mut self, proposal: &ProposalId, hash: &[u8; 32]) -> Option<Vec<SigningNonces>> {
        let nonces = self.read(proposal, hash)?;
        fs::remove_file(self.file(proposal, hash)).ok()?; // delete before use: never reusable
        Some(nonces)
    }
}

/// File-backed [`PoolStore`]: one file per pre-published commitment, named by the hash of
/// the commitment. Same storage rules as [`FileNonceStore`].
pub struct FilePoolStore(PathBuf);

impl FilePoolStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self(dir.into())
    }

    fn file(&self, commitment: &[u8]) -> PathBuf {
        let hash = blake2b_simd::Params::new()
            .hash_length(32)
            .personal(b"Zafe_PoolNonce__")
            .hash(commitment);
        self.0.join(format!("{}.bin", hash.to_hex()))
    }

    /// Number of stored nonces (pre-published, not yet used or forgotten).
    pub fn len(&self) -> usize {
        fs::read_dir(&self.0).map_or(0, |d| {
            d.filter_map(Result::ok)
                .filter(|e| e.path().extension().is_some_and(|x| x == "bin"))
                .count()
        })
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl PoolStore for FilePoolStore {
    fn put(&mut self, commitment: &[u8], nonces: SigningNonces) -> Result<(), SessionError> {
        let storage = |e: &dyn std::fmt::Debug| SessionError::Storage(format!("{e:?}"));
        let bytes = version::frame(
            Format::PoolNonce,
            &nonces.serialize().map_err(|e| storage(&e))?,
        );
        fs::create_dir_all(&self.0).map_err(|e| storage(&e))?;
        let path = self.file(commitment);
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, bytes).map_err(|e| storage(&e))?;
        fs::rename(&tmp, &path).map_err(|e| storage(&e))
    }

    fn contains(&self, commitment: &[u8]) -> bool {
        self.file(commitment).exists()
    }

    fn take(&mut self, commitment: &[u8]) -> Option<SigningNonces> {
        let path = self.file(commitment);
        let bytes = fs::read(&path).ok()?;
        fs::remove_file(&path).ok()?; // delete before use: never reusable
        SigningNonces::deserialize(version::unframe(Format::PoolNonce, &bytes).ok()?).ok()
    }

    fn forget(&mut self, commitment: &[u8]) {
        let _ = fs::remove_file(self.file(commitment));
    }

    fn keep_vote(
        &mut self,
        proposal: &ProposalId,
        pczt_hash: &[u8; 32],
        event: &[u8],
    ) -> Result<(), SessionError> {
        let storage = |e: &dyn std::fmt::Debug| SessionError::Storage(format!("{e:?}"));
        fs::create_dir_all(&self.0).map_err(|e| storage(&e))?;
        // The event bytes carry their own version tag (`VAULT_EVENT`).
        let path = self.vote_file(proposal, pczt_hash);
        let tmp = path.with_extension("tmp");
        fs::write(&tmp, event).map_err(|e| storage(&e))?;
        fs::rename(&tmp, &path).map_err(|e| storage(&e))
    }

    fn kept_vote(&self, proposal: &ProposalId, pczt_hash: &[u8; 32]) -> Option<Vec<u8>> {
        fs::read(self.vote_file(proposal, pczt_hash)).ok()
    }

    fn drop_vote(&mut self, proposal: &ProposalId, pczt_hash: &[u8; 32]) {
        let _ = fs::remove_file(self.vote_file(proposal, pczt_hash));
    }
}

impl FilePoolStore {
    /// A kept one-tap vote ([`PoolStore::keep_vote`]); `.vote`, so [`FilePoolStore::len`]
    /// doesn't count it as a nonce.
    fn vote_file(&self, proposal: &ProposalId, pczt_hash: &[u8; 32]) -> PathBuf {
        self.0.join(format!(
            "{}-{}.vote",
            hex::encode(proposal),
            hex::encode(pczt_hash)
        ))
    }
}
