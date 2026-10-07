# Loom Sheets

Loom Sheets is a local-first spreadsheet application. It edits multi-sheet workbooks with formulas and charts on your own computer. Nothing is uploaded and no account is needed. Native workbooks are open `.loomtable` packages; it also imports and exports CSV and XLSX.

**Platforms:** Windows x86-64 and Linux x86-64. macOS is out of scope for v1 and is not claimed.

**Status: `ACCEPTANCE_BLOCKED`.** The supervisor has not recorded `ACCEPTED`. This is not a production-ready claim. The live ledger is the current-truth section of the repository `AGENTS.md`; this README lists what has evidence behind it and what does not.

Evidence tags used below:

- **[tests]** covered by an automated test that passes (`cargo test --workspace`; the ledger records 440 app tests and 250 core tests on 2026-10-07; not re-run while writing this README).
- **[live-W]** exercised by hand in a real Windows 11 window.
- **[live-L]** exercised by hand in a real Linux window (WSLg, Ubuntu). Linux checks are older builds unless stated.

## What works

### Files
- Native `.loomtable` Open, Save, Save As; all tabs, styles, alignment, freeze panes, charts, shapes and embedded images are stored in the package. Legacy single-sheet files still open. [tests] [live-W]
- Open also takes `.csv` and `.xlsx` (imports become unsaved; Save then asks for a native name). Command line: `loom-sheets path/to/file`. [tests]
- CSV import and export, formulas preserved, delimiter detection on import. [tests]
- XLSX export: sheet names, formulas, cached values, cell styles, column widths, freeze panes, arrays, one chart per sheet, shapes and embedded images. A set of exported workbooks opened in real Microsoft Excel 16 with cells, styles, widths, freeze panes, arrays, one chart and one shape matching. [tests] [live-W] A LibreOffice Calc 24.2.7.2 round trip (XLSX to ODS to XLSX to Loom) recovered four sheets, formulas, a styled cell, a chart and a shape label (Linux, 2026-09). [live-L]
- XLSX import: sheets, formulas, cached values, cell styles, charts, shapes and images, read by a streaming reader checked against Excel-authored fixtures in `crates/loom-sheets-core/tests/fixtures/`. [tests]
- Before an XLSX import replaces the open workbook, a warning names what Loom knows it will lose or approximate: defined names, links to other workbooks, conditional formatting, data validation, merged cells (imported as separate cells), hidden sheets/rows/columns (imported visible), multi-cell array formulas (only the first cell keeps the formula), time/fraction/non-dollar currency number formats, fonts, text colours, wrapped text, exact fills and border styles, partial-text formatting, comments and hyperlinks, Excel tables and filters, text that looks like a number or formula, formulas Loom calculates differently, the 1904 date system, PivotTables (cached cells only), unsupported or extra chart types, plots and series, additional charts on a sheet, absolute-anchored objects and missing drawing parts. Cancel leaves the workbook and recovery data untouched. This covers the file picker and the `--open` start-up path. [tests] The list is the known set, not a promise of full Excel round-tripping.
- Templates: eleven seeded templates in five categories (Basic, Personal Finance, Personal, Business, Education) with a real Recents list; each creates the workbook it shows. [tests] [live-W]
- Closing with unsaved changes asks Save and close / Discard / Cancel; Discard clears recovery so the next start shows no draft. [tests] [live-W]

### Grid, selection and editing
- Growing sheet (up to 1,048,576 rows by 16,384 columns), pinned headers, text overflowing into empty neighbours, numbers shrunk (fewer decimals, scientific, `####`) rather than cut off. [tests] [live-W]
- Mouse: click, drag a range, Shift-click, click a column or row header, the select-all corner, drag a header edge to resize (one undo step), double-click an edge to autofit, double-click a cell to edit, name box (`D50`, `B2:C3`), wheel scrolling, auto-scroll when dragging past the edge. [tests] [live-W]
- Keyboard: arrows, Shift+arrows, Tab / Shift+Tab, Enter, Page Up/Down, Home, Ctrl+Home/End, Ctrl+Arrow data-edge jumps (with Shift to extend), Ctrl+A. Typing starts an edit. After a run of Tab-then-Enter the cursor returns to the column where the run began. [tests] [live-W]
- Formula bar with an in-cell mirror; Enter commits and moves down, Tab commits and moves right, Escape cancels. Quick SUM/AVG/COUNT formulas and the status-bar SUM/AVG/COUNT for a selection. [tests] [live-W]
- Copy, cut and paste through the real system clipboard as tab-separated text. Formulas keep their references shifted when pasted inside Sheets; text from other programs pastes as cells; one copied cell fills a selection. Fill down. [tests] [live-W]
- Undo and redo for every edit, including formatting, sorting, row/column changes, sheet operations, objects and gestures. [tests] [live-W]
- Sort ascending/descending, add/delete rows and columns, freeze header row, unfreeze panes. Sort stops above the first formula row and says so. [tests] [live-W]
- Sheet tabs under the toolbar: add, switch, rename (references in formulas follow the new name), delete. [tests]
- Zoom 50% to 300%. [tests]

