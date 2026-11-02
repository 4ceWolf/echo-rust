//! Downloads the Depth Anything V2 Small ONNX model on first run, if it's
//! not already present. See docs/ARCHITECTURE_DECISIONS.md for exactly
//! which file this is, why (license, not a different quantization/export),
//! and where the checksum below comes from.

use std::io::Write;
use std::path::{Path, PathBuf};

use log::{info, warn};
use sha2::{Digest, Sha256};

use crate::error::PlayerError;

const MODEL_URL: &str =
    "https://huggingface.co/onnx-community/depth-anything-v2-small/resolve/main/onnx/model.onnx";
const EXPECTED_SHA256: &str = "afb6a5c28f3b6bf1618c6e43f02073ef9dfdc70e937502d51603e57b0a1df10c";
const EXPECTED_SIZE_BYTES: u64 = 99_100_000; // ~99.1MB per the model page; used only as a sanity bound for the download limit, not exact-match verified

pub fn model_cache_path() -> PathBuf {
    let base = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join(".local")
        .join("share")
        .join("echo")
        .join("models")
        .join("depth-anything-v2-small.onnx")
}

/// Downloads the model if `model_cache_path()` doesn't already have a
/// file that passes checksum verification. Blocking - meant to be called
/// once at startup before the depth engine is needed, not from inside
/// the UI event loop.
pub fn ensure_model_downloaded() -> Result<PathBuf, PlayerError> {
    let path = model_cache_path();

    if path.exists() {
        match verify_checksum(&path) {
            Ok(true) => {
                info!("depth model already present and verified at {}", path.display());
                return Ok(path);
            }
            Ok(false) => {
                warn!(
                    "depth model at {} failed checksum verification (corrupted or wrong file?) - re-downloading",
                    path.display()
                );
                let _ = std::fs::remove_file(&path);
            }
            Err(e) => {
                warn!("failed to verify existing depth model, re-downloading: {e}");
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| PlayerError::Other(format!("failed to create model cache dir: {e}")))?;
    }

    info!("downloading depth model (~99MB) from {MODEL_URL} ...");
    let bytes = ureq::get(MODEL_URL)
        .call()
        .map_err(|e| PlayerError::Other(format!("depth model download request failed: {e}")))?
        .body_mut()
        .with_config()
        // A generous ceiling above the expected size so a misbehaving
        // server/redirect can't exhaust memory - see ureq 3.x docs on
        // why an explicit limit() matters here (unbounded by default).
        .limit(EXPECTED_SIZE_BYTES * 2)
        .read_to_vec()
        .map_err(|e| PlayerError::Other(format!("failed to read depth model download body: {e}")))?;

    // Write to a .part file and rename into place only after a full,
    // successful write - so a crash/power-loss mid-download can never
    // leave a partial file sitting at the real final path looking like a
    // complete (if corrupt) model. The checksum check below still runs
    // as a second, independent line of defense either way.
    let part_path = path.with_extension("onnx.part");
    let mut file = std::fs::File::create(&part_path)
        .map_err(|e| PlayerError::Other(format!("failed to create model file: {e}")))?;
    file.write_all(&bytes)
        .map_err(|e| PlayerError::Other(format!("failed to write model file: {e}")))?;
    file.sync_all()
        .map_err(|e| PlayerError::Other(format!("failed to flush model file to disk: {e}")))?;
    drop(file);
    std::fs::rename(&part_path, &path)
        .map_err(|e| PlayerError::Other(format!("failed to finalize model file: {e}")))?;

    if !verify_checksum(&path)? {
        let _ = std::fs::remove_file(&path);
        return Err(PlayerError::Other(
            "downloaded depth model failed checksum verification - download may be corrupted \
             or the upstream file may have changed; see docs/ARCHITECTURE_DECISIONS.md for the \
             expected hash and re-verify against the model's Hugging Face page before updating it"
                .into(),
        ));
    }

    info!("depth model downloaded and verified: {}", path.display());
    Ok(path)
}

fn verify_checksum(path: &Path) -> Result<bool, PlayerError> {
    let bytes = std::fs::read(path).map_err(|e| PlayerError::Other(format!("failed to read model file for checksum: {e}")))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let actual = format!("{:x}", hasher.finalize());
    Ok(actual == EXPECTED_SHA256)
}
