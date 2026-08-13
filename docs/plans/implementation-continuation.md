# Loom implementation continuation

## Goal

Continue the Loom implementation from the verified foundation slices by making
the primary editing surfaces interactive and making the newer app metadata
lists selectable. Preserve the existing package formats, visual layout, and
honest limitation reporting.

## Current truth audit — 2026-08-01

This plan is being continued from a dirty working tree, not from a clean
release state. The branch is `cline-implementation` at `f01eca3`
(`Keep formula bar draft synchronized on selection`), matching
`origin/cline-implementation` at audit time.

Implementation status is split by git state:

- Task 1 (Writer) is present only as uncommitted changes in
  `loom-writer/crates/loom-writer-app/src/main.rs`,
  `loom-writer/crates/loom-writer-app/ui/app.slint`, and
  `loom-writer/crates/loom-writer-core/src/lib.rs`.
- Task 2 (Sheets) is represented in `HEAD` by `99e2d82`, `8415de6`, and
  `f01eca3`; the latest Sheets change is committed, but the available aggregate
  test logs predate those commits and are not current verification of `HEAD`.
- Task 3 (Photo/Motion) is represented in `HEAD` by `233af96` and `33f8eb7`.
- Task 4 (Video/Studio/Encode) is represented in `HEAD` by `744d9fa`.

The working tree also has pre-existing uncommitted changes to
`loom-bootstrap/scripts/visual-qa-all.sh` and `loom-bootstrap/scripts/img-compare.sh`,
an untracked `loom-bootstrap/scripts/cleanup-targets.sh`, and this untracked
plan. An additional untracked `docs/visual-qa-baseline-review.md` appeared
during the audit; it was not modified or used as evidence. This audit does not
modify source, scripts, baselines, or package files.

## Global constraints

- Work only in the task's listed repositories/files; do not reformat unrelated
  repositories or rewrite bootstrap/reporting code.
- Keep the headless `--screenshot`, `--smoke`, and `--open` paths working.
- Add a focused core regression test for every new state mutation or bounds
  check. Do not add tests that only assert constants or source text.
- Run the owning workspace's `cargo fmt --all -- --check` and
  `cargo test --locked --workspace` before reporting completion.
- Do not commit or push from worker agents; the controller will stage the
  exact verified task files, commit the first breakthrough, and push it.
- Keep default sample screenshots structurally stable; new selection behavior
  must not introduce a different default selection state.

## Task 1 — Writer editable document surface (working-tree implementation)

Files: `loom-writer/crates/loom-writer-app/src/main.rs`,
`loom-writer/crates/loom-writer-app/ui/app.slint`, and, if needed for a pure
document-edit helper and tests, `loom-writer/crates/loom-writer-core/src/lib.rs`.

Replace the Writer read-only document text display with an editable multiline
surface using the Slint primitive/component already supported by the pinned
Slint version. Add a UI callback carrying the edited plain text. On edits,
update the current document as paragraph blocks: preserve existing block
styles by position where possible, create paragraph blocks for new content,
and remove blocks for deleted paragraphs. Keep the title, save, PDF export,
undo, redo, screenshot, smoke, and package round-trip behavior intact. Add a
focused core/helper test covering paragraph replacement and empty input.

The implementation is present in the working tree, but it is not in `HEAD` and
must not be reported as delivered until the owning workspace is freshly built,
tested, smoke-tested, visually reviewed, and committed.

## Task 2 — Sheets formula-bar editing (implemented in `HEAD`; reverify)

Files: `loom-sheets/crates/loom-sheets-app/src/main.rs`,
`loom-sheets/crates/loom-sheets-app/ui/app.slint`, and, if needed for a pure
selection/edit helper and tests, `loom-sheets/crates/loom-sheets-core/src/lib.rs`.

Add an editable formula/value field for the selected cell. Add a callback that
writes the raw text into the selected `CellRef`, including formulas beginning
with `=` and empty text. Preserve the selected cell when the sheet is
re-rendered, update evaluated value/formula display, and record edits in the
existing undo/redo stacks. Keep the grid click selection, CSV/package
round-trips, screenshot, and smoke paths working. Add a focused test for
editing a cell and retaining formula/raw semantics.

