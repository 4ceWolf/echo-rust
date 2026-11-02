//! Per-track depth-mask generation from album art, using Depth Anything
//! V2 Small - the same technique Noctalia's `wallpaper_depth` plugin uses
//! for desktop wallpapers (see docs/ARCHITECTURE_DECISIONS.md), applied
//! to album art instead.
//!
//! CRITICAL, read before touching the model file or these constants:
//! - Only the **Small** Depth Anything V2 variant is Apache-2.0.
//!   Base/Large/Giant are CC-BY-NC-4.0 (non-commercial). Never substitute
//!   a different variant in, even for "better quality" - see
//!   docs/ARCHITECTURE_DECISIONS.md's "license trap" section.
//! - This model outputs **relative inverse depth**, not metric depth in
//!   meters. A different, separate ONNX export of the same architecture
//!   produces metric output - that is a different file and would need
//!   different post-processing. Do not mix them up.

use std::path::{Path, PathBuf};

use image::{DynamicImage, GenericImageView};
use log::info;
use ndarray::{Array4, Axis};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::TensorRef;
use sha1::{Digest, Sha1};

use crate::error::PlayerError;

/// Identifies the exact model + post-processing contract that produced a
/// cached mask. Bumped (or changed) whenever the model file or the
/// normalization contract changes, so stale masks are automatically
/// invalidated via the cache key (see `cache_path_for`).
pub const MODEL_VERSION: &str = "dav2-small-v1";

const MODEL_INPUT_SIZE: u32 = 518;
const IMAGENET_MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const IMAGENET_STD: [f32; 3] = [0.229, 0.224, 0.225];

pub struct DepthEngine {
    session: Session,
}

impl DepthEngine {
    /// `model_path` must point at the Depth Anything V2 **Small**
    /// (Apache-2.0) ONNX file - see module doc comment.
    pub fn new(model_path: &Path) -> Result<Self, PlayerError> {
        if !model_path.exists() {
            return Err(PlayerError::Other(format!(
                "depth model not found at {} - see docs/TODO.md for where to download it",
                model_path.display()
            )));
        }

        let session = Session::builder()
            .map_err(|e| PlayerError::Other(format!("ort session builder init failed: {e}")))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| PlayerError::Other(format!("failed to set optimization level: {e}")))?
            .commit_from_file(model_path)
            .map_err(|e| PlayerError::Other(format!("failed to load depth model: {e}")))?;

