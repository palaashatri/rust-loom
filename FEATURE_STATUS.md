# Loom — Feature Status

Status classes: COMPLETE | FUNCTIONAL_WITH_LIMITATIONS | EXPERIMENTAL | SCAFFOLDED | NOT_STARTED | BLOCKED

Last updated: 2026-08-01 (fresh macOS audit; see VERIFICATION_REPORT.md and visual-qa-report.md)

## Shared Platform (loom-core)

| Area | Status | Evidence |
|------|--------|----------|
| Package format (ZIP + manifest, checksums, limits) | COMPLETE | loom-package tests green (cargo test --workspace: 84 passed) |
| Storage, atomic writes, recovery journal | COMPLETE | loom-storage tests green (7 passed, cross-platform) |
| Jobs (queue, priority, pause, cancel) | COMPLETE | loom-jobs 5/5, priority-order test made deterministic (gate job) |
| App runtime, commands, settings, logging | COMPLETE | crates present, tests green |
| Color pipeline foundation | FUNCTIONAL_WITH_LIMITATIONS | core conversions tested; ICC/HDR beyond initial scope |
| Text/typography foundation | EXPERIMENTAL | layout tests pass; shaping/fallback not yet built |
| Autosave | EXPERIMENTAL | atomic write + journal exist; periodic autosave not wired into apps |
| Undo/redo | FUNCTIONAL_WITH_LIMITATIONS | in-memory stacks in writer/sheets apps; no disk-backed history |
| UI library (loom-ui) | FUNCTIONAL_WITH_LIMITATIONS | components + smoke window; visual baseline green |
| Visual regression | FUNCTIONAL_WITH_LIMITATIONS | 16 fresh captures, 4 baseline comparisons at metric 0.000000, 12 required baselines missing |

## Applications

| App | Status | Notes |
|-----|--------|-------|
| loom-writer | FUNCTIONAL_WITH_LIMITATIONS | Runnable Slint app/CLI, `.loomdoc` package + manifest validation, PDF export, sample document, smoke/screenshot; the current UI displays a document but is not a full text editor; 6 tests |
| loom-sheets | FUNCTIONAL_WITH_LIMITATIONS | Runnable Slint app/CLI, formula engine, `.loomtable` package + manifest validation, CSV export, sample grid, smoke/screenshot; cells are not yet editable in the UI; 12 tests |
| loom-present | FUNCTIONAL_WITH_LIMITATIONS | Runnable model/CLI/Slint showcase, PDF export, `.loomdeck` package + manifest validation, selectable slide navigator, smoke/screenshot; sample-deck UI and open/edit workflows remain limited; 5 tests |
| loom-photo | FUNCTIONAL_WITH_LIMITATIONS | Runnable layer metadata model/CLI/Slint showcase, `.loomphoto` package + manifest validation, smoke/screenshot; pixel decode/compositing is not implemented; 3 tests |
| loom-motion | FUNCTIONAL_WITH_LIMITATIONS | Runnable layer/keyframe model/CLI/Slint showcase, `.loommotion` package + manifest validation, smoke/screenshot; interpolation/render/playback is not implemented; 3 tests |
| loom-video | FUNCTIONAL_WITH_LIMITATIONS | Runnable track/clip model/CLI/Slint showcase, `.loomvideo` package + manifest validation, smoke/screenshot; media decode/playback/export is not implemented; 3 tests |
| loom-studio | FUNCTIONAL_WITH_LIMITATIONS | Runnable track/region model/CLI/Slint showcase, `.loomstudio` package + manifest validation, smoke/screenshot; audio engine/export is not implemented; 3 tests |
| loom-encode | FUNCTIONAL_WITH_LIMITATIONS | Runnable queue/preset model/CLI/Slint showcase, `.loomencode` package + manifest validation, smoke/screenshot; encoder invocation and batch execution are not implemented; 2 tests |

## Shared platforms

| Area | Status | Notes |
|------|--------|-------|
| loom-vision | SCAFFOLDED | traits/provider registry/model-pack validation built + tested; no models shipped |
| loom-plugin-sdk | SCAFFOLDED | plugin host foundation built + tested; WASM sandbox not implemented |

## Quality gates (latest session runs)

- `bash -n scripts/*.sh`, `env-check.sh`, `build-all.sh`, `test-all.sh`, and `clippy-all.sh`: fresh macOS runs pass all 11 Cargo workspaces; clippy reports zero Loom-crate issues.
- `fmt-all.sh`: fresh macOS run passes all 11 Cargo workspaces after the Sheets, Studio, and Encode import-layout fixes.
- `scripts/run-apps.sh`: fresh release smoke exits pass for all 8 declared application binaries; no app is skipped.
- `scripts/visual-qa-all.sh`: intentionally **INCOMPLETE/FAIL** until all 16 required app baselines exist; current run captured 16, compared 4, found 0 diffs, and found 12 missing baselines.
- `VERIFICATION_REPORT.md`: generated from current `.work/` build, test, smoke, and visual evidence rather than metadata or stale prose.
