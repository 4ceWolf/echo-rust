//! Application entry point. Wires together (in dependency order):
//! 1. Logging (must be first - see docs/TODO.md, everything after this
//!    should be able to log if it fails).
//! 2. The depth model (downloaded on first run if missing).
//! 3. The GStreamer player.
//! 4. The wry/tao window + webview, with the confirmed Wayland-correct
//!    build path (see src/ui_window.rs's doc comment - do not touch that
//!    file without reading it first).
//! 5. The thin Rust<->JS bridge: JS sends JSON commands via
//!    `window.ipc.postMessage(...)`, Rust replies by pushing JSON state
//!    back via `webview.evaluate_script(...)`, triggered through a custom
//!    `AppEvent` on the event loop (required because the ipc_handler
//!    closure is built before the webview exists and can't capture it -
//!    confirmed pattern, see docs/ARCHITECTURE_DECISIONS.md).
//!
//! Deliberately boring on purpose, per the architecture decision to keep
//! the Rust<->JS bridge simple: commands and state are both just JSON,
//! no custom protocol, no cleverness.

mod art;
mod asset_server;
mod blur;
mod depth;
mod depth_model;
mod error;
mod history;
mod library;
mod logging;
mod pipeline;
mod player;
mod queue;
mod settings;
mod ui_window;

use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use log::{error, info};
use serde::{Deserialize, Serialize};
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

use error::PlayerError;
use player::Player;
use queue::QueueState;

/// Events the Rust side sends to itself via the event loop, so the
/// ipc_handler (which can't capture the not-yet-built webview) can still
/// trigger a script evaluation on it once the loop is running.
pub enum AppEvent {
    PushStateToUi,
    /// A depth-mask computation finished (or failed) for `track_path`.
    /// Carries the result so the worker thread can report back without
    /// touching the webview (which is main-thread-only).
    DepthMaskReady { track_path: String, mask_path: Option<String> },
    /// A library scan finished; the full track list is handed to the main
    /// thread to store, persist, and push to the UI.
    LibraryScanned { tracks: Vec<library::Track> },
    /// Push the full library index to the UI (separate from the per-tick
    /// `UiState` push, because the library is too large to send every 200ms).
    PushLibraryToUi,
}

/// One JSON command from the JS side. `cmd` names the action;
/// `arg` is a loosely-typed payload since different commands need
/// different shapes - deliberately not over-engineered into a strict
/// enum yet, per the "keep the bridge boring" decision. Revisit if this
/// becomes unwieldy.
#[derive(Debug, Deserialize)]
struct IpcCommand {
    cmd: String,
    #[serde(default)]
    arg: serde_json::Value,
}

/// The full state snapshot pushed to the UI after every command. Kept as
/// one flat struct rather than granular diffs - simplicity over
/// bandwidth efficiency, appropriate for a local single-user app talking
/// to its own webview on localhost, not a network protocol.
#[derive(Debug, Serialize, Default)]
struct UiState {
    playing: bool,
    position_ms: u64,
    duration_ms: u64,
    speed: f64,
    mono_downmix: bool,
    concert_hall_enabled: bool,
    dsp_speed_available: bool,
    dsp_concert_hall_available: bool,
    last_error: Option<String>,
    /// Path of the currently loaded track, if any.
    current_track: Option<String>,
    /// On-disk path of the cached depth mask for the current track, once
    /// computed (None while computing, or if the track has no art).
    depth_mask_path: Option<String>,
    /// Queue state surfaced to the UI for shuffle/repeat controls.
    queue_len: usize,
    queue_index: usize,
    shuffle: bool,
    smart_shuffle: bool,
    /// Smart-shuffle bias strength (0 = uniform random, 1 = favor unplayed).
    smart_shuffle_bias: f64,
    repeat: queue::RepeatMode,
    /// The next few queued tracks with their durations, for the rolling
    /// playback timeline. Capped at `UPCOMING_COUNT`.
    upcoming: Vec<UpcomingTrack>,
    /// Number of indexed tracks in the library (the full list is pushed via
    /// `__echoLibraryUpdate`, not here - too large for a per-tick field).
    library_count: usize,
    /// Session crossfade duration (ms). 0 = gapless.
    crossfade_ms: u64,
    /// A crossfade is currently in progress.
    crossfade_active: bool,
    /// The track being crossfaded into (None when idle).
    crossfade_to: Option<String>,
    /// 0.0..1.0 progress of the in-flight fade.
    crossfade_progress: f32,
}

/// One upcoming track entry for the timeline: its path + probed duration.
#[derive(Debug, Serialize, Clone)]
struct UpcomingTrack {
    path: String,
    duration_ms: u64,
}

/// How many upcoming tracks to expose to the UI (enough for the timeline's
/// "current + next + following" without probing a whole library).
const UPCOMING_COUNT: usize = 5;

