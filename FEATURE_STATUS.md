# Loom — Feature Status

Status classes: COMPLETE | FUNCTIONAL_WITH_LIMITATIONS | EXPERIMENTAL | SCAFFOLDED | NOT_STARTED | BLOCKED

Last updated: 2026-08-01 (session-verified; see VERIFICATION_REPORT.md and visual-qa-report.md)

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
| Visual regression | COMPLETE | snapshot pipeline + baseline + tolerance; fonts are host-dependent (see KNOWN_LIMITATIONS.md) |

## Applications

| App | Status | Notes |
|-----|--------|-------|
| loom-writer | FUNCTIONAL_WITH_LIMITATIONS | Slint UI, themes, sample doc, .loomdoc save/load, PDF export, undo/redo, --screenshot/--smoke; visual QA PASS (light+dark, metric 0.0) |
| loom-sheets | FUNCTIONAL_WITH_LIMITATIONS | Slint UI (8×6 grid, formula bar, headers), formulas SUM/AVERAGE via loom-sheets-core, .loomtable save/load, CSV export, undo/redo; visual QA PASS |
| loom-present | NOT_STARTED | repo exists (docs only) |
| loom-photo | NOT_STARTED | repo exists (docs only) |
| loom-motion | NOT_STARTED | repo exists (docs only) |
| loom-video | NOT_STARTED | repo exists (docs only) |
| loom-studio | NOT_STARTED | repo exists (docs only) |
| loom-encode | NOT_STARTED | repo exists (docs only) |

## Shared platforms

| Area | Status | Notes |
|------|--------|-------|
| loom-vision | SCAFFOLDED | traits/provider registry/model-pack validation built + tested; no models shipped |
| loom-plugin-sdk | SCAFFOLDED | plugin host foundation built + tested; WASM sandbox not implemented |

## Quality gates (latest session runs)

- cargo fmt --check: PASS (loom-core, loom-writer, loom-sheets)
- cargo clippy --all-targets -- -D warnings: PASS (0 diagnostics)
- cargo test --workspace: PASS (loom-core 84, loom-writer 6, loom-sheets 12)
- scripts/run-apps.sh: PASS (writer, sheets smoke; 6 apps skipped — no binary)
- scripts/visual-qa-all.sh: PASS (writer + sheets, light + dark, metric 0.000000)
- scripts/test-all.sh --offline (container, --network none): PASS for writer/sheets/vision/plugin-sdk; loom-core fails only on the font-dependent visual baseline (see KNOWN_LIMITATIONS.md)
- scripts/build-all.sh, fmt-all.sh, clippy-all.sh, env-check.sh: PASS
