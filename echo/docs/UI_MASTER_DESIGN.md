# Echo UI / UX Master Design Direction
## DeepSeek implementation handoff

> **Purpose:** This document consolidates the current Echo UI direction, the accumulated brainstorm, the visual/analytics concepts, and the playback-transition visualization into one clear handoff for an AI coding agent.
>
> **Source hierarchy:** `DESIGN_DIRECTION.md` contains confirmed aesthetic direction. `FEATURE_IDEAS.md` contains the broader brainstorm and explicitly says those ideas are logged rather than committed. This document preserves that distinction while expanding the ideas enough to make future implementation decisions coherent.

---

# 1. Product-level intent

Echo is not intended to be a generic music player with a pretty skin.

The goal is a **local-first music application whose interface makes the underlying playback system, library, media characteristics, and visual identity feel visible and understandable**.

The interface should therefore do three things simultaneously:

1. Be beautiful and immersive — with **depth-reactive artwork as a core, non-optional part of that immersion** (see §5).
2. Be practical and fast for everyday music browsing/playback.
3. Expose useful technical information and playback behavior without turning the whole app into a sterile engineering dashboard.

A recurring principle is:

> **Do not merely tell the user what is happening when the interface can make the behavior visible.**

Examples:
- Instead of only showing `Crossfade: 4s`, visualize the two tracks overlapping.
- Instead of only saying `96 kHz`, expose the negotiated audio path when useful.
- Instead of merely displaying album artwork, let it have subtle depth and motion.
- Instead of hiding smart-shuffle reasoning, expose why a track was selected.
- Instead of showing an opaque error toast, visually identify the affected component.

---

# 2. Confirmed visual language

## 2.1 Overall aesthetic

The core aesthetic is:

- Dark glassmorphic dashboard.
- Frosted/translucent panels.
- Rounded corners.
- Data-dense cards.
- One coherent glowing accent language.
- Deliberately varied card sizes.
- Strong visual hierarchy.
- Large visual focal areas mixed with compact information cards.
- Thin connector lines and subtle grid details where they help create structure.
- Full-bleed album imagery can sit behind or beneath translucent UI surfaces.

This is **not** a single now-playing page. The required high-level product structure is a **dashboard-centric application** with multiple functional areas.

The first generic uniform-grid concept was rejected because it felt bland and generic. Echo should instead use **off-grid composition**: cards may have intentionally different widths/heights and may form asymmetric groupings while still aligning to a coherent spacing system.

"Off-grid" means **controlled variation**, not random placement.

## 2.2 Brand and motion

The current brand direction uses a dark squircle-like mark with a stylized `E` made from stacked/slanted bars, with a glossy violet/blue gradient and subtle bevel/3D character. Reference material also used pink/magenta progress treatment. These literal colors are **not** a fixed palette specification.

**Progressive blur** is a reusable signature motion motif. It can be used for track changes, panel reveals, album-art transitions, depth-mask reveals, and other moments of state change. Blur should communicate movement, depth, or focus rather than being added everywhere.

---

# 3. Accent-color system

Echo should not be locked to one hard-coded accent.

Modes:
- **Static** — stable Echo/brand accent.
- **Dynamic** — accent extracted from current album art.
- **Dynamic + animated** — restrained animation/variation.
- **Custom** — manual override.

Custom controls can include presets plus a color wheel/eyedropper directly on artwork.

The intended visual relationship is:

`album art -> extracted palette -> accent/glow -> related ambient background`

This should feel cohesive, not flashy.

---

# 4. Ambient artwork system

The current artwork can influence the environment around the player.

Generate a soft blurred ambient background from the artwork. When tracks change, the palette should **cross-fade rather than hard-cut**. Frosted panels must keep text and controls readable above it.

The effect should feel like the application is visually responding to the current music without turning the whole interface into a visualizer.

---

# 5. Depth-reactive album artwork

> **Depth is a core pillar of Echo's identity, not a bonus effect.** The
> player must feel immersive by default; the depth layer is what makes the
> artwork feel *present* and alive rather than a flat image in a box. Treat
> depth as load-bearing to the aesthetic, on par with the glass and the
> accent — it is one of the primary reasons Echo looks the way it does.

