//! niri compositor blur toggle.
//!
//! niri 26.04+ supports per-window `background-effect { blur true }` inside a
//! `window-rule` (format confirmed against the niri wiki - see
//! docs/ARCHITECTURE_DECISIONS.md). This module manages a single, clearly-
//! delimited block in niri's config so toggling is idempotent and never
//! clobbers the user's own rules. niri auto-reloads its config on save, so
//! no explicit reload call is needed.

use std::path::PathBuf;

use log::{info, warn};

use crate::error::PlayerError;

const MARK_START: &str = "// === echo blur (managed by Echo, do not edit) ===";
const MARK_END: &str = "// === /echo blur ===";

/// The window-rule block we manage, wrapped in the marker comments so
/// `remove_block` can find it again (and so a human can see it's ours).
fn block() -> String {
    format!(
        "{MARK_START}\n\
         window-rule {{\n\
         \x20   match title=\"^Echo\"\n\
         \x20   background-effect {{\n\
         \x20       blur true\n\
         \x20   }}\n\
         }}\n\
         {MARK_END}\n"
    )
}

pub fn config_path() -> PathBuf {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            std::env::var("HOME")
                .map(|h| PathBuf::from(h).join(".config"))
                .unwrap_or_else(|_| PathBuf::from("."))
        });
    base.join("niri").join("config.kdl")
}

/// Pure function: returns the config text with the echo blur block added
/// (enabled) or removed (disabled). Separated from file IO so it's unit-
/// testable.
pub fn toggle_blur(config_text: &str, enabled: bool) -> String {
    let without = remove_block(config_text);
    if !enabled {
        return without;
    }
    let mut out = without;
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    out.push_str(&block());
    out
}

fn remove_block(text: &str) -> String {
    match (text.find(MARK_START), text.find(MARK_END)) {
        (Some(start), Some(end)) => {
            let end = end + MARK_END.len();
            let mut out = String::new();
            out.push_str(&text[..start]);
            let rest = text[end..].trim_start_matches('\n');
            out.push_str(rest);
            out
        }
        _ => text.to_string(),
    }
}

/// Write the blur state to niri's config. Fails (returns Err) if the config
/// file is missing - there is no niri config to edit. A missing config is
/// the one case where the toggle genuinely can't do its job.
pub fn set_enabled(enabled: bool) -> Result<(), PlayerError> {
    let path = config_path();
    if !path.exists() {
        return Err(PlayerError::Other(format!(
            "niri config not found at {} - cannot apply blur",
            path.display()
        )));
    }
    let text = std::fs::read_to_string(&path)
        .map_err(|e| PlayerError::Other(format!("failed to read niri config: {e}")))?;
    let new_text = toggle_blur(&text, enabled);
    std::fs::write(&path, new_text)
        .map_err(|e| PlayerError::Other(format!("failed to write niri config: {e}")))?;
    info!(
        "niri blur {} ({}).",
        if enabled { "enabled" } else { "disabled" },
        path.display()
    );
    Ok(())
}

/// Best-effort apply at startup; never fails startup (blur is cosmetic).
#[allow(dead_code)] // niri blur is removed from the UI for now
pub fn apply_at_startup(enabled: bool) {
    if !enabled {
        return;
    }
    if let Err(e) = set_enabled(true) {
        warn!("could not apply niri blur at startup: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enable_adds_block() {
        let text = "layout {\n    focus-ring { off }\n}\n";
        let out = toggle_blur(text, true);
        assert!(out.contains(MARK_START));
        assert!(out.contains("blur true"));
        assert!(out.contains(MARK_END));
        assert!(out.contains("focus-ring"));
    }

    #[test]
    fn disable_removes_block() {
        let text = "layout {\n    focus-ring { off }\n}\n";
        let enabled = toggle_blur(text, true);
        let out = toggle_blur(&enabled, false);
        assert!(!out.contains("blur"));
        assert!(!out.contains(MARK_START));
        assert!(out.contains("focus-ring"));
    }

    #[test]
    fn toggle_is_idempotent() {
        let text = "layout {\n    focus-ring { off }\n}\n";
        let once = toggle_blur(text, true);
        let twice = toggle_blur(&once, true);
        assert_eq!(once.matches(MARK_START).count(), 1);
        assert_eq!(twice.matches(MARK_START).count(), 1);
    }
}
