//! Album-art extraction from audio-file tags, via `lofty`, with a
//! filesystem sidecar fallback.
//!
//! This replaces the old Python `art.py`'s `mutagen`-based tag/art
//! reading (see docs/TODO.md's "no album art extraction" gap, now closed).
//! `lofty` was chosen over alternatives per docs/ARCHITECTURE_DECISIONS.md
//! (actively maintained, pure Rust, MIT OR Apache-2.0, all common formats).
//!
//! Artwork resolution order (added 2026-09-13 so the depth engine actually
//! runs for the many libraries that keep cover images as loose files):
//! 1. Embedded front-cover tag (the authoritative source when present).
//! 2. A sidecar image in the same folder: `cover`/`folder`/`front`/
//!    `album`/`albumart`/`artwork` with an image extension, then
//!    `<track-stem>.<ext>`.
//!
//! The depth pipeline (`src/pipeline.rs`) and the `echo://art/` asset server
//! both go through `extract_front_cover`, so this is the single choke point
//! for "what artwork belongs to this track".

use std::path::{Path, PathBuf};

use lofty::file::TaggedFileExt;
use log::warn;

use crate::error::PlayerError;

/// Raw artwork plus its detected mime type (best-effort; falls back to
/// `image/jpeg` if the bytes can't be sniffed).
pub struct Artwork {
    pub data: Vec<u8>,
    pub mime: String,
}

/// Preferred sidecar basenames, in priority order (case-insensitive).
const SIDECAR_STEMS: &[&str] = &["cover", "folder", "front", "album", "albumart", "artwork"];

/// Image extensions accepted for sidecar artwork (case-insensitive).
const SIDECAR_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// Extract the front-cover art for `path`: embedded tag art first, then a
/// sidecar image in the same folder.
///
/// Returns `Ok(None)` when neither source has art - a normal, common case
/// (many files are untagged and have no `cover.jpg`), NOT an error. Callers
/// should treat `None` as "no art available" and fall back. `Err` is
/// reserved for genuine failures (unreadable/corrupt file).
pub fn extract_front_cover(path: &Path) -> Result<Option<Artwork>, PlayerError> {
    match extract_embedded(path) {
        Ok(Some(artwork)) => return Ok(Some(artwork)),
        Ok(None) => {}
        Err(e) => {
            // A file lofty can't parse may still have a usable sidecar cover,
            // so log and fall through rather than failing outright.
            warn!("embedded art read failed for {}: {e}", path.display());
        }
    }

    if let Some(sidecar) = find_sidecar_art(path) {
        return read_sidecar(&sidecar).map(Some);
    }

    Ok(None)
}

/// Read the embedded front cover from `path`'s tags.
fn extract_embedded(path: &Path) -> Result<Option<Artwork>, PlayerError> {
    let tagged = lofty::read_from_path(path).map_err(|e| {
        PlayerError::Other(format!(
            "failed to read tags for {}: {e}",
            path.display()
        ))
    })?;

    // Prefer the primary tag (e.g. ID3v2 for MP3); fall back to the first
    // available tag of any type if there is no primary one.
    let Some(tag) = tagged.primary_tag().or_else(|| tagged.first_tag()) else {
        return Ok(None);
    };

    let pictures = tag.pictures();
    if pictures.is_empty() {
        return Ok(None);
    }

    // Prefer the explicitly-tagged front cover; some files only tag a
    // generic "other" picture, in which case use the first one anyway.
    let picture = pictures
        .iter()
        .find(|p| p.pic_type() == lofty::picture::PictureType::CoverFront)
        .or_else(|| pictures.first());

    let Some(picture) = picture else {
        return Ok(None);
    };

    let data = picture.data().to_vec();
    if data.is_empty() {
        warn!("empty embedded art in {}", path.display());
        return Ok(None);
    }

    // Detect the mime from the actual bytes rather than trusting the tag's
    // (sometimes wrong/absent) mime field - the depth pipeline only needs
    // the bytes, but the mime is kept for cache/serving correctness.
    let mime = sniff_mime(&data);
    Ok(Some(Artwork { data, mime }))
}

