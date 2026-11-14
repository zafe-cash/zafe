//! File-backed FROST nonce store (one file per proposal and PCZT).
//!
//! Nonces are secret and single-use. On devices the directory must be app-private and
//! excluded from backups and device transfer (spec §9.4): Android `allowBackup="false"`,
//! iOS `isExcludedFromBackup`. Restoring an old copy could make a member sign twice with the
//! same nonces, which leaks their key share.
//!
//! Files are versioned (`zafe_proto::version`). A file in a version this build can't read
//! counts as missing: its nonces are never used, which is always safe.
//!
//! Since version 2 the nonces are encrypted (XChaCha20-Poly1305, the file name as
//! associated data) with a [`NonceKey`] derived from the member's identity seeds, which
//! live in the phone's secure storage: a leaked nonce file plus the share later signed
//! with it would give away the member's key share. Version 1 files (plaintext, written
//! before) are still read, so published one-tap commitments stay usable after an update.

use std::{fs, path::PathBuf};

use chacha20poly1305::{
    aead::{Aead, KeyInit, Payload},
    XChaCha20Poly1305, XNonce,
};
use rand::RngCore;
use reddsa::frost::redpallas::round1::{SigningCommitments, SigningNonces};
use zafe_proto::version::{self, Format};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::session::{NonceStore, PoolStore, ProposalId, SessionError};

/// The key nonce files are encrypted with, one per member and vault.
#[derive(Clone, Zeroize, ZeroizeOnDrop)]
pub struct NonceKey([u8; 32]);

impl NonceKey {
    /// Derived from this member's identity seeds for the vault (BLAKE3 `derive_key`,
    /// length-prefixed), like the wallet database key: nothing more to store.
    pub fn derive(identity_seeds: &[u8], vault_id: &[u8]) -> Self {
        let mut hasher = blake3::Hasher::new_derive_key("Zafe signing nonce file key v1");
        hasher.update(&(identity_seeds.len() as u64).to_le_bytes());
        hasher.update(identity_seeds);
        hasher.update(vault_id);
        Self(*hasher.finalize().as_bytes())
    }

    /// A key derived elsewhere (e.g. `moves::MoveState`'s, from the vault secret).
    pub(crate) fn from_raw(key: [u8; 32]) -> Self {
        Self(key)
    }

    /// `nonce || ciphertext`, bound to `name` (the file name).
    pub(crate) fn seal(&self, name: &str, plain: &[u8]) -> Vec<u8> {
        let mut nonce = [0u8; 24];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let aead = XChaCha20Poly1305::new((&self.0).into());
        let sealed = aead
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plain,
                    aad: name.as_bytes(),
                },
            )
            .expect("encryption of a small buffer can't fail");
        [nonce.as_slice(), &sealed].concat()
    }

    pub(crate) fn open(&self, name: &str, sealed: &[u8]) -> Option<Vec<u8>> {
        let (nonce, body) = sealed.split_at_checked(24)?;
        XChaCha20Poly1305::new((&self.0).into())
            .decrypt(
                XNonce::from_slice(nonce),
                Payload {
                    msg: body,
                    aad: name.as_bytes(),
                },
            )
            .ok()
    }
}

/// The body of a nonce file of `format`: decrypted for the current version, as stored for
/// version 1 (plaintext). `None` for anything else, or a file that doesn't decrypt.
fn file_body(format: Format, key: &NonceKey, path: &std::path::Path) -> Option<Vec<u8>> {
    file_body_named(format, key, path, path.file_name()?.to_str()?)
}

/// [`file_body`] for a file sealed under the name `name` (a claimed nonce file was renamed).
fn file_body_named(
    format: Format,
    key: &NonceKey,
    path: &std::path::Path,
    name: &str,
) -> Option<Vec<u8>> {
    let bytes = fs::read(path).ok()?;
    match version::split(format, &bytes).ok()? {
        (1, body) => Some(body.to_vec()),
        (v, body) if v == format.current() => key.open(name, body),
        _ => None,
    }
}

