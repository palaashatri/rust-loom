# Accessibility

Accessibility is release-blocking. A feature that fails any requirement in
this document is not complete. This is the single contract all applications
and components implement against.

## 1. Keyboard navigation

* Every control, tool, panel, and command is reachable and operable with
  the keyboard alone — including canvas objects, timeline clips, grid
  cells, and audio regions.
* Tab order is logical (visual reading order): title bar → toolbar →
  document/canvas → sidebars/panels in the order they were opened →
  inspector → status bar controls. Each major region is a Tab group;
  Tab moves between groups, arrows move within a group
  (`KEYBOARD.md` §4).
* No keyboard trap except where a dialog is open (`DIALOGS.md` §4) and in
  explicit modal contexts; Esc always exits the trap.
* Every drag-and-drop has a keyboard path (nudge, cut/paste, menu
  commands) — `DRAG_AND_DROP.md` §5.
* Canvas/timeline/grid keyboard navigation is specified per surface:
  `CANVAS.md` §6, `TIMELINE.md` §6, `SPREADSHEET.md` §3/§7,
  `DOCUMENT_EDITOR.md` §6.

## 2. Focus visibility

* Focus is always visible: 2 px focus ring (`border-width-strong`,
  `color-accent-default`), offset 2 px outside the control bounds
  (`COMPONENTS.md` state model).
* Ring contrast: the ring must hold ≥ 3:1 against every adjacent surface
  in every theme; in the high-contrast theme the ring is black-on-white or
  white-on-black per surface.
* Focus visibility is unconditional: no "focus until mouse is used"
  suppression. Hover states never replace focus states; focus and hover
  states combine (focused control that is also hovered shows both, ring
  wins visually).
* When focus moves, the target is announced (screen reader) and the
  visible ring moves within 120 ms.

## 3. Screen-reader labels

* Every interactive control has an `accessible-description` — a human
  name (not an icon name): "Undo", "Layer 3: rectangle", "Play",
  "Filter clips".
* Grouping: panels and sections expose a group role with a title; toolbar
  groups, property rows, and table headers announce their context.
* Text alternatives for all meaningful graphics (icons carry their
  control's label; charts and previews expose a summary + data table
  where feasible).
* State is announced: checked, disabled, selected, expanded/collapsed,
  value (sliders announce value changes — debounced 120 ms), and
  completion ("Export finished").
* Live regions: errors, progress completion, search results count, and
  status-bar transitions announce without stealing focus; error messages
  use the alert role.
* Custom controls (canvas objects, timeline clips) implement the
  accessible object model of the toolkit with roles mapped per surface;
  see §7.

## 4. Focus order and management

* Focus order is stable: opening a panel places focus on its first
  control; closing returns focus to the opener (dialogs, popovers,
  palettes — `WINDOWS.md`, `DIALOGS.md`).
* The command palette returns focus to the prior element; the inspector
  returns focus to the canvas object it edited when focus was there.
* Popover focus policy per `WINDOWS.md` §4: keyboard-activatable popovers
  take focus; mouse-only conveniences do not steal it.
* Focus never jumps randomly (no focus "helpfully" moving to a spinner);
  background job completion never steals focus.

## 5. High-contrast theme

* A built-in high-contrast theme (true black/white, doubled-contrast
  accents — `COLOR.md` §4) is always available and follows the OS
  high-contrast preference by default; manually switchable.
* In high contrast: all hairlines become solid 1 px white or black;
  selection = white outline on black / black outline on white (never
  accent-only); focus rings are 2 px solid with 2 px offset; icons render
  at full ink; hover = inverted fills (white box + black glyph).
* No feature may depend on theme internals; themes are token swaps
  (`THEMING.md`).

## 6. Text scaling, reduced motion, non-color indicators

* Text scale 1.0/1.25/1.5 (`TYPOGRAPHY.md` §8); at 1.5 no clipping, no
  unusable layouts (visual-QA gate).
* Reduced-motion mode (`MOTION.md` §4): all translation/scale animations
  disabled, opacity only at ≤ 120 ms; applied automatically from the OS
  preference and toggleable.
* Non-color status indicators (`COLOR.md` §6): every status has text/icon/
  shape/position; charts disambiguate series by pattern/dash/label.

## 7. Canvas and media accessibility

* Canvas objects expose: name, type, bounds, role, and actionable
  commands (select, move, resize, edit text) to assistive technology —
  mapped to the platform accessibility tree.
* Timeline: clips are navigable objects with timecode names; transport is
  keyboard-driven (Space/J/K/L); waveform/thumbnail channels are
  decorative for AT (the timecode and metadata carry meaning).
* Spreadsheet: cells announce "A1: 42" with row/column headers; ranges
  announce start/end; formulas announce their formula text in edit mode
  (`SPREADSHEET.md` §7).
* Charts: every chart exposes a data summary ("Bar chart: sales by
  quarter, Q1 12k, Q2 18k…") and keyboard navigation of data points with
  value announcement; scopes (video) expose numeric readouts as text.
* Document text: caret/selection/composition reported per
  `DOCUMENT_EDITOR.md` §6.
* Media playback: play/pause/mute/volume all keyboard-reachable; closed
  captions are first-class text content.

## 8. Errors and announcements

* Errors announce via live regions with the alert role, in plain language
  (`NOTIFICATIONS.md` §4 shape), and repeat the actionable path.
* Validation errors focus the offending control and describe the fix
  ("End date is before start date — swap them").
* Toast announcements are polite; error announcements are assertive only
  for fatal conditions.
* All announcements respect reduced-motion (announce once, no re-announce
  loops) and are not throttled into silence during rapid typing
  (debounce 300 ms, never drop the final state).

## 9. Verification gates (every app, every release)

1. Full keyboard walkthrough of every feature (scripted checklist per
   `UX_ACCEPTANCE_CHECKLIST.md`).
2. Focus ring visible test: automated screenshots at each focus stop.
3. Screen-reader pass with at least one major desktop screen reader on
   Linux (Orca); label lint: every control has
   `accessible-description` (CI assertion).
4. High-contrast visual QA pass (`VISUAL_QA.md`).
5. Text-scale 1.5× layout stress pass (screenshots at 1280 × 800 × 1.5).
6. Reduced-motion pass: assert no translation/scale animations active.
7. Contrast verification: CI computes WCAG contrast for every
   token-on-token pair in use (`COLOR.md` §7).

A gate failure is release-blocking regardless of feature completeness.