The implementation commits are in `HEAD`; current functional verification is
still required because the retained bootstrap logs were generated before the
latest Sheets commits.

## Task 3 — Photo and Motion selectable metadata lists (implemented in `HEAD`; reverify)

Files are limited to the Photo and Motion repositories:
`loom-photo/crates/loom-photo-app/src/main.rs`,
`loom-photo/crates/loom-photo-app/ui/app.slint`,
`loom-photo/crates/loom-photo-core/src/lib.rs`,
`loom-motion/crates/loom-motion-app/src/main.rs`,
`loom-motion/crates/loom-motion-app/ui/app.slint`, and
`loom-motion/crates/loom-motion-core/src/lib.rs`.

Make the Photo layer list and Motion timeline-layer list selectable. The
selected row must be visibly styled using the existing theme tokens, and the
status/inspector text must identify the selected item. Add bounds-safe core
selection helpers and tests; invalid indices must leave the previous
selection unchanged. Keep the current default sample selection and screenshot
layout stable.

The implementation commits are in `HEAD`; current functional and visual
verification is still required.

## Task 4 — Video, Studio, and Encode selectable metadata lists (implemented in `HEAD`; reverify)

Files are limited to the Video, Studio, and Encode repositories:
`loom-video/crates/loom-video-app/src/main.rs`,
`loom-video/crates/loom-video-app/ui/app.slint`,
`loom-video/crates/loom-video-core/src/lib.rs`,
`loom-studio/crates/loom-studio-app/src/main.rs`,
`loom-studio/crates/loom-studio-app/ui/app.slint`,
`loom-studio/crates/loom-studio-core/src/lib.rs`,
`loom-encode/crates/loom-encode-app/src/main.rs`,
`loom-encode/crates/loom-encode-app/ui/app.slint`, and
`loom-encode/crates/loom-encode-core/src/lib.rs`.

Make Video tracks, Studio tracks, and Encode jobs selectable. Use existing
theme tokens for the selected-row treatment and expose selected metadata in
the existing status/inspector area. Add bounds-safe core selection helpers and
tests for each model. Preserve current default selections and screenshot
geometry.

The implementation commit is in `HEAD`; current functional and visual
verification is still required.

## Verification and delivery — evidence required before continuation

The retained host logs (`loom-bootstrap/.work/`) were generated around
`2026-08-01T14:23:51+0530` and report 250 tests across 11 workspaces. They
predate the Sheets commits above and the uncommitted Writer implementation, so
they are historical evidence, not a current full-gate pass. The retained
default visual evidence is also incomplete:

- the default harness is 8 apps × light/dark = 16 possible captures;
- the latest retained host run captured 16/16, attempted four existing-baseline
  comparisons, found two Writer diffs (`0.079132`, `0.084621`), passed two
  Sheets comparisons (`0.013406`, `0.008322`), and has 12 missing baselines;
- the complete design-bible matrix is not run by this harness and remains
  unverified;
- the latest current host invocation captured 0/16 because cleaned target
  directories leave all app binaries absent;
- the retained Docker visual report has 16 captures but blank comparison
  metrics, and the older Docker test evidence includes a `loom-ui` font/baseline
  mismatch. A fresh post-rebuild Docker gate is unverified;
- Docker offline artifacts show dependency-resolution failures such as missing
  `image` and `slint` packages in offline mode; no network-isolated pass is
  established.

The package checksum is internally consistent, but `Loom-Complete.zip` was
generated before the current `HEAD` and does not prove the current source or
untracked files are packaged. The README's “deterministic” and “lite tests”
descriptions also need to be treated as stale until corrected: `package.sh`
sorts the input list but preserves file mtimes, while `verify-package.sh` tests
every extracted Cargo workspace with locked offline tests.

Before claiming completion or pushing, run the current host fmt, clippy, test,
release build, smoke, and default visual harnesses; perform a separate review
of the complete design-bible matrix; rerun Docker after the image rebuild and
rerun the hard-isolated offline check; repackage and run the actual extracted
workspace verifier; regenerate status/manifest artifacts from the current
commit; use the allowlisted cleanup script for generated targets; then run
`git diff --check` and review the exact staged file list. Preserve all logs,
screenshots, and unapproved visual diffs while cleaning targets.
