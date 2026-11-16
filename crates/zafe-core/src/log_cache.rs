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

#[derive(Clone, Debug)]
pub struct LogCache {
    dir: PathBuf,
}

static DEFAULT: RwLock<Option<LogCache>> = RwLock::new(None);
static TMP: AtomicU64 = AtomicU64::new(0);
static WRITE: Mutex<()> = Mutex::new(());

pub fn configure(dir: impl Into<PathBuf>) {
    *DEFAULT.write().expect("lock") = Some(LogCache::in_dir(dir));
}

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
                Err(_) => break,
            }
        }
        Ok(entries)
    }

    pub fn write(&self, mailbox: &MailboxId, entries: &[LogEntry]) -> Result<bool, LogCacheError> {
        let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        if self.read(mailbox)?.len() >= entries.len() {
            return Ok(false);
        }
        self.write_locked(mailbox, entries)?;
        Ok(true)
    }

    pub fn replace(&self, mailbox: &MailboxId, entries: &[LogEntry]) -> Result<(), LogCacheError> {
        let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
        self.write_locked(mailbox, entries)
    }

    fn write_locked(&self, mailbox: &MailboxId, entries: &[LogEntry]) -> Result<(), LogCacheError> {
        let raw = entries
            .iter()
            .map(|e| e.to_bytes().map_err(|_| io::Error::other("encoding")))
            .collect::<Result<Vec<_>, _>>()?;
        let bytes =
            version::encode(Format::LogCache, &raw).map_err(|_| io::Error::other("encoding"))?;
        fs::create_dir_all(&self.dir)?;
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
        Ok(())
    }

    pub fn remove(&self, mailbox: &MailboxId) {
        let _ = fs::remove_file(self.file(mailbox));
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn written_at(&self, mailbox: &MailboxId) -> Option<u64> {
        let modified = fs::metadata(self.file(mailbox)).ok()?.modified().ok()?;
        modified
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
    }

    fn status_file(&self, vault: &MailboxId) -> PathBuf {
        self.dir.join(format!("{}.status", hex::encode(vault)))
    }

    pub fn relay_status(&self, vault: &MailboxId) -> RelayStatus {
        if let Some(s) = STATUS
            .lock()
            .ok()
            .and_then(|m| m.get(&self.status_file(vault)).copied())
        {
            return s;
        }
        fs::read(self.status_file(vault))
            .ok()
            .and_then(|b| version::decode(Format::RelayStatus, &b).ok())
            .unwrap_or_default()
    }

    pub fn record_relay(&self, vault: &MailboxId, reached: bool) {
        let now = unix_now();
        let old = self.relay_status(vault);
        let new = if reached {
            RelayStatus {
                last_reached: Some(now),
                unreachable_since: None,
            }
        } else {
            RelayStatus {
                last_reached: old.last_reached,
                unreachable_since: old.unreachable_since.or(Some(now)),
            }
        };
        let stale = |at: Option<u64>| at.is_none_or(|t| now.saturating_sub(t) >= 60);
        let changed = if reached {
            old.unreachable_since.is_some() || stale(old.last_reached)
        } else {
            old.unreachable_since.is_none()
        };
        if !changed {
            return;
        }
        let path = self.status_file(vault);
        if let Ok(mut m) = STATUS.lock() {
            m.insert(path.clone(), new);
        }
        if let Ok(bytes) = version::encode(Format::RelayStatus, &new) {
            let _ = fs::create_dir_all(&self.dir);
            let _ = crate::durable::write(&path, &bytes);
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RelayStatus {
    pub last_reached: Option<u64>,
    pub unreachable_since: Option<u64>,
}

static STATUS: Mutex<std::collections::BTreeMap<PathBuf, RelayStatus>> =
    Mutex::new(std::collections::BTreeMap::new());

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
