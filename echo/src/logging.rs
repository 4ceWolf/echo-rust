//! Logging, built first and deliberately, per docs/TODO.md's "Debugging &
//! Logging" section: the previous project's biggest recurring problem was
//! bugs that "should" have been visible but weren't, because nothing was
//! logged anywhere durable. Two things this module guarantees:
//!
//! 1. Every run appends to a real log file on disk (~/.local/share/echo/logs/),
//!    not just stdout that vanishes when a terminal closes.
//! 2. The last N error/warn-level lines are kept in memory too, so the
//!    Settings UI can show "recent errors" without needing the user to
//!    dig through a log file or paste terminal output at all - that
//!    back-and-forth was the single biggest bottleneck debugging the
//!    previous project.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use log::{Level, LevelFilter, Log, Metadata, Record};

const MAX_LOG_FILE_BYTES: u64 = 5 * 1024 * 1024; // 5MB, then rotate
const MAX_RECENT_ERRORS: usize = 200;

// Not yet used: recent_errors() and the RecentError struct exist for the
// future Settings panel's "recent errors" view (see docs/TODO.md) - no
// UI calls it yet since there's no Settings panel at all currently.
#[allow(dead_code)]
pub struct RecentError {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

struct EchoLogger {
    file: Mutex<std::fs::File>,
    recent_errors: Mutex<Vec<RecentError>>,
}

static LOGGER: std::sync::OnceLock<EchoLogger> = std::sync::OnceLock::new();

fn log_dir() -> PathBuf {
    let base = dirs_home().join(".local").join("share").join("echo").join("logs");
    base
}

// Deliberately not pulling in the `dirs` crate for one call - HOME is
// reliable enough on the Linux desktop this targets, and one less
// dependency to version-track.
fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
}

fn rotate_if_needed(path: &PathBuf) {
    if let Ok(meta) = fs::metadata(path) {
        if meta.len() > MAX_LOG_FILE_BYTES {
            let rotated = path.with_extension("log.old");
            let _ = fs::rename(path, rotated);
        }
    }
}

impl Log for EchoLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let line = format!(
            "[{timestamp}] [{}] [{}] {}\n",
            record.level(),
            record.target(),
            record.args()
        );

        if let Ok(mut file) = self.file.lock() {
            // Best-effort: a failed write here must never panic or crash
            // the app - logging a problem should never itself become one.
            let _ = file.write_all(line.as_bytes());
            let _ = file.flush();
        }

        if matches!(record.level(), Level::Error | Level::Warn) {
            if let Ok(mut recent) = self.recent_errors.lock() {
                recent.push(RecentError {
                    timestamp: timestamp.to_string(),
                    level: record.level().to_string(),
                    target: record.target().to_string(),
                    message: record.args().to_string(),
                });
                if recent.len() > MAX_RECENT_ERRORS {
                    let overflow = recent.len() - MAX_RECENT_ERRORS;
                    recent.drain(0..overflow);
                }
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut file) = self.file.lock() {
            let _ = file.flush();
        }
    }
}

/// Call once, at the very start of `main()`, before anything else that
/// might fail - so failures during startup itself still get logged.
pub fn init() -> std::io::Result<()> {
    let dir = log_dir();
    fs::create_dir_all(&dir)?;
    let path = dir.join("echo.log");
    rotate_if_needed(&path);

    let file = OpenOptions::new().create(true).append(true).open(&path)?;

    let logger = EchoLogger {
        file: Mutex::new(file),
        recent_errors: Mutex::new(Vec::new()),
    };

    // If init() is somehow called twice, keep the first logger rather
    // than panicking - a duplicate init attempt is a bug worth surfacing
    // via a log line, not a crash.
    if LOGGER.set(logger).is_err() {
        log::warn!("logging::init called more than once - ignoring second call");
        return Ok(());
    }

    log::set_logger(LOGGER.get().unwrap())
        .map(|()| log::set_max_level(LevelFilter::Debug))
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;

    log::info!("logging initialized, writing to {}", path.display());
    Ok(())
}

/// For the Settings UI's "recent errors" panel - see docs/TODO.md.
#[allow(dead_code)] // see docs/TODO.md - no Settings panel exists yet to call this
pub fn recent_errors() -> Vec<String> {
    LOGGER
        .get()
        .map(|l| {
            l.recent_errors
                .lock()
                .map(|errs| {
                    errs.iter()
                        .map(|e| format!("[{}] [{}] {}: {}", e.timestamp, e.level, e.target, e.message))
                        .collect()
                })
                .unwrap_or_default()
        })
        .unwrap_or_default()
}