struct AppState {
    player: Player,
    queue: QueueState,
    /// Send half of the depth-worker channel; `begin_track` pushes a path
    /// here to have its depth mask computed off-thread.
    depth_tx: mpsc::Sender<PathBuf>,
    current_track: Option<String>,
    depth_mask_path: Option<String>,
    /// Last-played timestamps, feeding smart shuffle (see src/history.rs).
    play_history: history::PlayHistory,
    /// User settings (persisted; see src/settings.rs).
    settings: settings::Settings,
    /// The track we'll crossfade into next (`peek_next` result). `None` when
    /// shuffle is on or the queue is exhausted.
    preloaded: Option<PathBuf>,
    /// The track currently being crossfaded into (set at `begin_crossfade`,
    /// cleared when the fade completes). Tracks the A->B transition for the UI.
    crossfade_to: Option<PathBuf>,
    /// Precomputed upcoming tracks (path + duration), refreshed on track
    /// change so the UI's timeline doesn't trigger a metadata probe every
    /// state tick.
    upcoming: Vec<UpcomingTrack>,
    /// The indexed library (persisted to JSON). Pushed to the UI on demand
    /// via `PushLibraryToUi`, not included in the per-tick `UiState`.
    library: Vec<library::Track>,
    /// The configured music folder to scan; None until onboarding/settings
    /// set it.
    library_path: Option<PathBuf>,
    /// Send half of the library-scan worker channel.
    library_tx: mpsc::Sender<PathBuf>,
}

fn main() {
    if let Err(e) = logging::init() {
        // Logging itself failed to initialize - fall back to eprintln
        // since there's nowhere else to put this. Should be rare (only
        // realistic cause is a filesystem permissions problem).
        eprintln!("FATAL: failed to initialize logging: {e}");
        std::process::exit(1);
    }

    info!("Echo starting");

    // --self-test: exercises everything that doesn't need a display
    // (GStreamer init/pipeline build, depth model download+checksum,
    // depth engine construction) and exits - added specifically so this
    // can be verified over SSH / in a headless session without needing
    // wry/tao's window+webview, which does need a real display. See
    // test.sh and docs/TODO.md.
    if std::env::args().any(|a| a == "--self-test") {
        std::process::exit(self_test());
    }

    // --self-test-art <path>: exercises album-art extraction against one
    // real file and exits - same headless-verification idea as --self-test,
    // but for the lofty-based art reading that needs an actual tagged
    // audio file to prove anything. Scriptable (exit 0 = ok).
    if let Some(pos) = std::env::args().position(|a| a == "--self-test-art") {
        let path = std::env::args().nth(pos + 1);
        std::process::exit(self_test_art(path));
    }

    // --self-test-mask <path>: runs the full art->depth-mask pipeline on
    // one real file (needs the model downloaded + a tagged audio file).
    // Scriptable; exit 0 = mask produced or file simply has no art.
    if let Some(pos) = std::env::args().position(|a| a == "--self-test-mask") {
        let path = std::env::args().nth(pos + 1);
        std::process::exit(self_test_mask(path));
    }

    // --self-test-library <folder>: scans a folder and prints the indexed
    // tracks. Scriptable; verifies the library layer on real files.
    if let Some(pos) = std::env::args().position(|a| a == "--self-test-library") {
        let path = std::env::args().nth(pos + 1);
        std::process::exit(self_test_library(path));
    }

    if let Err(e) = run() {
        error!("fatal startup error: {e}");
        eprintln!("Echo failed to start: {e}");
        eprintln!("See ~/.local/share/echo/logs/echo.log for details.");
        std::process::exit(1);
    }
}

fn self_test_art(path: Option<String>) -> i32 {
    let Some(p) = path else {
        eprintln!("usage: echo --self-test-art <audio-file>");
        return 2;
    };
    let filepath = std::path::Path::new(&p);
    match art::extract_front_cover(filepath) {
        Ok(Some(a)) => {
            println!(
                "OK: extracted {} bytes of art (mime {})",
                a.data.len(),
                a.mime
            );
            0
        }
        Ok(None) => {
            println!("no artwork for {p} (no embedded tag art and no sidecar cover)");
            0
        }
        Err(e) => {
            println!("FAILED: {e}");
            1
        }
    }
}

fn self_test_mask(path: Option<String>) -> i32 {
    let Some(p) = path else {
        eprintln!("usage: echo --self-test-mask <audio-file>");
        return 2;
    };
    let track_path = PathBuf::from(&p);

    let model_path = match depth_model::ensure_model_downloaded() {
        Ok(mp) => mp,
        Err(e) => {
            println!("FAILED: depth model download: {e}");
            return 1;
        }
    };
    let mut engine = match depth::DepthEngine::new(&model_path) {
        Ok(e) => e,
        Err(e) => {
            println!("FAILED: depth engine construction: {e}");
            return 1;
        }
    };

    match pipeline::compute_depth_mask(&mut engine, &track_path) {
        Ok(Some(mask_path)) => {
            println!("OK: depth mask -> {}", mask_path.display());
            0
        }
        Ok(None) => {
            println!("no artwork for {p} - cannot compute a mask without art");
            0
        }
        Err(e) => {
            println!("FAILED: {e}");
            1
        }
    }
}

