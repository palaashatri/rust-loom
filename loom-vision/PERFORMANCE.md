# Performance

## Benchmark harness

```sh
cargo run --release --bin loom-vision -- bench crates/loom-vision-cli/fixtures/hello.png
```

Runs the QR provider 20 times on a 232×232 image and prints min / median /
max wall-clock milliseconds, plus how many runs decoded. Release profile is
required for meaningful numbers (debug builds are 10–100× slower).

Reference measurements (2026-08-01, Apple M-series laptop, release build):
min ~16.6 ms, median ~17.0 ms, max ~17.5 ms per decode — dominated by
`rqrr` grid search, which scales with image area.

## Budgets and rules

- No synchronous file or media operations on any UI thread — Loom Vision
  itself is called from background jobs; `run` never performs I/O.
- Providers must poll `check_cancelled()` every few rows/iterations so
  cancellation feedback is immediate.
- Memory is bounded: QR decoding allocates one luma buffer plus rqrr's
  working set (~3× image bytes); model-pack validation streams files in
  64 KiB chunks (never loads a model into memory) and enforces a 2 GiB
  total unpacked limit.
- Regression rule: `bench` timings for the same fixture and machine must
  not regress more than 50% without a reviewed waiver; CI will re-measure
  on the reference machine.

## Where to look if it's slow

1. `image_to_luma_checked` — single pass, cache-friendly row loops.
2. `QrCodeProvider::run` — rqrr `detect_grids` dominates; downscale input
   before decode when the QR occupies a small fraction of the frame.
3. Model-pack validation — SHA-256 streaming dominates; this is bounded by
   disk bandwidth.
