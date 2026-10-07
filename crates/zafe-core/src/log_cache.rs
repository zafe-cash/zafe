//! This device's own copy of each vault's log (spec §6.3, §14): the relay is not trusted
//! to remember it.
//!
//! The copy doubles as the device's **anchor**: `node::load_log` re-verifies it, asks the
//! relay for the last entry it holds and refuses a relay that no longer has it (rolled
//! back to an older database, wiped) or that serves another entry at that position (a
//! fork). It is also what a member re-seeds a relay from (`node::reseed_relay`).
//!
//! Entries are stored exactly as the relay serves them (`LogEntry::to_bytes`: AEAD
//! ciphertext under the vault log key, signed by the author), not decrypted. They hold no
//! more than the relay holds, and the log key stays in secure storage. A file is replaced
//! atomically and only ever grows.
//!
//! A missing file means "no anchor yet" (first load on this device, a restored backup):
//! the first log the relay serves is trusted once, as before. A file that doesn't parse is
//! treated the same (a crash can't leave one: writes are atomic), a file in a newer
//! version is an error (update the app).

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, RwLock,
    },
};

use zafe_proto::{
    version::{self, DecodeError, Format, UnsupportedVersion},
    LogEntry, MailboxId,
};

#[derive(Debug, thiserror::Error)]
pub enum LogCacheError {
    #[error("log copy on this device: {0}")]
    Io(#[from] io::Error),
    #[error(transparent)]
    UnsupportedVersion(#[from] UnsupportedVersion),
}

/// A directory holding one `<mailbox hex>.log` file per vault.
#[derive(Clone, Debug)]
pub struct LogCache {
    dir: PathBuf,
}

static DEFAULT: RwLock<Option<LogCache>> = RwLock::new(None);
static TMP: AtomicU64 = AtomicU64::new(0);
/// Serializes the check-then-replace in [`LogCache::write`] between threads of this
/// process (the app calls `load_log` from several bridge threads at once): without it a
/// slower writer could pass the length check, then rename its shorter copy over a longer
/// one. Other processes (a background isolate) can still race; the next `load_log`
/// rewrites the longer chain, so the effect there is a briefly weaker anchor.
static WRITE: Mutex<()> = Mutex::new(());

/// Makes `dir` the log copy every [`crate::relay_client::RelayClient::new`] picks up from
/// now on (the app bridge and the CLI call this once at startup). Tests build their own
/// with `RelayClient::with_log_cache`.
pub fn configure(dir: impl Into<PathBuf>) {
    *DEFAULT.write().expect("lock") = Some(LogCache::in_dir(dir));
}

/// The configured copy, if any.
pub fn configured() -> Option<LogCache> {
    DEFAULT.read().expect("lock").clone()
}

impl LogCache {
    pub fn in_dir(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn file(&self, mailbox: &MailboxId) -> PathBuf {
        self.dir.join(format!("{}.log", hex::encode(mailbox)))
    }

    /// The saved entries in log order: empty when there is no usable copy.
    pub fn read(&self, mailbox: &MailboxId) -> Result<Vec<LogEntry>, LogCacheError> {
        let bytes = match fs::read(self.file(mailbox)) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let raw: Vec<Vec<u8>> = match version::decode(Format::LogCache, &bytes) {
            Ok(raw) => raw,
            Err(DecodeError::Unsupported(v)) => return Err(v.into()),
            Err(DecodeError::Malformed(_)) => return Ok(Vec::new()),
        };
        let mut entries = Vec::with_capacity(raw.len());
        for bytes in raw {
            match LogEntry::from_bytes(&bytes) {
                Ok(e) => entries.push(e),
                // Unreadable entries end the usable prefix (the caller re-verifies it).
                Err(_) => break,
            }
        }
        Ok(entries)
    }

    /// Saves `entries` unless a copy at least as long is already there (another process,
    /// e.g. a background check, may have saved a longer one meanwhile): the copy never
    /// shrinks. Returns whether it wrote.
    pub fn write(&self, mailbox: &MailboxId, entries: &[LogEntry]) -> Result<bool, LogCacheError> {
        let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        if self.read(mailbox)?.len() >= entries.len() {
            return Ok(false);
        }
        let raw = entries
            .iter()
            .map(|e| e.to_bytes().map_err(|_| io::Error::other("encoding")))
            .collect::<Result<Vec<_>, _>>()?;
        let bytes =
            version::encode(Format::LogCache, &raw).map_err(|_| io::Error::other("encoding"))?;
        fs::create_dir_all(&self.dir)?;
        // Write, flush, rename: a crash never leaves a truncated copy. The temporary name
        // is unique, so two writers never share one.
        let path = self.file(mailbox);
        let tmp = self.dir.join(format!(
            ".{}.{}.{}.tmp",
            hex::encode(mailbox),
            std::process::id(),
            TMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut file = fs::File::create(&tmp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&tmp, &path)?;
        Ok(true)
    }

    /// Deletes a vault's copy (removing the vault from this device).
    pub fn remove(&self, mailbox: &MailboxId) {
        let _ = fs::remove_file(self.file(mailbox));
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }
}