The existing Depth Anything infrastructure enables **cinematic album
artwork**, not a cheap floating-3D gimmick.

Depth should be present everywhere artwork appears:
- The Now Playing hero should *always* attempt a depth pass on current art.
- Ambient/background art and the fullscreen immersion view should reuse the
  same mask (computed once, cached per track — see `src/pipeline.rs`).
- The rolling timeline's track bars can carry small depth-masked art thumbs.

Possible behavior:
- Subtle mouse parallax.
- Layered motion derived from the depth mask.
- Very small playback-linked movement.
- Smooth camera-like transitions.
- Integration with fullscreen immersion mode.

Graceful fallback is mandatory (depth must degrade, not break):
- No valid depth mask -> flat artwork still looks complete and polished.
- While a mask resolves -> use a tasteful blur/reveal state rather than a
  broken/loading-looking artifact. The reveal (mask "resolving from blur")
  is itself part of the motion language.

---

# 6. Dashboard

The dashboard is required and is a first-class page, not a decorative landing screen.

It should be modular and customizable.

## Candidate cards

### Now Playing hero
Large focal card containing:
- Artwork.
- Track / artist / album.
- Position.
- Format/quality badges when the data actually exists (for example FLAC, 24-bit, 96 kHz, Lossless).
- Active processing badges such as Concert Hall and Depth.
- Primary transport controls.

### Continue Listening
Remember playback positions per item.

### Quick Resume
Resume an album, playlist, folder, or last session.

### Listening Session
Useful local session statistics such as duration, tracks/albums/artists encountered, and most-played item in the current session.

### Queue Preview
Show upcoming queue entries directly on the dashboard. Tracks should be draggable from the card.

### Smart Shuffle explanation
Expose the actual reason a smart-shuffle decision happened, such as "never played before", "same genre", "underplayed", or another real reason implemented by the algorithm. Do not invent explanations.

### Library Pulse
Track count, album count, artist count, total size, recently-added activity.

### Library Health
Actionable indicators for missing files, duplicates, missing artwork, broken lyrics, metadata inconsistencies, and stale depth/cache entries.

### Audio Quality Overview
Lossless/lossy, format, sample-rate, and bit-depth breakdowns. Where practical, charts are clickable library filters.

### Listening Heatmap
Local-only day/hour activity grid.

---

# 7. Dashboard customization

The dashboard should eventually support:
- Show/hide cards.
- Reorder cards.
- Resize cards.
- Saved named layouts.

Possible presets include `Minimal`, `Default`, `Power User`, `Audio Nerd`, and `Immersion`.

Persist dashboard layout across restarts.

The composition should remain visually interesting: intentionally varied card sizes are part of the identity, not a temporary mockup trick.

---

# 8. Library browsing: view lenses

Provide a persistent top-level switcher for:

- Folder.
- Albums.
- Artists.
- Songs.
- Genres.
- Years.
- Formats.
- Recently Added.

## Folder view
Folder mode represents the **real filesystem hierarchy**, not a metadata-derived hierarchy.

## View-lens principle
Search/filter state should persist when switching lenses.

Example:

`Ado + 2022-2026` -> Albums -> Artists -> Folder

The same filter context remains active and meaningful in each view.

The views are therefore **different lenses over the same filtered collection**, not isolated pages.

---

# 9. Music Map and identity inspection

## Music Map

An advanced view can show an expandable relationship graph:

`Artist -> Album -> Track -> physical file`

The graph should expose real relationships and avoid decorative "spaghetti" connections.

## Split by reality

A track has three useful identities:

### Metadata identity
Artist, album, title, genre, year, etc.

### Filesystem identity
Physical path, folder, filename.

### Technical identity
Format/codec, sample rate, bit depth, channels, bitrate, lossless/lossy status, and other real media properties.

Echo should let users move between these identities rather than collapsing everything into one opaque "song."

## File inspection actions

A track inspection/reveal surface can provide:
- Physical path.
- Format.
- Sample rate.
- Bit depth.
- Channels.
- Relevant metadata.
- Codec/container/bitrate when available.

