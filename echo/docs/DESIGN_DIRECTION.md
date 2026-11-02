# UI Design Direction (confirmed from user)

Source: pasted Claude conversation summary + dribbble links + explicit user
corrections. This is the brief to build against — not brainstorm filler.

## ⚠️ DIRECTION PIVOT (2026-09-13) — supersedes the glassmorphic section below
The user reviewed the built UI and rejected it. Their words: **"current doesn't
look good"**; it reads as **generic / AI-made**, **too empty/spaced out**, AND
**too busy/cluttered** (i.e. poor hierarchy/density — airy where it should be
tight, cluttered where it should be quiet).

**New direction: MINIMAL, DARK BUT DEFINED.**
- Dark base, but **crisp and high-contrast** — clear panel edges, strong type,
  no muddy low-contrast wash.
- **Avoid heavy `backdrop-filter` blur** (it's what made it look generic/washed).
  Prefer solid or lightly-translucent panels with **defined 1px borders**.
- **Minimal** = fewer badges/labels/panels visible at once, deliberate use of
  space, strong hierarchy (progressive disclosure).
- **Personality** required (anti-generic): a real type scale, one decisive
  accent, deliberate asymmetry / a signature detail — not the safe AI default.
- Keep the functional scope (dashboard, now playing, queue, browse, settings,
  onboarding) and — non-negotiable — the existing IPC bridge (see AGENTS.md).

Everything below this line is the *previous* (glassmorphic) direction, kept for
history. Where it conflicts with the pivot, the pivot wins.

## Confirmed aesthetic (PREVIOUS — glassmorphic, now superseded)
- **Dark glassmorphic dashboard**, NOT a single now-playing page.
- Blurred translucent panels (glass).
- **Glowing single accent color** — one accent, not multiple/rainbow.
- Rounded corners.
- Data-dense cards.
- **Deliberate off-grid card sizing** — explicit anti-bland direction.
  The first design attempt was rejected by the user as "too bland and generic";
  the fix is breaking the uniform grid, varied card sizes, distinctive layout.

## Reviewed references (8 screenshots the user pasted; review saved here so it
survives context loss)

**A. Echo brand mark** — dark squircle tile; stylized "E" from 3 stacked slanted
bars. One shot is the progressive-blur transition (E smears into black on the
right). Settled mark: glossy violet→blue gradient, 3D bevel. Player bar below
uses a pink/magenta progress fill. Progressive-blur is a reusable UI transition
motif (track change, panel reveal).

**B. Dashboard layout skeleton** (education dashboard) — strongest layout ref.
Left icon-only sidebar, top search, then modular cards of deliberately VARYING
sizes (big gauge, bar chart, calendar, two list cards, chat card). Dark base,
one acid-lime accent, heavy rounding. This is the off-grid data-dense target.

**C. Card anatomy** (procurement dashboard) — status chip (orange/blue/red) +
label + "N days left" top row, big number, progress bar, mini-timeline, footer
action buttons. Has a faded/unfilled state (shot 2) vs populated (shot 3) —
good model for empty queue / loading depth mask. Bottom row of quick-action
cards. (That ref is light-themed; anatomy transfers, colors don't.)

**D. Glass + glow** (AI travel app) — translucent frosted panels over full-bleed
imagery, pill chips/tabs, glowing violet orb as focal control. The glassmorphic
+ single-glow-accent language.

**E. Mood/hero** (inno100) — soft airy gradient, huge rounded display type,
floating pill labels with thin connector lines, faint diagonal grid lines.
The "not generic" polish layer.

## These are design REFERENCES only
Layout/mood/structure references. The literal colors in the shots (lime green,
violet, pink) are NOT a palette spec — do not copy them as "the accent."

## Synthesized direction
- Brand: dark squircle + gradient "E"; progressive blur = signature motion.
- Layout: education-dashboard modularity + procurement card anatomy, dark glass.
- Signature details: thin connector lines / faint grid, pill chips, frosted
  overlays over art, ONE glowing focal element.

## Accent color — ALREADY RESOLVED in FEATURE_IDEAS.md (not a conflict)
Explicit in `docs/FEATURE_IDEAS.md`:
- "Dynamic accent color extracted from current album art (with a Static /
  Dynamic / Dynamic+animated setting), plus manual override via presets or a
  color-wheel/eyedropper directly on the artwork."
- Appearance settings: Theme (Dark/Light/System/OLED), Accent
  (Static/Dynamic/Custom), Motion, Transitions, Album Art, Visualization.
So: default accent = per-track, dynamic from album art. Static fallback =
brand gradient. There is no single fixed accent to pick from the references.

## Other explicit UI features already logged (FEATURE_IDEAS.md) — respect these
- Light-mode "flashbang" transition (REAL request, not brainstorm): dark→light
  triggers a white flash synced to `assets/sfx/flashbang.mp3`, then fades to the
  light theme. User-configurable (Flashbang / Gentle / Instant / Reduced motion)
  for accessibility.
- Depth-reactive album art: mouse-parallax + subtle motion on the depth mask
  ("cinematic," not a cheap 3D gimmick).
- Modular, rearrangeable/hideable dashboard cards (named presets).
- Album-art-derived ambient blurred background, cross-fading between tracks.

## Required pages
- Dashboard (necessary, per user)

## Prior art in repo
- `docs/FEATURE_IDEAS.md` — full brainstorm (dashboard cards, visuals, etc.)
- `docs/TODO.md` — current build status (real UI NOT started; placeholder
  `ui/dist/index.html` only).

## Note on process
User expects real research before code, decisions documented with sources in
ARCHITECTURE_DECISIONS.md, honest flagging of unverified API calls, and TODO.md
kept accurate. Direct, terse communication style.