fn self_test_library(path: Option<String>) -> i32 {
    let Some(p) = path else {
        eprintln!("usage: echo --self-test-library <folder>");
        return 2;
    };
    let dir = PathBuf::from(&p);
    if !dir.is_dir() {
        println!("FAILED: {} is not a directory", dir.display());
        return 1;
    }
    let tracks = library::scan_folder(&dir);
    println!("OK: scanned {} -> {} tracks", dir.display(), tracks.len());
    for t in tracks.iter().take(10) {
        println!(
            "     {} - {} ({} ms, {} Hz)",
            if t.artist.is_empty() { "?" } else { &t.artist },
            t.title,
            t.duration_ms,
            t.sample_rate.map(|s| s.to_string()).unwrap_or_else(|| "?".into())
        );
    }
    0
}

/// Returns a process exit code (0 = all checks passed) rather than
/// Result, since this is meant to be scriptable - test.sh checks $? for
/// each step and wants a plain numeric contract, not error formatting.
fn self_test() -> i32 {
    let mut failed = false;

    print!("[1/3] GStreamer pipeline construction... ");
    match Player::new() {
        Ok(player) => {
            let dsp = player.dsp_available();
            println!(
                "OK (speed: {}, reverb: {}, crossfeed: {})",
                dsp.speed, dsp.reverb, dsp.crossfeed
            );
            if !dsp.speed {
                println!("      WARNING: speed/pitch control unavailable - missing gst-plugins-bad's soundtouch element?");
            }
            if !dsp.concert_hall() {
                println!("      WARNING: Concert Hall partially/fully unavailable - missing gst-plugins-bad's freeverb and/or bs2b elements?");
            }
        }
        Err(e) => {
            println!("FAILED: {e}");
            failed = true;
        }
    }

    print!("[2/3] Depth model download + checksum verification... ");
    let model_result = depth_model::ensure_model_downloaded();
    match &model_result {
        Ok(path) => println!("OK ({})", path.display()),
        Err(e) => {
            println!("FAILED: {e}");
            failed = true;
        }
    }

    print!("[3/3] Depth inference engine construction... ");
    match &model_result {
        Ok(model_path) => match depth::DepthEngine::new(model_path) {
            Ok(_) => println!("OK"),
            Err(e) => {
                println!("FAILED: {e}");
                println!(
                    "      This is the least-verified part of the stack (see \
                     docs/ARCHITECTURE_DECISIONS.md's ort notes) - if this fails \
                     specifically, check the ort/session API notes in src/depth.rs first."
                );
                failed = true;
            }
        },
        Err(_) => println!("SKIPPED (model download already reported failed above)"),
    }

    if failed {
        println!("\nSelf-test FAILED - see errors above. Also check ~/.local/share/echo/logs/echo.log.");
        1
    } else {
        println!("\nSelf-test PASSED. Note: this does not test the window/webview (wry/tao) - that needs a real display. See docs/TODO.md.");
        0
    }
}