Actions:
- Open containing folder.
- Copy path.
- Inspect metadata.
- Reveal the file in a filesystem-oriented view.

---

# 10. Smart folders / saved views

Support future live rule-based views such as:

- Genre = Electronic.
- Format = FLAC.
- Year >= 2020.
- Artist contains X.
- Lossless only.
- Sample rate >= 96 kHz.

These are **queries/views**, not file duplication into playlists.

---

# 11. Search, command palette, and interaction model

## Global search

Search across songs, albums, artists, files, and lyrics, with categorized result counts.

## Command palette

A keyboard-oriented surface such as `Ctrl+K` can expose actions like:

- Play/pause.
- Enable Concert Hall.
- Shuffle this album.
- Reveal file.
- Scan library.
- Toggle visualization.
- Open diagnostics.
- Enter immersion mode.

## Keyboard shortcuts

Eventually support a broad, user-remappable shortcut set. Examples from the brainstorm: Space, arrows, N/P/S, Ctrl+K, Ctrl+L.

## Universal drag and drop

Behavior depends on target:
- Queue -> append.
- Now Playing -> play now.
- Playlist -> add.
- Folder -> move/copy only with explicit safe semantics.

Middle-click-to-queue can be supported for album/artist/folder surfaces.

---

# 12. Queue as a structured workspace

The queue should not have to be a single undifferentiated list.

Potential collapsible sections:
- Current album.
- Manually added tracks.
- Smart-queue additions.
- Other generated sections.

Dashboard queue preview should share the same underlying state so the user is never looking at a fake summary.

---

# 13. SIGNATURE FEATURE: rolling playback timeline

## Why it exists

Echo should make the relationship between **current track, next track, queue timing, gapless playback, and crossfade** visible.

The timeline is one of the most important UI concepts in this document because it is a place where visualization and direct manipulation meet.

Imagine:

```text
TRACK 111111111111111111111111111111
                         22222222222222222222222222
                         <--- overlap --->
```

The actual UI should be a **duration-proportional horizontal timeline**, not an abstract queue list.

## 13.1 Core behavior

Represent upcoming tracks as horizontal bars whose lengths correspond to their durations.

At minimum show:
- Current track.
- Next track.
- Following track(s) when space allows.

The current track contains a live progress fill.

As time advances, the strip rolls forward. Completed material slides away; the next track moves into the current slot; later tracks enter the visible window.

This should feel like a live transport timeline.

## 13.2 Gapless

For true gapless playback, bars **butt directly against one another**:

```text
TRACK 1 ------------------------------|
                                      |TRACK 2 --------------------
```

There should be no visual blank space at the seam. The exact handoff point becomes visible.

## 13.3 Crossfade

For crossfade, tracks overlap:

```text
TRACK 1 --------------------------
                         █████████████
                              █████████████████ TRACK 2
                              <--- overlap --->
```

The overlap zone can be shaded/blended to communicate that two tracks are simultaneously active during the transition.

The visualization must represent **real playback state**, not a fake decorative overlap.

## 13.4 Direct manipulation: drag the seam

This is specifically desired:

> **The user can drag the transition/overlap boundary to adjust crossfade duration for the current listening session.**

Example:

Before:

```text
TRACK 1 --------------------------------|
                                      |<-- 4s -->|
                                             TRACK 2 -----------
```

After dragging the seam earlier:

```text
TRACK 1 --------------------------|
                                |<------ 8s ------>|
                                      TRACK 2 -----------
```

The intended interaction:

1. User grabs the visible transition seam/handle.
2. Dragging changes the crossfade duration.
3. A small inline label shows the value (`4.0 s`, `8.0 s`, etc.).
4. The timeline redraws immediately.
5. The Rust playback engine receives the session value.
6. The next transition uses the new value.

**Important:** the drag edits the **current session** first. It should not silently overwrite a global default.

Potential secondary affordances:
- `Save as default`.
- `Reset to default`.

The user should be able to experiment with transitions without opening Settings.

## 13.5 Crossfade curve editor

The broader crossfade editor may support:
- Equal-power.
- Linear.
- Custom curves.

Example conceptual curve:

