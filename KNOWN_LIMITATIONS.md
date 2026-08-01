# Loom — Known Limitations

Last updated: 2026-08-01. This list is honest, not aspirational. Anything not listed here that is described as implemented is verified in FEATURE_STATUS.md / VERIFICATION_REPORT.md.

## Cross-platform visual baselines
- The existing writer/sheets baselines and current host captures use the macOS
  font stack. Inside the Ubuntu CI container (`fonts-noto-core`) the same tests
  render glyph edges differently (the historical smoke comparison recorded a
  mean absolute error of 2.6077024936676025 and a differing ratio of
  0.030915431678295135), so those macOS images are not interchangeable with
  container images. New application baselines must be generated deliberately
  in the Docker visual environment at fixed 1280×800; they are never
  auto-approved. A pinned cross-platform font policy remains TODO.

## Applications
- All eight application repositories now contain compilable Rust/Slint binaries and headless smoke/screenshot paths. Present, Photo, Motion, Video, Studio, and Encode are small functional vertical slices, not documentation-only repositories; their production feature sets remain limited (see FEATURE_STATUS.md).
- Writer: the package model, sample document, PDF export, CLI, and persistence are implemented, but the current UI renders document text rather than providing a complete editable rich-text surface. No tables, footnotes, change tracking, or broad import/export beyond the `.loomdoc` package and PDF export.
- Writer PDF export is a minimal paginated renderer (title, headings, paragraphs); no embedded images, tables, or style tables yet.
- Sheets: fixed 8×6 visible grid window (virtualization not implemented), cells are not yet editable through the UI, and formulas are limited to the loom-sheets-core evaluator (basic arithmetic, SUM, AVERAGE, etc. as shipped); no charts, pivot tables, XLSX/ODS import yet.
- Sheets CSV export uses the core `to_csv`; import accepts .csv and .loomtable.
- Present: sample-deck UI and PDF export are functional; interactive editing, full open behavior, presenter display, and richer slide content are not complete.
- Photo: layer metadata and package persistence are functional; image decode, pixel buffers, compositing, and real adjustments are not complete.
- Motion: layer/keyframe metadata and persistence are functional; interpolation, rendering, preview, and playback are not complete.
- Video: track/clip metadata and persistence are functional; media decode, playback, trimming against media, and export are not complete.
- Studio: track/region metadata and persistence are functional; audio I/O, mixing, plugin hosting, and audio export are not complete.
- Encode: queue/preset metadata and persistence are functional; codec invocation, batch execution, pause/resume, and output inspection are not complete.
- Undo/redo in both apps is in-memory only and lost on exit; no disk-backed history.
- Autosave exists as a library (atomic write + journal) but is not yet wired into the apps.

## Shared platform
- Loom Vision: provider traits, registry, model-pack validation are implemented and tested; no model files are bundled (licensing) and no inference backend is wired.
- Plugin SDK: command/plugin host foundation exists; the WebAssembly/WASI sandbox is not implemented.
- Text shaping/fallback and full color management (ICC/HDR) are architecture only.
- Search/indexing, mail merge, localization catalogs, and pseudolocale are not yet implemented.

## Testing
- Fuzzing targets are not yet runnable in CI; property-test coverage is partial (serialization round-trips, undo invariants).
- E2E UI automation (menus, shortcuts, drag-and-drop) is not implemented; verification is screenshot + smoke based.
- Performance budgets have no automated benchmark harness yet.

## Process
- `package.sh`/`verify-package.sh` run on the host. The verifier now checks
  every extracted Cargo workspace with offline locked tests in a temporary
  target directory; it does not prove that a GUI can launch inside the
  extracted archive or that visual baselines are complete.
- The strict visual gate requires a baseline for every light/dark application capture. Current evidence has 12 missing baselines, so visual QA is not release-complete even though the 4 existing comparisons are clean.
