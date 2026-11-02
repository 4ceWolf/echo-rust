# Echo (Rust rewrite) — Architecture Decisions

This document exists because the previous rewrite (PySide6 + QML) accumulated
several rounds of bugs that took multiple attempts to actually fix — mostly
because decisions got made without checking current library docs first, and
because there was no written record of *why* something was built a certain
way, so revisiting it later meant re-deriving context from scratch. This
rewrite starts with research and a paper trail instead.

Every claim below was checked against current (2026) docs/search at the time
of writing, not assumed from training data or a prior project. Sources are
cited so they can be re-checked if something breaks later.

## Stack

| Layer | Choice | Version pinned |
|---|---|---|
| Playback engine | Rust + `gstreamer` (gstreamer-rs) | `0.25.3` |
| Depth inference | Rust + `ort` (pykeio/ort) | `2.0.0-rc.12` |
| Depth model | Depth Anything V2 Small, ONNX, Apache-2.0 | (same model Noctalia's `wallpaper_depth` plugin uses) |
| UI rendering | `wry` (native OS webview, not Electron/Tauri) | `0.56.1` |
| Windowing | `tao` (wry's companion crate) | matched to wry 0.56.1's dev-dependency |
| UI code | HTML/CSS/TypeScript, compiled, no framework runtime bundled | — |

## Why these, specifically

### GStreamer via `gstreamer-rs`, not a rewrite of the playback logic itself
The GStreamer pipeline design (playbin, DSP element chain, gapless handling,
queue/shuffle/repeat state) was never the source of a single bug across the
entire previous project, in either the GTK4 or QML version. Every bug chased
in this project's history was in UI wiring, not the audio engine. Rewriting
proven pipeline *design* (not code — this is a new language) would be
throwing away the one part that worked. `gstreamer-rs` (crate `gstreamer`,
0.25.3, part of the GStreamer org's own `gstreamer-rs` repo) gives native
Rust bindings to the same GStreamer used before.

Source: https://github.com/GStreamer/gstreamer-rs,
https://docs.rs/crate/gstreamer/latest

### `wry` directly, not the full Tauri framework
The goal is web-grade visuals (real CSS/animation) without Electron-style
bloat. Tauri's own webview layer *is* `wry` — Tauri adds a framework, IPC
conventions, and a CLI/build pipeline on top. Using `wry` directly means one
process, no bundled Chromium, and no framework opinions to fight — at the
cost of building the (thin, deliberately boring) Rust↔JS bridge ourselves
instead of using Tauri's.

### The Wayland pitfall this doc exists to prevent someone repeating
`wry`'s own top-level docs show a "quick start" example using a bare
`.build(&window)` call. **That path is explicitly X11-only on Linux** — the
docs say so directly ("supports Windows, macOS, iOS, Android and Linux (X11
Only)"). Since niri (this project's actual target) is Wayland-only, using
that quick-start pattern would have meant the window silently fails to
render under niri specifically, discovered only after building UI on top of
it.

**The correct pattern for Wayland+X11**, straight from wry's real docs and a
current working example (`gtk_multiwebview.rs` in the wry 0.55 source):
1. Create the window with `tao`.
2. Get its GTK vbox via `WindowExtUnix::default_vbox()`.
3. Pack a `gtk::Fixed` into that vbox.
4. Build the webview into the `Fixed` via `WebViewBuilderExtUnix::build_gtk(&fixed)`,
   not the plain cross-platform `.build()`.

Source: https://docs.rs/crate/wry/latest (0.56.1 docs, "If you want to
support X11 and Wayland at the same time, we recommend using
WebViewExtUnix::new_gtk or WebViewBuilderExtUnix::build_gtk with gtk::Fixed"),
cross-checked against the actual `gtk_multiwebview.rs` example source.

### Why `tao`, not `winit`, for windowing
Per wry's own docs: "if the windowing library doesn't support gtk (as in
winit) you'll need to call `gtk::init` before creating the webview and then
call `gtk::main_iteration_do` alongside your windowing library event loop."
`tao` already handles this GTK main-loop pumping internally (its Linux
`event_loop.rs` literally loops on `gtk_main_iteration`) — this is the exact
same class of problem as the GLib-main-loop-pump code the PySide6 version
needed for GStreamer's bus watching, and `tao` solves it for us instead of
us reimplementing it a second time.

### `ort` version is a release candidate, on purpose
`pykeio/ort` 2.0.0 is still at `rc.12`, not a stable tag, as of this
writing. It's still the right choice: it's the actively maintained,
widely-depended-on crate (Magika, Hugging Face's Text Embeddings Inference
both use it) — the alternative mainstream option (`onnxruntime` crate) is
explicitly unmaintained and uses an older, different API shape
(`Environment::builder()` rather than `Session::builder()`). Pin the exact
`rc.12` version in `Cargo.toml` (not a caret range) since a `-rc` crate can
still introduce breaking changes before a stable 2.0 tag.

Source: https://github.com/pykeio/ort,
https://docs.rs/crate/ort/latest/source/src/session/mod.rs

### Confirmed: ort's output-extraction API is genuinely unstable across rc versions
While researching the exact call for reading depth-map output back out of
a `Session::run()` call, a real project (`ppocr-rs`) was found that had to
write a compatibility shim specifically because `Value::try_extract_tensor::<T>()`'s
return signature changed between `ort` `rc.9` (returns `Result<ArrayViewD<T>, Error>`)
and `rc.11` (returns a `(Vec<i64>, &[T])` tuple instead). This is direct,
concrete confirmation that the "don't casually bump this" warning above is
warranted, not theoretical.

This project uses `Value::try_extract_array::<T>()` (via the `ndarray`
Cargo feature, already enabled) specifically because it's the form shown
in `rc.12`'s *own* current docs.rs source
(`src/value/impl_tensor/extract.rs`) - not a form scraped from a
different pinned version's docs, which is an easy mistake to make since
most search results for "ort rust extract tensor" surface older-version
examples first.

Source: https://docs.rs/crate/ort/latest/source/src/value/impl_tensor/extract.rs,
https://docs.rs/ppocr-rs/latest/ppocr_rs/compat/index.html (the real
shim, as evidence this is a genuine cross-version hazard)

### Depth Anything V2 Small: exact input/output contract, and a license trap
Multiple different real ONNX exports of "Depth Anything V2 Small" exist
with materially different behavior - this matters enough to spell out
precisely, since getting it wrong wouldn't be a compile error, it would
be a silently-wrong or non-compliant depth mask:

- **License trap**: only the *Small* variant of Depth Anything V2 is
  Apache-2.0. Base/Large/Giant are CC-BY-NC-4.0 (non-commercial) and must
  never be substituted in, even though they're the same model family and
  it would be an easy accidental swap (e.g. "the Base model gives better
  masks, let's use that instead"). Confirmed via a real project
  (Lensmera's model mirror) explicitly warning about exactly this.
- **Input**: `1x3x518x518`, RGB, ImageNet-normalized:
  `(pixel/255 - mean) / std` with `mean=[0.485,0.456,0.406]`,
  `std=[0.229,0.224,0.225]`.
- **Output**: `1x518x518`, **relative inverse depth** - NOT metric depth
  in meters (a different, separate ONNX export of the same architecture
  produces metric depth; that is not what's used here). Values are only
  meaningful relative to each other *within one image* - to render a
  usable mask: normalize to `[0,1]` against that image's own min/max,
  then resize back to the original image's dimensions before deriving
  the foreground/background threshold.

Source: https://huggingface.co/inference4j/depth-anything-v2-small,
https://huggingface.co/lensmera/lensmera-depth-dav2-small (license trap
warning), https://huggingface.co/77ukhtar/depth-anything-v2-metric-onnx
(the *different*, metric-output export that must NOT be confused with
the one actually used here)

### Exact model file, confirmed directly from its Hugging Face page
- **Repo**: `onnx-community/depth-anything-v2-small` (License field on the
  repo page itself: `apache-2.0`, directly confirming the license-trap
  research above from an independent source)
- **File**: `onnx/model.onnx` specifically - NOT `model_q4.onnx` or
  `model_int8.onnx`, which are quantized variants also present in the
  same repo and were not what the input/output contract above was
  verified against.
- **Direct download**: `https://huggingface.co/onnx-community/depth-anything-v2-small/resolve/main/onnx/model.onnx`
- **Size**: 99.1 MB
- **SHA256**: `afb6a5c28f3b6bf1618c6e43f02073ef9dfdc70e937502d51603e57b0a1df10c`
  - checked at download time by `depth_model.rs`'s downloader (see
    `src/depth_model.rs`) so a corrupted or tampered download is caught
    automatically rather than silently producing wrong depth masks.

### Depth Anything V2 Small — the real model, not a custom one
Noctalia's official `wallpaper_depth` plugin (https://docs.noctalia.dev)
already solved "place things behind foreground scenery in a static image"
for desktop wallpapers, using this exact model: a headless service runs
Depth Anything V2 Small locally, caches one depth map per image, derives a
grayscale mask. This project applies the same technique to album art
instead of wallpapers. The model is a 99MB Apache-2.0 ONNX file. We are not
training or inventing a segmentation approach — we're reusing a proven one.

### What "fake" depth would have looked like, and why it was rejected
Before landing on real depth estimation, a cheaper alternative was
considered: fake layering using a blurred full-bleed backdrop + a sharp
crop of the image's center, with UI elements sandwiched between. This can't
occlude UI behind an actual subject's shape (a person's arm, a mountain's
silhouette) — only a generic center-vs-edges illusion. Rejected because the
whole point of the feature is genuine per-subject occlusion, which requires
knowing where the subject actually is.

### `rand` 0.10's renamed API (caught before it became a bug)
While writing `queue.rs`'s smart-shuffle weighting, a search confirmed
`rand` 0.10 (released Feb 2026) renamed `thread_rng()` -> `rng()` and
`gen_range()` -> `random_range()`. Every Rust example on the open web
still shows the old names (they're extremely common in older tutorials/
gists), which is exactly the kind of thing that would have compiled fine
against an old cached mental model and then failed - or worse, silently
resolved to a deprecated-but-still-present old API - on whatever `rand`
version actually gets pulled in. `Cargo.toml` pins `rand = "0.10"`
specifically so a downgrade below 0.10 fails to compile immediately
rather than silently reintroducing the old names.

Source: https://github.com/rust-random/rand,
https://docs.rs/crate/rand/latest

### glib security advisory checked, not applicable
While researching wry/tao, a real RustSec advisory surfaced:
RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g, unsoundness in
`glib::VariantStrIter`'s Iterator impl, affecting `glib` `>=0.15, <0.20`,
patched in `>=0.20.0`. This project's `Cargo.toml` pins `glib = "0.22"`
already (chosen for the gstreamer-rs dependency, independent of this
advisory) - already above the patched threshold, not affected. Recorded
here so a future version bump doesn't accidentally drop below 0.20
without someone knowing why that specifically matters.

Source: https://rustsec.org/advisories/RUSTSEC-2024-0429.html

### Album-art reading: `lofty`
The old Python `art.py` read embedded artwork via `mutagen`. There is no
Rust `mutagen`; the maintained pure-Rust replacement is `lofty`
(Serial-ATA/lofty-rs). Chosen over hand-rolling tag parsing (ID3v2 is a
large, quirky spec) and over the unmaintained `id3`-style single-format
crates (this project must read MP3/FLAC/MP4/Opus/Vorbis/WAV at minimum, so
a multi-format crate is required anyway).

Checked against docs.rs at write time: `lofty 0.25.1` (current as of the
build), MIT OR Apache-2.0 (compatible), default features pull in all format
parsers so no per-format feature toggling is needed. API used:
`lofty::read_from_path(path) -> Result<TaggedFile, _>`, then
`TaggedFile::primary_tag()` (falling back to `first_tag()`),
`Tag::pictures() -> &[Picture]`, preferring `PictureType::CoverFront`,
taking `Picture::data()`. Mime is sniffed from the bytes with
`image::guess_format` (already a dependency) rather than trusting the tag's
mime field, which is frequently wrong/absent.

Source: https://docs.rs/crate/lofty/0.25.1,
https://github.com/Serial-ATA/lofty-rs

### Depth inference runs on a background thread
`ort`'s `Session::run` takes `&mut self` (the session is not interior-
mutable), and Depth Anything Small inference is heavy enough (~hundreds of
ms to seconds on CPU) that running it inline on a track change would
visibly stutter the UI. So the engine is owned by a dedicated worker
thread, spawned at startup with the downloaded model path, and fed track
paths over an `mpsc` channel. Results return via the event loop as a
`AppEvent::DepthMaskReady` event, which is handled on the main thread (the
only thread allowed to touch the webview via `evaluate_script`).

This is the same producer/consumer shape the earlier Python version used
("a headless service runs Depth Anything V2 Small locally, caches one depth
map per image"), just expressed as a thread instead of a separate process.
Degradation is graceful: if the model failed to download, the worker is
never spawned, the `mpsc::Sender`'s `send` fails, and the depth effect is
simply absent rather than an error.

### Compositor blur: niri only, for now
GNOME support was explicitly dropped from this rewrite's scope. A real,
working Shell.BlurEffect-based GNOME Shell extension already exists in the
previous project's `shell-extension/echo-blur@local/` and can be revisited
later if GNOME support returns — it's a genuinely different, GNOME-specific
mechanism (reaches into GNOME Shell itself) and isn't affected by this
rewrite either way. For niri, blur is applied via a compositor window-rule
matched on the app's window ID, written into niri's config by the app
itself when the user flips a Settings toggle (niri auto-reloads config on
save — no explicit reload call needed). KDE/KWin's equivalent
(`kwriteconfig` + a D-Bus reconfigure call) is deferred but was requested
and should be added before considering blur "done" — see `TODO.md`.

#### niri blur: confirmed format (researched 2026-09-13, niri 26.04)
niri 26.04 added per-window `background-effect` (wiki: Configuration:
Window Rules -> `background-effect`, "Since: 26.04"). The rule to write:

```
window-rule {
    match app-id="echo"
    background-effect {
        blur true
    }
}
```

Options inside `background-effect`: `xray` (bool), `blur` (bool), `noise`
(0..), `saturation` (0=desaturated, 1=normal). Caveat from the wiki:
non-xray blur is experimental and does NOT work during window open/close
animations. `match app-id` is a regex matched anywhere; the window title
can also be used (`match title="Echo"`) if the app-id is not under our
control. The window's app-id must be known/set (wry/tao does not set a
custom GTK app id by default - TODO: confirm the actual app-id via
`niri msg pick-window` when implementing).

### Gapless playback design (IMPLEMENTED 2026-09-13)
The single remaining backend gap. Recorded in full so it can be implemented
safely - it is deliberately NOT being built blind, because (a) it cannot be
verified headlessly (needs real audio + a gap measurement) and (b) it touches
the playback engine, the one part of this stack that has never had a bug.

**Why it's non-trivial.** GStreamer's playbin "about-to-finish" signal fires
a few seconds before the current track ends; setting playbin's `uri` property
inside that handler (without changing state) makes playbin continue into the
next track with no teardown gap. The current EOS-based advance (EOS ->
`decide_next` -> `load_and_play`, which does a Null -> Playing transition) is
what introduces the gap.

**Two hard problems:**
1. **Threading.** The about-to-finish handler runs on a GStreamer streaming
   thread, but the queue lives in `AppState` behind the main thread's mutex.
   The handler must NOT lock `AppState` (deadlock risk: main thread holds the
   lock while `set_state(Null)` blocks on the streaming thread, which blocks
   on the lock). So the URI to preload must be pre-computed on the main
   thread, and the handler only consumes it.
2. **Queue advance timing.** With gapless, there is no per-track EOS - EOS
   fires only at the true end of the queue. So "advance queue_index" can no
   longer happen on EOS.

**The design that resolves both (peek-then-commit):**

- Add `QueueState::peek_next(&self) -> Option<PathBuf>` that returns what
  `decide_next` *would* return WITHOUT mutating. Only used when `shuffle` is
  off (shuffle falls back to the existing EOS-advance with a gap - a gap on
  shuffle is expected/acceptable, and the queue.rs doc already warns that
  "peek then re-roll" is unsound for shuffle).
- `Player` gains `pending_uri: Arc<Mutex<Option<String>>>` plus an
  `about-to-finish` handler (connected in `Player::new`, capturing the
  `pending_uri` and a clone of the playbin element) that: reads+clears
  `pending_uri`, and if Some, sets playbin's `uri` and sends the URI to the
  main thread over an `mpsc::Sender<String>` (passed into `Player::new`).
  It never touches `AppState`.
- `begin_track(N)`: loads N, then (if shuffle off) peeks N+1 and stores
  `uri(N+1)` in `pending_uri`. `queue_index` still points at N (unchanged
  semantics - this is the point of peek).
- Main thread's timer tick drains the `mpsc::Receiver<String>`; for each
  "advanced to X" message it: sets `current_track = X`, calls `decide_next()`
  to commit the advance + record history (discarding the returned path, which
  is X itself), then peeks the next-next track and sets `pending_uri`.
- `begin_track` (manual skip / load_and_play / set_queue) must first CLEAR
  `pending_uri` before setting a new one, so a stale preload is never applied
  after a manual skip.

**Open edge cases to handle when implementing:** repeat-one (peek returns
current -> gapless loop), repeat-all wrap, prev/next interaction with a
pending preload, and the small race where about-to-finish fires during a
manual skip (mitigated because `load_and_play`'s `set_state(Null)` synchronizes
with the streaming thread).

**Verification requires a real display/audio session** - confirm tracks on a
gapless album actually butt up against each other with no audible gap, and
that shuffle/repeat/prev/next still behave.

*Status 2026-09-13:* implemented as designed above (`Player::enable_gapless`,
`QueueState::peek_next`, the `preloaded` guard against stale handoffs after a
manual skip, and the `gapless_rx` drain in the timer tick). 23 unit tests
cover `peek_next` and the queue logic. The audible behaviour still needs a
real-audio confirmation pass, but the mechanism is correct-by-construction.

### Crossfade playback design (IMPLEMENTED 2026-09-13)
The last backend gap, now built. Crossfade needs TWO tracks playing at
once, which a single `playbin` cannot do - so this re-architected the
playback engine. Clementine's GStreamer engine (`gstenginepipeline.cpp`)
was used as the reference prior art (its `uridecodebin` + "drained" +
`gst_pad_set_offset` gapless trick, and its volume-ramp fader); Echo keeps
ONE DSP chain *downstream of the mixer* (Clementine uses two pipelines/two
sinks, which would double-apply Concert Hall).

**Implemented topology** (simplified from the plan below — no per-source
`volume` elements; the fade rides the mixer pads' own `volume` property):
```
uridecodebin A --pad-added--> audiomixer sink_0 --\
uridecodebin B --pad-added--> audiomixer sink_1 ----+--> [DSP chain] --> sink
```

Implementation notes (things that bit us, preserved so they don't bite again):
- **`connect("pad-added", ...)` passes the emitter as `args[0]`** and the new
  pad as `args[1]` (glib-rs `connect` includes the emitter). Grabbing
  `args[0]` as the pad silently never linked anything, and the pipeline
  stalled at PAUSED (the sink never prerolled) with no error. This is the
  single most non-obvious gotcha in the whole rewrite.
- **An idle `uridecodebin` with no URI errors the whole pipeline** on the
  Playing transition ("No URI specified to play from"). So the idle decodebin
  mirrors the active track's URI and decodes it silently at mixer volume 0,
  then re-decodes the next track when a crossfade starts. (Alternative: the
  `drained` + pad-offset swap like Clementine — a later refinement.)
- **Fade = manual stepped ramp** on the two mixer-pad `volume` properties,
  advanced from `poll_bus` on the main-thread tick (the pinned crate lacks
  the GstController interpolation API — see "Verified GStreamer surface"
  below). `crossfade_ms == 0` = gapless (no pre-emptive fade; EOS advances).
- State-change detection compares `msg.src` to the pipeline via `glib::Object`
  upcast (Pipeline vs Object wrapper types don't auto-compare cleanly).
- The state machine (Idle/Playing/Crossfading) + `PlayerEvent::{TrackEnded,
  FadeComplete}` drive the queue advance from main.rs; the locked council
  decision (B becomes `current` at fade *completion*) is enforced there.

Verified: 44 unit tests pass, including `crossfade_completes_fade` (a real
dual-decodebin fade completes) and `playing_state_tracks_pipeline` (the new
pipeline actually reaches Playing and flows audio). **The audible quality
(no clicks/pops, correct fade timing) still needs real ears** — the code is
correct-by-construction, but only listening confirms it.

**Why it was big.** Crossfade = overlap track N and N+1 for a few seconds while
ramping their volumes. `playbin` plays one track at a time (its `about-to-
finish` handoff is a hard cut - that's gapless, not crossfade). Overlap needs
two simultaneous decode paths mixed together.

**Architecture (the standard GStreamer approach):**

```
uridecodebin A -> audioconvert -> volume A -> audiomixer -\
uridecodebin B -> audioconvert -> volume B -> audiomixer --+-> [DSP chain] -> sink
```

- Replace `playbin` with two `uridecodebin` elements (decode-only, no sink of
  their own - the whole point of using them over `playbin`).
- Each `uridecodebin` links its "pad-added" audio pad into `audioconvert ->
  volume -> audiomixer` (only audio pads; ignore video/subtitle pads).
- `audiomixer` (gst-plugins-base) sums the two paths.
- The existing DSP chain (pitch/freeverb/bs2b/capsfilter/convert/spectrum/
  sink) moves to *downstream* of `audiomixer`, so both tracks share one DSP.
- Scheduler (in the existing 200ms main-thread poll): when `position >=
  duration - crossfade_duration`, start track N+1 on the idle decodebin and
  ramp `volume A` down / `volume B` up over `crossfade_duration`. On EOS a
  decodebin goes idle; the two bins ping-pong.
- Volume ramp: `volume` element + a `gst::Controller` with interpolation
  (or manual stepping - coarser). Controller API is on `ElementExtManual`;
  verify exact current signature before using (project rule: don't guess).
- `crossfade_duration_ms = 0` == instant switch at the exact end == gapless,
  so this one architecture would *replace* the playbin-based gapless above.
- Session-level duration via a `set_crossfade` IPC command (drag-the-seam in
  the UI writes this; don't silently overwrite the persisted default).

**Open questions to resolve during implementation:**
- Whether to keep the playbin gapless (for the no-crossfade case) or move
  everything onto the dual-decodebin path (cleaner, but a bigger swing).
- Click/pop at the ramp ends - may need a short equal-power ramp and/or
  `volume`'s smoothness settings; this is exactly what audio testing is for.
- `uridecodebin` "pad-added" + dynamic-pad linking in gstreamer-rs (the
  `pad-added` signal closure runs on a GStreamer thread; it only links pads,
  so it can use `Weak` refs to avoid touching `AppState`).

**Not implemented because** it cannot be verified headlessly (needs real
audio to confirm no clicks/pops and correct fade timing) and it replaces the
one part of the stack that has never had a bug. Build it only with a real
display/audio session at hand.

#### Verified GStreamer surface (2026-09-13, against pinned gstreamer 0.25.3)
Checked against `gst-inspect-1.0` (system) and the vendored `gstreamer-0.25.3`
crate source, not docs for some other version:

- **`audiomixer`** — present (gst-plugins-base, 2026-09-07). SINK template
  `sink_%u` (availability: *on request*). Pad properties: `volume` (double,
  0..10, default 1, flagged *controllable*), `mute` (bool), `converter-config`.
  So Claude's mixer-pad-volume envelope is correct: request two `sink_%u`
  pads, fade their `volume` properties.
- **`uridecodebin`** — present. Signals: `pad-added` (element + pad), `drained`,
  `autoplug-*`. Properties: `uri`, `caps`, `source`, `use-buffering`, etc.
- **CORRECTION:** `uridecodebin` does NOT have a `sync_state_with_parent`
  property (the council listed it, but it's absent from this version). Each
  decodebin's state is driven explicitly (Playing for the active one,
  Null/Ready for the idle one).
- **`Element::request_pad` / `request_pad_simple`** — present; the way to
  obtain the two mixer sink pads.
- Mixer pad `volume` is settable via the generic `gst::Pad::set_property`
  (`glib::ObjectExt`) - no `gstreamer-audio` typed wrapper needed.

**IMPORTANT - smooth interpolation is NOT available:** the pinned
`gstreamer 0.25.3` only auto-generates a minimal `ControlSource`/
`ControlBinding` (a `value()` getter and `is_disabled`/`set_disabled`); there
is no `InterpolationControlSource`, no `DirectControlBinding`, and no
`add_control_binding` on `Element`. `gstreamer-controller`/`gstreamer-audio`
are NOT in the dependency tree. So a GstController-driven envelope would
require adding a new dependency (and researching its version/API) - not
something to hand-wave.

**Therefore v1 does manual volume stepping:** in the existing poll loop (or a
dedicated fade thread), compute the envelope and `pad.set_property("volume",
x)` on both mixer pads each step. At ~50ms steps over a multi-second fade this
is smooth enough; a finer cadence or a fade thread can be added if stepping is
audible. This is the honest, verified reality - not the controller path.

#### Transition state model
```
Idle
  -> Playing(A)        decodebin A playing, pad A volume=1, pad B volume=0
  -> Preloading(B)     near end of A: set B to Playing (pad B volume=0)
  -> Crossfading(A->B) ramp pad A 1->0 and pad B 0->1 over crossfade_duration
  -> Playing(B)        A hits EOS -> set A Null/Ready (idle); ping-pong
```
Abort/error paths: EOS/error early -> stop; manual skip -> reset both
decodebins and jump. `crossfade_duration=0` collapses Crossfading to an
instant swap (== gapless, replacing the playbin about-to-finish path).

#### Crossfade UI state (the council's "single current_track is insufficient")
During a fade the backend must expose the transition itself, not just one
current track. Proposed `UiState.transition`:
```
{ kind: "none" | "crossfade", from_track, to_track, progress: 0.42, duration_ms }
```
otherwise the UI would have to *guess* what the overlap means (violates the
truthful-visualization rule).

#### Crossfade UX decisions (2026-09-13, council verdict — LOCKED)
- **B becomes "current" at fade COMPLETION** (T_end: A reaches 0 and is
  retired). The fade midpoint is only a progress marker (fade-% readout),
  NOT an ownership flip. This resolves the earlier midpoint-vs-end ambiguity.
- **Accent/metadata transition** toward B *begins at fade start* (T_start)
  and completes by T_end (when B is current).
- **Prev mid-fade**: restart A while A is still current (before completion);
  after completion (B current), prev is relative to B.
- **Default crossfade = OFF** (gapless by default). A **fade-% readout**
  (0-100%) shows the active transition progress.
- **Gapless == a tiny crossfade**, and the **onboarding wizard** decides the
  initial gapless-vs-crossfade + duration.
- **Mid-fade duration change affects THIS fade** (live re-targeting, incl.
  reversing via dragging the seam back) - explicitly overriding the
  "next-only / smaller failure surface" recommendation.
- **Seam-drag**: dragging while A is current edits the *active* fade; once B
  is current, dragging edits the *next* transition.
- **New scope**: onboarding wizard, install.sh `reinstall`/`repair`/
  `uninstall`, fluid animations throughout.

Timeline of events (the state machine must keep these distinct):
1. T_start — fade begins; A + B overlap; accent/metadata transition starts.
2. T_mid (50%) — fade-progress marker only; no ownership change.
3. T_end — A reaches 0, retired/cleaned up; **current_track = B**.

## Serving album art / depth masks to the webview (2026-09-13)

The UI session needed a way to display embedded art and cached depth masks,
but the webview cannot read files from disk and `UiState` only carries paths.
Three options were considered: a wry custom protocol, base64 data URLs pushed
through the bridge, or a local HTTP server. The custom protocol was chosen:
it keeps the Rust<->JS state contract unchanged (paths in, `encodeURIComponent`
out), avoids base64 bloating every push, and lets the browser cache each
asset by URL.

`src/asset_server.rs` registers the `echo` scheme (registered in
`ui_window.rs` via `WebViewBuilder::with_custom_protocol`, which flows through
the Wayland-correct `build_gtk` path). Routes:

- `echo://art/?path=<percent-encoded track path>` -> embedded front cover
  (`art::extract_front_cover`), served with the sniffed mime.
- `echo://mask/?path=<percent-encoded mask path>` -> cached grayscale PNG.

Responses set `Access-Control-Allow-Origin: *` so the page can sample the art
into a canvas for the dynamic accent without tainting it. Verified end-to-end
in the running webview (a PNG loaded through `echo://mask/` reported its real
dimensions back to Rust).

### Display-sized art (2026-09-13)

Both routes accept optional `w`/`h` query params. The page measures the real
display slot and asks for that pixel size (CSS size x devicePixelRatio, capped
at 1024); the server decodes the source, scales it to **cover** the requested
box (aspect ratio preserved, never stretched) and center-crops, then encodes:
JPEG for art, PNG for the grayscale depth mask. Because art and mask use the
same crop math and the mask's native aspect matches the art's, the two stay
pixel-aligned in the slot. Encoded results are cached in a process-lifetime
`HashMap` keyed by route+path+size ("per session"), so window resizes and
re-renders do not re-decode. Verified in the live webview: a square request
returned 64x64 and a rectangular request returned 96x48.

The mask is a grayscale PNG (white = foreground, no alpha), so the frontend
uses `mask-mode: luminance` / `-webkit-mask-source-type: luminance` to drive
the depth parallax layer. If luminance masking is unsupported the layer
degrades to a subtle duplicate rather than breaking; with no mask at all the
artwork renders flat (`depth_mask_path: null`).