fn run() -> Result<(), PlayerError> {
    // Depth model: download on first run if missing, but don't treat a
    // failure here as fatal to the whole app - a music player that can't
    // play music because an image-effect model failed to download would
    // be a bad trade. Log clearly and continue without the depth effect.
    let depth_model_path = match depth_model::ensure_model_downloaded() {
        Ok(path) => Some(path),
        Err(e) => {
            error!("depth model unavailable, depth-effect album art will be disabled: {e}");
            None
        }
    };

    let mut player = Player::new()?;
    info!("dsp availability: {:?}", {
        let d = player.dsp_available();
        (d.speed, d.reverb, d.crossfeed)
    });

    // Apply persisted settings to the fresh player/queue before the first
    // track ever plays (so a restart resumes the same DSP + queue state).
    let settings = settings::load();
    player.set_speed(settings.speed);
    player.set_mono_downmix(settings.mono_downmix);
    player.set_concert_hall(settings.concert_hall);
    let mut queue = QueueState::new();
    queue.shuffle = settings.shuffle;
    queue.smart_shuffle = settings.smart_shuffle;
    queue.repeat = settings.repeat;
    queue.smart_shuffle_bias = settings.smart_shuffle_bias;

    // Restore the compositor blur state (best-effort - cosmetic, so a
    // failure here must not block startup). Only re-applies when blur was
    // left ON, since OFF needs no config edit.
    // niri blur is removed from the UI for now. Strip any previously written
    // window-rule so the compositor state matches the app; best-effort (a
    // missing niri config just means there is nothing to clear).
    let _ = blur::set_enabled(false);

    // Depth-mask worker channel: the UI-facing code pushes a track path
    // here to have its mask computed off-thread (ort inference is heavy
    // enough to stutter the UI if it ran inline on a track change).
    let (depth_tx, depth_rx) = mpsc::channel::<PathBuf>();

    // Apply the persisted crossfade duration to the fresh engine.
    player.set_crossfade_ms(settings.crossfade_ms);

    // Library-scan channel: the UI sends a folder path here to have it
    // scanned on a background thread (scanning + tag-reading a large music
    // folder is slow and must not block the UI).
    let (library_tx, library_rx) = mpsc::channel::<PathBuf>();

    // Restore the persisted library index + configured folder at startup,
    // so the UI has the library immediately without a rescan.
    let library = library::load();
    let library_path = settings.library_path.as_ref().map(PathBuf::from);

    let state = Arc::new(Mutex::new(AppState {
        player,
        queue,
        depth_tx,
        current_track: None,
        depth_mask_path: None,
        play_history: history::load(),
        settings,
        preloaded: None,
        crossfade_to: None,
        upcoming: Vec::new(),
        library,
        library_path,
        library_tx,
    }));

    let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    // Library-scan worker: scans folders on a background thread and reports
    // the result back through the event loop (same pattern as the depth
    // worker). Detached; lives for the process lifetime.
    {
        let worker_proxy = proxy.clone();
        std::thread::Builder::new()
            .name("echo-library-worker".into())
            .spawn(move || library_worker(library_rx, worker_proxy))
            .map_err(|e| PlayerError::Other(format!("failed to spawn library worker: {e}")))?;
    }

    // Depth worker: owns the DepthEngine (Session::run needs &mut self, so
    // the engine is not shared), computes masks on request, and reports
    // results back through the event loop. Spawned after the proxy exists
    // but before the loop runs. Detached on purpose - it lives for the
    // process lifetime and is torn down on exit.
    if let Some(model_path) = depth_model_path {
        let worker_proxy = proxy.clone();
        std::thread::Builder::new()
            .name("echo-depth-worker".into())
            .spawn(move || depth_worker(model_path, depth_rx, worker_proxy))
            .map_err(|e| PlayerError::Other(format!("failed to spawn depth worker: {e}")))?;
    } else {
        // No model, no worker: drop the receiver so any send through
        // depth_tx fails cleanly (the send site treats that as a no-op).
        drop(depth_rx);
    }

    let ipc_state = Arc::clone(&state);
    let ipc_proxy = proxy.clone();
    // NOTE on wry::http::Request: the confirmed examples researched for
    // this import `Request` from the standalone `http` crate directly
    // (`use http::Request;`), not necessarily as `wry::http::Request`.
    // wry commonly re-exports its `http` dependency, but that specific
    // re-export path wasn't independently confirmed. If this doesn't
    // compile, add `http = "1"` to Cargo.toml and use `http::Request`
    // instead of `wry::http::Request` here.
    let ipc_handler = move |request: wry::http::Request<String>| {
        let body = request.body();
        match serde_json::from_str::<IpcCommand>(body) {
            Ok(command) => {
                // This closure runs inside a GTK/WebKit `extern "C"` callback,
                // which cannot unwind: any panic here would abort the whole
                // process (that is exactly how a bad queue slice took the app
                // down). Catch panics per-command so one bad command can never
                // kill the player. A panic mid-command may poison the state
                // mutex; every lock site already handles poisoning gracefully.
                let push_library = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    handle_command(&ipc_state, &command)
                }))
                .unwrap_or_else(|_| {
                    error!("IPC command '{}' panicked; ignoring it", command.cmd);
                    false
                });
                if push_library {
                    let _ = ipc_proxy.send_event(AppEvent::PushLibraryToUi);
                }
            }
            Err(e) => {
                error!("failed to parse IPC command '{body}': {e}");
            }
        }
        // Always push fresh state after handling a command, whether it
        // succeeded or not (a failed command still needs the UI to know
        // the real current state, e.g. to reset an optimistic UI update).
        let _ = ipc_proxy.send_event(AppEvent::PushStateToUi);
    };

    // Bundled into the binary at compile time - see module doc comment
    // for why (no "did the assets ship" runtime failure mode). ui/ is
    // where the actual frontend source lives.
    let html = include_str!("../ui/dist/index.html");

    let app_window = ui_window::build(&event_loop, html, ipc_handler)?;

    // Poll GStreamer's bus + push position updates on a timer, matching
    // the polling cadence used successfully in the earlier PySide6
    // version of this project for the equivalent GLib-pump problem -
    // here it's tao's event loop driving it instead of a Qt timer, same
    // underlying idea.
    let poll_state = Arc::clone(&state);
    let poll_proxy = proxy.clone();

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(
            std::time::Instant::now() + std::time::Duration::from_millis(200),
        );

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                { let s = lock_state(&poll_state);
                    if let Err(e) = s.player.stop() {
                        error!("error stopping player on close: {e}");
                    }
                    history::save(&s.play_history);
                }
                *control_flow = ControlFlow::Exit;
            }
            Event::NewEvents(tao::event::StartCause::ResumeTimeReached { .. }) => {
                // Drive the crossfade engine + bus polling on the main thread
                // tick (see player.rs's poll_bus, which advances the fade and
                // drains EOS/error/state messages).
                { let mut guard = lock_state(&poll_state);
                    let s = &mut *guard;

                    match s.player.poll_bus() {
                        player::PlayerEvent::FadeComplete => {
                            // The crossfade finished: crossfade_to is now current.
                            if let Some(to) = s.crossfade_to.take() {
                                s.current_track = Some(to.to_string_lossy().to_string());
                                s.depth_mask_path = None;
                                history::record(&mut s.play_history, &to);
                                history::save(&s.play_history);
                                let _ = s.depth_tx.send(to);
                            }
                            // Advance the queue past the now-current track.
                            let _ = s.queue.decide_next(&s.play_history);
                            s.preloaded = s.queue.peek_next();
                            refresh_upcoming(s);
                        }
                        player::PlayerEvent::TrackEnded => {
                            let next_path = s.queue.decide_next(&s.play_history);
                            match next_path {
                                Some(path) => {
                                    begin_track(s, path);
                                }
                                None => {
                                    info!("queue exhausted (or empty) - stopping");
                                    if let Err(e) = s.player.stop() {
                                        error!("error stopping at end of queue: {e}");
                                    }
                                }
                            }
                        }
                        player::PlayerEvent::None => {
                            // Begin a crossfade when the active track nears its
                            // end and we have a next track preloaded.
                            if s.crossfade_to.is_none() && s.player.near_end() {
                                if let Some(next) = s.preloaded.clone() {
                                    match s.player.begin_crossfade(&next) {
                                        Ok(()) => {
                                            s.crossfade_to = Some(next);
                                            s.preloaded = None;
                                        }
                                        Err(e) => {
                                            error!("begin_crossfade failed for {}: {e}", next.display());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                let _ = poll_proxy.send_event(AppEvent::PushStateToUi);
            }
            Event::UserEvent(AppEvent::PushStateToUi) => {
                { let s = lock_state(&poll_state);
                    let dsp = s.player.dsp_available();
                    let ui_state = UiState {
                        playing: s.player.playing,
                        position_ms: s.player.position_ms().unwrap_or(0),
                        duration_ms: s.player.duration_ms().unwrap_or(0),
                        speed: s.player.speed,
                        mono_downmix: s.player.mono_downmix,
                        concert_hall_enabled: s.player.concert_hall_enabled,
                        dsp_speed_available: dsp.speed,
                        dsp_concert_hall_available: dsp.concert_hall(),
                        last_error: None,
                        current_track: s.current_track.clone(),
                        depth_mask_path: s.depth_mask_path.clone(),
                        queue_len: s.queue.queue.len(),
                        queue_index: s.queue.queue_index,
                        shuffle: s.queue.shuffle,
                        smart_shuffle: s.queue.smart_shuffle,
                        smart_shuffle_bias: s.queue.smart_shuffle_bias,
                        repeat: s.queue.repeat,
                        upcoming: s.upcoming.clone(),
                        library_count: s.library.len(),
                        crossfade_ms: s.player.crossfade_ms,
                        crossfade_active: s.crossfade_to.is_some(),
                        crossfade_to: s.crossfade_to.as_ref().map(|p| p.to_string_lossy().to_string()),
                        crossfade_progress: s.player.fade_progress,
                    };
                    if let Ok(json) = serde_json::to_string(&ui_state) {
                        let script = format!("window.__echoStateUpdate && window.__echoStateUpdate({json})");
                        if let Err(e) = app_window.webview.evaluate_script(&script) {
                            error!("failed to push state to UI: {e}");
                        }
                    }
                }
            }
            Event::UserEvent(AppEvent::DepthMaskReady { track_path, mask_path }) => {
                // Ignore stale results: a mask arriving for a track we've
                // already moved past must not overwrite the current one.
                { let mut s = lock_state(&poll_state);
                    if s.current_track.as_deref() == Some(track_path.as_str()) {
                        s.depth_mask_path = mask_path;
                    }
                }
                let _ = poll_proxy.send_event(AppEvent::PushStateToUi);
            }
            Event::UserEvent(AppEvent::LibraryScanned { tracks }) => {
                // Store + persist the freshly scanned library, then push it
                // to the UI (not part of the per-tick UiState).
                { let mut s = lock_state(&poll_state);
                    s.library = tracks;
                    library::save(&s.library);
                }
                let _ = poll_proxy.send_event(AppEvent::PushLibraryToUi);
                let _ = poll_proxy.send_event(AppEvent::PushStateToUi);
            }
            Event::UserEvent(AppEvent::PushLibraryToUi) => {
                // Push the full library index (too large for the per-tick
                // UiState). The UI defines `window.__echoLibraryUpdate`.
                { let s = lock_state(&poll_state);
                    if let Ok(json) = serde_json::to_string(&s.library) {
                        let script = format!("window.__echoLibraryUpdate && window.__echoLibraryUpdate({json})");
                        if let Err(e) = app_window.webview.evaluate_script(&script) {
                            error!("failed to push library to UI: {e}");
                        }
                    }
                }
            }
            _ => (),
        }
    });
}

/// Lock the shared state, recovering the inner value if a previous panic
/// poisoned the mutex. Without this, one caught IPC panic leaves the mutex
/// poisoned, every later lock fails, and the UI silently freezes (commands
/// and state pushes both stop) even though the process is still alive.
fn lock_state(state: &Arc<Mutex<AppState>>) -> std::sync::MutexGuard<'_, AppState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            error!("state mutex was poisoned by a previous panic; recovering");
            poisoned.into_inner()
        }
    }
}

