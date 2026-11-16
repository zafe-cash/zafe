use std::{
    io::Write,
    panic::PanicHookInfo,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const FILE_NAME: &str = "rust-panics.log";
const TIMINGS_FILE: &str = "timings.log";
const SLOW: Duration = Duration::from_millis(1500);
const MAX_BYTES: u64 = 16 * 1024;

static DIAG_DIR: OnceLock<PathBuf> = OnceLock::new();

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

pub(crate) fn remember_support_dir(db_dir: &str) {
    if db_dir.is_empty() {
        return;
    }
    let _ = DIAG_DIR.set(Path::new(db_dir).join("diagnostics"));
}

fn note(info: &PanicHookInfo<'_>) {
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

fn line(secs: u64, location: &str) -> String {
    format!("{} panic at {location}", stamp(secs))
}

fn stamp(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
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
        keep_newest_half(&path);
    }
    let mut file = options.open(&path)?;
    file.write_all(format!("{text}\n").as_bytes())
}

fn keep_newest_half(path: &Path) {
    let kept = std::fs::read(path).ok().and_then(|bytes| {
        let half = bytes.len() / 2;
        let start = half + bytes[half..].iter().position(|b| *b == b'\n')? + 1;
        Some(bytes[start..].to_vec())
    });
    let tmp = path.with_extension("trim");
    match kept {
        Some(kept) if std::fs::write(&tmp, &kept).is_ok() => {
            let _ = std::fs::rename(&tmp, path);
        }
        _ => {
            let _ = std::fs::remove_file(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_has_date_and_location_only() {
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
    fn a_large_file_keeps_its_newest_lines() {
        let dir = std::env::temp_dir().join(format!("zafe-diag-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let lines: Vec<String> = (0..2000).map(|i| format!("line {i:05}")).collect();
        std::fs::write(dir.join(FILE_NAME), lines.join("\n") + "\n").unwrap();
        write_line(&dir, FILE_NAME, "new").unwrap();
        let text = std::fs::read_to_string(dir.join(FILE_NAME)).unwrap();
        assert!(text.len() as u64 <= MAX_BYTES);
        assert!(text.starts_with("line ") && text.ends_with("line 01999\nnew\n"));
        assert!(!text.contains("line 00000"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
