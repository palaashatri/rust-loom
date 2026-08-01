# Loom — Known Limitations

Last updated: 2026-08-01. This list is honest, not aspirational. Anything not listed here that is described as implemented is verified in FEATURE_STATUS.md / VERIFICATION_REPORT.md.

## Cross-platform visual baselines
- Visual regression baselines (loom-ui smoke, writer, sheets) are generated on macOS with the macOS font stack. Inside the Ubuntu CI container (fonts-noto-core) the same tests render glyph edges differently (mean abs error ≈ 2.6, differing ratio ≈ 3% on the smoke window), so the container cannot reuse macOS baselines. Structural layout is identical. Deterministic cross-platform baselines require a documented pinned font stack in the visual image (TODO).

## Applications
- Only two of eight applications exist as code: **loom-writer** and **loom-sheets**. present/photo/motion/video/studio/encode repositories contain specifications and documentation only (see FEATURE_STATUS.md). Their README build/test instructions are NOT yet runnable.
- Writer: only a fixed set of block styles (heading1/heading2/paragraph) and basic inline formatting are implemented. No tables, footnotes, change tracking, import/export beyond the .loomdoc package and PDF export.
- Writer PDF export is a minimal paginated renderer (title, headings, paragraphs); no embedded images, tables, or style tables yet.
- Sheets: fixed 8×6 visible grid window (virtualization not implemented), formulas limited to the loom-sheets-core evaluator (basic arithmetic, SUM, AVERAGE, etc. as shipped); no charts, pivot tables, XLSX/ODS import yet.
- Sheets CSV export uses the core `to_csv`; import accepts .csv and .loomtable.
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
- `package.sh`/`verify-package.sh` run on the host; the extracted-copy verification runs a lightweight pass (env check, cargo metadata, loom-core tests) — full suite-from-archive verification is a manual step.
