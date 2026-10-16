//! Notes for the local diagnostic report (Settings > Diagnostic report): panics and slow
//! steps.
//!
//! The panic hook writes one line per panic to `<support>/diagnostics/rust-panics.log`:
//! a UTC timestamp and the panic's `file:line:column`, **never the message or payload**
//! (they can hold secrets). `Stopwatch` writes one line to `timings.log` for a slow call
//! (or the first of its kind in this process): the call's name and how long each fixed
//! step took, nothing else. Nothing is sent anywhere. The directory is learned from the
//! first wallet call (`remember_support_dir`): until then nothing is written.

use std::{
    io::Write,
    panic::PanicHookInfo,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const FILE_NAME: &str = "rust-panics.log";
const TIMINGS_FILE: &str = "timings.log";
/// A call at least this slow is noted (so is the first of each kind in a process).
const SLOW: Duration = Duration::from_millis(1500);
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
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let Some(dir) = DIAG_DIR.get() else { return };
        let location = match info.location() {
            Some(l) => format!("{}:{}:{}", l.file(), l.line(), l.column()),
            None => "unknown".to_string(),
        };
        let _ = write_line(dir, FILE_NAME, &line(unix_now(), &location));
    }));
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// `<ISO-8601 UTC> panic at <file:line:col>`: the message is not included.
fn line(secs: u64, location: &str) -> String {
    format!("{} panic at {location}", stamp(secs))
}

/// `2026-10-07T10:00:00Z` for a Unix time in seconds.
fn stamp(secs: u64) -> String {
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
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        rem % 3_600 / 60,
        rem % 60
    )
}

/// Times a call step by step. `lap` names the step that just ended; `finish` writes one
/// line (`<time> sync total=7120ms lock=2ms open=1200ms ...`) when the call was slow or
/// is the first of its kind in this process. Only fixed step names and durations.
pub(crate) struct Stopwatch {
    op: &'static str,
    start: Instant,
    last: Instant,
    steps: Vec<(&'static str, u128)>,
}

impl Stopwatch {
    pub(crate) fn new(op: &'static str) -> Self {
        let now = Instant::now();
        Self {
            op,
            start: now,
            last: now,
            steps: Vec::new(),
        }
    }

    pub(crate) fn lap(&mut self, step: &'static str) {
        let now = Instant::now();
        self.steps
            .push((step, now.duration_since(self.last).as_millis()));
        self.last = now;
    }

    /// Steps timed elsewhere (e.g. inside a core call) that ran since the last lap.
    pub(crate) fn steps(&mut self, steps: &[(&'static str, u128)]) {
        self.steps.extend_from_slice(steps);
        self.last = Instant::now();
    }

    pub(crate) fn finish(self) {
        static SEEN: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());
        let total = self.start.elapsed();
        let first = SEEN.lock().is_ok_and(|mut seen| {
            let first = !seen.contains(&self.op);
            if first {
                seen.push(self.op);
            }
            first
        });
        if total < SLOW && !first {
            return;
        }
        let Some(dir) = DIAG_DIR.get() else { return };
        let text = timing_text(unix_now(), self.op, total.as_millis(), &self.steps);
        let _ = write_line(dir, TIMINGS_FILE, &text);
    }
}

/// `<time> <op> total=<n>ms <step>=<n>ms ...`
fn timing_text(secs: u64, op: &str, total_ms: u128, steps: &[(&str, u128)]) -> String {
    let mut text = format!("{} {op} total={total_ms}ms", stamp(secs));
    for (step, ms) in steps {
        text.push_str(&format!(" {step}={ms}ms"));
    }
    text
}

fn write_line(dir: &Path, file: &str, text: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let path = dir.join(file);
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
    fn timing_lines_hold_only_names_and_durations() {
        assert_eq!(
            timing_text(1_791_367_200, "sync", 7120, &[("lock", 2), ("open", 1200)]),
            "2026-10-07T10:00:00Z sync total=7120ms lock=2ms open=1200ms"
        );
    }

    #[test]
    fn restarts_a_large_file() {
        let dir = std::env::temp_dir().join(format!("zafe-diag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(FILE_NAME), vec![b'x'; MAX_BYTES as usize + 1]).unwrap();
        write_line(&dir, FILE_NAME, "new").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.join(FILE_NAME)).unwrap(),
            "new\n"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