        info!("depth model loaded from {}", model_path.display());
        Ok(Self { session })
    }

    /// Runs inference and returns a grayscale depth mask resized back to
    /// the *original* image's dimensions - white (255) = closest/foreground,
    /// black (0) = farthest/background, after per-image min/max
    /// normalization (required - see module doc comment on relative
    /// inverse depth).
    ///
    /// Takes `&mut self` because `ort`'s `Session::run` is not interior-
    /// mutable; callers doing this on a background thread own the engine
    /// exclusively (see main.rs's depth worker) and must not share it.
    pub fn infer_mask(&mut self, img: &DynamicImage) -> Result<image::GrayImage, PlayerError> {
        let (orig_w, orig_h) = img.dimensions();

        let resized = img.resize_exact(
            MODEL_INPUT_SIZE,
            MODEL_INPUT_SIZE,
            image::imageops::FilterType::CatmullRom, // bicubic-equivalent
        );
        let rgb = resized.to_rgb8();

        // NCHW f32 tensor, ImageNet-normalized - see module doc comment
        // for the exact contract this model expects.
        let mut input = Array4::<f32>::zeros((1, 3, MODEL_INPUT_SIZE as usize, MODEL_INPUT_SIZE as usize));
        for (x, y, pixel) in rgb.enumerate_pixels() {
            for c in 0..3 {
                let value = pixel[c] as f32 / 255.0;
                let normalized = (value - IMAGENET_MEAN[c]) / IMAGENET_STD[c];
                input[[0, c, y as usize, x as usize]] = normalized;
            }
        }

        let outputs = self
            .session
            .run(ort::inputs![TensorRef::from_array_view(&input)
                .map_err(|e| PlayerError::Other(format!("failed to build input tensor: {e}")))?])
            .map_err(|e| PlayerError::Other(format!("depth inference failed: {e}")))?;

        // Confirmed against the real `SessionOutputs` source
        // (docs.rs/crate/ort/latest/source/src/session/output.rs) - it
        // implements IntoIterator yielding (&str, ValueRef) pairs via
        // .iter(), exactly as used below. Note: that fetch resolved to
        // rc.13 (one patch above this project's pinned rc.12) since
        // "latest" always serves the newest available - the SessionOutputs
        // shape is unlikely to have changed in a single patch release,
        // but if this doesn't compile against rc.12 specifically, this
        // is the first place to check.
        let (_output_name, output_value) = outputs
            .iter()
            .next()
            .ok_or_else(|| PlayerError::Other("depth model produced no outputs".into()))?;

        // try_extract_array is the rc.12-current API - see
        // docs/ARCHITECTURE_DECISIONS.md's note on ort's output-extraction
        // API having genuinely changed shape across rc versions. Do not
        // "simplify" this to try_extract_tensor without re-checking that
        // note first. One more honest caveat: output_value here is a
        // ValueRef (from SessionOutputs::iter()), not a DynValue directly
        // - calling try_extract_array on it was not independently
        // confirmed against a ValueRef-specific example (only against
        // DynValue/Tensor examples), though reference-wrapper types in
        // this crate typically expose the same extraction methods as
        // their owned counterpart. If this specific line fails to
        // compile, try `output_value.try_extract_array::<f32>()` first
        // (as written), then fall back to checking whether ValueRef needs
        // an explicit `.downcast()` or similar step first.
        let depth_view = output_value
            .try_extract_array::<f32>()
            .map_err(|e| PlayerError::Other(format!("failed to extract depth output: {e}")))?;

        // Output is 1x518x518 relative inverse depth (NOT metric - see
        // module doc comment). Must normalize against THIS image's own
        // min/max since values are only meaningful within one image.
        let depth_2d = depth_view
            .index_axis(Axis(0), 0)
            .to_owned();
        let (min, max) = depth_2d.iter().fold((f32::MAX, f32::MIN), |(mn, mx), &v| {
            (mn.min(v), mx.max(v))
        });
        let range = (max - min).max(1e-6); // avoid divide-by-zero on a flat/blank image

        let mut mask_small = image::GrayImage::new(MODEL_INPUT_SIZE, MODEL_INPUT_SIZE);
        for y in 0..MODEL_INPUT_SIZE as usize {
            for x in 0..MODEL_INPUT_SIZE as usize {
                let normalized = (depth_2d[[y, x]] - min) / range;
                let pixel_value = (normalized.clamp(0.0, 1.0) * 255.0) as u8;
                mask_small.put_pixel(x as u32, y as u32, image::Luma([pixel_value]));
            }
        }

        let mask_full_size = image::imageops::resize(
            &mask_small,
            orig_w,
            orig_h,
            image::imageops::FilterType::Triangle,
        );

        Ok(mask_full_size)
    }
}

/// Directory where cached depth masks live (`~/.local/share/echo/depth_masks/`).
pub fn depth_cache_dir() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(".local")
        .join("share")
        .join("echo")
        .join("depth_masks")
}

/// Cache path for a track's depth mask, keyed by `sha1(track path + "|" +
/// art hash + "|" + MODEL_VERSION)`. Including the art hash and model
/// version means a track whose embedded art changes (without the file path
/// changing) or whose model is upgraded gets a fresh mask instead of a
/// stale one - this was the audit's "depth cache key" finding, now fixed.
pub fn cache_path_for(track_path: &Path, art_hash: &str, cache_dir: &Path) -> PathBuf {
    let mut hasher = Sha1::new();
    hasher.update(track_path.to_string_lossy().as_bytes());
    hasher.update(b"|");
    hasher.update(art_hash.as_bytes());
    hasher.update(b"|");
    hasher.update(MODEL_VERSION.as_bytes());
    let hash = hasher.finalize();
    cache_dir.join(format!("{:x}.png", hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_is_stable_for_same_inputs() {
        let dir = Path::new("/tmp/cache");
        let a = cache_path_for(Path::new("/m/a.mp3"), "hash1", dir);
        let b = cache_path_for(Path::new("/m/a.mp3"), "hash1", dir);
        assert_eq!(a, b);
    }

    #[test]
    fn cache_key_varies_by_track_path() {
        let dir = Path::new("/tmp/cache");
        let a = cache_path_for(Path::new("/m/a.mp3"), "hash1", dir);
        let b = cache_path_for(Path::new("/m/b.mp3"), "hash1", dir);
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_varies_by_art_hash() {
        let dir = Path::new("/tmp/cache");
        let a = cache_path_for(Path::new("/m/a.mp3"), "hash1", dir);
        let b = cache_path_for(Path::new("/m/a.mp3"), "hash2", dir);
        assert_ne!(a, b);
    }

    #[test]
    fn cache_key_ends_with_png() {
        let dir = Path::new("/tmp/cache");
        let p = cache_path_for(Path::new("/m/a.mp3"), "hash1", dir);
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some("png"));
    }
}
