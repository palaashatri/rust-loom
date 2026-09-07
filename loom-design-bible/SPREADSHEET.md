# Spreadsheet

The grid is Sheets' primary surface: a virtualized, formula-driven, fully
keyboard-navigable workspace.

## 1. Grid chrome

```
┌────────────┬──────────────────────────────┐
│ formula bar│ fx =SUM(A1:A12)              │  (32 px)
├────────────┼──────────────────────────────┤
│  row       │  column headers (28 px)      │
│  headers   ├──────────────────────────────┤
│  (40 px)   │                              │
│            │         cell grid            │
│            │                              │
└────────────┴──────────────────────────────┘
```

* **Formula bar**: 32 px row above the headers: cell reference label (left,
  tabular figures) + formula input (the same TextField affordances as cell
  editing; see §5). The formula bar is always visible in Sheets.
* **Column headers**: 28 px, letters (A, B, …, Z, AA…), freeze-capable;
  selected columns highlight with accent-tinted header fill (15%).
* **Row headers**: 40 px wide, numbers, freeze-capable; height 28 px default
  (rows are resizable 16–512 px via the header edge drag).
* **Cell grid**: the largest surface; gridlines ink at 8%; cells
  `color-surface-canvas` background with `color-surface-raised` content
  wells only where a cell is in edit mode.
* **Freeze panes**: freeze rows/columns via a drag handle at the header
  intersection (crosshair cursor); frozen regions get a 2 px accent divider
  line; unfreeze via the same handle (drag back) or a header menu command.
* **Sheet tabs**: bottom tab bar (28 px) with sheet names, + button,
  scroll arrows; tab underline 2 px accent for the active sheet (TabBar
  component, `COMPONENTS.md` §19). Reorder by drag (200 ms settle);
  keyboard: Ctrl+PageUp/PageDown cycles sheets.
* **Status bar**: cell-mode indicator (Ready/Enter/Edit), selected-range
  summary ("=SUM 3 cells"), zoom readout; per `LAYOUT.md` §7.

## 2. Cell selection

* Click selects a cell; drag extends the range; Shift+arrows extend;
  Shift+click sets the range end; Cmd/Ctrl+click adds disjoint selections
  (`[goal]` in v1 — disjoint selection is a pro feature, tracked).
* Selected range: accent outline 2 px around the whole range; active cell
  (top-left anchor) shows the white fill + accent outline; fill-handle
  (bottom-right 8 × 8 px square, accent) drags to autofill.
* The selection outline stays inside the viewport edges (clamped); moving
  the active cell past the edge auto-scrolls (edge-scroll rate 50 px per
  frame at 60 fps, proportional to distance from edge).
* Selection is announced to screen readers on change ("A1 through C5
  selected").

## 3. Navigation (keyboard)

| Key | Behavior |
|---|---|
| Arrows | Move active cell 1 cell (Shift: extend range) |
| Tab / Shift+Tab | Right / left one cell (enters or exits selection per mode setting) |
| Enter / Shift+Enter | Down / up one cell, committing edit |
| Home | Column A (Cmd: A1) |
| Ctrl/⌘+Home | A1 |
| PageUp/PageDown | Viewport-height jump, active cell moves with view |
| Ctrl/⌘+Arrow | Jump to the edge of the data block (hold Shift to extend) |
| Ctrl/⌘+G | Go To (dialog: cell reference) |
| F2 | Enter edit mode on active cell |
| Esc | Exit edit / cancel edit / (in selection) move to cell-move mode |

Numeric entry starts editing and committing on Enter — a full cell editor
(`[goal]`: input-box overlay with formula highlighting).

## 4. Virtualization and performance

* The grid is virtualized: only visible rows/columns render; cell values
  are text fragments, styles resolved on demand; scrolling reuses buffers
  (zero allocations per frame in the scroll path).
* Recalculation is incremental and off-thread (formula engine contract in
  `loom-core`); the grid never blocks on `=SUM` chains; a recalculation
  banner (status bar chip: "Calculating…") appears only when a recalculation
  exceeds 250 ms and is cancellable.
* Benchmark gates: 1,000,000-cell random data sheet scrolls at 60 fps;
  recalc of 10,000 formulas < 200 ms on the mainstream tier
  (`PERFORMANCE.md`).
* Freeze panes + virtual scrolling compose: frozen region is a separate
  buffer overlaid on the virtual grid.

## 5. Editing

* Cell entry: type = replace value; F2/double-click = in-cell caret edit;
  the formula bar edits the same buffer (edits from either surface stay
  synchronized, commit on Enter/Esc).
* Formula entry: `=` prefix opens formula context — token coloring
  (functions accent, references ink-primary, errors danger), reference
  highlighting: when the caret touches a reference, the referenced range
  is outlined with the reference's color (per reference, cycling the
  data-viz palette for multi-reference formulas).
* Commit semantics: Enter commits and moves down; Tab commits and moves
  right; Esc cancels; invalid input (bad formula, wrong type) blocks commit
  with an inline error message under the cell (never a dialog —
  `DIALOGS.md` policy).
* IME input: in-cell editing supports IME composition with the standard
  composition underline (`DOCUMENT_EDITOR.md` §5 rules apply to the grid).

## 6. Rows, columns, structure

* Header context menus: insert/delete row(s)/column(s), hide/unhide, freeze,
  width/height, sort, filter — every entry also available via
  keyboard (`Cmd+Shift+K` insert, `Cmd+Delete` delete) and palette.
* Row/column resize: drag header edge, live redraw, snap to content on
  double-click (auto-fit); width readout in the header chip while dragging.
* Grouping/outlining: group rows/columns via header menu; the group rail
  (12 px) shows collapse chevrons with a non-color state (chevron
  orientation + disclosure); groups collapse/expand with animation 200 ms
  out-quad, reduced motion instant.
* Filters: header dropdown (ComboBox-list popover) with filter state shown
  as a funnel glyph + tinted header fill (15% accent) — never color-only.

## 7. Accessibility

* The grid is fully keyboard-operable (table semantics per cell: row
  header, column header announced on navigation).
* Screen reader announcements: cell reference + value + formula when the
  cell has one; range selections summarized; frozen state announced.
* Cell target size: minimum 24 px height at default zoom with 44 px
  effective row hit targets for pointer when rows are at minimum height
  (`POINTER_AND_PEN.md`).
* At 1.5× text scale, headers grow to fit; the grid keeps 1:1 cell-to-value
  mapping (cell content scales, geometry grows).
