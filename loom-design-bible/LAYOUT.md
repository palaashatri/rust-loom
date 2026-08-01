# Layout

The layout contract for Loom application windows. All values are DPI-scaled
logical pixels.

## 1. Window anatomy (main window)

```
┌──────────────────────────────────────────────────────────┐
│ title bar                      (40 px)                    │
├─────────────┬────────────────────────────────────────────┤
│             │  context toolbar            (40 px)        │
│  sidebar    ├────────────────────────────────────────────┤
│  (240 px)   │                                            │
│             │            content canvas (fills)          │
│  collapsible│                                            │
│             │                                            │
│  (drag to   │                                            │
│   resize)   │                                            │
├─────────────┴────────────────────────────────────────────┤
│ status bar                         (28 px)               │
└──────────────────────────────────────────────────────────┘
```

Fixed chrome heights:

* Title bar: **40 px**. Contains window controls (left on Linux, or platform
  convention), document title (truncates with ellipsis), and the command
  palette/help entry point on the right.
* Context toolbar: **40 px**, single row (see `TOOLBARS.md`).
* Sidebar: **240 px** default width, collapsible, resizable 180–400 px
  (see `SIDEBARS.md`).
* Inspector: **280 px** default width, resizable 240–360 px, may dock left or
  right; never appears while the sidebar is open on the same side
  (see `INSPECTORS.md`).
* Status bar: **28 px**. Progress, cancellation, transient status messages,
  and zoom/snapping readouts (see `NOTIFICATIONS.md`).
* Canvas: fills the remainder; never has a fixed size.

## 2. Grid and spacing rules

* All chrome spacing uses the spacing scale (2, 4, 6, 8, 12, 16, 20, 24, 32,
  40, 48, 64 px). Only these values, with the single documented exception of
  1 px hairlines inside components.
* Panel padding: `space-16`. Section spacing inside panels: `space-20`.
  Control-to-control gaps: `space-8`. Control-to-label: `space-8`.
  Component internal padding: `space-4`–`space-8` per `COMPONENTS.md`.
* Alignment: chrome edges align to the window edge at `space-0`; panels align
  flush to chrome; content never floats with arbitrary margins.
* Minimum window sizes: content-first apps (Writer, Sheets) 800 × 600;
  canvas-first apps (Photo, Motion, Video, Present) 1024 × 640; Studio
  1024 × 640. Below minimums, chrome collapses (sidebar auto-collapses) before
  content suffers.
* Baseline alignment: text baselines align across a toolbar row where control
  heights differ; labels align to control text baselines, not control boxes.

## 3. Content surfaces

* Document pages: on `color-surface-canvas` with a drop page shadow* — *no*:
  pages sit on the canvas flat (depth by color only), with a 1 px
  `border-width-hairline` in dark theme to separate page from canvas.
* Media beds (image viewers, video scopes, previews): `color-surface-sunken`
  wells. Media never touches `color-surface-canvas` directly without a well.
* Tables, grids, spreadsheets: header rows on `color-surface-sunken`, body on
  `color-surface-raised` (Spreadsheet app uses canvas/raised per
  `SPREADSHEET.md`).

## 4. Toolbar layout

* Single row, 40 px, horizontal, scrollable? — never scrollable. Overflow
  goes to an overflow popover (`overflow-ellipsis` icon at the row end); see
  `TOOLBARS.md`.
* Left: primary tools for the current context. Center-right: contextual
  actions. Right: suite actions that are always available (search, share*,
  help) — `share` is out of scope; right side hosts commands, zoom, and
  export.
* No groups separated by vertical rules; groups are separated by `space-8`
  gaps and, at most, a hairline between major groups.

## 5. Sidebar layout

* Media/collections sidebar (left): panel stack, each panel with a header
  (24 px) and body. Panels collapse to headers; the sidebar itself collapses
  to 0 and the toolbar's toggle restores the last width.
* Width is dragged at the sidebar's right edge (left edge when docked right);
  drag hit area is 4 px (2 px visual + 2 px grace) with an inset resize
  cursor. See `SIDEBARS.md`.

## 6. Inspector layout

* Sections: object, style, document, metadata, advanced (see
  `INSPECTORS.md`). Section headers 32 px; section body padding `space-16`;
  two-column property rows (label left, control right) on a 12-column grid
  within the panel width.
* The inspector never scrolls the whole panel with nested scrolls; sections
  collapse and the panel scrolls as one surface.

## 7. Status bar layout

* Left: primary progress/cancellation area (longest message wins, truncates
  with ellipsis; full text in tooltip).
* Right: readouts (zoom, snapping, color space, transport state in Studio/
  Video). Readouts are `type-size-11`, `color-ink-secondary`, tabular figures
  for numbers.

## 8. DPI and scaling

* All layout units are logical pixels at 1.0 text scale; the UI must render
  correctly at 1×, 2×, and 1.5× text scale (`TYPOGRAPHY.md` §8).
* Canvas contents (documents, images) may render at their own zoom
  (25–800%) independent of UI scaling — zoom is a content property
  (`CANVAS.md`).
* Visual QA captures at 1280 × 800 logical, software renderer, 1.0 scale
  (`VISUAL_QA.md`); additional captures at 1.5 text scale are required for
  the layout-stress gate.
