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

## Round-1 review fixes (2026-08-31)

The review follow-up closes the nine reported correctness and interaction
findings. Geometry validation now rejects overflowing rectangle edges,
non-finite or non-invertible affine matrices, malformed persisted documents,
and invalid composed canvas/layer transforms. Compositing and bounds use the
single composed transform, bounds are payload-aware, and non-pixel layers do
not expose pixel geometry. The app now scopes adjustment controls to the
active adjustment layer, disables mismatched sliders, keeps the inspector
bounded, maps overlays to the document aspect ratio, separates pan mode from
painting, and exposes active-layer crop operations through the controller.

### Focused regression checks

Commands were run from `loom-photo/` after the review edits. Each listed
focused test reported `1 passed; 0 failed`:

```text
cargo test -p loom-photo-core --locked geometry_validation_rejects_overflow_and_unsafe_affines
cargo test -p loom-photo-core --locked document_validation_rejects_malformed_persisted_state
cargo test -p loom-photo-core --locked non_pixel_layers_cannot_receive_pixel_geometry_or_bounds
cargo test -p loom-photo-core --locked layer_bounds_compose_canvas_and_layer_transforms_from_source_corners
cargo test -p loom-photo-core --locked layer_crop_changes_composited_pixels_without_changing_canvas_size
cargo test -p loom-photo-core --locked transformed_layer_bounds_and_pixels_follow_persisted_geometry
cargo test -p loom-photo-core --locked canvas_transform_is_applied_to_every_layer_and_round_trips
cargo test -p loom-photo-core --locked rejected_geometry_edits_do_not_create_history_entries
cargo test -p loom-photo-core --locked selection_crop_and_adjustment_edits_are_validated_and_undoable
cargo test -p loom-photo-app --locked adjustment_callbacks_are_scoped_to_the_active_adjustment_layer
cargo test -p loom-photo-app --locked photo_edit_callbacks_mutate_selection_transform_crop_and_adjustment
cargo test -p loom-photo-app --locked pan_tool_is_a_viewport_mode_separate_from_brush
cargo test -p loom-photo-app --locked imported_raster_canvas_preserves_payload_identity_through_reopen
```

The focused core run covered nine tests and the focused app run covered four
tests; all reported zero failures. `cargo fmt --all -- --check` and
`git diff --check` also exited 0.

### Full and release verification

| Check | Actual result |
| --- | --- |
| `cargo test --workspace --all-targets --locked` | PASS (exit 0): 13 `loom-photo-app` tests + 47 `loom-photo-core` tests; CLI target ran 0 tests; 0 failures |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | PASS (exit 0); generated Slint export/deprecation warnings only |
| `cargo build --workspace --release --locked` | PASS (exit 0): `Finished release profile [optimized]` |
| `./loom-photo/target/release/loom-photo --journey .work/evidence/ui/photo-task-6-release-review1-20260831 --size 1280x800 --theme dark` | PASS; printed `keyboard journey: PASS` and `photo journey: PASS` |
| `./loom-photo/target/release/loom-photo --smoke --size 1280x800 --theme dark` | PASS (exit 0) |
| `./loom-photo/target/release/loom-photo --screenshot .work/evidence/ui/photo-task-6-release-review1-20260831/release-smoke.png --size 1280x800 --theme dark` | PASS (exit 0) |
| `file .../release-smoke.png` | PNG image data, 1280 x 800, 8-bit/color RGBA, non-interlaced |
| `jq` palette transcript assertions | PASS: `passed=true`, `app=photo`, 10 steps, final `5-dismiss` step |
| Vertical journey capture assertions | PASS: 17 `photo-vertical-*.png` files; all are 1280 x 800 8-bit RGBA PNGs |
| Export/package assertions | PASS: exported `photo-vertical.png` is a 480 x 300 RGBA PNG; `unzip -t photo-vertical.loomphoto` reports no errors |
| Failure/cancellation transcript assertions | PASS: invalid import, directory-target PNG export failure, and import cancellation messages are present in `photo-vertical.log` |
| `git diff --check` | PASS (exit 0) |

The release journey is the exact optimized binary built by the release check;
the screenshots and transcripts are retained as inspectable evidence below.

## Journey artifacts

The review-fix release journey is retained under:

```text
.work/evidence/ui/photo-task-6-release-review1-20260831/
```