/// A nonce file of `format` holding `plain`, encrypted.
fn sealed_file(format: Format, key: &NonceKey, path: &std::path::Path, plain: &[u8]) -> Vec<u8> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    version::frame(format, &key.seal(name, plain))
}

pub struct FileNonceStore(PathBuf, NonceKey);

impl FileNonceStore {
    pub fn new(dir: impl Into<PathBuf>, key: NonceKey) -> Self {
        Self(dir.into(), key)
    }

    fn file(&self, proposal: &ProposalId, hash: &[u8; 32]) -> PathBuf {
        self.0.join(format!(
            "{}-{}.bin",
            hex::encode(proposal),
            hex::encode(hash)
        ))
    }

    /// A signed reply kept until sent (`NonceStore::keep_reply`), by request hash.
    fn reply_file(&self, request_hash: &[u8; 32]) -> PathBuf {
        self.0.join(format!("{}.reply", hex::encode(request_hash)))
    }

    /// Where a `take` moves a nonce file while it signs: `<file>.<random>.taking`, one name
    /// per take, so a later take (of nonces a re-approval wrote afresh) can never replace
    /// the file an earlier one is still reading.
    fn claim_file(&self, proposal: &ProposalId, hash: &[u8; 32]) -> PathBuf {
        let mut tag = [0u8; 8];
        rand::rngs::OsRng.fill_bytes(&mut tag);
        let mut name = self.file(proposal, hash).into_os_string();
        name.push(format!(".{}.taking", hex::encode(tag)));
        name.into()
    }

    fn read(&self, proposal: &ProposalId, hash: &[u8; 32]) -> Option<Vec<SigningNonces>> {
        let body = file_body(Format::Nonces, &self.1, &self.file(proposal, hash))?;
        decode_nonces(&body)
    }
}

fn decode_nonces(body: &[u8]) -> Option<Vec<SigningNonces>> {
    let encoded: Vec<Vec<u8>> = postcard::from_bytes(body).ok()?;
    encoded
        .iter()
        .map(|b| SigningNonces::deserialize(b).ok())
        .collect()
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
        let plain =
            zeroize::Zeroizing::new(postcard::to_allocvec(&encoded).map_err(|e| storage(&e))?);
        let path = self.file(&proposal, &hash);
        let bytes = sealed_file(Format::Nonces, &self.1, &path, &plain);
        fs::create_dir_all(&self.0).map_err(|e| storage(&e))?;
        // Atomic and flushed: a crash never leaves a truncated nonce file.
        crate::durable::write(&path, &bytes).map_err(|e| storage(&e))
    }

    fn take(&mut self, proposal: &ProposalId, hash: &[u8; 32]) -> Option<Vec<SigningNonces>> {
        // Claim by renaming: a rename is atomic, so of two processes taking at once (the
        // app and a background check) only one gets the file. Reading and then deleting by
        // name let a take that had read the old nonces delete a re-approval's fresh file
        // instead, and return the old nonces another take already had. A claim a crash
        // left behind counts as released (`claimed`): never read again nor renamed back.
        let file = self.file(proposal, hash);
        let claim = self.claim_file(proposal, hash);
        fs::rename(&file, &claim).ok()?;
        let name = file.file_name()?.to_str()?;
        let body = zeroize::Zeroizing::new(file_body_named(Format::Nonces, &self.1, &claim, name)?);
        // Delete before use, durably (this also makes the rename durable): a file back
        // after a power cut could sign twice.
        crate::durable::remove(&claim).ok()?;
        decode_nonces(&body)
    }

    fn claimed(&self, proposal: &ProposalId, hash: &[u8; 32]) -> bool {
        let file = self.file(proposal, hash);
        let Some(prefix) = file
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| format!("{n}."))
        else {
            return false;
        };
        fs::read_dir(&self.0).is_ok_and(|mut dir| {
            dir.any(|e| {
                e.ok()
                    .and_then(|e| e.file_name().into_string().ok())
                    .is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".taking"))
            })
        })
    }

    fn keep_reply(&mut self, request_hash: &[u8; 32], reply: &[u8]) -> Result<(), SessionError> {
        let storage = |e: &dyn std::fmt::Debug| SessionError::Storage(format!("{e:?}"));
        fs::create_dir_all(&self.0).map_err(|e| storage(&e))?;
        // The reply carries its own version tag (`SIGNATURE_SHARES`).
        crate::durable::write(&self.reply_file(request_hash), reply).map_err(|e| storage(&e))
    }

    fn kept_reply(&self, request_hash: &[u8; 32]) -> Option<Vec<u8>> {
        fs::read(self.reply_file(request_hash)).ok()
    }

    fn drop_reply(&mut self, request_hash: &[u8; 32]) {
        let _ = fs::remove_file(self.reply_file(request_hash));
    }

    fn kept_replies(&self) -> Vec<Vec<u8>> {
        fs::read_dir(&self.0).map_or_else(
            |_| Vec::new(),
            |dir| {
                dir.filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.extension().is_some_and(|x| x == "reply"))
                    .filter_map(|p| fs::read(p).ok())
                    .collect()
            },
        )
    }
}

