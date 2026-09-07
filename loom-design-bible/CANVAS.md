# Canvas

The canvas is where the work happens: pages, images, timelines' previews,
slides, compositions. This document fixes the chrome and interaction around
it; surface-specific rules live with the surface (`TIMELINE.md`,
`SPREADSHEET.md`, `DOCUMENT_EDITOR.md`).

## 1. Canvas chrome

* The canvas fills the space between sidebar, toolbar, and status bar
  (`LAYOUT.md`). It has no default chrome of its own beyond:
* **Rulers** (`[goal]` v1, `[future]` full): optional top/left rulers
  (toggle `Cmd+R`), 20 px strips, tick marks in tabular figures at
  `space-8`-aligned intervals; rulers are hidden by default in canvas apps
  (Photo: always hidden; Writer: visible in page mode; Motion/Video:
  hidden — time ruler lives in the timeline).
* **Scrollbars**: overlay-style scrollbars (auto-hide) in canvas apps;
  always-visible thin scrollbars (8 px) in document/grid surfaces;
  scrollbar color: ink at 30% on hover 45%; keyboard scrolling via arrows
  with Shift=fast (`KEYBOARD.md`).
* **Zoom readout**: status bar right (`LAYOUT.md` §7), tabular figures,
  click opens zoom menu; `Cmd+0` fit, `Cmd+1` 100%, `Cmd+Plus/Minus` step.

## 2. Zoom

* Range: **25% – 800%**, stepping: 25/33/50/66/75/100/125/150/200/300/400/
  600/800%; free zoom in between allowed via pinch/wheel, snapped readout
  shows the actual value.
* Zoom anchored at the pointer (wheel/pinch zooms toward the cursor);
  control-driven zoom (menu, buttons) anchors at the canvas center.
* Zoom animation: 200 ms in-out when triggered by controls; **instant while
  actively zooming** (wheel/pinch) — never animate behind an active gesture.
* Zoom is a content property: zoom state persists per document; text-scale
  (1.0/1.25/1.5) is separate and never affects canvas zoom
  (`TYPOGRAPHY.md` §8).
* At any zoom, minimum readable canvas feedback: panning is smooth at 60 fps;
  at zoom < 50% large objects may render simplified only if visually
  equivalent (no detail flaking).

## 3. Pan

* Space+drag or middle-mouse drag pans; two-finger scroll pans (canvas apps
  map scroll to pan when the document fits, scroll-to-zoom with modifier);
  scroll wheel scrolls (vertical/horizontal per platform), Shift+wheel
  horizontal.
* Hand tool (`H`) pans; double-click hand tool fits content.
* Panning is instant, 1:1, no easing; inertia is `[future]` (off by default
  when added — predictable panning is the baseline).
* Bounds: content may be panned freely; a grid dot pattern
  (`color-ink-primary` at 5%) fills the canvas beyond content so emptiness
  is legible; the pattern only appears when content is smaller than the
  viewport.

## 4. Guides and snapping

* Guides: drag from rulers (when visible); vertical/horizontal lines,
  `color-accent-default` at 60% opacity, 1 px; guides are per-document,
  movable, deletable (drag off-canvas), lockable.
* Snapping: toggles — snap to guides, snap to grid, snap to object bounds,
  snap to center/midlines (per app default documented in PRODUCT_SPEC:
  Photo/Motion default on; Writer page-flow does not snap).
* Snap feedback: during a drag, a snap indicator line (accent, 1 px) shows
  the snapped alignment plus a 40 px snap halo around the pointer; snap
  activates within 8 px (halo), snaps instantly (no magnetic animation).
* Grid: `space-16` default grid (documented per app), dots at 6% ink, togglable.

## 5. Direct manipulation targets

* Every selectable object offers handles (per `SELECTION.md`); targets are
  ≥ 8 px visual with 48 px effective hit area; small objects get an
  invisible expanded hit box (never silently overlapping neighbors — hit
  priority: topmost object first, then by z-order).
* Rotate: handle above top-center; Option/Alt rotates around center.
* Nudge: arrows (1 px, Shift 10 px) — undoable in one command per gesture
  series (coalesced undo).
* Constrain: Shift constrains aspect/direction during drags (move: axis
  lock after initial vector; resize: preserve aspect; rotate: 15° steps
  from 0°/90°).
* Alt/Option duplicates on drag (Photoshop-style convention) only in
  Photo/Motion; duplicate is always undoable.
* Live feedback during manipulation: geometry readout near the pointer
  (width × height, rotation, x/y in tabular figures, `space-8` from the
  pointer, `type-size-11`, contrast-backed chip).

## 6. Canvas accessibility strategy

* Canvas surfaces expose an object model to accessibility: each object has
  a name, type, bounds, and role (per `ACCESSIBILITY.md` §canvas).
* Keyboard navigation of canvas objects: Tab moves between top-level
  objects in z-order; arrows nudge; Enter opens the object's inspector
  section; context-menu key opens object actions.
* Text objects are editable by keyboard (caret entry); media objects expose
  play/pause via keyboard.
* The canvas never depends on pointer-only operations; any drag operation
  has a keyboard path (nudge, arrow-based resize, or numeric inspector
  fields) — verified by the acceptance checklist.
* Reduced motion: all canvas feedback (snap halos, zoom animation, selection
  morphs) goes instant; scrubbing stays functional.
