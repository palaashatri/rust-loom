# Loom — Feature Status

Status classes: COMPLETE | FUNCTIONAL_WITH_LIMITATIONS | EXPERIMENTAL | SCAFFOLDED | NOT_STARTED | BLOCKED

Last updated: 2026-08-02 (package verification and Docker visual audit; see VERIFICATION_REPORT.md and visual-qa-report.md)

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
| Visual regression | FUNCTIONAL_WITH_LIMITATIONS | 16 default light/dark captures and 16 baseline comparisons at metric 0.000000; full design-bible matrix not run |

## Applications

| App | Status | Notes |
|-----|--------|-------|
| loom-writer | FUNCTIONAL_WITH_LIMITATIONS | Runnable Slint app/CLI, `.loomdoc` package + manifest validation, PDF export, sample document, smoke/screenshot, and editable multiline text surface; full rich-text editing remains limited; 20 tests |
| loom-sheets | FUNCTIONAL_WITH_LIMITATIONS | Runnable Slint app/CLI, formula engine, `.loomtable` package + manifest validation, CSV export, sample grid, smoke/screenshot, and formula/value-bar editing for the selected cell; full in-grid editing and virtualization remain limited; 18 tests |
| loom-present | FUNCTIONAL_WITH_LIMITATIONS | Runnable model/CLI/Slint showcase, PDF export, `.loomdeck` package + manifest validation, selectable slide navigator, smoke/screenshot; sample-deck UI and open/edit workflows remain limited; 5 tests |
| loom-photo | FUNCTIONAL_WITH_LIMITATIONS | Runnable layer metadata model/CLI/Slint showcase, `.loomphoto` package + manifest validation, smoke/screenshot; pixel decode/compositing is not implemented; 4 tests |
| loom-motion | FUNCTIONAL_WITH_LIMITATIONS | Runnable layer/keyframe model/CLI/Slint showcase, `.loommotion` package + manifest validation, smoke/screenshot; interpolation/render/playback is not implemented; 5 tests |
| loom-video | FUNCTIONAL_WITH_LIMITATIONS | Runnable track/clip model/CLI/Slint showcase, `.loomvideo` package + manifest validation, smoke/screenshot; media decode/playback/export is not implemented; 4 tests |
| loom-studio | FUNCTIONAL_WITH_LIMITATIONS | Runnable track/region model/CLI/Slint showcase, `.loomstudio` package + manifest validation, smoke/screenshot; audio engine/export is not implemented; 4 tests |
| loom-encode | FUNCTIONAL_WITH_LIMITATIONS | Runnable queue/preset model/CLI/Slint showcase, `.loomencode` package + manifest validation, smoke/screenshot; encoder invocation and batch execution are not implemented; 3 tests |

## Shared platforms

| Area | Status | Notes |
|------|--------|-------|
| loom-vision | SCAFFOLDED | traits/provider registry/model-pack validation built + tested; no models shipped |
| loom-plugin-sdk | SCAFFOLDED | plugin host foundation built + tested; WASM sandbox not implemented |

## Quality gates (latest session runs)

- `bash -n scripts/*.sh`, `env-check.sh`, `build-all.sh`, `test-all.sh`, and `clippy-all.sh`: the complete extracted-package gate passes all 11 Cargo workspaces; the fresh Docker test phase was interrupted during photo testing and is not reported as a completed aggregate run.
- `fmt-all.sh` and `clippy-all.sh`: fresh Docker gates pass all 11 workspaces; clippy reports zero Loom-crate issues.
- `scripts/run-apps.sh`: fresh release smoke exits pass for all 8 declared application binaries; no app is skipped.
- `scripts/visual-qa-all.sh`: default light/dark gate **PASS**; current run captured 16, compared 16, found 0 diffs, 0 missing baselines, and 0 input failures. The full design-bible matrix remains unrun, and the newly added baselines make this a capture/baseline consistency gate rather than independent historical regression evidence.
- `VERIFICATION_REPORT.md`: reconciled from current package, build, test, smoke, and visual evidence rather than metadata or stale prose.