/// File-backed [`PoolStore`]: one file per pre-published commitment, named by the hash of
/// the commitment. Same storage rules as [`FileNonceStore`].
pub struct FilePoolStore(PathBuf, NonceKey);

impl FilePoolStore {
    pub fn new(dir: impl Into<PathBuf>, key: NonceKey) -> Self {
        Self(dir.into(), key)
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
        let plain = zeroize::Zeroizing::new(nonces.serialize().map_err(|e| storage(&e))?);
        let path = self.file(commitment);
        let bytes = sealed_file(Format::PoolNonce, &self.1, &path, &plain);
        fs::create_dir_all(&self.0).map_err(|e| storage(&e))?;
        crate::durable::write(&path, &bytes).map_err(|e| storage(&e))
    }

    fn contains(&self, commitment: &[u8]) -> bool {
        self.file(commitment).exists()
    }

    fn take(&mut self, commitment: &[u8]) -> Option<SigningNonces> {
        let path = self.file(commitment);
        let body = zeroize::Zeroizing::new(file_body(Format::PoolNonce, &self.1, &path)?);
        crate::durable::remove(&path).ok()?; // delete before use, durably: never reusable
        SigningNonces::deserialize(&body).ok()
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
        crate::durable::write(&self.vote_file(proposal, pczt_hash), event).map_err(|e| storage(&e))
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

#[cfg(test)]
mod tests {
    use super::FileNonceStore;
    use crate::session::NonceStore;

    #[test]
    fn a_kept_reply_survives_until_dropped() {
        let dir = std::env::temp_dir().join(format!("zafe-replies-{}", std::process::id()));
        let key = || super::NonceKey::derive(b"seeds", b"vault");
        let mut store = FileNonceStore::new(&dir, key());
        let hash = [7u8; 32];
        assert_eq!(store.kept_reply(&hash), None);
        store.keep_reply(&hash, b"shares").unwrap();
        // Another handle on the same directory (a restarted app) finds it.
        assert_eq!(
            FileNonceStore::new(&dir, key())
                .kept_reply(&hash)
                .as_deref(),
            Some(&b"shares"[..])
        );
        // A reply is not a nonce file.
        assert!(!store.contains(&[0; 16], &hash));
        store.drop_reply(&hash);
        assert_eq!(store.kept_reply(&hash), None);
        std::fs::remove_dir_all(dir).unwrap();
    }

    /// A take claims the file by renaming it, so of two processes taking at once only one
    /// gets the nonces; a claim a crash left behind counts as released (`claimed`) and is
    /// never read again, while a re-approval's fresh nonces are taken normally.
    #[test]
    fn a_take_claims_the_file_first() {
        use std::sync::{Arc, Barrier};
        let dir = std::env::temp_dir().join(format!("zafe-take-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let key = || super::NonceKey::derive(b"seeds", b"vault");
        let (p, h) = ([1u8; 16], [2u8; 32]);
        for _ in 0..50 {
            FileNonceStore::new(&dir, key())
                .put(p, h, vec![nonces()])
                .unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let takers: Vec<_> = (0..2)
                .map(|_| {
                    let (barrier, dir) = (barrier.clone(), dir.clone());
                    std::thread::spawn(move || {
                        let mut store = FileNonceStore::new(&dir, key());
                        barrier.wait();
                        store.take(&p, &h).is_some()
                    })
                })
                .collect();
            let got = takers
                .into_iter()
                .filter_map(|t| t.join().unwrap().then_some(()))
                .count();
            assert_eq!(got, 1);
        }

        // A crash between the claim and the delete: the claim stays, nothing reads it.
        let mut store = FileNonceStore::new(&dir, key());
        store.put(p, h, vec![nonces()]).unwrap();
        assert!(!store.claimed(&p, &h));
        let file = store.file(&p, &h);
        std::fs::rename(&file, store.claim_file(&p, &h)).unwrap();
        assert!(store.claimed(&p, &h) && !store.contains(&p, &h));
        assert!(store.take(&p, &h).is_none());
        // A fresh approval's nonces are taken; the old claim still says "released".
        store.put(p, h, vec![nonces()]).unwrap();
        assert!(store.take(&p, &h).is_some());
        assert!(store.claimed(&p, &h));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn nonces() -> reddsa::frost::redpallas::round1::SigningNonces {
        use rand::SeedableRng;
        let mut one = [0u8; 32];
        one[0] = 1;
        let share = reddsa::frost::redpallas::keys::SigningShare::deserialize(&one).unwrap();
        reddsa::frost::redpallas::round1::SigningNonces::new(
            &share,
            &mut rand::rngs::StdRng::seed_from_u64(9),
        )
    }

    #[test]
    fn pool_nonces_are_encrypted_and_bound_to_the_key() {
        use super::{FilePoolStore, NonceKey};
        use crate::session::PoolStore;
        let dir = std::env::temp_dir().join(format!("zafe-pool-enc-{}", std::process::id()));
        let key = |seeds: &[u8]| NonceKey::derive(seeds, b"vault");
        let n = nonces();
        let secret = n.serialize().unwrap();
        let mut store = FilePoolStore::new(&dir, key(b"seeds"));
        store.put(b"commitment", n.clone()).unwrap();

        // Nothing of the secret nonces is on disk in the clear.
        let file = std::fs::read_dir(&dir)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let raw = std::fs::read(&file).unwrap();
        assert!(!raw.windows(32).any(|w| secret.windows(32).any(|s| s == w)));

        // Another member's key (or a copied file) reads nothing, and leaves the file.
        assert!(FilePoolStore::new(&dir, key(b"other"))
            .take(b"commitment")
            .is_none());
        assert_eq!(store.take(b"commitment").unwrap(), n);
        assert!(!store.contains(b"commitment"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn version_1_plaintext_nonces_are_still_used() {
        use super::{FilePoolStore, NonceKey};
        use crate::session::PoolStore;
        use zafe_proto::version::{self, Format};
        let dir = std::env::temp_dir().join(format!("zafe-pool-v1-{}", std::process::id()));
        let mut store = FilePoolStore::new(&dir, NonceKey::derive(b"seeds", b"vault"));
        let n = nonces();
        // What a build before encryption wrote: tag 1, then the serialized nonces.
        let mut old = 1u16.to_le_bytes().to_vec();
        old.extend_from_slice(&n.serialize().unwrap());
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(store.file(b"commitment"), old).unwrap();
        assert_eq!(store.take(b"commitment").unwrap(), n);
        assert!(version::POOL_NONCE > 1 && Format::PoolNonce.current() > 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
