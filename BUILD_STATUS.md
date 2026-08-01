# Loom suite build status

This is a current, evidence-based snapshot. The authoritative generated report
is [VERIFICATION_REPORT.md](VERIFICATION_REPORT.md); run the bootstrap gates
again before treating this page as release evidence.

## Workspace matrix

| Repository | Cargo workspace | Build | Tests | Clippy | Format |
|---|---:|---:|---:|---:|---:|
| `loom-core` | yes | PASS | PASS (84) | PASS | PASS |
| `loom-writer` | yes | PASS | PASS (6) | PASS | PASS |
| `loom-sheets` | yes | PASS | PASS (12) | PASS | PASS |
| `loom-present` | yes | PASS | PASS (4) | PASS | PASS |
| `loom-photo` | yes | PASS | PASS (3) | PASS | PASS |
| `loom-motion` | yes | PASS | PASS (3) | PASS | PASS |
| `loom-video` | yes | PASS | PASS (3) | PASS | PASS |
| `loom-studio` | yes | PASS | PASS (3) | PASS | PASS |
| `loom-encode` | yes | PASS | PASS (2) | PASS | PASS |
| `loom-vision` | yes | PASS | PASS (72) | PASS | PASS |
| `loom-plugin-sdk` | yes | PASS | PASS (57) | PASS | PASS |

The counts above come from the current `.work/` logs, not source-file
presence. Missing workspaces, missing binaries, and zero-test workspaces now
make their orchestration gate incomplete/failing instead of producing a false
pass.

## Application and visual evidence

- All 8 declared application binaries built in release mode and all 8 exited
  `0` under `scripts/run-apps.sh`.
- Fresh visual captures: 16/16. Existing baselines compared: 4/4 with
  `metric=0.000000` and 0 diffs.
- Required baselines still missing: 12 (Present, Photo, Motion, Video,
  Studio, and Encode in both themes). Therefore the visual gate is
  **INCOMPLETE/FAIL**, not PASS.
- The fresh screenshots were inspected. They show the sample models and both
  light/dark theme paths; the missing-baseline state is the remaining visual
  release blocker.

## Commands

Run from `loom-bootstrap/`:

```bash
bash scripts/env-check.sh
bash scripts/fmt-all.sh
bash scripts/clippy-all.sh
bash scripts/test-all.sh
bash scripts/build-all.sh --release
bash scripts/run-apps.sh
bash scripts/visual-qa-all.sh
```

Docker CI/offline/visual results are recorded separately only after the
corresponding command completes successfully; an image-build or package
verification failure is not converted into PASS prose.
