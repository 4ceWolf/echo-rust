//! Window + webview setup.
//!
//! CRITICAL: this file must never be "simplified" to use the plain
//! `WebViewBuilder::new().build(&window)` path shown in most wry
//! quick-start examples online. That path is explicitly **X11-only** on
//! Linux per wry's own docs. Since niri (this project's target) is
//! Wayland-only, that would mean the window silently fails to render
//! under niri specifically - discovered only after building UI on top
//! of it. See docs/ARCHITECTURE_DECISIONS.md's "Wayland pitfall" section
//! for the full story and sources.
//!
//! REVISED after the first real window test: the original `gtk::Fixed`
//! + manual `vbox.pack_start` approach compiled and ran, but produced a
//! tiny (~200x300px) unstyled window instead of filling it - `gtk::Fixed`
//! is a fixed-position/fixed-size container that does NOT participate in
//! GTK's normal size negotiation, so telling the *vbox* to expand the
//! `Fixed` never actually told the webview widget inside it to fill that
//! space. A confirmed-current wry docs example (docs.rs/wvwasi-wry, which
//! mirrors wry's real public API) shows a much simpler, correct pattern
//! that skips the Fixed container entirely: `WindowExtUnix::gtk_window()`
//! directly, `WebViewBuilderExtUnix::new_gtk(&window.gtk_window())`. No
//! manual box-packing, no size-negotiation bug to introduce in the first
//! place. This is that corrected version.

use log::info;
use tao::event_loop::EventLoop;
use tao::window::{Window, WindowBuilder};
use wry::WebView;

use crate::error::PlayerError;

#[cfg(target_os = "linux")]
use wry::WebViewBuilderExtUnix;
#[cfg(target_os = "linux")]
use tao::platform::unix::WindowExtUnix;

pub struct AppWindow {
    // `window` isn't read anywhere yet, but must be kept alive for as
    // long as `webview` is - dropping it would drop the underlying GTK
    // window out from under the webview. Not dead code in the usual
    // sense; #[allow] documents that its purpose is to be held, not read.
    #[allow(dead_code)]
    pub window: Window,
    pub webview: WebView,
}

/// `html` is the full page source to load - see ui/ for the actual
/// frontend, bundled into the binary via `include_str!` in main.rs
/// rather than loaded from a loose file at runtime, so the app is a
/// single self-contained binary with no "did the assets ship" failure
/// mode.
///
/// `ipc_handler` receives every `window.ipc.postMessage(...)` call from
/// the JS side, as a raw `Request<String>` (confirmed against wry's
/// current examples). Sending data BACK to JS is a separate mechanism -
/// wry's own docs confirm the ipc_handler closure can't capture the
/// webview itself (it doesn't exist yet at builder time), so replies use
/// an `EventLoopProxy` + `webview.evaluate_script()` from the event loop
/// instead. See main.rs for how those two halves connect.
pub fn build(
    event_loop: &EventLoop<crate::AppEvent>,
    html: &str,
    ipc_handler: impl Fn(wry::http::Request<String>) + 'static,
) -> Result<AppWindow, PlayerError> {
    let window = WindowBuilder::new()
        .with_title("Echo")
        .with_inner_size(tao::dpi::LogicalSize::new(1100.0, 720.0))
        .with_min_inner_size(tao::dpi::LogicalSize::new(760.0, 480.0))
        .build(event_loop)
        .map_err(|e| PlayerError::Other(format!("failed to create window: {e}")))?;

    #[cfg(target_os = "linux")]
    let webview = {
        // Pack the webview directly into tao's default vertical gtk::Box,
        // which participates in GTK's normal size negotiation (unlike
        // gtk::Fixed, which is fixed-size and caused the earlier tiny-
        // window bug). build_gtk() on a gtk::Box uses
        // Box::pack_start(webview, true, true, 0), so the webview fills
        // the window. The correct API is WebViewBuilderExtUnix::build_gtk
        // - WebViewBuilder::new_gtk does not exist, and WebViewExtUnix::
        // new_gtk lives on WebView (the built value), not the builder.
        let vbox = window
            .default_vbox()
            .ok_or_else(|| PlayerError::Other("window has no default vbox".into()))?;
        wry::WebViewBuilder::new()
            .with_html(html)
            .with_ipc_handler(ipc_handler)
            // Serve album art / depth masks to the page. The webview can't
            // read files from disk, so the frontend turns the paths in
            // `UiState` into `echo://art/` / `echo://mask/` URLs. See
            // src/asset_server.rs for the decision + routes.
            .with_asynchronous_custom_protocol(crate::asset_server::SCHEME.into(), crate::asset_server::serve_async)
            .build_gtk(vbox)
            .map_err(|e| PlayerError::Other(format!("failed to build webview (gtk path): {e}")))?
    };

    #[cfg(not(target_os = "linux"))]
    let webview = wry::WebViewBuilder::new()
        .with_html(html)
        .with_ipc_handler(ipc_handler)
        .with_asynchronous_custom_protocol(crate::asset_server::SCHEME.into(), crate::asset_server::serve_async)
        .build(&window)
        .map_err(|e| PlayerError::Other(format!("failed to build webview: {e}")))?;

    info!("window + webview created");

    Ok(AppWindow { window, webview })
}

