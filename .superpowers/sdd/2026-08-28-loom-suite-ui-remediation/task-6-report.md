# Task 6 report — Photo layer and transform workflow

## Status

The Photo layer/transform vertical slice is implemented in the active
`cline-implementation` checkout. The requested commit subject is:

```text
feat(photo): establish real layer transform editing
```

This report records deterministic software-renderer and headless evidence. It
does not claim native AppKit/Linux menu delivery, screen-reader runtime output,
or GPU compositor behavior.

## Changed files

- `loom-photo/crates/loom-photo-core/src/lib.rs`
- `loom-photo/crates/loom-photo-app/src/main.rs`
- `loom-photo/crates/loom-photo-app/ui/components.slint`
- `loom-photo/crates/loom-photo-app/ui/product_workspace_v4.slint`

No shared crates, CLI sources, other applications, score files, or `TRUTH.md`
were modified.

## Implemented vertical slice

- Added persisted `Rect` geometry with finite/positive and document-bound
  validation, transformed bounds, and affine inverse support.
- Extended pixel layers with persisted affine transform, optional source crop,
  and a stable imported-payload digest. Extended documents with persisted
  canvas transform, selection, and nondestructive crop state.
- Replaced direct raster placement with inverse-affine compositing and
  bilinear sampling. Layer and canvas transforms, source crop, mask alpha,
  blend mode, opacity, and document crop all participate in the preview/export
  path.
- Added session operations for layer transform/crop, selection, crop, and
  canvas transform. Invalid/no-op edits are rejected or skipped before a
  history checkpoint; valid edits are undoable and survive package round-trip.
- Replaced the calibration-gradient sample with a deterministic encoded PNG
  still-life payload and route it through the same decode path as imports.
  Imported layers retain a stable `layer-imported-<digest>` identity and source
  digest across save/reopen.
- Bound the selected-layer inspector to live layer id/type, transform sliders,
  transformed bounds, selection/crop geometry, and adjustment controls.
  Pixel-layer transforms are enabled; adjustment/text/vector layers explain
  that transforms apply to pixel layers only.
- Added selected-layer bounds overlay and real callbacks for transform,
  selection, crop, brightness, contrast, saturation, import, and export.
  Callback paths mutate `PhotoSession`, refresh the UI, and report actionable
  errors for invalid import/export operations.
- Replaced the palette-only journey with a controller-backed journey covering
  import → select → transform → bounds selection → crop → brightness → undo →
  save → reopen → PNG export, followed by invalid import, failed export, and
  cancelled import checks. The existing keyboard palette journey remains a
  separate regression at the end.

## Core and app test coverage

Core tests cover affine operations/inversion, transformed layer bounds and
pixels, canvas transforms, selection/crop/adjustment validation and undo,
rejection without history entries, package round-trip, and real PNG/JPEG
exports. App tests cover imported payload identity, callback-backed editing and
reopen, dialog routing, command projection, palette/menu behavior, and the
existing Photo interaction contracts.

## Fresh verification (2026-08-31)

Commands were run after the final source edit from `loom-photo/` unless noted.

| Check | Actual result |
| --- | --- |
| `cargo fmt --all -- --check` | PASS (exit 0) |
| `cargo test --workspace --all-targets --locked` | PASS (exit 0): 42 `loom-photo-core` tests + 12 `loom-photo-app` tests; CLI target ran 0 tests; 0 failures |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS (exit 0); existing generated Slint component warnings only |
| `cargo build --workspace --release --locked` | PASS (exit 0) |
| `./target/release/loom-photo --smoke --size 1280x800 --theme dark` | PASS (exit 0) |
| `./loom-photo/target/release/loom-photo --screenshot .work/evidence/ui/photo-task-6-final-20260831/release-smoke.png --size 1280x800 --theme dark` | PASS (exit 0) |
| `file .work/evidence/ui/photo-task-6-final-20260831/release-smoke.png` | PNG image data, 1280 x 800, 8-bit/color RGBA, non-interlaced |
| `./loom-photo/target/release/loom-photo --journey .work/evidence/ui/photo-task-6-final-20260831 --size 1280x800 --theme dark` | PASS; printed `keyboard journey: PASS` and `photo journey: PASS` |
| `jq` palette transcript assertions | PASS: `passed=true`, `app=photo`, expected 10-step journey and final dismiss step |
| Vertical journey capture assertions | PASS: 14 `photo-vertical-*.png` files; all are 1280 x 800 8-bit RGBA PNGs |
| Export/package assertions | PASS: exported `photo-vertical.png` is a 480 x 300 RGBA PNG; `unzip -t photo-vertical.loomphoto` reports no errors |
| Failure/cancellation transcript assertions | PASS: invalid import, directory-target PNG export failure, and import cancellation messages are present in `photo-vertical.log` |
| `git diff --check` | PASS (exit 0) |

The first fresh Clippy run found two `manual_range_contains` diagnostics in
the deterministic sample generator. The expression was corrected to use
`Range::contains`; formatter, tests, Clippy, release build, and all journey
checks above were rerun on the corrected tree.

## Journey artifacts

The final release journey is retained under:

```text
.work/evidence/ui/photo-task-6-final-20260831/
```

It contains the 14 vertical captures (`initial`, `imported`, `selected`,
`transformed`, `selection`, `cropped`, `adjusted`, `undo-adjustment`, `saved`,
`reopened`, `exported`, `import-failure`, `export-failure`, and `import-cancel`),
the separate ten-step keyboard-palette captures and `photo.json`, the source
and invalid import fixtures, `photo-vertical.loomphoto`, `photo-vertical.png`,
`photo-vertical.log`, and the release smoke screenshot. The transformed and
adjusted captures were visually inspected after generation: the transformed
image is visibly rotated/scaled with updated inspector geometry, and the
adjusted image shows the brightness change and selection/crop state.

## Evidence boundaries and known limitations

- The screenshots and callbacks use the deterministic Slint software renderer
  and scripted dialogs. They prove controller/state/persistence behavior but
  not a foreground native AppKit or Linux portal click, screen-reader output,
  physical GPU rendering/performance, or OS file-panel integration.
- `canvas_transform` is persisted, composited, tested, and exposed in the core
  API, but the current app inspector has controls for the selected layer
  transform only; there is no dedicated canvas-transform inspector control.
- History remains bounded snapshot history for this slice. A production
  operation-delta journal, autosave crash replay, and shared render-graph/GPU
  path remain later roadmap work.
- Selection and crop geometry are persisted in the current Photo package, but
  native multi-selection, brush/mask editing, colour-managed ICC workflows,
  and full professional layer tooling remain outside this tranche.