### Formulas
- More than 100 Excel-compatible functions: math, statistics (including `SUMIFS`, `COUNTIFS`, `AVERAGEIFS`, `RANK.EQ`, `STDEV.S`), text, logic (`IFS`, `SWITCH`, `XOR`, `IFNA`, `IS*`), lookup (`VLOOKUP`, `HLOOKUP`, `INDEX`, `MATCH`, `XLOOKUP`), dates, finance (`PMT`, `FV`, `PV`, `NPV`, `IRR`, `NPER`, `RATE`) and dynamic arrays (`FILTER`, `SORT`, `UNIQUE`, `SEQUENCE`, `TRANSPOSE`). Operators `+ - * / ^ & %` and comparisons use Excel precedence; cross-sheet and absolute references and cycle detection work. [tests]
- A differential test compares 2,072 formulas against results produced by real Microsoft Excel 16 (`tests/fixtures/excel_corpus.json`). There are zero unexplained mismatches and 38 allow-listed differences, listed in `tests/excel_differential.rs` (locale-dependent text parsing, fraction formats, omitted arguments, whole-column references such as `A:C`, the space intersection operator, a few rare lookup modes). [tests]

### Formatting, charts and objects
- Bold, italic, underline, left/centre/right alignment, font size, borders, fill colour (cycle), number formats General / Number / Currency / Percent with a decimals stepper, row height and column width. Inspector tabs: Organize (table name, row/column counts) and Format (cell). [tests]
- Charts tied to a cell range: Line, Bar and Pie, redrawn after edits and Undo, with the range, series and unit shown. Chart data can be read point by point from the keyboard. [tests] [live-W]
- Anchored shapes (seven fills with readable labels in all themes) and images; objects can be moved and resized with the pointer or the keyboard. Image bytes are stored inside the workbook, so a workbook still opens with its pictures after the source file is deleted. [tests]
- Formula-backed pivot summaries (Sum, Count, Average, Min, Max) from the Table menu / palette. [tests]

### Recovery and responsiveness
- Formula calculation, Open, Save and CSV/XLSX export run on background workers; each commit shows "Calculating..." and results are applied only if they are still the newest for that tab. [tests]
- Crash recovery: a journal of cell edits after a complete checkpoint, replayed at the next start. Checkpoints are written at 16 MiB of journal, 2,000 records or five minutes. If recovery cannot keep up, editing pauses (title and status say "recovery paused"), every mutating command is refused while Save, Save As, export and close stay available, and Retry Recovery (command palette) retries with a complete checkpoint. A second instance on the same recovery store is told so. Fault-injection tests (failed append, checkpoint, pointer replace, journal rewrite) each followed by a restart return the last acknowledged workbook. [tests]
- Measured on an Intel i5-12400F, test profile at opt-level 3 (not `--release`, not a GPU frame): journal append p95 4.4 / 7.5 / 7.5 ms for the 1st, 10th and 100th unsaved edit in a 100-cell workbook; for one million unique cells, evaluation 214 ms, first recovery package 470 ms, one edit to the journal 2.7 ms, scroll projection p95 0.07 ms. Scroll callback p95 1.9 ms and a full software-renderer frame p95 21.7 ms at 300,000 cells (Windows). These are measurements, not an acceptance of the 60 fps or memory gates.

### Keyboard and accessibility
- Every menu, toolbar item, dialog (Save Changes, XLSX warning, template chooser, command palette) and the cell/formula/save workflow is operable from the keyboard with focus trapped in dialogs and restored on close; 13 keyboard-flow tests send real key events and read the accessibility tree. [tests]
- Accessibility tree: every interactive control has a name; the live Windows UI Automation dump showed 0 unnamed interactive controls. This is tree verification only; spoken screen-reader output has not been verified. [tests] [live-W]
- Native-window Alt+menu delivery was confirmed by hand in Writer only; Sheets relies on the tested key-event path.

### Appearance
- Appearance: View > Appearance: System, Light, Dark and High Contrast (also in the toolbar View menu and the command palette) switches the whole window live, including dialogs and popups, and is remembered per user in `settings.toml` under the platform config directory (`LOOM_CONFIG_DIR` overrides it). System follows the operating system's light/dark preference and falls back to light. `--theme system|light|dark|high-contrast` overrides the saved choice for that run without changing it. Document artwork (page, cells, slides) is not recoloured. [tests]
- Text scale 1.0 to 2.0 (`--text-scale`) and a real device scale factor 1.0 to 4.0 (`--scale-factor`); tests render every reachable surface at text scale 1.0/1.5/2.0 and scale factors 1.25/1.5/2.0 and check controls stay inside the window. Right-to-left (`--rtl`) mirrors the window chrome. [tests]
- Renders at 1024x720, 1280x800, 1440x900 and 1920x1200 in all three themes were inspected without clipping (renderer evidence, not human sign-off).

## Keyboard shortcuts