```text
Volume
1.0 |██████████████\
    |              \
0.5 |               \____
    |                    \____
0.0 +--------------------------
       transition time ->
```

The first implementation should be simple; direct curve editing can come later.

## 13.6 Track-change motion

When current becomes next:
- Current bar rolls away.
- Next bar slides into the now-playing position.
- Following track enters behind it.

Progressive blur can be used here as a signature motion motif, but keep the transition readable and restrained.

## 13.7 Interaction

Potentially support:
- Click a track bar to jump.
- Drag to scrub.
- Drag the transition seam to adjust crossfade.
- Hover for precise timestamps.
- Album-art/depth thumbnail on track bars.
- Track/artist label when space allows.

Long track titles and tiny-duration tracks must degrade gracefully.

## 13.8 Why this feature is important

It visually ties together:
- Queue state.
- Current/next state.
- Track duration.
- Playback position.
- Preloading.
- EOS/about-to-finish logic.
- Gapless playback.
- Crossfade.

It turns a hidden playback-engine concept into an understandable and manipulable surface.

## 13.9 Fade percentage and reversible seam control

The transition visualization now has an explicit **fade-% indicator**. It represents the real active transition state, not merely cursor position. `0%` means the transition has just begun; `100%` means the outgoing side has reached the configured fade completion point. The indicator can appear beside the seam or as a compact contextual chip while dragging.

The seam is a **live control**. While A is still the logical/current track, dragging the seam edits the **active fade**. Moving the seam earlier or later retargets the current transition, and dragging back can reverse/re-expand the target. The UI must update the geometry and fade-% state from authoritative backend state.

Once B becomes the logical/current track, the active A→B transition is no longer edited by that seam. Dragging then edits the **next transition**.

The current product decision therefore differs from a conservative "next transition only" policy: **mid-fade duration changes intentionally affect the current fade**. This is a required interaction, not optional polish.

### Reversible transition invariants

The implementation must explicitly guard against:
- Overshooting the target fade endpoint.
- Applying an old ramp after a new seam position has been chosen.
- Starting the same transition twice.
- A ending while the target is being moved backward.
- B becoming current at the wrong point.
- UI progress disagreeing with actual mixer-pad volumes.

The Rust state machine remains authoritative. The UI never reconstructs transition state from animation timing alone.

## 13.10 Transition identity timing (RESOLVED)

The council verdict (2026-09-13) locks the timing:

- **Fade start (T_start):** metadata/artwork/accent transition toward B begins.
- **Fade midpoint (50%):** fade-progress marker only — **no ownership change**.
- **Fade completion (T_end):** A reaches 0 and is retired/cleaned up; **B becomes the logical/current track**.

The earlier midpoint-vs-end wording conflict is resolved: the three events (visual transition start, logical current-track commit, fade completion/cleanup) are distinct, and the current-track commit happens at **completion**.

---

# 14. Audio-nerd / DSP interfaces

## Audio Forensics

A per-track panel may expose:
- Peak.
- RMS.
- ReplayGain.
- Clipping detection.
- Dynamic range.
- Leading/trailing silence.
- Waveform/loudness/spectrum representation.

## Playback Chain

Show the real live DSP graph:

`Decoder -> Resampler -> Mono/Stereo -> Concert Hall -> Crossfeed -> Volume -> Output`

Clicking a stage can reveal its actual parameters. For Concert Hall this may include room size, damping, wet level, and relevant crossfeed values.

Inactive stages should visibly show that they are inactive.

## A/B comparison

Allow instant comparison of processing states at the **same playback position**, e.g. Concert Hall on/off or crossfeed on/off. The purpose is honest listening comparison rather than memory.

---

# 15. Audio diagnostics

## PipeWire/output inspector

Potential information:
- Output device.
- Negotiated sample rate.
- Format.
- Volume/mute.

Useful question it should answer:

> "Why isn't my audio doing 96 kHz?"

Expose source, processing, and output facts separately so resampling can be understood.

## End-to-end diagnosis

A visualization can highlight suspicious stages:

```text
Source FLAC 96k
      |
   Decoder
      |
Resampler 96k -> 48k   !
      |
 Concert Hall
      |
  PipeWire
      |
  Output
```

