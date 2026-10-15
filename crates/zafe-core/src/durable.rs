//! File writes and deletes that survive a crash or power loss (audit L1, L2). Signing
//! nonces must stay deleted once a share has gone out, and the leader's files must never
//! be left half written, so both go through here.

use std::{fs, io, path::Path};

/// Writes `bytes` to `path` atomically: a temp file next to it, flushed to disk, renamed
/// over `path`, then the directory flushed so the rename itself is durable.
pub fn write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = path.with_extension("tmp");
    {
        let mut file = fs::File::create(&tmp)?;
        io::Write::write_all(&mut file, bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    sync_dir(path)
}

/// Deletes `path` and flushes its directory, so the file can't come back after a crash.
pub fn remove(path: &Path) -> io::Result<()> {
    fs::remove_file(path)?;
    sync_dir(path)
}

/// Flushes the directory holding `path` (its entries: creations, renames, deletions).
#[cfg(unix)]
fn sync_dir(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => fs::File::open(dir)?.sync_all(),
        _ => fs::File::open(".")?.sync_all(),
    }
}

/// Other systems can't open a directory to flush it; the rename is as durable as they allow.
#[cfg(not(unix))]
fn sync_dir(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn writes_replace_and_removes_delete() {
        let dir = std::env::temp_dir().join(format!("zafe-durable-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.bin");
        super::write(&path, b"one").unwrap();
        super::write(&path, b"two").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        assert!(!path.with_extension("tmp").exists());
        super::remove(&path).unwrap();
        assert!(!path.exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
