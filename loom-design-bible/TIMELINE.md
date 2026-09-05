# Timeline

The timeline is the primary surface of Motion, Video, and Studio's
arrangement view. This document fixes its anatomy and scrubbing behavior;
per-app details (track types, connect semantics) live in each app's
PRODUCT_SPEC.

## 1. Anatomy

```
┌───────────────┬────────────────────────────────────────────┐
│ track header  │ ruler (24 px)                              │
│ (160 px)      ├────────────────────────────────────────────┤
│               │  track lanes                               │
│               │  ┌────────────────────────────────────┐    │
│               │  │  clip   [keyframe ◆ ◆]             │    │
│               │  └────────────────────────────────────┘    │
│               │  waveform lane (audio clips)               │
└───────────────┴────────────────────────────────────────────┘
```

* **Track header**: 160 px fixed, resizable 96–320 px. Contains track name,
  type glyph, mute/solo/record-arm controls (Studio), lock/visibility
  toggles (Video/Motion), track height control. Header rows align
  ʹ1:1 with lanes; horizontal scrolling of the timeline never detaches the
  header.
* **Ruler**: 24 px strip above lanes; time in tabular figures
  (`type-size-11`, secondary ink), tick density adapts to zoom (major ticks
  with labels, minor ticks unlabeled); timecode format per project
  (SMPTE drop/non-drop for video, bars/beats for Studio, frames for
  Motion — the ruler honors the project timebase, never mixed units).
* **Lanes**: horizontal tracks; clip blocks `radius-4`, raised fill,
  1 px border; selected clip = accent outline (`SELECTION.md` §1 adapted:
  outline stays 2 px, overlay 15%).
* **Playhead**: 1 px accent line, full lane height, triangular head in the
  ruler; current time shown in the ruler at the playhead
  (tabular figures, chip-backed). Playhead is draggable (scrub), reaches
  via keyboard (see §4).
* **Keyframes**: diamonds 9 × 9 px (`color-accent-default` fill, white
  center dot) on their value track; selected keyframe = white fill +
  accent ring; adjacent keyframes connect with hairline value lines only in
  the graph editor view.
* **Waveform lane**: audio clips render a waveform (min/max peaks) at
  `color-ink-primary` 55%, on sunken bed; generated off-thread
  (`loom-core` media contract), cached per clip+zoom, rendered within one
  frame from cache; waveform never blocks scrubbing.
* **Clip edges**: trim handles 6 px at each clip end (drag to trim, ripple
  by default in Video); speed glyph at clip top-right when ≠ 100%.

## 2. Timeline chrome

* Horizontal scrollbar at the bottom of the timeline area (overlay style);
  vertical scrollbar right of the lanes; header and ruler are sticky.
* **Zoom (time)**: `Cmd+Plus/Minus` zooms time centered on the playhead;
  wheel with `Option/Alt` zooms around pointer (anchor-at-pointer rule from
  `CANVAS.md`); range: 1 frame per 2 px to 1 hour per 10 px (per app
  limits).
* **Zoom (track height)**: `Cmd+Shift+Plus/Minus` or drag on header bottom
  edge; per-track height override persists.
* **Auto-scroll**: during playback, the view follows the playhead; during
  scrubbing, follows with a 10% margin trigger (never fights the pointer:
  if the user drags against the auto-scroll, the pointer wins).
* **Fit**: double-click in the ruler empty area fits the whole sequence
  (or `Cmd+Shift+F`).

## 3. Selection and editing

* Click clip = select; Shift+click = add; drag on empty lane = marquee
  (`SELECTION.md` §5); clicking the lane header selects all clips in that
  track.
* Trim/roll/slip/slide operate per `loom-spec` editing model; feedback:
  ghost of the affected region during the drag (ink at 15% overlay,
  live value readout in the status bar), commit on release — undoable.
* Ripple mode (Video): trimming or deleting ripples downstream; non-ripple
  leaves gaps (indicated with a hatch pattern in the gap, never a
  fake clip).
* Clip snapping: clip edges, playhead, markers, 1 s time divisions —
  snap halo 8 px, accent indicator (`CANVAS.md` §4).
* Track reorder: drag the track header (lift + 1.02 scale, 120 ms;
  displaced tracks slide 200 ms out-quad per `MOTION.md` grammar).
* Keyboard delete: Backspace/Delete deletes selected clips (ripple per mode
  setting); undo restores exactly.

## 4. Scrubbing

* **Pointer scrub**: drag on the ruler or the playhead; time follows the
  pointer 1:1, zero lag, no easing; preview updates at the display rate —
  target 60 fps, acceptable ≥ 24 fps for heavy compositions with an
  explicit "resolution scrub" indicator (a small "HD/1/4" chip) showing
  degraded preview quality during heavy scrubs.
* **Space = play/pause** (`KEYBOARD.md`); spacebar during playback stops
  instantly (no fade-out tail).
* **J/K/L** transport: J = reverse, K = pause, L = forward; J/L hold
  accelerates (1× → 2× → 4× after 700 ms per press repeat, shown as a chip
  near the playhead); Shift+J/L = 1-frame step. JKL works during scrubbing
  and over any focused surface except text fields (where they type).
* **Frame stepping**: arrows with Shift when timeline focused (Left/Right =
  1 frame; Up/Down = previous/next edit point or marker).
* **Audio scrub**: playing region is silence-suppressed only in Studio's
  scrub-chips mode (`[goal]`); default scrub is silent in Video/Motion,
  audible-but-stable in Studio (audio continues at 1× pitch).
* **Reduced motion**: scrubbing is instantaneous by nature (no animation);
  playhead follows frame-exact. No reduced-motion changes needed beyond
  disabling the (optional) playhead travel animation — there is none.

## 5. Markers, ranges, loops

* Markers: diamond flags in the ruler (drag to move, Option+drag to
  duplicate); marker menu (rename, color per marker category with text
  label — never color-only).
* Loop range: drag in the ruler to select a range (accent-tinted band,
  in/out handles); loop toggle (`Cmd+Shift+L`); loop range persists with
  the project.
* Work area (Encode/Video export): distinct band style (hatched), separate
  from loop range; exports honor the work area by default.

## 6. Performance and accessibility

* The timeline is a virtualized view: visible range renders only
  (clips/keyframes/waveforms fetched on demand, cached); 10,000+ clip
  projects stay fluid; nothing on the UI thread except compositing.
* Timeline keyboard operation is complete: arrows navigate, Tab moves
  between clips and controls, every clip action has a shortcut or palette
  entry (`KEYBOARD.md`, `COMMAND_PALETTE.md`).
* Screen reader: timeline exposes clip names/positions via the
  accessible object model; selected clip announces "Clip 3 of 12, 00:02:10 –
  00:05:00".
* At 1.5× text scale the ruler height grows to 32 px; lane heights grow
  proportionally; nothing clips.
