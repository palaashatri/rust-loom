# Task 13 report — Sheets componentized Numbers surface

## Status

IMPLEMENTED — committed for coordinator review and serialization onto
`cline-implementation`.

## Scoped changes

- Added `loom-sheets/crates/loom-sheets-app/ui/components.slint` with named
  Sheets-owned objects: `SheetTabs`, `SheetActionToolbar`, `FormulaNameBar`,
  `SheetSelectionOverlay`, `SheetScrollbars`, `SheetTable`,
  `SheetGridSurface`, and `SheetInspector`.
- Refactored `ui/app.slint` so the Window is a thin composition root. The
  grid/table, selection overlay, scrollbars, formula/name bar, tabs, toolbar,
  and contextual inspector now have explicit component boundaries and retain
  live controller bindings for selection, scrolling, formula commit/cancel,
  fill, undo, and accessibility labels.
- Added the new Slint module to `build.rs` rerun triggers.
- Renamed the first-launch fixture from `sample_sheet()` to
  `starter_workbook()` and documented it as an editable starter workbook.
  Unsupported Add Sheet and insertion actions remain disabled rather than
  implying an implemented multi-sheet or insertion engine.
- Extended the core regression suite for invalid/out-of-range viewport
  clamping, anchor/focus selection extension and collapse, multi-cell formula
  copy/revert/no-op detection, and sparse reversibility. Added the 1920 px
  breakpoint assertion in the app suite.
- Replaced the workspace split with builtin `HorizontalLayout` so the grid
  remains the dominant stretchable surface when the inspector is visible.

## Verification evidence

| Check | Actual result |
| --- | --- |
| `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-core` | PASS — 58 passed; 0 failed; doc-tests 0 passed; 0 failed |
| `cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app` | PASS — 27 passed; 0 failed |
| `cargo check --quiet --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app` | PASS (exit 0) |
| `cargo fmt --all --manifest-path loom-sheets/Cargo.toml -- --check` | PASS (exit 0) |
| `git diff --check` | PASS (exit 0) |

The app test/build output retains the existing `loom-ui` Slint warnings for
exported components that do not inherit `Window`; no warning was promoted to
an error or hidden by this task.

## Visual and journey evidence

Fresh software-rendered light-theme captures exist for the required layout
matrix and were inspected:

- `.work/evidence/ui/task-13-slice/sheets-1024x720-light.png`
- `.work/evidence/ui/task-13-slice/sheets-1280x800-light.png`
- `.work/evidence/ui/task-13-slice/sheets-1440x900-light.png`
- `.work/evidence/ui/task-13-slice/sheets-1920x1080-light.png`

The inspected 1024 px capture hides the inspector at the compact breakpoint;
the 1280 px and wider captures keep the grid and inspector legible, while the
1920 px capture exposes the labeled Numbers-style toolbar. The recorded
2026-08-30 headless run reported both `keyboard journey: PASS` and
`sparse workbook journey: PASS`. Its sparse evidence is retained under
`.work/evidence/ui/task-13-slice/journey-20260830/`, including:

- `01-start.png` through `07-save-reopen.png`;
- `sparse-1000.loomtable`; and
- `sparse-journey.txt`, which records `rows=1000`, the `A995:A996` range,
  formula `=A995+1`, `undo=true`, and the save path.

## Evidence boundaries and open limitations

- A fresh journey rerun after the coordinator's current shared `toolkit.slint`
  edits is blocked before application startup: Slint reports the hyphenated
  `label-text` identifier at `toolkit.slint:815-820` and `DirectionalLayout`
  errors in `smoke.slint`. The captured journey above predates that shared
  edit and remains the relevant task-owned visual artifact until the shared
  fix lands.
- The repository UI audit is likewise not evaluable in the current mixed
  worktree: `python3 loom-bootstrap/scripts/audit-product-ui.py` aborts in the
  coordinator's uncommitted audit script with `NameError: visible_groups is
  not defined` at `audit-product-ui.py:291` before manifest comparison.
- The grid is a bounded materialized viewport projection, not million-row
  production virtualization. Sparse tail functional assertions pass, but the
  current sparse-tail frames render blank cells after scrolling; this was
  reproduced on the pre-componentization baseline and is retained as an
  existing viewport-rendering limitation.
- No native macOS window/menu/file-dialog/screen-reader acceptance was
  established. No visible clipboard UX was added; range copy is covered in
  core tests. Add Sheet remains disabled because the controller is still
  single-sheet.

## Review-fix round 1 — cumulative grid metrics and keyboard guards

Implementation commit: `0a47b8ab35a977489927f11ccdb666bf05dab7be`

Changed production files:

- `loom-sheets/crates/loom-sheets-app/src/main.rs` — cumulative row/column
  projection, offsets, clamping, and the custom-dimension regression.
- `loom-sheets/crates/loom-sheets-app/ui/components.slint` — rejects named
  non-printable `Key.*` values, maps Tab/Backtab navigation, and exposes the
  polite table live region; the key regression covers Backspace, Delete, F1,
  Home, PageUp, and Backtab.

Fresh verification (exact commands and observed results):

```text
cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-core
test result: ok. 59 passed; 0 failed; doc-tests 0 passed; 0 failed

cargo test --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app
test result: ok. 33 passed; 0 failed

cargo fmt --all --manifest-path loom-sheets/Cargo.toml -- --check && git diff --check
exit 0

cargo clippy --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app -- -D warnings
Finished `dev` profile [unoptimized + debuginfo] (exit 0; existing loom-ui Slint export warnings only)

cargo build --release --manifest-path loom-sheets/Cargo.toml -p loom-sheets-app
Finished `release` profile [optimized] (exit 0)

loom-sheets/target/release/loom-sheets --smoke --size 1280x800
smoke_exit=0; smoke_png=/var/folders/0x/c0gh1m0j0yxd01cvk4p6lxn40000gp/T/loom-sheets-smoke-33940.png; smoke_png_bytes=64425
```
