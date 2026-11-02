//! User settings persistence.
//!
//! Persists the playback/queue/compositor settings that should survive a
//! restart (speed, mono downmix, Concert Hall, shuffle/repeat, niri blur).
//! Theme/accent/appearance are frontend concerns and intentionally NOT here
//! (no backend representation exists for them yet).
//!
//! Deliberately boring: one flat JSON file, load-at-startup + save-on-change.

use std::path::PathBuf;

use log::warn;
use serde::{Deserialize, Serialize};

use crate::queue::RepeatMode;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub speed: f64,
    pub mono_downmix: bool,
    pub concert_hall: bool,
    pub shuffle: bool,
    pub smart_shuffle: bool,
    pub repeat: RepeatMode,
    pub niri_blur: bool,
    /// The user's music folder for library scanning (set via onboarding or
    /// the `set_library_path` IPC command). None until configured.
    pub library_path: Option<String>,
    /// Smart-shuffle bias strength: 0.0 = ignore play history (uniform
    /// random), 1.0 = strongly favor tracks not played recently. Persisted.
    #[serde(default = "default_smart_shuffle_bias")]
    pub smart_shuffle_bias: f64,
    /// Crossfade duration in ms. 0 = gapless (default).
    #[serde(default)]
    pub crossfade_ms: u64,
}

fn default_smart_shuffle_bias() -> f64 {
    1.0
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            speed: 1.0,
            mono_downmix: false,
            concert_hall: false,
            shuffle: false,
            smart_shuffle: false,
            repeat: RepeatMode::Off,
            niri_blur: false,
            library_path: None,
            smart_shuffle_bias: default_smart_shuffle_bias(),
            crossfade_ms: 0,
        }
    }
}

pub fn settings_path() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(".local")
        .join("share")
        .join("echo")
        .join("settings.json")
}

/// Load settings, or defaults if missing/corrupt (corruption is logged and
/// treated as defaults - the user can just re-set them).
pub fn load() -> Settings {
    let path = settings_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Settings>(&text) {
            Ok(s) => s,
            Err(e) => {
                warn!("settings at {} are corrupt, using defaults: {e}", path.display());
                Settings::default()
            }
        },
        Err(_) => Settings::default(),
    }
}

/// Persist settings. Best-effort; a failed write is logged and swallowed.
pub fn save(settings: &Settings) {
    let path = settings_path();
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(settings).map_err(std::io::Error::other)?;
        std::fs::write(&path, json)
    };
    if let Err(e) = write() {
        warn!("failed to persist settings to {}: {e}", path.display());
    }
}
