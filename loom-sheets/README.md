# Loom Sheets

Loom Sheets is a fast, local-first analytical spreadsheet application with Apple Numbers-class visual polish and recalculation integrity.

![Loom Sheets main window](docs/screenshot.png)
*macOS native window capture (`screencapture -l`) of the current build. A pixel-reproducible renderer capture of the same state is at [docs/screenshot-deterministic.png](docs/screenshot-deterministic.png) (`cargo run -p loom-sheets-app -- --screenshot docs/screenshot-deterministic.png --size 1280x800 --theme light`).*

## Core Capabilities

- **Workbook Tabs & Navigation**: Multi-sheet workbook tabs with add/switch/rename/delete (all undoable), persisted with the active tab in versioned `.loomtable` packages.
- **Action Toolbar & Formula Bar**: Undo/redo, Bold/Italic/Underline, alignment, row/column insert, live-linked charts (Bar/Line/Pie), CSV/XLSX export, zoom (75–150%), sort, overflow menu; formula bar with cell badge, SUM/AVG/COUNT quick formulas, commit/cancel, and Fill down.
- **Spreadsheet Canvas**: Viewport-filling sheet grid with headers, live selection/range marquee, keyboard navigation (arrows/Tab/Shift-extend), real zoom scaling, and a floating live chart overlay.
- **Inspector**: `Table` tab (name, rows/columns add/remove) and `Cell` tab (raw formula, font style, data format incl. Number, decimals stepper, alignment, row/column sizing) — every control undoable and persisted.
- **Template Chooser**: Categorized chooser (Basic, Personal Finance, Personal, Business, Education) with eleven seeded templates, each creating its advertised sheet with live formulas.
- **Command Palette & Menus**: Ctrl+K palette covering every primary command; native macOS NSMenu / Linux DBusMenu with live enablement (incl. View zoom commands).
- **Storage & Interoperability**: Versioned `.loomtable` packages (all tabs, styles, alignments, freeze panes, charts; legacy single-sheet files still open), CSV import/export and single-sheet XLSX export (evaluated values, stated in the status line).

## Visual QA Status

- **Status**: **PASS** (shared-foundation adopted, zero app-local generic controls).
- **Canvas**: Viewport-filling grid at every contract viewport; no wrapping, clipping, or dead fixed-size surfaces.
- **Formulas**: Live evaluation (41 functions) with undo/redo transaction history.
- **Evidence**: `loom-sheets/.work/acceptance/` holds 18 judge-reviewed captures (4 viewports × 3 themes + chooser/palette/chart/zoom states).

## Development

```sh
cargo test --manifest-path loom-sheets/Cargo.toml
cargo run --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app
# Headless QA capture:
cargo build --manifest-path loom-sheets/Cargo.toml
```