/// Find a sidecar cover image next to `track_path`, if any. Named covers
/// (`cover.jpg` etc.) win over a track-stem match (`song.jpg`).
pub fn find_sidecar_art(track_path: &Path) -> Option<PathBuf> {
    let dir = track_path.parent()?;
    let stem = track_path
        .file_stem()
        .map(|s| s.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let mut best: Option<(usize, PathBuf)> = None;
    let mut stem_match: Option<PathBuf> = None;

    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let candidate = entry.path();
        if !candidate.is_file() {
            continue;
        }
        let Some(ext) = candidate.extension().and_then(|e| e.to_str()) else {
            continue;
        };
        if !SIDECAR_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()) {
            continue;
        }
        let file_stem = candidate
            .file_stem()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        if !stem.is_empty() && file_stem == stem {
            stem_match = Some(candidate.clone());
        }
        if let Some(rank) = SIDECAR_STEMS.iter().position(|n| *n == file_stem) {
            let better = match &best {
                Some((current, _)) => rank < *current,
                None => true,
            };
            if better {
                best = Some((rank, candidate));
            }
        }
    }

    best.map(|(_, path)| path).or(stem_match)
}

/// Read a sidecar image file into `Artwork`.
fn read_sidecar(path: &Path) -> Result<Artwork, PlayerError> {
    let data = std::fs::read(path).map_err(|e| {
        PlayerError::Other(format!("failed to read sidecar art {}: {e}", path.display()))
    })?;
    if data.is_empty() {
        return Err(PlayerError::Other(format!(
            "empty sidecar art {}",
            path.display()
        )));
    }
    let mime = sniff_mime(&data);
    Ok(Artwork { data, mime })
}

fn sniff_mime(data: &[u8]) -> String {
    image::guess_format(data)
        .map(|f| f.to_mime_type().to_string())
        .unwrap_or_else(|_| "image/jpeg".to_string())
}

/// Probe a track's duration in milliseconds from its container header (no
/// audio decode - reads the tag/file properties). Returns `None` for
/// unreadable files or files with no known duration.
pub fn track_duration_ms(path: &Path) -> Option<u64> {
    use lofty::file::AudioFile;
    let tagged = lofty::read_from_path(path).ok()?;
    Some(tagged.properties().duration().as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::find_sidecar_art;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("echo_art_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_named_sidecar_cover() {
        let dir = temp_dir("named");
        let track = dir.join("song.mp3");
        std::fs::write(&track, b"not audio").unwrap();
        std::fs::write(dir.join("cover.png"), b"x").unwrap();

        assert_eq!(find_sidecar_art(&track), Some(dir.join("cover.png")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn prefers_named_cover_over_track_stem() {
        let dir = temp_dir("prefer");
        let track = dir.join("song.mp3");
        std::fs::write(&track, b"not audio").unwrap();
        std::fs::write(dir.join("song.jpg"), b"x").unwrap();
        std::fs::write(dir.join("folder.jpg"), b"x").unwrap();

        assert_eq!(find_sidecar_art(&track), Some(dir.join("folder.jpg")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn falls_back_to_track_stem_image() {
        let dir = temp_dir("stem");
        let track = dir.join("song.mp3");
        std::fs::write(&track, b"not audio").unwrap();
        std::fs::write(dir.join("song.png"), b"x").unwrap();

        assert_eq!(find_sidecar_art(&track), Some(dir.join("song.png")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ignores_non_image_files() {
        let dir = temp_dir("ignore");
        let track = dir.join("song.mp3");
        std::fs::write(&track, b"not audio").unwrap();
        std::fs::write(dir.join("cover.txt"), b"x").unwrap();
        std::fs::write(dir.join("notes.jpg"), b"x").unwrap();

        assert_eq!(find_sidecar_art(&track), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