fn handle_command(state: &Arc<Mutex<AppState>>, command: &IpcCommand) -> bool {
    let mut guard = lock_state(state);
    // Deref the guard once so field borrows below are disjoint (e.g.
    // `s.queue.decide_next(&s.play_history)` borrows queue mutably and
    // play_history immutably at once - allowed on &mut AppState, not
    // through the guard's single DerefMut).
    let s: &mut AppState = &mut *guard;

    match command.cmd.as_str() {
        "play" => {
            if let Err(e) = s.player.play() {
                error!("play command failed: {e}");
            }
        }
        "pause" => {
            if let Err(e) = s.player.pause() {
                error!("pause command failed: {e}");
            }
        }
        "seek" => {
            if let Some(ms) = command.arg.as_u64() {
                if let Err(e) = s.player.seek(ms) {
                    error!("seek command failed: {e}");
                }
            } else {
                error!("seek command missing/invalid 'arg' (expected ms as u64)");
            }
        }
        "set_speed" => {
            if let Some(v) = command.arg.as_f64() {
                s.player.set_speed(v);
                s.settings.speed = v;
                settings::save(&s.settings);
            } else {
                error!("set_speed command missing/invalid 'arg'");
            }
        }
        "set_mono_downmix" => {
            if let Some(v) = command.arg.as_bool() {
                s.player.set_mono_downmix(v);
                s.settings.mono_downmix = v;
                settings::save(&s.settings);
            } else {
                error!("set_mono_downmix command missing/invalid 'arg'");
            }
        }
        "set_concert_hall" => {
            if let Some(v) = command.arg.as_bool() {
                s.player.set_concert_hall(v);
                s.settings.concert_hall = v;
                settings::save(&s.settings);
            } else {
                error!("set_concert_hall command missing/invalid 'arg'");
            }
        }
        "load_and_play" => {
            if let Some(path_str) = command.arg.as_str() {
                begin_track(s, PathBuf::from(path_str));
            } else {
                error!("load_and_play command missing/invalid 'arg' (expected a file path string)");
            }
        }
        "set_queue" => {
            // arg: {"paths": ["/a.mp3", ...], "start_index": 0}
            let mut start_index = 0usize;
            let mut paths: Vec<PathBuf> = Vec::new();
            if let Some(obj) = command.arg.as_object() {
                if let Some(arr) = obj.get("paths").and_then(|v| v.as_array()) {
                    paths = arr
                        .iter()
                        .filter_map(|v| v.as_str().map(PathBuf::from))
                        .collect();
                }
                if let Some(idx) = obj.get("start_index").and_then(|v| v.as_u64()) {
                    start_index = idx as usize;
                }
            }
            if paths.is_empty() {
                error!("set_queue missing/invalid 'arg' (expected object with a 'paths' array)");
            } else {
                s.queue.set_queue(paths, start_index);
                if let Some(first) = s.queue.queue.get(s.queue.queue_index).cloned() {
                    begin_track(s, first);
                }
            }
        }
        "next" => {
            let next_path = s.queue.decide_next(&s.play_history);
            match next_path {
                Some(path) => begin_track(s, path),
                None => info!("next: queue exhausted"),
            }
        }
        "prev" => {
            // If more than 3s in, restart the current track instead of
            // going back - the same rule as the Python version (see
            // queue.rs's decide_prev doc comment).
            if s.player.position_ms().unwrap_or(0) > 3000 {
                if let Some(cur) = s.queue.queue.get(s.queue.queue_index).cloned() {
                    begin_track(s, cur);
                }
            } else if let Some(path) = s.queue.decide_prev() {
                begin_track(s, path);
            }
        }
        "set_shuffle" => {
            if let Some(v) = command.arg.as_bool() {
                s.queue.shuffle = v;
                s.settings.shuffle = v;
                settings::save(&s.settings);
            } else {
                error!("set_shuffle command missing/invalid 'arg'");
            }
        }
        "set_smart_shuffle" => {
            if let Some(v) = command.arg.as_bool() {
                s.queue.smart_shuffle = v;
                s.settings.smart_shuffle = v;
                settings::save(&s.settings);
            } else {
                error!("set_smart_shuffle command missing/invalid 'arg'");
            }
        }
        "set_smart_shuffle_bias" => {
            if let Some(v) = command.arg.as_f64() {
                s.queue.smart_shuffle_bias = v.clamp(0.0, 1.0);
                s.settings.smart_shuffle_bias = s.queue.smart_shuffle_bias;
                settings::save(&s.settings);
            } else {
                error!("set_smart_shuffle_bias command missing/invalid 'arg'");
            }
        }
        "cycle_repeat" => {
            s.queue.repeat = s.queue.repeat.cycle();
            s.settings.repeat = s.queue.repeat;
            settings::save(&s.settings);
        }
        "set_crossfade" => {
            // arg: number = crossfade duration in ms (0 = gapless). Sets the
            // session value and persists it as the default.
            if let Some(ms) = command.arg.as_u64() {
                s.player.set_crossfade_ms(ms);
                s.settings.crossfade_ms = ms;
                settings::save(&s.settings);
            } else {
                error!("set_crossfade command missing/invalid 'arg' (expected ms)");
            }
        }
        "set_niri_blur" => {
            if let Some(v) = command.arg.as_bool() {
                match blur::set_enabled(v) {
                    Ok(()) => {
                        s.settings.niri_blur = v;
                        settings::save(&s.settings);
                    }
                    Err(e) => error!("set_niri_blur failed: {e}"),
                }
            } else {
                error!("set_niri_blur command missing/invalid 'arg'");
            }
        }
        "set_library_path" => {
            // arg: filesystem path to the music folder. Persist it, then kick
            // off a scan (the result arrives via PushLibraryToUi).
            if let Some(path_str) = command.arg.as_str() {
                let path = PathBuf::from(path_str);
                s.library_path = Some(path.clone());
                s.settings.library_path = Some(path_str.to_string());
                settings::save(&s.settings);
                let _ = s.library_tx.send(path);
            } else {
                error!("set_library_path command missing/invalid 'arg' (expected a path string)");
            }
        }
        "scan_library" => {
            // Rescan the currently-configured folder.
            if let Some(path) = s.library_path.clone() {
                let _ = s.library_tx.send(path);
            } else {
                error!("scan_library called but no library path is configured (set_library_path first)");
            }
        }
        "get_library" => {
            // The library is already in AppState; just tell the caller to
            // push it (see the ipc_handler, which sends PushLibraryToUi).
            return true;
        }
        other => {
            error!("unknown IPC command: '{other}'");
        }
    }
    false
}

