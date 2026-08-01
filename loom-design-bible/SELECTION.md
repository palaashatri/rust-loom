# Selection

Selection is the primary state of direct manipulation: what the user means
by their next action. The visuals and rules below are suite-wide; canvas
apps, grids, and lists adapt the primitives but never invent their own.

## 1. Visual language

* **Canvas selection** (objects, clips, cells, slides): accent outline
  2 px (`border-width-strong`, `color-accent-default`) around the object
  bounds, plus a soft overlay: accent fill at 8% inside the outline for
  filled objects, accent at 15% behind the outline for wireframes.
* **Outline offsets**: 0 px for objects with their own border (snaps to
  bounds); 1 px outward otherwise; the outline never obscures the object's
  own content.
* **Handles**: 8 × 8 px squares (`color-surface-raised` fill, 1 px accent
  border) at the 8 handles of the bounds (4 corners, 4 edges); rotation
  handle above top-center with a 24 px arm. Handles appear on selection and
  are always ≥ 6 px of interactive area (48 px target per
  `POINTER_AND_PEN.md` hit rules — 8 px visual + 20 px grace each side).
* **List/grid selection** (sidebar items, sheets cells, timeline clips):
  row fill accent 15% + accent text; never full accent fill.
* **Focus vs selection**: selection = accent outline/overlay; keyboard focus
  = focus ring (2 px accent ring, 2 px offset). A selected object that is
  not focused shows outline only; the ring appears when the object or its
  handles receive keyboard focus.

## 2. Selection model

* Click selects; Shift+click adds/toggles; drag on empty canvas marquee-selects
  (see §5); Cmd/Ctrl+click toggles without clearing (selection additive);
  clicking empty canvas clears (with undo-able state — selection changes are
  recorded as a discrete command where the app model supports it).
* Parent/group selection: clicking a group member selects the member; double-
  click selects the group (and re-click drills in); pressing Esc selects the
  parent, Esc again clears.
* Locked/hidden objects are excluded from hit-testing; locked objects are
  unselectable but remain visible; a locked object in a selection shows
  lock glyphs on its handles.
* Selection persists across mode switches (select a clip, switch to the
  scissors tool: selection remains, tool applies to it).

## 3. Multi-select

* Multi-selection outline: a single bounding box around all selected
  objects, with per-object outlines at 40% opacity under it; the bounding
  box carries the handles.
* Inspector shows aggregate values: identical properties show the value;
  mixed properties show a dash ("—") and editing applies to all.
* Multi-select ordering: selection order is preserved (Shift+click order)
  and exposed to alignment/distribution commands; the last-clicked object is
  the "anchor" for alignment and scale operations.

## 4. Keyboard selection

* Arrows nudge the selected object(s) 1 px (Shift: 10 px); Option/Alt+arrows
  nudge 0.1 px at high zoom.
* Tab / Shift+Tab moves selection to next/previous sibling (object order);
  arrows also move the caret in text; per-surface key maps in `KEYBOARD.md`.
* Select All (`Cmd+A`), Deselect (Esc or `Cmd+Shift+A`), Invert
  (`Cmd+Shift+I` where supported).
* Keyboard-selected objects announce their name and type via
  accessible-description ("Layer 3, rectangle, selected").

## 5. Marquee

* Drag on empty canvas (with the selection tool) draws a marquee: 1 px
  accent outline, ink 10% fill; live preview of what will be selected
  (objects the marquee intersects highlight at 60% opacity as it grows);
  release commits.
* Intersection rule: an object is selected when the marquee contains its
  bounds center (common default) — for canvas apps, contained-by-marquee for
  shapes (design intent), intersect-for-clips (timeline precedent); the rule
  is fixed per surface in the app's PRODUCT_SPEC.
* Marquee is keyboard-reachable: Shift+arrows extend selection by marquee
  equivalent (`[goal]` in canvas apps; mandatory in Spreadsheet where cell
  range selection uses Shift+arrows natively).

## 6. Selection motion and feedback

* Selection change animates: the outline morphs from previous bounds to new
  bounds, out-quad 200 ms (reduced motion: instant). During a marquee drag,
  outline follows the marquee instantly (no animation while dragging).
* The animation is skipped when many objects (≥ 50) change selection at once
  (batch rule: animate ≤ 50, snap beyond).
* Selection is announced to screen readers as a concise summary ("3 layers
  selected") — announcements are debounced 300 ms to avoid chatter.

## 7. Non-visual selection channels

* Status bar shows the selection summary ("3 layers · 2 shapes, 1 text").
* Inspector reflects the selection per `INSPECTORS.md`.
* Where color-blindness affects selection visibility (accent on accent
  fills), the overlay + outline double-channel (fill + outline + handles)
  keeps selection distinguishable; high-contrast theme uses black outline
  with white handles inside an accent outline.