## MPRIS inspector

Potential diagnostics:
- Bus name.
- Playback status.
- Position.
- Metadata.
- Supported capabilities.

## Real-time telemetry

Potential panel for:
- Decoder state.
- Input/output format.
- Resampling state.
- Active effects.
- Peak/LUFS.
- Buffer health.

---

# 16. Visualizers and graphs

Echo should have a family of visual modes rather than one permanent spectrum.

Potential modes:
- Spectrum bars.
- Waveform.
- Spectrogram.
- Circular spectrum.
- Mirrored spectrum.
- Hybrid.

Potential controls:
- Smoothing.
- Decay.
- Sensitivity.
- Band count.

## Loudness graph

Show integrated and/or short-term LUFS with peak over time.

## Interactive waveform

Use the waveform as a multi-purpose timeline with:
- Playback position.
- Loudness.
- Bookmarks.
- Lyric timestamps.
- Silence detection.
- Clipping markers.

## Dynamic-range graph

Optional advanced panel for DR information. Do not imply a single score completely defines perceived quality.

## Spectrogram / frequency heatmap

Scrolling frequency energy over time, useful both visually and analytically.

## Library analytics

Small clickable visualizations can expose:
- Format distribution.
- Sample-rate distribution.
- Bit-depth distribution.

Clicking a slice/bar can filter Browse to the relevant tracks.

---

# 17. Fullscreen Immersion Mode

`F11` can become a chrome-free mode focused on:
- Album art.
- Depth.
- Spectrum.
- Waveform.
- Lyrics.
- Minimal controls.

Controls appear on mouse movement.

The visual priority becomes music and artwork rather than dashboard chrome.

---

# 18. Subtle reactive motion

A restrained "breathing" accent animation may respond loosely to music energy.

Important constraint:

> **Not a full-UI rave pulse.**

Animation should be subtle enough that reading and interacting remain comfortable.

---

# 19. Appearance settings

Eventually expose a consolidated Appearance section:

### Theme
- Dark.
- Light.
- System.
- OLED.

### Accent
- Static.
- Dynamic.
- Custom.

### Motion
- Full.
- Reduced.
- Minimal.

### Transitions
- Instant.
- Smooth.
- Cinematic.

### Album Art
- Flat.
- Depth.
- Parallax.
- Reactive.

### Visualization
- Off.
- Spectrum.
- Waveform.
- Spectrogram.
- Hybrid.

---

# 20. First-run onboarding wizard

Echo should include a short first-run onboarding flow that establishes the most important user-level preferences without turning setup into a settings marathon.

## 20.1 Playback transition choice

The first-run flow should ask whether the user wants:
- **Gapless** playback / no crossfade.
- **Crossfade**, with an initial duration.

The product default remains **crossfade off / gapless** at the configuration level, while onboarding may explicitly set the user's initial preference.

The wording should explain the behavior rather than expose implementation details:
- Gapless = tracks meet without intentional overlap.
- Crossfade = the ending of one track overlaps the beginning of the next.

## 20.2 Appearance setup

Include concise choices for:
- Theme: Dark / Light / System / OLED.
- Accent: Static / Dynamic / Custom.
- Motion: Full / Reduced / Minimal.

The default Echo presentation remains the dark glassmorphic aesthetic. Dynamic album-art-derived accents remain available.

## 20.3 Library setup

Ask for the user's local music/library folder as part of first-run setup. The purpose is to establish the initial scan target and make Echo usable immediately. The folder can be changed later from settings.

## 20.4 Onboarding rules

- Keep the flow short and visual.
- Do not demand unnecessary configuration before first playback.
- Make every choice reversible from Settings.
- Respect reduced-motion choices immediately.
- Persist choices as real application settings/state, not UI-only state.

---

# 20.5 Fluid animation system

Fluid animation is part of Echo's visual language, especially for the fade/seam, accent changes, track changes, album-art transitions, dashboard reveals, and progressive-blur transitions.

The implementation should distinguish:
- **Continuous state movement:** live playback progress, seam dragging, fade percentage.
- **Discrete state changes:** A→B becoming the current track, queue changes, theme changes.

