# GUI Session — Handoff & Instructions

For the session that builds the frontend. **The backend is done** (everything
except crossfade, which is a separate later task). This file is the "how to
start building UI" guide; `AGENTS.md` is the project map, `docs/UI_MASTER_DESIGN.md`
is the authoritative design brief, `docs/DESIGN_DIRECTION.md` is the aesthetic.

## Read these first (in order)
1. `docs/UI_MASTER_DESIGN.md` — the full spec (dashboard, timeline, onboarding, etc.)
2. `docs/DESIGN_DIRECTION.md` — confirmed visual language (dark glassmorphic,
   off-grid cards, dynamic accent)
3. `AGENTS.md` — the "IPC contract" section (exact commands + state shapes)

## How the frontend works today
- ONE file: `ui/dist/index.html`, bundled into the binary via `include_str!`
  (`src/main.rs`). No build system, no framework — just hand-written HTML/CSS/JS.
- To iterate: edit `ui/dist/index.html`, then `cargo build --release` + run.
  (You can set up a TypeScript/Vite build later; nothing depends on it yet.)
- The window is created in `src/ui_window.rs` — do NOT touch that file without
  reading its doc comment (Wayland-correct wry/tao path).

## The bridge (exact contract)
JS → Rust (send commands):
```
window.ipc.postMessage(JSON.stringify({ cmd: "play", arg: ... }))
```
Commands: `play` `pause` `seek` `set_speed` `set_mono_downmix`
`set_concert_hall` `load_and_play` `set_queue` `next` `prev` `cycle_repeat`
`set_shuffle` `set_smart_shuffle` `set_niri_blur` `set_library_path`
`scan_library` `get_library`.

Rust → JS (you define these two global functions):
```
window.__echoStateUpdate(state)   // per-command + every ~200ms
window.__echoLibraryUpdate(tracks) // after get/scan/set_library_path
```
`state` shape and `tracks` shape are documented in `AGENTS.md` (IPC contract).

## Build order (recommended)
1. **Onboarding** — first-run: pick gapless-vs-crossfade + duration, theme/accent,
   motion, and the music folder. `set_library_path` triggers the scan; the result
   arrives via `__echoLibraryUpdate`. Persist choices by calling the relevant
   `set_*` commands (they persist to `~/.local/share/echo/settings.json`).
2. **Dashboard** (the required page) — Now Playing hero + queue preview + library
   pulse. Read `library_count` from state; get the list via `get_library`.
3. **Now Playing + transport** — play/pause/seek/next/prev; render
   `position_ms`/`duration_ms`/`current_track`.
4. **Queue** — render `queue_len`/`queue_index`/`upcoming`; `set_queue`/`next`/`prev`.
5. **Browse** — group `tracks` by album/artist/genre (client-side; the backend
   hands you the flat list).
6. **Rolling timeline** (later) — render `upcoming` bars; crossfade seam needs
   the crossfade backend (not built yet — see below).

## The one backend limitation to design around
The webview cannot read files from disk directly. `current_track`,
`depth_mask_path`, and each library track's `path` are *paths*, not image bytes.
To actually show album art / depth masks you must serve the files to the page:
- a wry **custom protocol** (register a handler in `src/ui_window.rs`), or
- **base64 data URLs** pushed through the bridge, or
- a tiny local HTTP server.
This is a UI-session decision. Until then, render a tasteful placeholder.

## Rules that matter (from UI_MASTER_DESIGN.md)
- **Truthful visualization**: never draw state the backend didn't send. No fake
  `96 kHz`, no fake crossfade overlap, no "playing" before the backend says so.
- **Off-grid, not random**: varied card sizes with a coherent spacing system.
- **Graceful degradation**: flat art if no depth mask; no Concert Hall badge if
  `dsp_concert_hall_available` is false.
- **Accent is dynamic** from album art by default (Static/Dynamic/Custom); the
  literal colors in the design references are NOT the palette.

## Not in scope this session
- **Crossfade** — backend not built (needs a real machine + ears). The rolling
  timeline can show *gapless* seams today; the draggable crossfade seam comes
  after crossfade exists.
- **MPRIS / PipeWire inspectors, audio forensics, visualizers** — Phase E/F, later.

## Run & test
- `cargo run --release` (needs a real display; niri works).
- In the running window, open devtools console and fire commands manually:
  `window.ipc.postMessage('{"cmd":"set_library_path","arg":"/home/user/Music"}')`.
- Backend headless checks: `cargo test` (25 tests), `./target/release/echo --self-test`.
