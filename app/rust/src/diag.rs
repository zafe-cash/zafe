//! Panic notes for the local diagnostic report (Settings > Diagnostic report).
//!
//! The hook writes one line per panic to `<support>/diagnostics/rust-panics.log`:
//! a UTC timestamp and the panic's `file:line:column`, **never the message or payload**
//! (they can hold secrets). Nothing is sent anywhere. The directory is learned from the
//! first wallet call (`remember_support_dir`): until then a panic leaves no note.

use std::{
    io::Write,
    panic::PanicHookInfo,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

const FILE_NAME: &str = "rust-panics.log";
/// Past this size the file is restarted, so it can't grow without bound.
const MAX_BYTES: u64 = 8 * 1024;

static DIAG_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Installs the hook once, after whatever hook was set before (default: stderr message).
pub(crate) fn install() {
    static ONCE: OnceLock<()> = OnceLock::new();
    ONCE.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            note(info);
            previous(info);
        }));
    });
}

/// Remembers `<support>/diagnostics` from the wallet DB directory the app already passes
/// to every wallet call (the support directory). The first call wins.
pub(crate) fn remember_support_dir(db_dir: &str) {
    if db_dir.is_empty() {
        return;
    }
    let _ = DIAG_DIR.set(Path::new(db_dir).join("diagnostics"));
}

fn note(info: &PanicHookInfo<'_>) {
    // The hook must never panic itself.
    let _ = std::panic::catch_unwind(|| {
        let Some(dir) = DIAG_DIR.get() else { return };
        let location = match info.location() {
            Some(l) => format!("{}:{}:{}", l.file(), l.line(), l.column()),
            None => "unknown".to_string(),
        };
        let _ = write_line(dir, &line(unix_now(), &location));
    });
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `<ISO-8601 UTC> panic at <file:line:col>`: the message is not included.
fn line(secs: u64, location: &str) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z panic at {location}",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

fn write_line(dir: &Path, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(FILE_NAME);
    let mut options = std::fs::OpenOptions::new();
    options.create(true).append(true);
    if std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) > MAX_BYTES {
        std::fs::remove_file(&path).ok();
    }
    let mut file = options.open(&path)?;
    // One write call per line (append mode): safe next to the other threads' panics.
    file.write_all(format!("{text}\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_has_date_and_location_only() {
        // 2026-10-07T10:00:00Z
        assert_eq!(
            line(1_791_367_200, "src/a.rs:1:2"),
            "2026-10-07T10:00:00Z panic at src/a.rs:1:2"
        );
        assert_eq!(line(0, "x"), "1970-01-01T00:00:00Z panic at x");
    }

    #[test]
    fn restarts_a_large_file() {
        let dir = std::env::temp_dir().join(format!("zafe-diag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        write_line(&dir, "new").unwrap();
        assert_eq!(std::fs::read_to_string(dir.join(FILE_NAME)).unwrap(), "new\n");
        std::fs::remove_dir_all(&dir).ok();
    }
}
