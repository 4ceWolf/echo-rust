//! The depth-effect pipeline: album art -> depth mask -> cached PNG.
//!
//! Ties together the two halves that were previously disconnected (see
//! docs/TODO.md's "depth engine never wired" gap, now closed):
//! `art::extract_front_cover` (tag reading) and `depth::DepthEngine`
//! (inference). This module is deliberately the ONLY place those two meet,
//! so the layering stays one-directional (art knows nothing about depth,
//! depth knows nothing about art).

use std::path::{Path, PathBuf};

use log::info;
use sha1::{Digest, Sha1};

use crate::art;
use crate::depth::{self, DepthEngine};
use crate::error::PlayerError;

/// Computes (or loads from cache) the depth mask for one track.
///
/// Returns `Ok(None)` when the track has no embedded art - normal, not an
/// error. Returns the mask's on-disk path on success. Inference happens
/// here, so this is meant to run on a background thread (it can take a
/// second-plus on CPU for a first-time track); see main.rs's worker.
pub fn compute_depth_mask(
    engine: &mut DepthEngine,
    track_path: &Path,
) -> Result<Option<PathBuf>, PlayerError> {
    let Some(artwork) = art::extract_front_cover(track_path)? else {
        return Ok(None);
    };

    let art_hash = sha1_hex(&artwork.data);
    let cache_dir = depth::depth_cache_dir();
    std::fs::create_dir_all(&cache_dir)
        .map_err(|e| PlayerError::Other(format!("failed to create depth cache dir: {e}")))?;

    let cache_path = depth::cache_path_for(track_path, &art_hash, &cache_dir);
    if cache_path.exists() {
        return Ok(Some(cache_path));
    }

    let img = image::load_from_memory(&artwork.data)
        .map_err(|e| PlayerError::Other(format!("failed to decode embedded art: {e}")))?;

    let mask = engine.infer_mask(&img)?;
    mask.save(&cache_path)
        .map_err(|e| PlayerError::Other(format!("failed to save depth mask cache: {e}")))?;

    info!(
        "computed depth mask for {} -> {}",
        track_path.display(),
        cache_path.display()
    );
    Ok(Some(cache_path))
}

fn sha1_hex(data: &[u8]) -> String {
    let mut hasher = Sha1::new();
    hasher.update(data);
    format!("{:x}", hasher.finalize())
}