Animations must never fabricate backend state. A purely visual pointer crossing must not cause the UI to claim that a track has changed before the backend has committed that state.

All motion must respect the configured Motion and Transition accessibility preferences.

---

# 21. Light-mode flashbang

Switching Dark -> Light may optionally trigger a deliberate bright flash/bloom.

This is an intentional fun feature, not an accidental white-flash bug.

Desired sequence:
1. User changes Dark -> Light.
2. White flash/bloom occurs.
3. Flash is synchronized with the bundled sound cue where that asset is present.
4. The UI settles from white into the normal light theme through a smooth fade.

The project notes specify the flash duration should follow the reference audio clip and that the sound is intended to live at `assets/sfx/flashbang.mp3`.

Accessibility settings must include:
- Flashbang.
- Gentle.
- Instant.
- Reduced motion.

Reduced-motion users should have a safe non-flashing option.

This is gated on the real UI existing and the asset being bundled.

---

# 22. Library maintenance and scan UX

## Duplicate detector

Distinguish:
- Exact duplicate files.
- Same audio in different files.
- Same metadata but different encodings.

## Orphan detector

Examples:
- `.lrc` without matching audio.
- Artwork without matching album.
- Playlist entries pointing to missing files.
- Stale depth-mask cache entries.

## Metadata conflict detector

Identify problems such as inconsistent Album Artist across one release and offer a one-click normalize action with preview.

## Incremental scan

Index changed files instead of doing a full rescan every time.

After a scan show a concise `What changed?` report:
- New tracks.
- Metadata changes.
- Missing files.
- Duplicates.
- Other actionable findings.

---

# 23. Lyrics UX

Potential features:
- Auto-scroll.
- Click-to-seek.
- Offset adjustment.
- Font/line-spacing control.
- Fullscreen lyrics.
- Follow-playback toggle.
- Saving corrected timing.

---

# 24. Metadata safety / undo

Metadata editing should use **preview-before-write**.

Destructive operations should support undo where feasible:
- Metadata edits.
- Deletions.
- Moves.
- Playlist changes.

Irreversible actions should feel deliberate.

---

# 25. Mini-player and compact modes

A compact always-on-top mode can preserve the same visual language while focusing on:
- Artwork.
- Track.
- Essential controls.
- Progress.
- Compact queue/transition indicator where space permits.

It should feel like Echo compressed, not like a separate application.

---

# 26. Local-first and portable library

A future portable mode may store a library's Echo database/metadata under a `.echo/` directory on a portable drive so the collection can travel with its data.

The concept should remain local-first; no cloud backend should be assumed.

---

# 27. Notes and bookmarks

Possible track-level features:

### Notes
Free-text notes attached to tracks.

### Bookmarks
Multiple named timestamps inside a track.

Bookmarks are different from playlists:
- Playlist = what to play.
- Bookmark = a location inside a track.

---

# 28. Folder-first actions and local recommendations

Folders should be first-class objects with actions such as:
- Play.
- Shuffle.
- Queue.
- Scan.
- Open in Browse.

A future `Play similar from my library` feature may use only local tracks and signals such as artist, album, genre, tempo, history, and folder proximity.

Do not quietly introduce a cloud recommendation dependency.

---

# 29. Persistent UI state

Useful state to preserve may include:
- Browse mode.
- Sidebar width.
- Selected folder.
- Dashboard layout.
- Lyrics state.
- Volume.
- Window size.
- Last playback position.
- Visualization mode.
- Appearance preferences.

Do not blindly persist arbitrary transient runtime data.

---

# 30. Installer lifecycle UX

`install.sh` should eventually support maintenance subcommands in addition to first-time installation:

- `reinstall` — reinstall application components while preserving user data where appropriate.
- `repair` — restore missing/broken installation artifacts or dependencies without unnecessarily resetting user library/configuration.
- `uninstall` — remove installed application components with explicit indication of what application files versus user data will be removed.

The operations should be deterministic, documented, and explicit about destructive effects. This is part of the expanded Echo product scope even though it is not a dashboard feature.

---

# 31. Error-state design

