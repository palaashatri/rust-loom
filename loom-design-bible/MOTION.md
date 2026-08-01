# Motion

Every animation in Loom must answer a usability question: origin, destination,
state change, hierarchy, completion, cancellation, relationship. Motion that
answers no question is decoration and is prohibited.

## 1. Tokens

Durations (ms):

| Token | Value | Use |
|---|---|---|
| `motion-duration-instant` | 0 | Hover on/off, press feedback, focus visibility changes |
| `motion-duration-fast` | 120 | Tiny state changes: checkmarks, icon swaps, toggles, progress pulses |
| `motion-duration-standard` | 200 | **Default**: panel transitions, selection moves, tool state changes |
| `motion-duration-deliberate` | 320 | Larger surfaces: sidebar expand/collapse, dialog fade, drag reorder |
| `motion-duration-slow` | 500 | Emphasis moments only: undo reveal, achievement of a long action; never per-frame UI |

Easings (cubic-bezier):

| Token | Control points | Use |
|---|---|---|
| `motion-easing-out-quad` | (0.33, 1.00, 0.68, 1.00) | **Default**: entrances, exits, state changes — quick start, calm finish |
| `motion-easing-in-out` | (0.65, 0.00, 0.35, 1.00) | Value transitions: progress fills, cross-fades, scrolling to a snap |
| `motion-easing-out-back` | (0.34, 1.56, 0.64, 1.00) | Spring-ish arrival, sparingly: command palette open, panel "landing" |

Rules: 200 ms default, 120 ms minimum for non-instant feedback, 500 ms maximum
for any single interaction animation. No custom durations or easings anywhere
in the suite — bespoke values require an ADR.

## 2. Motion grammar

* **Entrance** (panel opens, palette appears): out-quad, 200 ms; slight
  overshoot via out-back only for the command palette and popover "landing"
  (320 ms). Elements never fly from off-screen.
* **Exit** (panel closes): out-quad, 160 ms — exits are 20% faster than
  entrances (attention is already on the object; exit is confirmatory).
* **State change** (toggle, mode switch, checkmark): fast, 120 ms, usually
  opacity + small scale (1.0 → 1.05 → 1.0) on the state glyph only.
* **Selection** (selection moves across items, marquee completes): standard,
  200 ms, out-quad. The selection outline and overlay animate from the
  previous selection rect to the new one (see `SELECTION.md`).
* **Progress** (job progress, export, render): in-out, fill animations only,
  value changes snap the bar to the new fraction within fast (120 ms) —
  progress never bounces or overshoots, and never loops decoratively.
* **Drag feedback** (object, clip, row): the dragged item scales to 1.02 and
  lifts (opacity 0.95) within 120 ms; it follows the pointer 1:1 with zero
  lag — no easing on drag-follow. Drop landing animates 200 ms out-quad.
* **Reorder** (list row, track order): displaced items slide out-quad 200 ms;
  the dropped item settles 320 ms with a 2 px settle-overshoot via out-back
  only for timeline/arranger reorders.
* **Zoom** (canvas zoom, spreadsheet zoom): zoom follows the pointer; the
  content transform animates in-out 200 ms when triggered by a control, but
  is **instant** while the user is actively zooming (wheel/pinch).

## 3. Interruption

* Every animation is interruptible on the frame it is interrupted: a new
  gesture starts the new animation from the current state — no "finish
  current animation first" delays, no queued animations.
* Hover-based motion (tooltip, preview) is cancellable and never traps the
  pointer.
* Input latency is never sacrificed to finish an animation: if a 120 Hz
  pointer stream arrives during a 200 ms panel animation, the panel snaps to
  the target and input wins.
* Keyboard interaction interrupts animations the same way pointer input does.

## 4. Reduced motion

In reduced-motion mode (user preference, applied automatically and
switchable in settings):

* All translation and scale animations are disabled: panels appear and
  disappear instantly (`motion-duration-instant`), no slides, no overshoot,
  no lifts, no spring.
* Opacity changes remain, capped at `motion-duration-fast` (120 ms): fades
  convey state change without motion.
* Progress continues to animate fill changes at 120 ms (value feedback, not
  decoration).
* Hover feedback is instant color/opacity only.
* Reduced motion is a release gate in every app: verified with a dedicated
  visual-QA pass and automated assertion that no transform-animated element
  translates or scales in this mode.

## 5. Frame-drop policy

* Animations are driven per-frame from the compositor; at 60 Hz target, a
  frame that takes longer than 16.7 ms drops rather than stalling the next
  frame — animation time is wall-clock based, so dropped frames never
  accumulate delay.
* Below 45 fps sustained (e.g. software renderer in CI), animations still
  complete in wall-clock time; the UI never depends on frame count.
* The software-renderer visual QA path uses wall-clock timing and captures
  final states (reduced motion: instant), so baselines are deterministic.

## 6. Motion checklist (every feature)

1. Does the motion answer a usability question? If not — cut it.
2. Which token duration/easing is it? (No bespoke values.)
3. Is it interruptible and input-responsive?
4. What happens in reduced motion? (Default: opacity 120 ms or instant.)
5. Does it scale? (UI text scaling never animates; panel heights adjust
   instantly.)