/// Load a track into the player, record it as the current track, clear the
/// stale depth-mask state, and kick off a depth-mask computation on the
/// worker thread. The single choke point for "a new track is now playing"
/// - called from both the IPC `load_and_play` command and the EOS auto-
/// advance path, so both behave identically.
fn begin_track(s: &mut AppState, path: PathBuf) {
    // A manual load cancels any in-flight crossfade and resets the transition
    // state, so a stale handoff can't fire after a skip.
    s.crossfade_to = None;
    s.preloaded = None;

    if let Err(e) = s.player.load_and_play(&path) {
        error!("load failed for {}: {e}", path.display());
        return;
    }
    s.current_track = Some(path.to_string_lossy().to_string());
    s.depth_mask_path = None;
    // Record + persist the play, so smart shuffle has a real feed (was the
    // audit's "empty HashMap placeholder" finding).
    history::record(&mut s.play_history, &path);
    history::save(&s.play_history);
    // Best-effort: if the worker thread is gone (no model), the send fails
    // and we simply never get a mask - the effect is just absent, which is
    // the correct degraded behaviour, not an error.
    let _ = s.depth_tx.send(path);

    // Preload the track we'll crossfade into next (peek_next returns None
    // when shuffle is on or at end-of-queue, which disables the pre-emptive
    // fade for that transition).
    s.preloaded = s.queue.peek_next();

    refresh_upcoming(s);
}

