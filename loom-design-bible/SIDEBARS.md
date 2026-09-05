# Sidebars

The collapsible sidebar hosts the application's navigational and asset
surfaces: media libraries, project panels, layers, clips, pages, sheets,
scenes, tracks, and jobs.

## 1. Placement and anatomy

* Left dock by default (right dock allowed per user setting; never both at
  once with the inspector on the same side).
* Default width 240 px; resizable 180–400 px; width persists per app.
* Anatomy: column surface (`color-surface-raised`) with panel stack; each
  panel has a 40 px header (panel title, collapse chevron, optional pin
  button) and a body. The stack's top panel is the primary navigator for
  the app; the rest are contextual.
* A hairline separates the sidebar from the canvas.

## 2. Collapse behavior

* Toggle: toolbar `collapse-panel` IconButton; shortcut `Cmd+\` (Windows:
  `Ctrl+\`). Collapse animates the width to 0 (out-quad 200 ms); the canvas
  expands. Reduced motion: instant.
* Collapsed state: the sidebar is fully hidden; the toggle remains in the
  toolbar. No auto-peek on hover (hover-reveal prohibited).
* Restoring returns the last width, not a default.
* Auto-collapse at minimum window size (`LAYOUT.md` §2): when the window
  cannot fit sidebar + minimum content, the sidebar collapses and shows a
  transient toast ("Sidebar hidden to fit window"), restorable.
* Per-panel collapse: each panel header collapses its body to a 40 px header
  row; multiple panels can be collapsed; the last collapsed panel state
  persists per app.

## 3. Panel stacking

* Panels stack vertically with hairline separators; no overlap, no floating
  panels inside the sidebar (floating panels are `[future]` via WINDOWS.md
  utility windows).
* Stack order persists per app; users may reorder panels by drag within the
  sidebar (reorder feedback per `DRAG_AND_DROP.md` — 200 ms settle).
* Panel bodies scroll independently, but the sidebar does not nest scroll
  regions inside scroll regions: a panel body scrolls, the sidebar column
  itself never scrolls.

## 4. Resize

* Drag handle: the sidebar's inner edge (toward the canvas), 4 px hit area
  (2 px visual + 2 px grace), resize cursor; width clamps to 180–400 px with
  live redraw (instant, no animation during drag; settle at release).
* During drag, the canvas reflows continuously at 60 fps; no re-layout
  flicker, no async relayout.
* Keyboard resize (`[goal]`): `Alt+Left/Right` adjusts width by `space-8`
  steps when the sidebar has focus; announced via status bar.

## 5. Content patterns

* **Media/asset libraries** (Photo layers, Video clips, Studio tracks,
  Sheets sheets, Writer pages): ListItems with thumbnail rows (thumbnail
  32 × 32 `radius-4`, name `type-size-13`, secondary metadata line
  `type-size-11`), selection per `SELECTION.md`, drag out per
  `DRAG_AND_DROP.md`.
* **Search** inside libraries: a search field pinned at the panel top
  (filter-as-you-type, 120 ms debounce, results keep selection context).
* **Empty states** in panels: compact EmptyState variant with an action
  ("Import media", "Add layer").
* **Progress**: long-running library jobs (imports, thumbnails, proxies)
  show a compact progress row at the panel bottom with cancel; never block
  browsing (jobs are async; `loom-core` jobs contract).

## 6. Accessibility

* Sidebar toggle is keyboard-reachable from everywhere (toolbar shortcut);
  focus moves into the sidebar on open, returns to the canvas on close.
* Panel headers are focusable; chevron + title + pin have labels; collapsed
  panels are announced.
* Sidebar item drag is keyboard-alternative-able: items can be moved via
  cut/paste or a context-menu move command — drag is never the only path.
* At 1.5× text scale, sidebar width floor rises to 260 px so labels remain
  legible.
