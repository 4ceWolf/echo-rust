//! Persisted play history, feeding smart shuffle.
//!
//! Smart shuffle weights tracks by how long ago they were last played (see
//! `queue.rs`'s `weighted_choice`), which needs a real "last played"
//! timestamp per track - the audit's "smart shuffle has no play-history
//! feed" finding, now closed. The store is a simple `path -> unix_seconds`
//! map persisted as JSON, the same shape `decide_next` already consumes.
//!
//! Deliberately boring: one flat JSON file, best-effort writes, no
//! concurrency concerns (touched only from the main thread under the app's
//! state lock).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use log::warn;

pub type PlayHistory = HashMap<String, u64>;

pub fn history_path() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(".local")
        .join("share")
        .join("echo")
        .join("play_history.json")
}

/// Load the persisted play history, or an empty map if missing/corrupt.
/// Corrupt JSON is logged and treated as empty (a fresh start beats a
/// crash on startup over a file the user can regenerate by just playing
/// music).
pub fn load() -> PlayHistory {
    let path = history_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<PlayHistory>(&text) {
            Ok(map) => map,
            Err(e) => {
                warn!("play history at {} is corrupt, starting fresh: {e}", path.display());
                PlayHistory::new()
            }
        },
        Err(_) => PlayHistory::new(),
    }
}

/// Persist the play history. Best-effort: a failed write is logged and
/// swallowed (play history is a nicety, not a correctness-critical file).
pub fn save(history: &PlayHistory) {
    let path = history_path();
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string(history).map_err(std::io::Error::other)?;
        std::fs::write(&path, json)
    };
    if let Err(e) = write() {
        warn!("failed to persist play history to {}: {e}", path.display());
    }
}

/// Record that `track_path` just started playing.
pub fn record(history: &mut PlayHistory, track_path: &Path) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    history.insert(track_path.to_string_lossy().to_string(), now);
}
