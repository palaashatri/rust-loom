# Loom Sheets acceptance follow-up — 2026-09-24

## Status

Sheets is **not 100% accepted**. The source, core suite, app suite, current Linux application build, and one independent LibreOffice round-trip can be checked locally. The formal gate stays `ACCEPTANCE_BLOCKED` because the fixed Linux menu has not yet been inspected in an unlocked live window, and the full visual, responsiveness, memory, scrolling, accessibility, and cross-application checks are not complete.

## What changed

- The in-window File/Edit/View/Table/Help menu now gives the popup only the selected menu's rows. The old code let hidden rows from other menus take up space, so Edit appeared far below its menu label. The old native Edit capture is preserved as `loom-sheets/docs/qa-native/ui01-edit-menu-open-before-fix-linux.png`; it is before-fix evidence.
- XLSX export now gives sanitized sheet names unique names and rewrites formula and chart references to the names actually written to the file. For example, source tabs `A/B` and `AB` export as `AB` and `AB (2)` instead of silently colliding.
- Dynamic-array spill placement now uses row and column dimensions in the right order. A 2×3 `SEQUENCE` fills B1:D2 and formulas that read those cells before the spill are recalculated. A blocked spill leaves no partial values behind.
- The formula evaluator skips spill-reader bookkeeping when a workbook has no array formulas. The active sheet's calculated values are also reused for view-only updates (selection, scrolling, and resize) and recalculated after edits or a tab change.
- `loom-sheets/PERFORMANCE.md`, this report, the app README, `TRUTH.md`, and the tiny-model performance instructions in `AGENTS.MD` were updated with measured results and remaining limits.

## Automated and local verification

- Core: `CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-core` — **109 passed** (91 unit, 13 formula, 1 performance-fixture, 4 range; no failures).
- App: `SLINT_EMIT_DEBUG_INFO=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app` — **116 passed**, plus the separate evaluation-cache integration test **1 passed**. Slint debug metadata is required for the accessibility-tree tests.
- CI-equivalent workspace tests: `SLINT_EMIT_DEBUG_INFO=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --workspace --locked --offline` — **226 passed, 0 failed**, covering 116 app tests, 1 evaluation-cache integration test, 91 core unit tests, 13 formula tests, 1 performance fixture, and 4 range tests.
- CI-equivalent workspace lint: `PKG_CONFIG_PATH=/tmp/loom-fontconfig RUSTFLAGS='-L native=/tmp/loom-fontconfig' SLINT_EMIT_DEBUG_INFO=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo clippy --manifest-path loom-sheets/Cargo.toml --workspace --all-targets --locked --offline -- -D warnings` — passed. The first run identified a complex XLSX-name tuple; the helper now returns a named result struct. After removing the menu's redundant width binding, the focused all-target Sheets app Clippy check also passed, and the Sheets layout-cycle warning disappeared.
- Focused final app rebuild: `PKG_CONFIG_PATH=/tmp/loom-fontconfig RUSTFLAGS='-L native=/tmp/loom-fontconfig' SLINT_EMIT_DEBUG_INFO=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo build --manifest-path loom-sheets/Cargo.toml --locked --offline -p loom-sheets-app` — succeeded. Binary `loom-sheets/target/debug/loom-sheets` SHA-256: `f947c32a363d9146f245ef8d9d5380506eff946bc99fdbf47fa35fa0a2fe1667`.
- Fresh renderer screenshot, regenerated from the final app build at 1024×720: `loom-sheets/docs/qa-renderer/ui01-local-menu-current-1024-linux.png` (SHA-256 `4e556f6d3aeed49c2c62c5361ac29effb6f3dbea37ba4eb163ed7b1ec11744b`). It is software-rendered and does not replace the missing native screenshot.
- The focused tab-rename/tab-switch regression `rename_rewrites_qualifiers_and_rejects_collisions` passes. It checks the cross-sheet result stays 25 after the source tab is renamed.
- The evaluation-cache test checks that the same tab reuses one result, switching tabs recalculates, and an edit refreshes the values.
- Optimized 10,000-formula budget: `LOOM_ENFORCE_PERF_BUDGET=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 nice -n 19 ionice -c3 cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline --release -p loom-sheets-core --test perf_measure -- --nocapture` — passed the 200 ms calculation threshold at **100.3 ms** on an Intel Core i3-2350M, 2 cores, 7.7 GiB RAM. JSON output was 319,175 bytes and took 127.2 ms; parse took 20.7 ms. This is one low-end local machine result, not the specified mainstream desktop profile.
- Independent spreadsheet check: LibreOffice Calc **24.2.7.2** opened `interop/loom-name-collision-v2.xlsx` and converted it to `interop/lo-output/loom-name-collision-v2.ods`. The tabs remained `AB`, `AB (2)`, `Very Long Sheet Name That Excee`, and `Consumer`; the rewritten cross-sheet formula recalculated to `80`; and the quoted text `A/B!B2` remained text. The fixture, converter input/output, and export source are in the portable audit bundle under `.work/sheets-acceptance-2026-09-24/interop/`. This does not prove Excel or other spreadsheet applications behave identically.
- `cargo fmt --manifest-path loom-sheets/Cargo.toml --all -- --check` and `git diff --check` passed after the final source and documentation edits. The portable audit archive passed `unzip -t` integrity validation.
- The separate repo-wide `python3 loom-bootstrap/scripts/audit-code-structure.py` still fails on six legacy byte limits in locked apps (Encode, Motion, Video app/core, Writer core, and Photo). None is a Sheets source; no app-wide size limit was relaxed.

The first app run omitted `SLINT_EMIT_DEBUG_INFO=1`; two tests that inspect Slint's accessibility tree could not see elements. That run was invalid for those assertions. The rerun with Slint debug metadata passed all 116 app tests. During cache testing, direct test-only tab switching also exposed a stale result; the cache now checks the active tab and the regression passes.

## Still blocking a truthful acceptance claim

- **Native menu check:** Cinnamon reports its full-screen screensaver is active above the Sheets window. A native screenshot attempt showed the lock overlay, not Loom; that invalid screenshot was deleted. No lock or display setting was bypassed. Please unlock the desktop before capturing and checking the current menu. The app must be inspected with the native screenshot application and pointer/keyboard at supported sizes; the renderer preview is not a substitute.
- **Visual scope:** all required viewports, themes, text sizes, and RTL states still need a complete inspected matrix. This device's live display is 1366×768, so a native 1440×900 capture is not available here.
- **Responsiveness:** selection, scroll, resize, and tab changes reuse calculated values. A committed large formula edit still recalculates synchronously. Measure input feedback against the 16.7 ms rule and move calculation off the UI thread if it misses.
- **Scale and memory:** no real-window test proves 1,000,000 randomly placed cells scroll at 60 fps. The app's peak-memory limit still has no reviewed numeric target.
- **Interoperability:** only the described XLSX fixture was opened in LibreOffice. Microsoft Excel and the wider import/export feature matrix remain unverified. Unsupported workbook features must stay visible as explicit boundaries.
- **Accessibility:** existing native evidence covers the named Linux flows in the 2026-09-23 report. Full keyboard command, screen-reader, dialog error/cancel, and cross-platform coverage remains open.

Keep the app gate `ACCEPTANCE_BLOCKED` until each of these items has its own observed evidence. Do not report absolute 100% based on unit tests or the single local benchmark.
