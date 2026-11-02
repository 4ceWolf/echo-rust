# Feature Ideas — Logged, Not Committed

Everything in this file came from an external audit's brainstorm
documents (pasted into the project chat), covering browsing/library
views, dashboard design, QoL features, and graphs/visual analytics.
**None of this is a spec.** Nothing here is scheduled, prioritized, or
promised. It's logged so the ideas aren't lost, to be picked from
deliberately once the backend actually works end-to-end and real UI
exists to build on top of - see TODO.md for what that gate actually is.

Organized by theme, not by priority. Many of these overlap or would need
to be merged/simplified in practice - that's a design pass for later,
not something to resolve here.

## Library browsing / "view lenses"
- Switchable Browse views: Folder / Albums / Artists / Songs / Genres /
  Years / Formats / Recently Added, as a persistent top-of-page switcher.
- Folder view = literal filesystem hierarchy, not metadata-derived.
- "View lens" concept: current search/filter state persists across view
  switches (filter by "Ado, 2022-2026" then flip Album/Artist/Folder
  view and see it reflected in each).
- Interactive "Music Map": Artist -> Album -> Track -> physical file as
  an expandable node graph.
- "Reveal in library" / right-click a track for full technical +
  filesystem metadata (format, sample rate, bit depth, physical path),
  with "open folder" / "copy path" / "inspect metadata" actions.
- Smart folders / "Saved Views": live filtered views defined by rules
  (genre=Electronic, format=FLAC, year>=2020, etc.) without duplicating
  files into a playlist.
- "Split by reality": explicitly let the user switch between a track's
  metadata identity, filesystem identity, and technical identity.

## Dashboard
- Modular, rearrangeable/hideable dashboard cards (not one fixed layout).
- Now Playing as a large "hero card" with depth-reactive artwork, format
  badges (FLAC/24-bit/96kHz/Lossless), Concert Hall/Depth status badges.
- "Continue Listening" shelf that remembers playback position per item.
- "Quick Resume" (album/playlist/folder/last session).
- Live "Listening Session" card (duration, track/artist/album counts,
  most-played-this-session).
- Queue preview card on the dashboard itself (drag tracks directly from
  it, not just from a separate queue page).
- "Smart Shuffle explanation" card: transparent reasoning for why a
  track was picked ("never played before", "same genre", etc.) rather
  than opaque randomness.
- "Library Pulse" (track/album/artist counts, total size, last added).
- "Library Health" (missing files, duplicates, inconsistent metadata,
  missing artwork, broken lyric files) as an actionable dashboard card.
- "Audio Quality Overview" (format/lossless-vs-lossy breakdown by %,
  clickable to filter the library).
- "Listening Heatmap" (local-only, day/hour grid of listening activity).
- Dashboard customization UI (toggle/reorder/resize cards; named presets
  like "Minimal" vs "Power User").

## QoL / interaction
- Global search across songs+albums+artists+files+lyrics, with
  categorized result counts.
- Command palette (Ctrl+K): `play`, `enable concert hall`, `shuffle this
  album`, `reveal file`, `scan library`, etc.
- Full keyboard shortcut set (space, arrows, N/P/S, Ctrl+K, Ctrl+L),
  user-remappable.
- Mini-player / compact always-on-top mode.
- Lyrics QoL: auto-scroll, click-to-seek, offset adjustment, font/line
  spacing, fullscreen lyrics, "follow playback" toggle, save corrected
  timing.
- Metadata fixer with a preview-before-write step.
- Incremental/live library scanning (index only changed files, not a
  full rescan every time).
- "What changed?" summary after a scan (new tracks, metadata changes,
  missing files, duplicates found).
- Undo for destructive actions (metadata edits, deletions, moves,
  playlist changes).
- Right-click any folder: play/shuffle/queue/scan/open-in-browse as
  first-class actions, not just a compatibility fallback.
- Queue as a structured workspace: collapsible sections (current album /
  manually added / smart-queue additions) instead of one flat list.
- "Play similar from my library" using only locally-owned tracks (no
  cloud), based on artist/album/genre/tempo/history/folder proximity.
- Middle-click-to-queue everywhere (album/artist/folder).
- Universal drag-and-drop with different behavior per drop target
  (queue=append, now-playing=play-now, playlist=add, folder=move/copy).
- Persistent UI state across restarts (browse mode, sidebar width,
  selected folder, dashboard layout, lyrics state, volume, window size,
  last position).
- Portable/relocatable library database (point Echo at a USB drive's own
  `.echo/` folder and take the whole library+metadata with you).
- Per-track free-text notes.
- Track-level bookmarks (multiple named timestamps within one song,
  distinct from playlists).

## Audio-nerd / DSP-adjacent
- Crossfade editor with a draggable overlap curve (equal-power/linear/
  custom).
- Gapless-album auto-detection, disabling anything that would introduce
  an audible gap on detected gapless albums.
- ReplayGain / album-gain visualizer showing target vs. actual.
- A/B comparison tool: instantly toggle Concert Hall / crossfeed / other
  DSP on-or-off at the exact same playback position, to actually compare
  rather than remembering what you heard before.
- "Audio Forensics" per-track panel: peak, RMS, ReplayGain, clipping
  detection, dynamic range, leading/trailing silence detection, with
  visual waveform+loudness+spectrum representation.
