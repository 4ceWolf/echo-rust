# TODO / Known Gaps

Kept up to date as things are built. If it's not checked off, it does not
exist yet — this file is the source of truth over any assumption that
"probably X is done by now."

## Confirmed scope (from planning discussion)
- [x] Rust playback engine (`gstreamer-rs`) — core pipeline built
      (`src/player.rs`): Concert Hall (reverb+crossfeed), mono downmix
      (with the renegotiation-seek fix from day one), speed. Gapless
      about-to-finish handling and bus-message wiring (errors/EOS) are
      STILL NOT built - `Player` exposes `bus()` for this but nothing
      currently reads from it. See "Not yet built" below.
- [x] `wry` + `tao` webview UI, Wayland-correct build path implemented
      (`src/ui_window.rs`) via `WebViewBuilderExtUnix::build_gtk` per
      confirmed-current docs - genuinely uncompiled/untested, see the
      "Build verification constraint" note below.
- [x] Thin Rust<->JS bridge (`src/main.rs`): JSON commands in via
      `with_ipc_handler`, JSON state out via `evaluate_script` triggered
      through a custom `AppEvent`. Command set so far: play, pause, seek,
      set_speed, set_mono_downmix, set_concert_hall, load_and_play.
      Queue/shuffle/repeat commands NOT yet wired to IPC (QueueState
      exists in src/queue.rs and is held in AppState, but main.rs doesn't
      dispatch to it yet).
- [x] Real depth-effect engine (`src/depth.rs`): Depth Anything V2 Small
      via `ort`, per-track masking, cache-path helper. NOT yet connected
      to anything - no code calls `get_or_compute_mask` yet, and the
      depth model path resolved in main.rs (`_depth_model_path`) is
      currently discarded (prefixed `_`) rather than passed anywhere.
- [x] Model auto-download (`src/depth_model.rs`) with SHA256 verification
      against the confirmed hash.
- [ ] Depth-effect UI compositing — NOT started. This is entirely
      frontend (HTML/CSS/JS) work once there's a real UI to build it in.
- [ ] Settings menu — NOT started (backend or frontend).
- [ ] niri blur toggle — NOT started.
- [ ] Visual design (the actual dashboard-inspired UI) — NOT started.
      `ui/dist/index.html` is a placeholder proving the wry/tao plumbing
      works, not real UI.
- [x] Error logging (`src/logging.rs`) — file logging + rotation +
      in-memory recent-errors buffer, done. NOT yet exposed anywhere in
      the UI (no Settings panel exists yet to show `logging::recent_errors()`).
