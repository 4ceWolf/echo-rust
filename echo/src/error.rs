//! Real error type for the playback engine. `gstreamer-rs`'s own errors
//! aren't a single uniform type - `ElementFactory::make(...).build()`
//! returns `glib::BoolError`, `pipeline.set_state(...)` returns
//! `gst::StateChangeError`, `gst::init()` returns `glib::Error`. Wrapping
//! all of those under one `glib::Error` (an earlier draft of player.rs
//! did this) doesn't actually type-check - BoolError and StateChangeError
//! aren't glib::Error and don't convert into one for free. This is the
//! real, compiling shape instead.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlayerError {
    #[error("GStreamer failed to initialize: {0}")]
    Init(#[from] glib::Error),

    #[error("failed to build element: {0}")]
    ElementBuild(#[from] glib::BoolError),

    #[error("pipeline state change failed")]
    StateChange(#[from] gst::StateChangeError),

    #[error("{0}")]
    Other(String),
}