/// Recompute the upcoming-tracks list (path + probed duration) for the UI's
/// rolling timeline. Called on track changes only, not on every state tick,
/// so metadata probing stays cheap.
fn refresh_upcoming(s: &mut AppState) {
    let paths: Vec<PathBuf> = s.queue.queue[upcoming_range(s.queue.queue.len(), s.queue.queue_index)]
        .to_vec();
    s.upcoming = paths
        .iter()
        .map(|p| UpcomingTrack {
            path: p.to_string_lossy().to_string(),
            duration_ms: art::track_duration_ms(p).unwrap_or(0),
        })
        .collect();
}

/// The half-open range of queue entries to expose as "upcoming": the
/// `UPCOMING_COUNT` tracks after `index`, clamped to `len`. Returns an empty
/// range for an empty queue. This is a separate function (and unit-tested)
/// because the previous inline `queue[index + 1..]` slice panicked with
/// "range start index 1 out of range for slice of length 0" whenever a track
/// was played via `load_and_play` with no queue set (e.g. clicking a track in
/// Browse) - and because that ran inside the GTK IPC callback, the panic
/// aborted the whole process instead of unwinding.
fn upcoming_range(len: usize, index: usize) -> std::ops::Range<usize> {
    if len == 0 {
        return 0..0;
    }
    let start = index.saturating_add(1).min(len);
    let end = start.saturating_add(UPCOMING_COUNT).min(len);
    start..end
}