- [x] `install.sh` and `test.sh` — Arch package list confirmed against
      real package pages (note: `webkit2gtk-4.1`, NOT the AUR
      `webkit2gtk` package, which is intentionally pinned to the old
      2.50.x WebKit1 branch - see install.sh's comment). `main.rs` has a
      `--self-test` mode that exercises GStreamer pipeline construction,
      depth model download+checksum, and depth engine construction
      without needing a display - `test.sh` drives this plus
      `gst-inspect-1.0` plugin-availability checks. Neither script tests
      the actual window/webview, which needs a real display and remains
      the single least-verified part of the stack.

## Not yet built (concrete list, checked against actual source)
- GStreamer bus message handling: errors, EOS, gapless "about-to-finish".
  `Player::bus()` exists; nothing calls `bus.pop()` or connects to it.
  This means **EOS currently does nothing** - a track will finish and
  playback will just stop, it will not auto-advance to the next queued
  track. This is the single most important missing piece for basic
  usability - higher priority than the depth effect or settings menu.
- ~~`QueueState` (shuffle/repeat/history) is constructed and held in
  `AppState` but no IPC command reaches it yet.~~ DONE 2026-09-13:
  `next`/`prev`/`set_shuffle`/`set_smart_shuffle`/`cycle_repeat`/`set_queue`
  are now dispatchable from JS. See the dated section at the bottom.
- `Player` has no real "is it currently playing" boolean tracked - the
  `UiState.playing` field pushed to the frontend is hardcoded `false`
  (see the TODO comment directly in main.rs). Needs either a bus-message-
  driven state field or querying pipeline state directly.
- ~~Depth engine (`DepthEngine`) is never constructed in `main.rs`.~~
  DONE 2026-09-13: a background worker constructs the engine, and
  `pipeline::compute_depth_mask` loads album art, calls `infer_mask`, and
  caches the mask to disk; the result path is pushed to the UI via
  `UiState.depth_mask_path`. See the dated section at the bottom.
- ~~No album art extraction/reading exists~~ DONE 2026-09-13: `src/art.rs`
  extracts embedded front-cover art via `lofty` (chosen + researched, see
  ARCHITECTURE_DECISIONS.md). Verified against a real tagged MP3 via the
  new `--self-test-art <path>` mode. Remaining art work (title/artist/
  album/track extraction for the library index) is separate, not done.

## Explicitly deferred (not forgotten, just not now)
- [ ] KDE/KWin blur path (kwriteconfig + D-Bus reconfigure) - niri first,
      user said niri is current priority; KDE requested but secondary.
- [ ] GNOME blur - dropped from this rewrite's scope entirely. The old
      GNOME Shell extension (`shell-extension/echo-blur@local/`) from the
      previous GTK4 project is real, working code and can be revisited
      later; it's independent of this rewrite either way.
- [ ] Android - explicitly told not to let this shape any current decision.
      `wry`/`gstreamer-rs` both have rougher Android stories than desktop;
      deal with it when actually requested.

## Debugging & Logging (explicitly requested - do not skip)
The previous project's biggest recurring failure mode was bugs that
"should" have been visible but weren't, because nothing was actually being
logged anywhere the user or I could see it after the fact. This rewrite
must, from the start:
- Log to a real file (not just stdout that vanishes when a terminal
  closes), rotated/capped so it doesn't grow forever.
- Log the *specific* thing that failed with enough context to act on it
  (file path, model name/version, exact error from the underlying
  library) - not a generic "something went wrong."
- Surface a way to view recent errors from inside the app itself (part of
  the Settings menu), not only via `journalctl`/terminal output, since
  getting real terminal output out of the user was the single biggest
  bottleneck in the previous project's bug-fixing rounds.

## Build verification constraint (important context for debugging)
The Rust source in this project was written in an environment with no Rust
toolchain and no network access to rust-lang.org (crates.io itself is
reachable, but rustup's install host is not) — meaning **none of this
Rust code has been compiled or run before you build it yourself.**

This is a materially different situation than the previous Python/QML
project, where every file was at least syntax-checked (`ast.parse`) and
brace-balance-checked before being handed over. That safety net does not
exist here. Expect the first `cargo build` to surface real compile errors
- version/API mismatches especially around the `ort` release candidate and
`wry`'s Unix-specific extension traits are the most likely spots, since
those are the least-traveled parts of the stack. Paste the exact compiler
error (not a paraphrase) when something doesn't build - Rust's compiler
messages are usually specific enough to fix directly from, but only if the
real message is what gets shared.

## First real compile: results (2026-09-12)
System dependencies installed cleanly (webkit2gtk-4.1 confirmed correct).
`cargo build --release` surfaced 4 real, fixable issues - none in the
areas flagged as highest-risk (wry/tao Wayland setup didn't even get
reached yet; these are all earlier in the build):

1. **`gst` vs `gstreamer` crate name mismatch** - every source file wrote
   `use gst::...` assuming that was the crate's name, but `Cargo.toml`
   declared it as `gstreamer` with no rename. This caused the vast
   majority (~30) of the reported errors, all `E0433 cannot find crate
   'gst'`. Fixed by declaring `gst = { package = "gstreamer", version =
   "0.25.3" }` in Cargo.toml instead of `gstreamer = "0.25.3"`.
2. **`rand::RngExt` not imported** - `rand` 0.10's `random_range` method
   lives on a trait (`RngExt`) that must be explicitly imported, not just
   `Rng`. The compiler's own suggestion (`use rand::RngExt;`) was exactly
   right. Fixed in `src/queue.rs`.
3. **`ndarray` version conflict, exactly as anticipated** - `Cargo.toml`
   pinned `ndarray = "0.16"` but `ort 2.0.0-rc.12` itself depends on
   `ndarray 0.17.2` internally, producing two incompatible copies of the
   crate in the dependency graph (the compiler's error named this
   directly: "there are multiple different versions of crate `ndarray`").
   Fixed by changing the pin to `ndarray = "0.17"` to match what `ort`
   actually uses. This is the exact class of problem
   ARCHITECTURE_DECISIONS.md's ort section warned about, just one level
   up the dependency tree from where that warning anticipated it (output
   *extraction* API, not a version *conflict* - still the same root
   cause: this crate line is genuinely unstable pre-1.0 territory).
4. Two harmless warnings (unused `rand::Rng` import after adding
   `RngExt`, an unnecessary `mut`) - cleaned up, not real bugs.

None of these were in `ui_window.rs` (the wry/tao Wayland-correct build
path) or the `SessionOutputs`/`ValueRef` extraction call in `depth.rs`
that were flagged as the least-verified parts of the stack - those
haven't been exercised by a successful build yet, since compilation
stopped on the above first. Expect those to be the next things to check
once this round is fixed.

## Second compile round (2026-09-12): down to 1 error
All 3 previous fixes held (no regressions). One new, genuinely
unavoidable-to-catch-in-advance error surfaced:

5. **`gst::filename_to_uri` doesn't exist** - it turns out
   `gstreamer-sys` (the raw FFI layer) has a C symbol
   `gst_filename_to_uri`, but gstreamer-rs (the safe `gst` crate) never
   exposed a safe wrapper for it. The actual safe, `Path`-aware function
   lives on `glib` instead: `glib::filename_to_uri(path, None) ->
   Result<GString, glib::Error>`. Fixed in `src/player.rs`. `glib` was
   already a direct dependency, so no Cargo.toml change was needed for
   this one.

Also cleaned up the resulting unused `Rng` import (only `RngExt` is
actually called in `src/queue.rs`).

Build times: the first build compiles ~450 crates from scratch
(gstreamer-rs, wry, ort, and their own dependency trees), which is
genuinely CPU-heavy and can take 5+ minutes on a single build - this is
normal for a project with this dependency footprint, not a sign
something's wrong or inefficient about the setup. **Every build after
the first one only recompiles what actually changed** (usually just this
project's own small `.rs` files), so subsequent iterations should be
seconds, not minutes, once `cargo build`'s initial dependency compile is
done once. If a `cargo clean` is ever run, the next build pays that full
cost again - avoid running `cargo clean` between iteration rounds unless
actually necessary.

## External audit response (2026-09-12)
A third-party audit of the zip (not written by whoever's been building
this with Claude) found several real, accurate issues. Distinguishing
what got fixed from what's still open, and explicitly rejecting the
audit's separate feature-brainstorm documents as something to build
against (they read as speculative lists, not a spec - see the
conversation itself for that call):

**Fixed this round:**
- Spectrum flood: `post-messages` was `true` with nothing draining the
  bus - would genuinely accumulate a message every 50ms for the whole
  track. Set to `false` until real consumption exists (see next point).
- No bus/error/EOS handling at all: `Player::poll_bus()` now exists,
  drains every pending message, and distinguishes Error/EOS (both
  advance the queue)/Warning/StateChanged (drives the real `playing`
  flag). Wired into `main.rs`'s timer tick, and EOS now actually calls
  `QueueState::decide_next` and loads the next track - **this is the
  fix that makes tracks play one after another instead of just
  stopping**, the single most important functional gap before this.
- `UiState.playing` was hardcoded `false` - now reads `Player.playing`,
  which `poll_bus`'s StateChanged handling keeps accurate.
- Non-atomic model download - now writes to a `.part` file and
  `rename()`s into place only after a full write + `sync_all()`, so a
  crash mid-download can't leave a partial file at the real path (the
  checksum check was already there as a second line of defense, but
  atomicity is the more correct primary fix).

**Confirmed accurate, still open (not fixed this round, in priority order):**
- ~~Smart shuffle has no real play-history feed~~ FIXED 2026-09-13:
  `src/history.rs` persists a `path -> last_played_seconds` JSON map and
  `begin_track` records every play; `decide_next` now receives the real
  `AppState.play_history` instead of an empty `HashMap`.
- ~~`QueueState` still has no IPC commands reaching it~~ FIXED 2026-09-13:
  `next`/`prev`/`set_shuffle`/`set_smart_shuffle`/`cycle_repeat`/`set_queue`
  are now dispatchable from JS, and the queue state (len/index/shuffle/
  smart_shuffle/repeat) is surfaced in `UiState`.
- ~~Depth cache key is just `sha1(track path)`~~ FIXED 2026-09-13: the key
  is now `sha1(track path + "|" + art hash + "|" + MODEL_VERSION)` - a
  track whose embedded art changes (or whose model is upgraded) gets a
  fresh mask, not a stale one.
- ~~Depth engine still isn't constructed anywhere in `main.rs`~~ DONE
  2026-09-13: constructed in a background worker; `pipeline.rs` wires
  art extraction -> decode -> `infer_mask` -> cache. The remaining part of
  this bullet (album-art extraction) was already closed - see the dated
  sections at the bottom.
- No `Cargo.lock` committed - accurate and important given how much this
  project's docs emphasize exact pinned versions, but a lockfile can
  only be generated by actually running `cargo build`/`cargo generate-
  lockfile`, which requires the Rust toolchain this environment doesn't
  have. **Whoever runs the next successful build should commit the
  resulting `Cargo.lock`** and switch `install.sh` to `cargo build
  --locked --release` afterward, so this stops being a gap.
- No automated tests (`queue.rs`'s repeat/shuffle logic, `depth.rs`'s
  cache-key/normalization math, IPC command parsing) - accurate, real
  gap, not addressed this round.
- ~~Concert Hall availability conflates reverb-available with crossfeed-
  available~~ FIXED 2026-09-13: `DspAvailability` now reports `speed`,
  `reverb`, and `crossfeed` separately, with `concert_hall()` = `reverb &&
  crossfeed` (Concert Hall needs both).
- No UI beyond the placeholder page, and the "TypeScript frontend" in
  ARCHITECTURE_DECISIONS.md was aspirational, not built - accurate.
  Real UI work (including anything animation/graph/dashboard-related)
  is intentionally sequenced *after* the backend is trustworthy, which
  is exactly what's happening in this round.
- Blocking model download on startup with no progress UI - accurate,
  and there's nowhere to show progress until real UI exists anyway.
- IPC has no navigation/origin/command hardening yet - accurate, low
  priority while the webview only ever loads our own bundled HTML.

**Not scheduled, not forgotten:** the audit's separate brainstorm
documents (dashboard widgets, "Echo Lab", listening heatmaps, light-mode
flashbang, depth-reactive parallax, etc.) are treated as speculative
feature ideas, not a specification - every one of them is fully logged
in `docs/FEATURE_IDEAS.md` (organized by theme) so nothing from that
material is lost, even though none of it is prioritized or scheduled
yet. Sequencing reason, not dismissal: none of it should be built before
the app actually plays music end-to-end and has any real UI at all. The
flashbang light-mode request specifically needs real UI to exist first
(bundling the reference audio clip, building the actual white-flash +
fade transition) - full spec is in FEATURE_IDEAS.md's dedicated section,
not started.

## First real window render (2026-09-13): major milestone, one real bug
`cargo build --release` succeeded, `./test.sh` fully passed (including
the `--self-test` binary run), and **a real GTK/Wayland window with a
working webview opened under niri** - the single highest-risk unknown in
this entire rewrite. It rendered actual placeholder text on screen.

One real bug: the window rendered tiny (~200x300px, top-left corner)
with unstyled/overflowing text instead of filling the requested
1100x720. Root cause: `gtk::Fixed` is a fixed-position/fixed-size
container that does not participate in GTK's normal size negotiation
with its children - telling the *vbox* to expand the `Fixed`
(`pack_start(&fixed, true, true, 0)`) never told the webview widget
*inside* the `Fixed` to actually fill that space, so it stayed at its
natural/minimum size.

Fixed in `src/ui_window.rs` by switching to a simpler pattern that packs
the webview directly into tao's default vertical `gtk::Box` (which
participates in GTK's normal size negotiation, unlike `gtk::Fixed`):
`WindowExtUnix::default_vbox()` + `WebViewBuilderExtUnix::build_gtk(vbox)`.
This skips the `gtk::Fixed`/manual box-packing entirely, letting GTK size
the webview normally - no separate size-negotiation bug to introduce.
The `gtk` crate is no longer a direct dependency of this project as a
result (it was only ever needed for the now-removed `gtk::Fixed` call) -
removed from `Cargo.toml`.

NOTE: an earlier draft of this note (and the corresponding code) used
`WebViewBuilderExtUnix::new_gtk(...)` - that function does not exist.
The real wry 0.56.1 API is `WebViewBuilderExtUnix::build_gtk(...)` (on
the builder); `WebViewExtUnix::new_gtk(...)` does exist but lives on
`WebView` (the built value), not the builder. Caught on the next real
compile, which is exactly what the "Build verification constraint" note
above warns about.

Also cleaned up 15 dead-code warnings from the build output - all of
them expected/already-tracked "built but not yet wired up" gaps (the
depth engine, the logging recent-errors buffer, gapless preloading,
queue methods with no IPC command yet), not new bugs. Added targeted,
documented `#[allow(dead_code)]` on each specific item rather than a
blanket crate-level suppression, so genuinely new dead code introduced
later will still warn normally.

**Confirmed 2026-09-13**: window now renders correctly at full size under
niri - the `default_vbox()` + `build_gtk()` fix resolved the tiny-window
bug. This closes out the single highest-risk unknown in the rewrite
(wry/tao Wayland-correct rendering).

## Album-art extraction (2026-09-13)
`lofty` researched and chosen (see ARCHITECTURE_DECISIONS.md), added as a
dependency, and `src/art.rs` implemented: `extract_front_cover(path) ->
Result<Option<Artwork>, PlayerError>` returns embedded front-cover bytes +
sniffed mime, or `Ok(None)` for untagged/no-art files (normal, not an
error). Added a `--self-test-art <path>` mode to main.rs for scriptable,
headless verification of this specifically.

Verified both directions against real files:
- `Ok(None)` on three `~/Downloads/*.mp3` that genuinely lack art (cross-
  checked with ffprobe - only encoder tags, no attached picture).
- `Ok(Some)` on an ffmpeg-generated MP3 with an embedded PNG cover:
  extracted 174 bytes, mime `image/png`.

Not done (separate, still open): title/artist/album/track extraction for a
future library index; wiring the extracted art into the depth engine and UI
(next item).

## Depth engine wired (2026-09-13)
The depth engine is now constructed and actually used, closing the "never
wired" gap. Two new pieces:

- `src/pipeline.rs` - `compute_depth_mask(engine, track_path)` ties art and
  depth together: extract front cover -> sha1 the art bytes -> check/save
  the mask cache -> decode -> `infer_mask` -> write the PNG. Returns the
  cached mask path, or `Ok(None)` for artless tracks. This is the ONLY
  module where `art` and `depth` meet (layering stays one-directional).
- A background worker thread in `main.rs` (`depth_worker`) owns the
  `DepthEngine` and computes masks off the UI thread - `ort`'s
  `Session::run` needs `&mut self` (so the engine is not shared) and the
  inference is heavy enough to stutter the UI if run inline on a track
  change. Results come back through a new `AppEvent::DepthMaskReady` event,
  which only touches the webview from the main thread (correct pattern).

Also, as part of this: the depth cache key is now
`sha1(track path | art hash | MODEL_VERSION)` (was `sha1(track path)`),
fixing the audit's stale-mask finding - see ARCHITECTURE_DECISIONS.md.

New/changed UI surface (backend only - the placeholder HTML doesn't read it
yet): `UiState` now carries `current_track` and `depth_mask_path`. `begin_track`
is the single choke point for "a new track is playing" (used by both the
IPC `load_and_play` command and the EOS auto-advance path) and triggers the
depth request.

Verified headlessly: `--self-test-mask <file>` runs the full pipeline. On an
ffmpeg fixture with an embedded 64x64 PNG cover it produced a valid 64x64
grayscale mask PNG, and a second run hit the cache (no re-inference).
`--self-test` still passes (no regression). No dead-code warnings remain in
`depth.rs` (the `#[allow(dead_code)]`s there were removed once actually used).

Still open (unchanged): queue IPC commands, smart-shuffle play history,
gapless preloading, settings persistence, niri blur, automated tests.

## Queue IPC + play history (2026-09-13)
Queue control is now reachable from JS and smart shuffle has a real feed.

New IPC commands in `main.rs::handle_command`:
- `set_queue` (`arg`: `{"paths": [...], "start_index": 0}`) - replaces the
  queue and plays the start track.
- `next` - `decide_next` then load+play.
- `prev` - `decide_prev`; but if >3s into the track, restarts it instead
  (same rule as the Python version).
- `set_shuffle` / `set_smart_shuffle` (bool), `cycle_repeat` (off->all->one).

`UiState` now also carries `queue_len`, `queue_index`, `shuffle`,
`smart_shuffle`, and `repeat` so the future UI can render queue state.

Smart shuffle feed: new `src/history.rs` (`PlayHistory = HashMap<path,
last_played_secs>`) loaded at startup from `~/.local/share/echo/
play_history.json`, recorded + saved on every `begin_track`, and saved again
on close. `decide_next` now receives `&AppState.play_history` (no more empty
`HashMap` placeholder).

`begin_track` is now the single choke point for "a new track is playing",
used by: IPC `load_and_play`, IPC `set_queue`/`next`/`prev`, and the EOS
auto-advance path - all record play history, kick the depth worker, and
reset stale mask state identically.

Also removed the now-obsolete `#[allow(dead_code)]` markers on
`queue.rs`'s `set_queue`/`cycle`/`decide_prev`, and added `Default` for
`RepeatMode`. Added 7 unit tests for queue behaviour (`cargo test` passes).

## Settings persistence + Concert Hall fix (2026-09-13)
`src/settings.rs` persists user settings (`speed`, `mono_downmix`,
`concert_hall`, `shuffle`, `smart_shuffle`, `repeat`, `niri_blur`) to
`~/.local/share/echo/settings.json`, loaded at startup (applied to the fresh
player/queue before anything plays) and saved on every mutating IPC command
(`set_speed`/`set_mono_downmix`/`set_concert_hall`/`set_shuffle`/
`set_smart_shuffle`/`cycle_repeat`). `niri_blur` is stored but the toggle
itself is not implemented yet (see below).

Concert Hall availability was de-conflated: `DspAvailability` now exposes
`speed`/`reverb`/`crossfeed` separately, with `concert_hall()` = both present.

## niri blur toggle (2026-09-13)
`src/blur.rs` manages a single delimited `window-rule` block in niri's config
(`~/.config/niri/config.kdl`, or `$XDG_CONFIG_HOME/niri/config.kdl`):

```
// === echo blur (managed by Echo, do not edit) ===
window-rule {
    match title="^Echo"
    background-effect {
        blur true
    }
}
// === /echo blur ===
```

New `set_niri_blur` IPC command toggles it (idempotent - adds/removes only
our marked block, never clobbers other rules), and blur state is restored at
startup when the persisted `settings.niri_blur` is true. niri auto-reloads
config on save. `toggle_blur` is a pure function with 3 unit tests.

Caveat (documented, not a bug): it matches on `title="^Echo"` because
wry/tao doesn't set a custom GTK app-id; confirm the real app-id with
`niri msg pick-window` if title matching ever proves fragile. Also, niri's
non-xray blur is experimental and doesn't apply during window open/close
animations (per the niri wiki).

## Gapless playback (2026-09-13)
Gapless preloading is now implemented (was the "still open" item): `Player::
enable_gapless` connects the `about-to-finish` signal to consume a preloaded
path (no teardown, no gap), `QueueState::peek_next` preloads without
mutating, and `main.rs` commits the queue advance when the handoff fires.
Shuffle falls back to the EOS-advance path (a gap on shuffle is expected).
23 unit tests include `peek_next` coverage. The audible behaviour still needs
a real-audio confirmation pass; the mechanism is correct-by-construction.

## Library index + scanning (2026-09-13)`src/library.rs` closes the "play explicit file paths only" gap: `scan_folder`
recursively walks a folder for audio files and extracts metadata via `lofty`
(title/artist/album/duration/sample-rate/bit-depth/has-art), producing a
`Vec<Track>` persisted to `~/.local/share/echo/library.json`. Wired into
`main.rs` with a background scan worker (scanning is slow, must not block the
UI), `set_library_path` / `scan_library` / `get_library` IPC commands, and a
separate `window.__echoLibraryUpdate(tracks)` push (the library is too big for
the per-tick `UiState`; `UiState.library_count` reports just the count).
`settings.library_path` is persisted. The onboarding wizard sets the path.
Verified with 2 unit tests (a generated WAV yields correct duration/rate/depth;
non-audio + unreadable files are skipped).

## Window smoke test + set_speed bug fix (2026-09-13)
Ran the actual binary under Wayland for the first time since the gapless/
library/settings work. It panicked on startup:

```
property 'tempo' of type 'GstPitch' can't be set (expected 'gfloat', got 'gdouble')
```

Root cause: `Player::set_speed` passed `self.speed` (f64) to the `pitch`
element's `tempo` property, which is a gfloat (f32). This was latent because
`--self-test` never called `set_speed`, but the startup settings-apply path
(`player.set_speed(settings.speed)`) added earlier now triggers it on every
launch. Fixed with `self.speed as f32` (lossless for the [0.5, 2.0] range).
This is the same f64/f32 property-type class the docs already flagged for
freeverb - a good reminder that GStreamer element properties are typed.

After the fix: `timeout 8 ./target/release/echo` runs the full 8s without
crashing; the log confirms logging -> model verify -> pipeline build ->
**window + webview created** -> depth worker loaded. The one path the
headless `--self-test` can't reach (window/webview/IPC init) is now verified.
Also added a `--self-test-library <folder>` mode and confirmed it indexes 137
real tracks in `~/Downloads` with correct durations/sample-rates.

## Empty-queue play crash fix (2026-09-13)
Playing a single track via `load_and_play` with an empty queue (clicking a
track in Browse) panicked at `refresh_upcoming` with `range start index 1 out
of range for slice of length 0`, and because the command runs inside a
GTK/WebKit `extern "C"` IPC callback the panic could not unwind - it aborted
the whole process. Fixed by extracting `upcoming_range(len, index)` (returns
an empty range for an empty queue; unit-tested), and by wrapping each IPC
command in `catch_unwind` so no command can ever take the app down. Also
`self_test_art`/`self_test_mask` wording no longer says "embedded" when the
art may have come from a sidecar.

## UI stability + performance pass (2026-09-13)
- **Poisoned-mutex freeze fixed:** a panic caught by the IPC `catch_unwind`
  left the state mutex poisoned, so every later lock failed and the UI froze
  while the process stayed alive. Added `lock_state()` which recovers
  (`into_inner`) from poisoning; all lock sites use it.
- **Asset protocol is now async:** `with_asynchronous_custom_protocol` +
  a small worker pool. Decoding/resizing art no longer blocks the GTK main
  thread, which is what made Browse scroll lag. Added a session source-art
  cache (track path -> bytes) so thumbnails don't re-read/re-parse tags, and
  Browse images use `loading="lazy"`.
- **Depth motion:** removed the cursor-follow parallax; the depth now behaves
  like the iOS lockscreen - the subject sits still and occludes the lyrics
  and controls ("hide behind"). Dropped the second masked layer (subject
  shadow) to halve mask work after a WebKitWebProcess crash
  (`free(): corrupted unsorted chunks`, core dumped).
- **Seek sliders:** per-element drag tracking (`Echo.activeSeek`) instead of
  one shared flag that could stick and freeze all three sliders.
- **Single Depth badge** (removed the duplicate `#npDepthTag`).
- **Lyrics position setting** (Left/Center/Right/Off) in Setup > Appearance.

## UI round 2 fixes (2026-09-13)
- **Root cause of non-updating transport found:** `Player::new` called
  `bus.add_signal_watch()`, whose GLib source *pops* (consumes) bus messages,
  so `poll_bus()` never saw `StateChanged`/EOS/Error. `playing` was stuck
  false (play/pause icons never changed), EOS never advanced, errors were
  invisible. Removed the signal watch; added a regression test
  (`player::tests::playing_state_tracks_pipeline`).
- Depth is now static iOS-lockscreen style: no cursor parallax, the masked
  subject sits on top (z-index fixed - it was rendering *below* the UI, so
  occlusion never worked) and the lyrics/controls hide behind it. Removed the
  backdrop blur (art was blurry); bigger lyrics (27px).
- DSP controls added: Mono / Concert Hall toggles + speed slider in Now
  Playing, and the dashboard "Processing" rows are clickable (speed cycles).
- Browse now queues the filtered list (`set_queue`) instead of a lone
  `load_and_play`, so next/prev and the queue populate. Queue rows use lazy
  `<img>` art.
- niri window blur removed from the UI; startup strips any existing rule.
- Light-mode flashbang implemented (bundled `assets/sfx/flashbang.mp3` served
  over `echo://sfx/flashbang`, with Flash/Gentle/Instant/Off setting).
- Known limitation: the queue page can only show the current + next 5 tracks
  because `UiState.upcoming` is capped at `UPCOMING_COUNT`; showing the full
  queue needs a separate push (like the library).

## UI round 3 fixes (2026-09-13)
- **Depth double-image fixed:** WebKitGTK does not support CSS luminance
  masks, so the grayscale mask fell back to alpha (fully opaque) and the `fg`
  was a second, misaligned copy of the art. Added `echo://subject/?path=<art>
  &mask=<mask>` which composites the art with the mask's luminance as alpha
  (a real transparent cutout) server-side; the frontend uses it for the depth
  layer. Unit-tested.
- **Mono "party mode" fixed:** the mono capsfilter forced `channels=1` with no
  converter before it, so negotiation failed and the error path skipped every
  track. Added an `audioconvert` immediately before the capsfilter; regression
  test `mono_downmix_does_not_break_playback`.
- **Concert Hall** wet level 0.42 -> 0.97 (as requested).
- **Flashbang** now plays via Web Audio (media elements fail on the custom
  scheme) and its duration follows the mp3; Gentle is a soft radial bloom vs
  Flash's instant white.
- **Right-click context menu** on any track row/card: Play now, Play from
  here, Queue album/folder, Copy file path.
- **Smart-shuffle bias** setting (0 = uniform random, 1 = favor unplayed),
  persisted; new `set_smart_shuffle_bias` IPC command + `smart_shuffle_bias`
  state field.
- Still to do: m3u playlist/album import; full-queue listing (only current +
  next 5 are sent).

## Crossfade engine (2026-09-13)
The single-`playbin` engine was re-architected to **dual `uridecodebin` +
shared `audiomixer` + one DSP chain** (Clementine's approach, but with a
single shared effects path — see ARCHITECTURE_DECISIONS.md "Crossfade playback
design"). Fade = manual stepped ramp on the mixer pads' `volume`, driven from
the main-thread `poll_bus`; `crossfade_ms == 0` = gapless. `set_crossfade` IPC
command + `crossfade_ms`/`crossfade_active`/`crossfade_to`/`crossfade_progress`
in `UiState`. The locked council decision (B becomes `current` at fade
*completion*) is enforced. 44 unit tests pass, including a real crossfade
completion and real pipeline playback.

## Still open (accurate as of 2026-09-13)
- **Crossfade audible verification** - the engine is built and tested at the
  pipeline level, but no click/pop/timing check has been done with real ears.
  Run on a machine, enable a 2-5s crossfade, and listen for a clean overlap.
- **Crossfade timeline UI** - the `set_crossfade` command and transition state
  are exposed, but the frontend's rolling-timeline overlap/drag-seam has not
  been wired to them yet (it shows gapless-only, not faked).

## Automated tests (2026-09-13)
44 unit tests now cover: queue (repeat/shuffle/prev/clamp/peek-next - 11),
niri blur toggle (3), depth cache-key (4), IPC command parsing (5), the
`echo://` asset protocol (percent-decode/query, mask route/404, cover-crop
math, sized resize - 6), artwork sidecar resolution (4), and the crossfade
engine (pipeline playback, mono-downmix regression, crossfade completion - 3).
Depth-mask *normalization* math (the min/max -> grayscale step) is not yet
unit-tested (it needs a mock ort session; the pipeline itself is verified
end-to-end via `--self-test-mask`).