It contains 17 vertical captures (`initial`, `imported`, `selected`,
`transformed`, `pan-zoom`, `selection`, `cropped`, `layer-cropped`,
`adjustment-layer`, `adjusted`, `undo-adjustment`, `saved`, `reopened`,
`exported`, `import-failure`, `export-failure`, and `import-cancel`), the
separate ten-step keyboard-palette captures and `photo.json`, the source and
invalid import fixtures, `photo-vertical.loomphoto`, `photo-vertical.png`,
`photo-vertical.log`, and the optimized release smoke screenshot. The
transformed, pan/zoom, crop, adjusted, and failure captures were visually
inspected after generation; the transformed image shows rotated/scaled
geometry, pan/zoom changes the viewport, crop state is visible in the
inspector, brightness changes the pixels, and the failure state is actionable.

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

## Round-2 review fixes (2026-08-31)

The scoped re-review identified five remaining Important findings; this pass
closes each one without changing shared crates, other applications, score
files, or `TRUTH.md`:

- Active-layer crop now maps the document-space selection through the inverse
  composed canvas/layer transform, clips it to the source payload, and stores
  source-local coordinates. A pixel-level callback regression proves that a
  translated 4×1 layer keeps the selected source pixel at its translated
  destination and clears the adjacent pixels.
- Document and session transform setters validate the composed affine before
  mutation or `checkpoint()`. Overflowing but individually finite translations
  are rejected, leave both transforms unchanged, and do not create undo
  entries.
- `PhotoDocument::validate` rejects non-pixel layers carrying crop or
  non-identity transform state. A hand-built package with a valid manifest and
  checksum is rejected by both `load_photo` and `load_photo_canvas` for each
  malformed geometry variant.
- Inspector bounds selection is derived from `active_layer_bounds()` and is
  disabled when the active pixel layer has no payload. Invoking the callback in
  that state leaves selection/history unchanged and reports the actionable
  `Selection failed: selected layer has no visible bounds` status.
- The inspector Flickable now derives its viewport extent from the named
  content layout's preferred height, binds scroll state through the app, and
  removes the fixed 720px extent and spacer. The journey and a focused app
  regression exercise a negative lower-content scroll position.

### Round-2 verification

Commands were run from `loom-photo/` against the same `cline-implementation`
working tree:

```text
cargo test -p loom-photo-core --locked composed_transform_overflow_is_rejected_without_mutating_or_checkpointing
1 passed; 0 failed

cargo test -p loom-photo-core --locked malformed_non_pixel_geometry_is_rejected_from_memory_and_packages
1 passed; 0 failed

cargo test -p loom-photo-app --locked transformed_layer_crop_callback_maps_document_selection_to_source_pixels
1 passed; 0 failed

cargo test -p loom-photo-app --locked layer_bounds_selection_is_disabled_without_a_pixel_payload
1 passed; 0 failed

cargo test -p loom-photo-app --locked inspector_scroll_state_accepts_lower_content_positions
1 passed; 0 failed

cargo fmt --all -- --check
PASS (exit 0)

cargo test --workspace --all-targets --locked
PASS (exit 0): 16 `loom-photo-app` tests + 49 `loom-photo-core` tests; CLI
target ran 0 tests; 0 failures

cargo clippy --workspace --all-targets --locked -- -D warnings
PASS (exit 0); generated Slint export/deprecation warnings only

cargo build --workspace --release --locked
PASS (exit 0): Finished release profile [optimized]

./target/release/loom-photo --journey ../.work/evidence/ui/photo-task-6-release-review2-20260831 --size 1280x800 --theme dark
PASS (exit 0): printed `keyboard journey: PASS` and `photo journey: PASS`
```

The generated evidence at
`.work/evidence/ui/photo-task-6-release-review2-20260831/` was checked with
the following results:

- 17 `photo-vertical-*.png` captures are 1280×800 8-bit RGBA PNGs.
- `photo-vertical.png` is a 480×300 8-bit RGBA PNG.
- `photo.json` reports `passed=true`, `app=photo`, ten palette steps, and a
  final `5-dismiss` step.
- `unzip -t photo-vertical.loomphoto` reports all three entries `OK` and no
  errors.
- `photo-vertical.log` contains the invalid-import, directory-target PNG
  export, and cancelled-import messages.
- The adjustment-layer capture was visually inspected after generation; the
  lower controls are visible while the inspector is scrolled.
- `git diff --check` exited 0.

The evidence remains bounded to deterministic Slint/software-renderer and
scripted-dialog behavior. Native AppKit/Linux portal delivery, screen-reader
runtime output, GPU rendering, and production operation-level recovery remain
unverified roadmap work.