/// Background loop: owns the DepthEngine, computes masks for whatever
/// track paths come in, and reports each result back through the event
/// loop proxy (never touches the webview itself - that's main-thread-only).
fn depth_worker(
    model_path: PathBuf,
    rx: mpsc::Receiver<PathBuf>,
    proxy: tao::event_loop::EventLoopProxy<AppEvent>,
) {
    let mut engine = match depth::DepthEngine::new(&model_path) {
        Ok(e) => e,
        Err(e) => {
            error!("depth worker: failed to construct engine: {e}");
            return;
        }
    };

    while let Ok(track_path) = rx.recv() {
        let result = pipeline::compute_depth_mask(&mut engine, &track_path);
        let mask_path = match result {
            Ok(Some(p)) => Some(p.to_string_lossy().to_string()),
            Ok(None) => None,
            Err(e) => {
                error!(
                    "depth worker: failed to compute mask for {}: {e}",
                    track_path.display()
                );
                None
            }
        };
        let _ = proxy.send_event(AppEvent::DepthMaskReady {
            track_path: track_path.to_string_lossy().to_string(),
            mask_path,
        });
    }
}

/// Background loop for library scanning: receives a folder path, scans it
/// (slow - must not block the UI thread), and reports the track list back
/// through the event loop.
fn library_worker(rx: mpsc::Receiver<PathBuf>, proxy: tao::event_loop::EventLoopProxy<AppEvent>) {
    while let Ok(path) = rx.recv() {
        info!("library scan started: {}", path.display());
        let tracks = library::scan_folder(&path);
        info!("library scan finished: {} tracks", tracks.len());
        let _ = proxy.send_event(AppEvent::LibraryScanned { tracks });
    }
}

#[cfg(test)]
mod tests {
    use super::{upcoming_range, IpcCommand};

    #[test]
    fn upcoming_range_is_empty_for_empty_queue() {
        // Regression: load_and_play with no queue used to slice queue[1..0].
        assert_eq!(upcoming_range(0, 0), 0..0);
    }

    #[test]
    fn upcoming_range_starts_after_current() {
        assert_eq!(upcoming_range(10, 0), 1..6);
        assert_eq!(upcoming_range(3, 0), 1..3);
    }

    #[test]
    fn upcoming_range_clamps_at_end() {
        assert_eq!(upcoming_range(3, 2), 3..3);
        assert_eq!(upcoming_range(3, 99), 3..3);
    }

    #[test]
    fn ipc_parse_play_with_no_arg() {
        let c: IpcCommand = serde_json::from_str(r#"{"cmd":"play"}"#).unwrap();
        assert_eq!(c.cmd, "play");
        assert!(c.arg.is_null());
    }

    #[test]
    fn ipc_parse_seek_with_number() {
        let c: IpcCommand = serde_json::from_str(r#"{"cmd":"seek","arg":5000}"#).unwrap();
        assert_eq!(c.cmd, "seek");
        assert_eq!(c.arg.as_u64(), Some(5000));
    }

    #[test]
    fn ipc_parse_load_with_path_string() {
        let c: IpcCommand =
            serde_json::from_str(r#"{"cmd":"load_and_play","arg":"/music/a.mp3"}"#).unwrap();
        assert_eq!(c.cmd, "load_and_play");
        assert_eq!(c.arg.as_str(), Some("/music/a.mp3"));
    }

    #[test]
    fn ipc_parse_set_queue_object() {
        let c: IpcCommand =
            serde_json::from_str(r#"{"cmd":"set_queue","arg":{"paths":["/a.mp3"],"start_index":0}}"#)
                .unwrap();
        assert_eq!(c.cmd, "set_queue");
        let obj = c.arg.as_object().unwrap();
        assert_eq!(obj["paths"][0].as_str(), Some("/a.mp3"));
        assert_eq!(obj["start_index"].as_u64(), Some(0));
    }

    #[test]
    fn ipc_parse_rejects_non_object() {
        assert!(serde_json::from_str::<IpcCommand>(r#"not json"#).is_err());
    }
}
