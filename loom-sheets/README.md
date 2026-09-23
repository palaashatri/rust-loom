# Loom Sheets

Loom Sheets is a local-first spreadsheet application for editing workbooks, formulas, and charts on your computer.

## Screenshots

![Loom Sheets main window — Linux](docs/screenshot-linux.png)
*Linux X11 native window capture from the current build, taken after focusing the Sheets window with `wmctrl` and running `/usr/bin/gnome-screenshot -w`.*

![Loom Sheets chart overlay — Linux](docs/screenshot-linux-chart.png)
*Linux X11 native capture of the same workbook with its live formula-backed chart overlay visible.*

![Loom Sheets anchored objects — Linux](docs/screenshot-linux-objects.png)
*Linux X11 native capture of the same workbook with persisted anchored worksheet objects visible in the live grid.*

![Loom Sheets main window — macOS](docs/screenshot.png)
*macOS native window capture (`screencapture -l`) of the current build. A pixel-reproducible renderer capture of the same state is at [docs/screenshot-deterministic.png](docs/screenshot-deterministic.png) (`cargo run -p loom-sheets-app -- --screenshot docs/screenshot-deterministic.png --size 1280x800 --theme light`).*

## Native Linux self-audit — 2026-09-23

These screenshots come from the running desktop app via `/usr/bin/gnome-screenshot -w`. Each PNG includes the native title bar and measures 1024×741 for a requested 1024×720 window.

![Opened saved workbook with its saved filename — Linux](docs/qa-native/opened-saved-workbook-linux.png)

![Unsaved edit with the visible title marker — Linux](docs/qa-native/unsaved-edit-title-linux.png)

![Keyboard-selected Checklist template — Linux](docs/qa-native/template-chooser-checklist-selected-linux.png)

![Checklist workbook created from the selected template — Linux](docs/qa-native/template-created-checklist-linux.png)

The matching keyboard, AT-SPI, Orca, and cancellation observations are in [the native self-audit report](../.work/sheets-acceptance-2026-09-23/REPORT.md) and the repository's portable audit evidence archive.

## Core Capabilities

- **Workbook Tabs & Navigation**: Multi-sheet workbook tabs with add/switch/rename/delete (all undoable), persisted with the active tab in versioned `.loomtable` packages.
- **Action Toolbar & Formula Bar**: Undo/redo, Bold/Italic/Underline, alignment, row/column insert, live-linked charts (Bar/Line/Pie), formula-backed pivot summaries, anchored shape/image insertion, CSV/XLSX export, zoom (75–150%), sort, overflow menu; formula bar with cell badge, SUM/AVG/COUNT quick formulas, commit/cancel, and Fill down.
- **Spreadsheet Canvas**: Viewport-filling sheet grid with headers, live selection/range marquee, keyboard navigation (arrows/Tab/Shift-extend), dynamic-array spill/error projection, anchored worksheet objects, real zoom scaling, and a floating live chart overlay.
- **Inspector**: `Table` tab (name, rows/columns add/remove) and `Cell` tab (raw formula, font style, data format incl. Number, decimals stepper, alignment, row/column sizing) — every control undoable and persisted.
- **Template Chooser**: Categorized chooser (Basic, Personal Finance, Personal, Business, Education) with eleven seeded templates, each creating its advertised sheet with live formulas.
- **Command Palette & Menus**: Ctrl+K palette covering every primary command; native macOS NSMenu / Linux DBusMenu with live enablement (incl. View zoom commands).
- **Storage & Interoperability**: Versioned `.loomtable` packages (all tabs, styles, alignments, freeze panes, charts, anchored shapes/images, and package-owned embedded image assets; legacy single-sheet files still open), formula-preserving CSV import/export with dialect sniffing, and multi-sheet XLSX import/export preserving worksheet names, formulas, cached values, cell styles/alignments, basic charts, shapes, and embedded images.

## Visual QA Evidence

- The existing `.work/acceptance/` set contains 18 renderer captures across four viewports and three themes, plus chooser, palette, chart, and zoom states.
- Native Linux screenshots above verify the opened workbook, dirty title, chooser selection, and template creation at 1024×720. The Escape-preserves-workbook capture is `docs/qa-native/template-cancel-preserves-workbook-linux.png`.
- These captures are evidence for the named states, not a blanket acceptance claim. Remaining Sheets checks are tracked in the root `TRUTH.md` ledger.

## Development

```sh
cargo test --manifest-path loom-sheets/Cargo.toml
cargo run --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app
# Headless QA capture:
cargo build --manifest-path loom-sheets/Cargo.toml
# Native Linux X11 window capture (focus the Sheets window first):
wmctrl -l
wmctrl -i -a <sheets-window-id>
gnome-screenshot -w -f docs/screenshot-linux.png
gnome-screenshot -w -f docs/screenshot-linux-chart.png
gnome-screenshot -w -f docs/screenshot-linux-objects.png
```
