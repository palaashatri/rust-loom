# Pointer and Pen

Pointer targeting, cursors, and pen-input goals for Loom.

## 1. Targeting

* **Minimum interactive target: 44 × 44 px** (logical px at 1.0 scale) of
  effective hit area for every pointer target — buttons, handles, sliders,
  list rows, tabs, scrollbar thumbs.
* Effective area = visual area + invisible grace zones: a 32 px toolbar
  button gets 6 px grace on each side; a 8 px canvas handle gets 18 px
  grace (48 px effective, per `SELECTION.md` §1).
* Grace zones must not overlap a neighboring target's grace zone in a way
  that makes the wrong target win: when they would overlap, the visual
  target grows or spacing increases (`space-8` minimum between adjacent
  icon buttons).
* Small-but-prevalent objects (8 px handles, 9 px keyframe diamonds) are
  exempt from the 44 px rule for *positioning* but must keep ≥ 24 px
  effective hit areas and work at 1.5× scale; the exemption is documented
  per object type in `COMPONENTS.md`/`TIMELINE.md`.
* Hover activation: click target = the object; hover activation of
  sub-actions (row action buttons) requires the sub-target itself to be
  ≥ 44 px effective and to appear with hover AND focus
  (`ANTI_PATTERNS.md` #hover-only).
* Precision: at high zoom, canvas targets scale with zoom — hit boxes stay
  1:1 with the rendered object, so precision improves naturally; UI chrome
  targets never scale.

## 2. Cursors

The Loom cursor set (original designs, per `ICONOGRAPHY.md` art rules):

| Cursor | Use |
|---|---|
| arrow | Default over chrome and non-interactive canvas |
| text (I-beam) | Text fields, document text, in-cell editing |
| hand-open | Pan available (Space held) |
| hand-grabbing | While panning |
| crosshair | Marquee, draw tools, pen tools |
| move | Dragging objects, reorder |
| resize-e/w/ne-sw/nw-se/ew/ns | Canvas handles, panel edges, row/col resize |
| cell-cross | Spreadsheet grid (over cells) |
| not-allowed | Invalid drop target, disabled action |

Rules: cursors are small (16–20 px), stroke-weight 1.5 px family,
high-contrast variant inverts to white-on-black; cursor hot-spot at the
logical tip; cursors never animate (spinning state cursors prohibited —
use progress UI instead).

## 3. Pointer buttons and modifiers

* Left: select/manipulate. Middle: pan (canvas), autoscroll (document
  surfaces). Right: context menu.
* Shift: constrain (aspect, axis, 15° rotate steps, straight lines).
* Option/Alt: duplicate-on-drag (Photo/Motion), fine-tune (0.1 px nudge,
  scrub ×0.1), alternative drop role (`DRAG_AND_DROP.md` §4).
* Mod: additive selection, zoom out (with zoom tool).
* Wheel: scroll; Shift+wheel horizontal; Mod+wheel zoom (canvas apps);
  plain wheel zooms in Present edit mode only.
* Trackpad: scroll pans, pinch zooms (anchor at pinch center), two-finger
  scroll honors platform natural direction; edge-scroll while dragging
  (rate proportional to distance from edge, 50 px/frame max).

## 4. Pen input (goals)

* Pen support is a `[goal]` across canvas apps (Photo first): hover
  preview (pen-over-canvas shows the tool stroke cursor), pressure maps
  to brush size/opacity per tool, tilt/rotation supported where the
  backend reports them.
* Pen vs mouse: when a pen is in range, the UI shows pen-optimized cursors
  (no resize-cursor conflicts), palm rejection (ignore touches within
  400 ms of pen-down when the platform reports palm events), and the
  drawing tool activates without a click-to-focus step.
* Pen targeting: pen hit areas are the same as pointer (§1); small drawing
  tools scale the stroke preview with pressure, never the hit area.
* Erase/side-switch: pen barrel buttons toggle eraser where the platform
  exposes them; configurable in the pen settings section of Preferences.
* Reduced motion: pen strokes are instantaneous (they ARE the input);
  hover previews fade only (120 ms).
* Verification: pen goals are tracked in `FEATURE_STATUS.md` as
  `[goal]` until a device-backed test exists in CI (`[future]` hardware).