Errors should be contextual rather than generic toast spam.

Preferred behavior:
- Highlight the affected UI element.
- Use a small contextual glow/animation if appropriate.
- Keep useful text close to the affected feature.
- Offer a route to deeper diagnostics/logs.

Example: a failed depth mask should affect the artwork card's state, not make the entire player look broken.

---

# 32. Information hierarchy

Use progressive disclosure:

### Everyday layer
Track, artist, position, essential controls.

### Advanced layer
Format, processing badges, transition information, visual analysis.

### Diagnostic layer
Exact DSP graph, MPRIS, PipeWire, buffer state, internal details.

Echo can be technically deep without making every normal playback screen look like a laboratory.

---

# 33. Visualization should be truthful

No fake technicality.

Do not:
- Display a fake 96 kHz output value.
- Claim gapless playback without backend guarantees.
- Draw a crossfade overlap that does not match actual playback timing.
- Claim a depth effect exists when no valid mask exists.

Every technical visualization should be backed by real application state.

This is especially important for the rolling playback timeline: **the UI must be a view of the playback engine, not a mock animation pretending to be the engine.**

---

# 34. Graceful degradation

Not every machine will have every GStreamer plugin, every visualization feature, depth inference capability, or identical audio-output integration.

The UI should fall back cleanly:
- Flat artwork if depth unavailable.
- Static accent if extraction fails.
- No Concert Hall badge if the actual effect is unavailable.
- Clear `Unavailable`/`Fallback` status rather than fabricated data.

---

# 35. Accessibility

Account for:
- Reduced motion.
- Minimal motion.
- Avoiding forced flashing.
- Keyboard navigation.
- Readability over album art.
- Clear focus states.
- Text alternatives/tooltips for icon-only controls.
- State communication that does not depend only on color.

The Flashbang feature is especially important here: it must be optional and safe to disable.

---

# 36. Implementation sequencing

This is a roadmap, not a promise that every brainstorm item must ship.

## Phase A - playback correctness
Before sophisticated UI, stabilize:
- Bus handling.
- EOS handling.
- Track state.
- Queue IPC.
- Reliable current/next state.
- Gapless behavior.
- Crossfade semantics.

## Phase B - core UI
Build:
- Dashboard.
- Sidebar.
- Browse/lenses.
- Now Playing.
- Queue.
- Search.

## Phase C - signature playback timeline
Implement the rolling timeline once:
- Queue state exists.
- Durations are known.
- Current position is reliable.
- Next-track state is reliable.
- Gapless/crossfade semantics are real.

Then connect the drag interaction to **current-session crossfade duration**.

## Phase D - artwork
- Artwork extraction.
- Ambient blur.
- Dynamic accent.
- Depth mask.
- Depth-reactive artwork.

## Phase E - visual analytics
- Spectrum.
- Waveform.
- Spectrogram.
- Loudness.
- Dynamic range.
- Audio Forensics.

## Phase F - diagnostics
- Playback Chain.
- PipeWire inspector.
- MPRIS inspector.
- End-to-end audio diagnostics.

## Phase G - polish
- Progressive blur.
- Track-change transitions.
- Queue motion.
- Flashbang theme transition.
- Immersion mode.
- Persistent customization.

The sequencing exists to keep UI ambition from destabilizing a backend that is still changing.

---

# 37. Do not make these mistakes

## Generic dashboard syndrome
Do not make every card identical in dimensions and hierarchy.

## Decorative-data syndrome
Do not add charts without trustworthy data.

## Animation overload
Do not make the entire UI continuously pulse/glow/bounce.

## Fake technical status
Do not display guessed backend values.

## Overloaded now-playing page
Dashboard architecture should remain central; do not reduce Echo to one huge artwork screen.

## Settings-only interaction
Where a value is naturally visualized, allow direct manipulation instead of forcing the user into Settings.

## Cloud-first assumptions
Echo should remain local-first and must not assume online recommendation/library infrastructure.

---

# 38. Locked crossfade decisions (council verdict, 2026-09-13)

These are the current product direction and override earlier speculative suggestions where they conflict. They do **not** mean the backend already implements them.

