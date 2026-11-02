# Discover (Reel)

**Status:** design exploration (prototype: `~/Projects/designs/reel.html`)
**Paradigm:** full-bleed vertical feed — *not* a grid.

## Concept
Discover is a feed, not a catalog. One track owns the whole screen; you scroll
through them like a deck of cards. It is the "lean back" counterpart to Browse's
"lean in" grid.

## Behaviour
- **Full-bleed panels.** Each track is a 100vh panel with its album art as the
  background, a bottom scrim, and large display type (title → artist · album).
- **Snap scrolling.** `scroll-snap-type: y mandatory`; one panel per gesture.
- **Persistent dock.** A floating, translucent control bar (blur) stays fixed:
  thumb, title/artist, prev / play / next, progress, elapsed/total. It never
  scrolls away.
- **Auto-advance.** Playback progress advances the dock; on track end the feed
  scrolls to the next panel.
- **Scroll drives state.** An `IntersectionObserver` (threshold ~0.6) sets the
  current track from whichever panel is centred, and syncs the dock + progress.

## Why a feed
- Album art is the product; give it the whole viewport.
- Discovery is serendipity — a shuffled, immersive stream reads differently than
  a sortable table.
- Zero chrome; the interface recedes behind the music.

## Anatomy
```
┌──────────────────────────────────────────────┐
│  REEL            ● ○ ○ ○ ○ ○                  │  brand + position dots
│                                                │
│                                                │
│              (full-bleed album art)            │
│                                    ▁▃▅▂▇▅▃      │  per-panel equaliser
│  01 / 06                                       │
│  Red Thread                                    │  display type
│  Mira Vale · Forms of Silence                  │
│  [Lossless] [24-bit] [96 kHz]                  │
│                                                │
│      ┌──────────────────────────────────┐      │
│      │ ▨  Red Thread     ⏮  ▶  ⏭  ────  │      │  glass dock (fixed)
│      └──────────────────────────────────┘      │
└──────────────────────────────────────────────┘
```

## Data / IPC
No new backend required:
- art: existing `echo://art/?path=` protocol.
- current track: existing per-tick state push (`current_track`, `position_ms`,
  `duration_ms`, `upcoming`).
- feed source: the queue / smart-shuffle order already exposed.
- advancing a panel = `next` / `prev` (existing commands).

## Open questions
- Feed source: shuffle-by-default, or the current queue order?
- Should panels be tracks only, or mixed (track / album / artist) like a real feed?
- Autoplay vs. manual scroll — respect a "reduced motion / no autoplay" setting.
- Preload policy: only decode art for the centred panel ±1 to avoid the Browse
  perf trap (see `docs/TODO.md`).

## Design notes
- Dark, immersive base; one accent; dock uses a translucent blur — the one place
  blur earns its keep.
- Large, tight display type (Apple/editorial scale), not card labels.
- Tags are pills; keep them few and honest (only render what the backend sent).
