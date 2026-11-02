//! Library scanning + index.
//!
//! Walks a folder tree for audio files, extracts metadata (title/artist/
//! album/duration + technical sample-rate/bit-depth) via `lofty`, and holds
//! the result as an in-memory list persisted to JSON. This is the "browse my
//! library" layer that was missing - the app previously only played explicit
//! file paths (see docs/UI_MASTER_DESIGN.md's "split by reality" and the
//! library-index gap in docs/TODO.md).
//!
//! Deliberately boring: one flat JSON file (`~/.local/share/echo/library.json`),
//! recursive `std::fs` walk (no `walkdir` dependency), metadata from `lofty`
//! (already a dependency). A real DB can replace the JSON file later without
//! changing the `Track` shape.

use std::path::{Path, PathBuf};

use log::{info, warn};
use serde::{Deserialize, Serialize};

/// One indexed track. Carries the "metadata identity" (title/artist/album)
/// plus the "technical identity" (duration/sample-rate/bit-depth) the UI
/// needs for format badges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: u64,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub has_art: bool,
}

/// Audio extensions `lofty` parses (see ARCHITECTURE_DECISIONS.md "Album-art
/// reading: lofty" for the full supported-format list).
const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "m4a", "mp4", "ogg", "oga", "opus", "spx", "wav", "aac", "ape", "mpc", "wv",
    "aiff",
];

pub fn library_json_path() -> PathBuf {
    data_dir().join("library.json")
}

fn data_dir() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(".local").join("share").join("echo")
}

/// Recursively scan `dir` for audio files and extract metadata. Files that
/// fail to read are skipped (logged), not fatal. Sorted by path for a
/// deterministic, stable index.
pub fn scan_folder(dir: &Path) -> Vec<Track> {
    let mut tracks = Vec::new();
    walk(dir, &mut tracks);
    tracks.sort_by(|a, b| a.path.cmp(&b.path));
    info!("scanned {}: {} tracks", dir.display(), tracks.len());
    tracks
}

fn walk(dir: &Path, out: &mut Vec<Track>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            warn!("failed to read dir {}: {e}", dir.display());
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            // Simple recursion. A symlink cycle could loop; for a v1 local
            // music library this is an accepted, documented limitation (a
            // visited-set guard can be added if it ever matters).
            walk(&path, out);
        } else if ft.is_file() && is_audio(&path) {
            if let Some(track) = extract_track(&path) {
                out.push(track);
            }
        }
    }
}

fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn extract_track(path: &Path) -> Option<Track> {
    use lofty::file::{AudioFile, TaggedFileExt};
    use lofty::tag::Accessor;

    let tagged = match lofty::read_from_path(path) {
        Ok(t) => t,
        Err(e) => {
            warn!("failed to read tags for {}: {e}", path.display());
            return None;
        }
    };
    let props = tagged.properties();
    let tag = tagged.primary_tag().or_else(|| tagged.first_tag());

    let fallback_title = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();

    Some(Track {
        path: path.to_string_lossy().to_string(),
        title: tag
            .and_then(|t| t.title())
            .map(|c| c.to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or(fallback_title),
        artist: tag.and_then(|t| t.artist()).map(|c| c.to_string()).unwrap_or_default(),
        album: tag.and_then(|t| t.album()).map(|c| c.to_string()).unwrap_or_default(),
        duration_ms: props.duration().as_millis() as u64,
        sample_rate: props.sample_rate(),
        bit_depth: props.bit_depth(),
        // Art can be embedded in the tag OR a sidecar image next to the file
        // (cover.jpg/folder.jpg/<track>.jpg). The depth pipeline uses the
        // same resolution order, so this must match to keep the UI's
        // "Depth resolving" state truthful. See src/art.rs.
        has_art: tag.map(|t| !t.pictures().is_empty()).unwrap_or(false)
            || crate::art::find_sidecar_art(path).is_some(),
    })
}

/// Load the persisted library index, or an empty list if missing/corrupt.
pub fn load() -> Vec<Track> {
    let path = library_json_path();
    match std::fs::read_to_string(&path) {
        Ok(text) => match serde_json::from_str::<Vec<Track>>(&text) {
            Ok(v) => v,
            Err(e) => {
                warn!("library index at {} is corrupt, starting empty: {e}", path.display());
                Vec::new()
            }
        },
        Err(_) => Vec::new(),
    }
}

/// Persist the library index. Best-effort.
pub fn save(tracks: &[Track]) {
    let path = library_json_path();
    let write = || -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string(tracks).map_err(std::io::Error::other)?;
        std::fs::write(&path, json)
    };
    if let Err(e) = write() {
        warn!("failed to persist library index to {}: {e}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a minimal valid 8kHz/mono/8-bit PCM WAV of `seconds` duration.
    fn write_test_wav(path: &Path, seconds: u32) {
        let sample_rate: u32 = 8000;
        let data_len = sample_rate * seconds;
        let mut b = Vec::new();
        b.extend_from_slice(b"RIFF");
        b.extend_from_slice(&(36 + data_len).to_le_bytes());
        b.extend_from_slice(b"WAVE");
        b.extend_from_slice(b"fmt ");
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes()); // PCM
        b.extend_from_slice(&1u16.to_le_bytes()); // mono
        b.extend_from_slice(&sample_rate.to_le_bytes());
        b.extend_from_slice(&sample_rate.to_le_bytes()); // byte rate
        b.extend_from_slice(&1u16.to_le_bytes()); // block align
        b.extend_from_slice(&8u16.to_le_bytes()); // bits/sample
        b.extend_from_slice(b"data");
        b.extend_from_slice(&data_len.to_le_bytes());
        b.extend(std::iter::repeat(128u8).take(data_len as usize));
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn scan_extracts_wav_duration_and_rate() {
        let dir = std::env::temp_dir().join(format!("echo_lib_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write_test_wav(&dir.join("song.wav"), 1);

        let tracks = scan_folder(&dir);
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].duration_ms, 1000);
        assert_eq!(tracks[0].sample_rate, Some(8000));
        assert_eq!(tracks[0].bit_depth, Some(8));
        assert!(tracks[0].path.ends_with("song.wav"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn scan_filters_non_audio_and_unreadable() {
        let dir = std::env::temp_dir().join(format!("echo_lib_test2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("readme.txt"), b"hi").unwrap();
        std::fs::write(dir.join("fake.mp3"), b"not really audio").unwrap();

        let tracks = scan_folder(&dir);
        assert!(tracks.is_empty()); // txt filtered out, fake mp3 unreadable -> skipped

        let _ = std::fs::remove_dir_all(&dir);
    }
}