- "Playback Chain" visualization showing the live DSP graph (decoder ->
  resampler -> mono/stereo -> Concert Hall -> crossfeed -> volume ->
  output) with per-stage detail views (e.g. clicking "Concert Hall" shows
  its live room-size/damping/wet/crossfeed levels as bars).
- Native MPRIS inspector (bus name, playback status, position, metadata/
  controls capability) as a diagnostics panel.
- PipeWire output inspector (device, negotiated sample rate/format,
  volume/mute state) - explicitly framed as "why isn't my audio doing
  96kHz" troubleshooting.
- End-to-end "diagnose audio" pipeline visualization highlighting
  anything suspicious (e.g. an unwanted 96->48kHz resample).
- Real-time playback telemetry panel (decoder, input/output format,
  resampling status, active effects, peak/LUFS, buffer health).

## Library maintenance
- Duplicate detector distinguishing exact-duplicate / same-audio-
  different-file / same-metadata-different-encoding.
- Orphan detector (`.lrc` with no matching audio, art with no matching
  album, playlist entries pointing at deleted files, stale depth-mask
  cache entries).
- Metadata conflict detector (e.g. inconsistent Album Artist across a
  release's tracks) with a one-click normalize action.

## Visuals / graphs / "eye candy"
- Live spectrum in multiple render modes (bars, waveform, spectrogram,
  circular, mirrored) with user-adjustable smoothing/decay/sensitivity/
  band count.
- Track loudness graph over time (integrated/short-term LUFS + peak).
- Interactive waveform showing position, loudness, bookmarks, lyric
  timestamps, detected silence, and clipping directly on the waveform.
- Dynamic range graph/score (DR value) as an optional advanced panel.
- Scrolling frequency-heatmap/spectrogram view.
- Album-art-derived ambient blurred background, cross-fading smoothly
  between tracks' palettes rather than cutting.
- Dynamic accent color extracted from current album art (with a
  Static / Dynamic / Dynamic+animated setting), plus manual override via
  presets or a color-wheel/eyedropper directly on the artwork.
- Depth-reactive album art: mouse-parallax and subtle motion tied to
  playback, built on the existing Depth Anything infrastructure -
  described as "cinematic album artwork," not a cheap 3D gimmick.
- Fullscreen "Immersion Mode" (F11): chrome-free, art+depth+spectrum+
  waveform+lyrics only, controls appearing only on mouse movement.
- Subtle "breathing" accent-color animation tied loosely to music energy
  (explicitly NOT a full-UI rave-pulse - very restrained).
- Small library-wide analytics visualizations: format breakdown, sample
  rate breakdown, bit depth breakdown, all clickable to filter.
- A full "Appearance" settings section: Theme (Dark/Light/System/OLED),
  Accent (Static/Dynamic/Custom), Motion (Full/Reduced/Minimal),
  Transitions (Instant/Smooth/Cinematic), Album Art (Flat/Depth/
  Parallax/Reactive), Visualization (Off/Spectrum/Waveform/Spectrogram/
  Hybrid).
- Small polish-level motion ideas: animated track-change art/palette
  transition, queue-insertion animation, a "favorite" micro-animation,
  scan-progress animation, depth-mask "resolving from blur" reveal
  animation, and error states that animate/glow the affected UI element
  in place instead of a generic toast.

## Light-mode "flashbang" transition (explicitly requested, separate from
the rest of this list - real request, not brainstorm filler)
Switching Dark -> Light triggers a deliberate bright white flash/bloom
before settling into the light theme, as a fun/intentional moment rather
than an accident. Specific parameters given:
- Duration tied to a specific reference audio clip
  (`think-fast-chucklenuts_3ATncZo.mp3`, uploaded to the project) - the
  flash should last exactly as long as that clip. Bundled into the
  project at `assets/sfx/flashbang.mp3`.
- After the flash, fade from full white down to the theme's normal white/
  light background (not an instant cut).
- Must be user-configurable (Settings -> Appearance -> Light transition:
  Flashbang / Gentle / Instant / Reduced motion), so it doesn't become an
  accessibility problem for photosensitive users.
- Blocked on real UI existing at all (currently only a placeholder HTML
  page) and on bundling the reference audio file into the project.

## Playback timeline / "rolling tracks" strip (gapless + crossfade viz)
A horizontal strip visualizing the play sequence as duration-proportional
track bars that "roll" forward as playback progresses:
- Current track bar with a live progress fill; after it, the next track bar,
  then the following one(s), sliding into view as time passes.
- Gapless: adjacent bars butt up against each other with NO gap, so the exact
  handoff point (where track N ends and N+1 begins) is visible.
- Crossfade: the two bars overlap in the fade region, with a shaded/blended
  overlap zone showing how much they crossfade.
- Track changes become a visible "roll" (current bar slides off, the next
  slides into the now-playing slot) instead of an instant swap.
- Clickable/scrubable (click a bar to jump); ideally each bar's head carries
  the album-art/depth thumbnail.
- Ties directly into the planned gapless work (ARCHITECTURE_DECISIONS.md
  "Gapless playback design") and the crossfade-editor idea - this is the
  visualization that makes those two features legible. Needs a queue length
  > 1 and (for the seam) gapless/crossfade implemented first.

## Explicitly NOT logged here
Anything that was already tracked as a real, scoped, in-progress item in
TODO.md (EOS handling, queue IPC wiring, depth engine integration, the
Cargo.lock gap, etc.) stays in TODO.md, not here - this file is only for
the speculative brainstorm material.
