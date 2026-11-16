use std::{fs, io, path::Path};

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

pub fn create_new(path: &Path, bytes: &[u8]) -> io::Result<bool> {
    static CREATING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = CREATING.lock().unwrap_or_else(|e| e.into_inner());
    if path.exists() {
        return Ok(false);
    }
    write(path, bytes).map(|()| true)
}

pub fn remove(path: &Path) -> io::Result<()> {
    fs::remove_file(path)?;
    sync_dir(path)
}

#[cfg(unix)]
fn sync_dir(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => fs::File::open(dir)?.sync_all(),
        _ => fs::File::open(".")?.sync_all(),
    }
}

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
