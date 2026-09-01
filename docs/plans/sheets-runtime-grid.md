# Sheets runtime grid continuation

## Goal

Make the existing Sheets virtual-grid journey truthful: after scrolling to a
large sparse tail, the in-window projection must contain and render the tail
row headers and populated cells. Add evidence that checks projected UI data,
then address the already-observed selection accessibility and app clipboard
gaps only where the current contracts support a small, testable vertical slice.

## Constraints

- Work only in `loom-sheets/crates/loom-sheets-core`,
  `loom-sheets/crates/loom-sheets-app`, and its UI files.
- Preserve package, CSV, screenshot, smoke, and existing formula-bar paths.
- Do not modify `TRUTH.md` or parity claims until fresh runtime evidence exists.
- Use the existing `SheetViewport`, `GridGeometry`, `GridSelection`,
  `RangeEdit`, `CellEditTransaction`, and journey capture helpers where they
  are the correct contracts.
- Add behavior tests before production changes and verify with fresh command
  output.

## Task 1 — trace and regress the blank tail projection

Reproduce the sparse journey and inspect the data flow from scroll offsets,
viewport projection, geometry, and Slint row/cell materialization. Add a
focused failing regression test or journey assertion that requires visible
row headers near rows 975–1000 and populated cells `A995`, `A996`, and `A1000`
after the tail scroll. Implement the smallest root-cause fix, including
custom row/column dimensions if they are involved. Keep the default screenshot
layout stable.

## Task 2 — selection accessibility and clipboard evidence

Bind the live selection announcement to a real accessible status/grid node and
provide coordinate/value/formula semantics for materialized cells where the
pinned Slint API supports them. Add the smallest honest app-level copy/paste
path using the existing `RangeEdit::copy` and shared clipboard contract; if a
complete paste path cannot be implemented without a broader shared-platform
change, leave it disabled and document the verified limitation in the report
rather than adding a placebo control.

## Verification

Run the owning workspace format check, locked all-target tests, release sparse
journey, smoke/screenshot path, and inspect fresh post-scroll frames. Report
exact commands and outputs, including any unverified limitations. Do not
commit or push until the coordinator reviews the exact diff and evidence.