- B becomes the logical/current track at **fade completion** (A reaches 0 and is retired).
- Metadata/artwork/accent transition toward B begins at **fade start**.
- Previous during the fade restarts A while A remains current; after B becomes current, Previous is relative to B.
- Default crossfade is **off / gapless**, although onboarding can establish the user's initial value.
- The timeline displays a **fade-% indicator**.
- While A is current, dragging the seam edits the **active fade**.
- Active seam edits may move the target in either direction; live re-targeting is intentional.
- Once B is current, the seam controls the **next transition** instead.
- The product explicitly includes a first-run onboarding wizard covering playback transition choice/duration, theme/accent, motion, and library folder.
- `install.sh` gains `reinstall`, `repair`, and `uninstall` maintenance operations.

Timing is locked (see 13.10): visual transition starts at T_start, current-track commit at T_end, fade completion/cleanup at T_end.

---

# 39. Agent rules for DeepSeek

When implementing anything from this document:

1. **Do not assume every brainstorm feature is already committed.** This file consolidates direction and ideas; current project status/TODO/spec controls what is actually in progress.
2. **Check what backend state really exists before designing UI around it.** Never invent state to satisfy a visual mockup.
3. **Prefer real Rust -> UI data.** The playback timeline, diagnostics, and technical cards must reflect actual state.
4. **Document nontrivial decisions.** Use `docs/ARCHITECTURE_DECISIONS.md` for reasoning, version-specific API research, and unresolved uncertainty.
5. **Keep `docs/TODO.md` honest.** If a feature is not implemented and verified, it does not exist yet.
6. **Build/test after meaningful backend changes.** Avoid accumulating a large stack of speculative edits.
7. **Do not guess unstable APIs.** Verify current version-specific docs/source when necessary.
8. **Preserve graceful degradation.** Missing capabilities should produce explicit fallbacks.
9. **Keep UI behavior explainable.** Especially smart shuffle, transitions, audio pipeline, visual analysis, and depth inference.
10. **Prefer coherent integration over feature count.** A small set of deeply connected features is better than many disconnected widgets.
11. **Respect the pinned GStreamer surface.** The current verified environment does not expose `sync_state_with_parent` on `uridecodebin`, and the pinned Rust crate does not expose the smooth `GstController` interpolation path. Do not code against either stale assumption.
12. **Crossfade v1 uses verified capabilities only.** Manual mixer-pad volume stepping is the current verified fallback for fade control; any interpolation/controller redesign is future work unless the pinned dependency surface is changed and verified.
13. **Preserve explicit transition state.** The UI must receive authoritative state for active transition, fade percentage, current/next identity, and session crossfade duration.

---

# 40. North-star interaction example

Suppose the queue is:

`Track 1 -> Track 2 -> Track 3`

The timeline visibly shows something like:

```text
TRACK 1
██████████████████████████████████
                         █████████████
                              TRACK 2
                         <--- overlap --->

                                          █████████████████████
                                                    TRACK 3
```

The user drags the overlap boundary.

`Crossfade: 4.0 s` becomes `Crossfade: 8.0 s`.

The Rust playback engine receives the new **session-level** transition duration.

The timeline immediately redraws.

The real transition uses the new setting.

No Settings page was required.

The user **grabbed the thing they could see and changed the thing it represented**.

That interaction is a model for Echo's larger philosophy: visualization should be functional whenever practical.

---

# 41. Final design statement

Echo should feel like:

> **A local music library that happens to have a beautiful interface — except the interface is also a live visualization of what the playback engine, the library, and the music are actually doing.**

The strongest features should create a tight loop:

**see -> understand -> manipulate -> hear/observe the result**

Beauty is not separate from engineering here. The UI is one of the ways Echo exposes the truth of its underlying system.

---

# 42. Short note from ChatGPT

The feature I would protect hardest is the **rolling playback timeline + draggable crossfade seam**.

It connects queue state, real playback state, gapless behavior, crossfade, visualization, and direct manipulation into one feature that is both beautiful and genuinely useful.

It feels much more "Echo" than another generic settings panel.

Build the playback truth first. Then make the UI expose it beautifully.