| Keys | Action |
|---|---|
| Alt+F / Alt+E / Alt+V / Alt+T / Alt+H, or F10 | Open the File / Edit / View / Table / Help menu (F10 opens the first); arrows walk, Enter runs, Escape closes |
| Ctrl+K | Command palette |
| Ctrl+N / Ctrl+O | New / Open |
| Ctrl+S / Ctrl+Shift+S | Save / Save As |
| Ctrl+E | Export CSV |
| Ctrl+Z / Ctrl+Shift+Z | Undo / Redo (there is no Ctrl+Y) |
| Ctrl+X / Ctrl+C / Ctrl+V / Ctrl+A | Cut / Copy / Paste / Select all |
| Ctrl+B / Ctrl+I / Ctrl+U | Bold / Italic / Underline |
| Ctrl+= or Ctrl++ / Ctrl+- / Ctrl+0 | Zoom in / out / actual size |
| Arrows, Shift+Arrows | Move / extend selection |
| Ctrl+Arrow, Ctrl+Shift+Arrow | Jump to data edge / extend to it |
| Page Up / Page Down, Home, Ctrl+Home, Ctrl+End | Move a screen, to column A, to A1, to the last used cell (Shift extends) |
| Enter (grid) / typing | Start editing in the formula bar |
| Enter / Tab / Shift+Tab / Escape (formula bar) | Commit and move down / right / left; cancel |
| Delete or Backspace | Clear the selected cells |
| F6 | Move between the grid, worksheet objects, chart data (if shown) and the toolbar |
| Shift+F6 | Move grid, toolbar, inspector, grid |
| Objects (after F6): Tab / Shift+Tab, M, R, arrows, Enter, Escape | Browse / move / resize / preview / commit / cancel or return |
| Chart data: Left / Right, Home / End, Escape or F6 | Inspect points; return to the grid |

Menu mnemonics are the first letter of each menu not already used; Sheets has File, Edit, View, Table and Help. F2 is not an edit key.

## Known limitations

- No pivot tables (only the formula-backed summaries above); PivotTables in an XLSX import arrive as cached cells.
- XLSX export writes one chart series per chart at a fixed anchor. Import warns about, and drops or approximates, everything in the warning list above (for example merged cells, comments, fonts and text colours, extra chart series). Only one set of exported files was checked in Excel 16; import was checked on Excel-authored fixtures, not a wide corpus.
- Formulas outside the Excel corpus, fraction formats, omitted arguments, whole-column references and the space intersection operator are not supported or differ (see the 38 allow-listed differences).
- The UI offers charts of kind Line, Bar and Pie only. The Text menu's check marks are visual indicators only.
- Recovery (REC-02) is closed for v1 on Windows and Linux with these disclosed limits: a real full-volume disk-full is untested (only injected write errors); New and Open keep their own unsaved-work decision rather than the pause guard; Save while recovery is paused refuses a pending formula-bar draft; recovery locking is only verified on Windows and Linux.
- PERF-01 stays NEEDS_REVIEW: full-workbook copies for non-cell edits still run on the UI thread, export-scale performance, a reviewed app memory limit, and native million-cell scrolling in a real window are unmeasured. Test-profile timings above are not frame-rate proof.
- Accessibility is tree-verified only; spoken screen-reader output, a fractional-scale live window and Wayland are unverified. The Windows UI Automation dump and Linux AT-SPI checks cover named controls, not reading order in speech.
- Document content is left-to-right: `--rtl` mirrors the chrome, not column order.
- On Linux, native file dialogs need `zenity` or an `xdg-desktop-portal`. Linux was last driven interactively on an earlier nightly (typing, File menu, overflow menu, keyboard menus, recovery restore); the newest Sheets features and exports were not re-driven there.
- No wider interoperability matrix beyond the Excel 16 export check, the Calc round trip and the Excel-authored import fixtures.
- Builds are unsigned (Windows SmartScreen warns on first run).

## Install and run

Download the portable archive from the GitHub `nightly` release (`https://github.com/palaashatri/rust-loom/releases/tag/nightly`): `loom-nightly-<commit>-windows-x86_64.zip` or `loom-nightly-<commit>-linux-x86_64.tar.gz`. It holds all Loom apps; run `loom-sheets.exe` (Windows) or `./loom-sheets` (Linux). Nothing needs installing.

- Windows: unsigned, so choose "More info" then "Run anyway" on the SmartScreen warning.
- Linux: needs a desktop session (X11 or Wayland), OpenGL and fontconfig; Open and Save dialogs need `zenity` or an xdg portal. `chmod +x loom-*` if the execute bit was lost.

## Development

```sh
cd loom-sheets
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p loom-sheets-app
# open a file
cargo run -p loom-sheets-app -- path/to/book.loomtable
# headless render (light, dark, high-contrast; text and device scale; RTL)
cargo run -p loom-sheets-app -- --screenshot out.png --size 1280x800 --theme dark
cargo run -p loom-sheets-app -- --screenshot out.png --scale-factor 2 --rtl
```

Windows builds need a 32 MiB main-thread stack for the Slint build script; `loom-sheets/.cargo/config.toml` sets it, so run cargo from inside `loom-sheets`. `docs/` holds dated screenshots (renderer and native-window captures) that are evidence for the specific states named in the ledger, not a current acceptance record.
