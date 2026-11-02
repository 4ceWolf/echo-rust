# Echo — Project Reference (read THIS, not every file)

Local music player in Rust. Real per-track depth-effect album art. Arch + niri
target. **Read docs/TODO.md for full status; this file is the compact map so a
fresh session can get oriented without re-reading ~2,500 lines.** For UI work,
read `docs/UI_MASTER_DESIGN.md` (the master UI/UX handoff — the authoritative
brief) plus `docs/DESIGN_DIRECTION.md` (confirmed aesthetic) and
`docs/FEATURE_IDEAS.md` (brainstorm log).

## Stack (pinned on purpose — do NOT casually bump)
- Playback: `gstreamer-rs` (`gst` crate, 0.25.3) — playbin + custom audio-sink-bin.
- UI: `wry` 0.56.1 + `tao` 0.34 (native webview, NOT Electron/Tauri).
- Depth: `ort` =2.0.0-rc.12 (EXACT pin), `ndarray` 0.17 (must match ort's internal),
  Depth Anything V2 **Small** (Apache-2.0 only variant), 1x3x518x518 in, 1x518x518
  relative-inverse-depth out.
- Misc: `image`, `serde`/`serde_json`, `log`+`env_logger`, `sha1` (cache keys),
  `sha2` (model checksum), `ureq` 3.3 (model download), `rand` 0.10 (NEW api:
  `rng()`/`random_range()`), `thiserror`.

## Source map
- `src/main.rs` — entry, event loop, thin JSON IPC (see "IPC contract" below).
  `--self-test`/`--self-test-art`/`--self-test-mask` modes for headless checks.
- `src/player.rs` — GStreamer engine: pitch/freeverb/bs2b/capsfilter chain.
  `poll_bus()` drains EOS/Error/StateChanged and updates `self.playing`.
  `load_and_play` uses `glib::filename_to_uri` (NOT a gst wrapper). No 8D audio
  (removed by user decision).
- `src/queue.rs` — `QueueState`: `decide_next`/`decide_prev`, shuffle, smart
  shuffle (`weighted_choice`), RepeatMode Off/All/One. Unit-tested.
- `src/history.rs` — `PlayHistory` (path -> last_played_secs), JSON-persisted
  to `~/.local/share/echo/play_history.json`; feeds smart shuffle.
- `src/library.rs` — library index: `scan_folder(dir) -> Vec<Track>` (recursive
  audio walk + `lofty` metadata: title/artist/album/duration/sample_rate/
  bit_depth/has_art), persisted to `~/.local/share/echo/library.json`. Scanned
  on a background worker thread. `Track` = the UI's "metadata + technical
  identity" (see UI_MASTER_DESIGN.md "split by reality").
- `src/settings.rs` — persisted user settings (speed/mono/concert_hall/
  shuffle/smart_shuffle/repeat/niri_blur/library_path) at
  `~/.local/share/echo/settings.json`.
- `src/blur.rs` — niri blur toggle: writes/removes a delimited `window-rule`
  block in `~/.config/niri/config.kdl` (niri 26.04 `background-effect`).
  `set_niri_blur` IPC command; restored at startup.
- `src/art.rs` — artwork resolution: embedded front-cover extraction via
  `lofty` (`extract_front_cover(path)`), falling back to a sidecar image in
  the same folder (`cover`/`folder`/`front`/`album`/`albumart`/`artwork` +
  image ext, then `<track-stem>.<ext>`). `None` = no art at all (normal).
  `--self-test-art <path>` verifies it headlessly.
- `src/pipeline.rs` — `compute_depth_mask(engine, track_path)`: art -> sha1
  -> cache -> decode -> `infer_mask` -> PNG. Only place art & depth meet.
- `src/depth.rs` — `DepthEngine` (ort Session, `infer_mask` needs `&mut
  self`), `cache_path_for(track_path, art_hash, dir)` keyed by
  `sha1(path|art-hash|MODEL_VERSION)`, `depth_cache_dir()`. Run on a
  background worker thread (see main.rs `depth_worker`), NOT the UI thread.
- `src/depth_model.rs` — downloads model to `~/.local/share/echo/models/`,
  SHA256 verify, atomic .part→rename. Hash + URL in ARCHITECTURE_DECISIONS.md.
- `src/logging.rs` — file log `~/.local/share/echo/logs/echo.log`, rotation,
  `recent_errors()` buffer for future Settings panel.
- `src/error.rs` — `PlayerError` (thiserror wraps glib::Error, BoolError,
  StateChangeError, Other).
- `src/ui_window.rs` — wry/tao window. MUST use `default_vbox()` +
  `WebViewBuilderExtUnix::build_gtk(vbox)` (Wayland-correct). Plain `.build()`
  is X11-only. `build_gtk` (builder), not `new_gtk` (that's on WebView).
- `src/asset_server.rs` — `echo://` wry custom protocol serving embedded art
  (`echo://art/?path=`) and cached depth masks (`echo://mask/?path=`) to the
  webview, which otherwise cannot read files from disk. Optional `w`/`h`
  params return an aspect-correct ("cover") resize to the display slot
  (square slot -> square, rectangular -> rectangular); results are cached
  per session. Registered in `ui_window.rs`.
- `ui/dist/index.html` — the real single-file frontend (onboarding, dashboard,
  now playing, queue, browse, settings), bundled via `include_str!`.

## Status: compiles + window renders under niri (confirmed 2026-09-13)
NOT yet built (the real remaining work):
1. ~~Album-art extraction~~ DONE (`src/art.rs`, `lofty`); title/artist/album
   extraction for a library index still open.
2. ~~Depth engine never wired~~ DONE (`src/pipeline.rs` + background worker;
   `UiState.depth_mask_path` carries the mask path). Verify: `--self-test-mask`.
3. ~~Queue IPC~~ DONE: `next`/`prev`/`set_shuffle`/`set_smart_shuffle`/
   `cycle_repeat`/`set_queue` dispatchable; queue state in `UiState`.
4. ~~Smart shuffle~~ DONE: `src/history.rs` persisted play history feeds
   `decide_next` (no more empty `HashMap`).
5. **Settings persistence** DONE (`src/settings.rs`).
6. **niri blur toggle** DONE (`src/blur.rs`); matches `title="^Echo"` (real
   app-id unconfirmed - see ARCHITECTURE_DECISIONS.md).
7. **Gapless** DONE (`player.enable_gapless` + `queue.peek_next`); shuffle
   falls back to EOS-advance. Audible behaviour still needs a real-audio pass.
8. **Crossfade** DONE (`src/player.rs` dual `uridecodebin` + `audiomixer` +
   shared DSP; manual fade ramp; `set_crossfade` IPC). Audible quality still
   needs a real-ears check (clicks/pops/timing).
9. **Library index** DONE (`src/library.rs`): scan + title/artist/album +
   technical metadata + JSON persistence + `set_library_path`/`scan_library`/
   `get_library` IPC. The onboarding wizard sets the path.
10. **Real UI** — DONE (`ui/dist/index.html`): onboarding wizard, dashboard
    (off-grid glass cards, depth-reactive hero, rolling gapless timeline,
    queue preview, library pulse, audio-quality + DSP cards), now playing,
    queue, browse (songs/albums/artists/formats/folders lenses), settings.
    Album art + depth masks are served over the `echo://` custom protocol
    (see `src/asset_server.rs`). Crossfade remains out of scope; the timeline
    only visualizes the real gapless state.

## IPC contract (the thing the UI needs)
The frontend is a single bundled HTML page. JS sends commands via
`window.ipc.postMessage(JSON.stringify({cmd, arg}))`; Rust pushes state back
by calling `window.__echoStateUpdate(state)` (define this function in JS).

Commands (JS -> Rust). `arg` is a JSON value, optional unless noted:
- `play`, `pause`
- `seek` (arg: number = ms)
- `set_speed` (arg: number), `set_mono_downmix` (arg: bool),
  `set_concert_hall` (arg: bool)
- `load_and_play` (arg: string = file path)
- `set_queue` (arg: `{"paths": [...], "start_index": 0}`)
- `next`, `prev`, `cycle_repeat`
- `set_shuffle` (bool), `set_smart_shuffle` (bool),
  `set_smart_shuffle_bias` (number 0..1: 0 = uniform random, 1 = favor
  unplayed)
- `set_niri_blur` (bool)
- `set_crossfade` (arg: number = ms; 0 = gapless)
- `set_library_path` (arg: string = music folder; persists + scans)
- `scan_library` (rescan the configured folder), `get_library` (push current)

State (Rust -> JS), pushed on every command + ~200ms tick:
```json
{
  "playing": false, "position_ms": 0, "duration_ms": 0,
  "speed": 1.0, "mono_downmix": false, "concert_hall_enabled": false,
  "dsp_speed_available": true, "dsp_concert_hall_available": true,
  "last_error": null, "current_track": null, "depth_mask_path": null,
  "queue_len": 0, "queue_index": 0, "shuffle": false,
  "smart_shuffle": false, "repeat": "Off", "library_count": 0,
  "smart_shuffle_bias": 1.0,
  "crossfade_ms": 0, "crossfade_active": false, "crossfade_to": null,
  "crossfade_progress": 0.0,
  "upcoming": [{"path": "/music/b.mp3", "duration_ms": 203000}]
}
```
`depth_mask_path` is the on-disk path of the cached depth mask for the
current track (a grayscale PNG, white=foreground); `null` while computing or
when the track has no art. `upcoming` is the next up-to-5 queued tracks with
probed durations (for the rolling timeline). `library_count` is the indexed
track count.

The **library index** is pushed separately (it's too big for the per-tick
state): after `get_library`/`scan_library`/`set_library_path`, Rust calls
`window.__echoLibraryUpdate(tracks)` where each track is
`{path, title, artist, album, duration_ms, sample_rate, bit_depth, has_art}`.

The UI can't read files directly from disk, so art/masks are served over the
`echo://` custom protocol (`src/asset_server.rs`, registered in
`ui_window.rs`): the frontend turns a path into `echo://art/?path=<enc>` or
`echo://mask/?path=<enc>` with `encodeURIComponent`. Responses carry
`Access-Control-Allow-Origin: *` so the page can sample the art into a canvas
for the dynamic accent.

## Build/test
- `./install.sh` (pacman deps + `cargo build --release`) then `./test.sh`
  (gst-inspect checks + `cargo check` + `--self-test`). Window needs a real
  display; `--self-test` works headless. First build ~5min (450 crates);
  later builds seconds. Avoid `cargo clean`.
- `cargo test` — 25 unit tests (queue/blur/depth-cache-key/library/IPC parse).
- `Cargo.lock` exists and is current (no git repo here, so nothing to commit).

## Key gotchas (from docs)
- webkit2gtk-4.1 (NOT AUR `webkit2gtk`).
- Depth model is Small ONLY — Base/Large/Giant are CC-BY-NC (license trap).
- ort output-extraction API changed across rc versions (rc.9→rc.11→rc.12).
- Mono-downmix change needs a flushing seek to renegotiate (real past bug).
- niri blur: write window-rule to niri config (auto-reloads on save); KDE/GNOME
  deferred/dropped.
