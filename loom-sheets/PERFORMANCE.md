# Loom Sheets performance gate

These checks use the fixed budgets in [`loom-design-bible/PERFORMANCE.md`](../loom-design-bible/PERFORMANCE.md) and [`SPREADSHEET.md`](../loom-design-bible/SPREADSHEET.md). A passing formula-engine test does not prove that scrolling, typing, or the live window stays responsive.

## Required budgets

- Recalculate 10,000 chained formulas in under 200 ms on the mainstream desktop profile.
- Give the user visible input feedback within one 60 Hz frame (16.7 ms). No calculation or file operation may block the UI thread.
- Scroll a worksheet containing 1,000,000 randomly placed values at 60 fps on the mainstream profile.
- Measure peak resident memory on that same representative workbook. The repository's higher-level performance contract requires each app to name a memory budget, but it does not provide a numeric Sheets limit yet. Until that limit is reviewed and recorded, the memory part of this gate is open.

## Formula benchmark

The `perf_measure` integration test creates 500 rows with 20 chained formulas per row. It checks that the formula count is exactly 10,000 and that the last value is correct.

Run a quick correctness and timing sample without a budget assertion:

```sh
CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 \
  cargo test --manifest-path loom-sheets/Cargo.toml --locked \
  -p loom-sheets-core --test perf_measure -- --nocapture
```

Run the 200 ms budget check in the optimized test profile:

```sh
LOOM_ENFORCE_PERF_BUDGET=1 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 \
  cargo test --manifest-path loom-sheets/Cargo.toml --locked --release \
  -p loom-sheets-core --test perf_measure -- --nocapture
```

### Latest local result — 2026-09-24

On the local Intel Core i3-2350M (2 cores, 7.7 GiB RAM), the latest optimized 10,000-formula test passed: calculation **88.5 ms**, JSON save preparation **20.4 ms** for 319,175 bytes, and JSON parse **10.4 ms**. The command used the release profile and the explicit 200 ms assertion. Raw output is `.work/sheets-acceptance-2026-09-24/formula-perf-rerun.log`. This is a recorded result on this machine; it does not establish the separate mainstream desktop profile.

## Million-cell projection sample — 2026-09-24

The opt-in app test `million_sparse_cells_project_one_viewport_within_one_frame` fills a 2,048×2,048 address space with 1,000,000 sparse numeric cells. A fixed-key Feistel permutation assigns each cell a unique, random-looking address without allocating a second million-entry index. It checks all four workbook regions, then projects the visible cells at 60 positions for a 1,024×720 viewport, including the origin and clamped far corner. It asserts the visible-cell count is exact and each CPU projection finishes below 16.7 ms.

Run the optimized assertion with:

```sh
LOOM_ENFORCE_SCROLL_BUDGET=1 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=1 \
  cargo test --manifest-path loom-sheets/Cargo.toml --locked --offline --release \
  -p loom-sheets-app million_sparse_cells_project_one_viewport_within_one_frame -- --nocapture
```

On the same Intel Core i3-2350M, the standalone release test measured **0.17 ms p95** and **0.26 ms maximum** across the 60 CPU-side projections. `/usr/bin/time -v` measured **90,308 KiB peak RSS** for the test process, which included the one-million-cell fixture and test harness; it reported **0 swaps**. This is a test-process measurement, not the complete interactive app's memory use.

This is not a real-window scroll result: it excludes Slint rendering, native frame scheduling, compositing, and frame presentation. The native one-million-cell scroll check and a numeric owner-reviewed app memory cap remain open. A committed formula edit still clones workbook tabs, recalculates, and writes a recovery snapshot synchronously; the 16.7 ms input-feedback rule is not yet proved. Preserve these as separate costs when implementing a bounded background worker: moving calculation alone does not move workbook cloning or recovery-journal writes off the UI thread.
