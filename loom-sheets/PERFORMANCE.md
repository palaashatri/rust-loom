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

On the local Intel Core i3-2350M (2 cores, 7.7 GiB RAM), the optimized 10,000-formula test passed: calculation **100.3 ms**, JSON save preparation **127.2 ms** for 319,175 bytes, and JSON parse **20.7 ms**. The command used the release profile and the explicit 200 ms assertion. This is a recorded result on this machine; it does not establish the separate mainstream desktop profile.

View-only updates now reuse the active sheet's last calculated values. A workbook edit refreshes them. Recalculation on a committed edit still runs synchronously, so the 16.7 ms UI-response rule is not yet proved. The current integration test also does not measure 1,000,000-cell scrolling or peak RSS; those need a real-window workload harness on the representative profile. The memory budget still needs a numeric owner-approved limit.
