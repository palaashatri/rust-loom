# Loom suite build status

This is the current evidence-based snapshot, reconciled on 2026-08-02. The
complete package gate is the authoritative functional result; the interrupted
Docker CI run is recorded separately and is not counted as a completed
aggregate test run.

## Workspace matrix

| Repository | Cargo workspace | Build | Tests | Clippy | Format |
|---|---:|---:|---:|---:|---:|
| `loom-core` | yes | PASS | PASS (84) | PASS | PASS |
| `loom-writer` | yes | PASS | PASS (20) | PASS | PASS |
| `loom-sheets` | yes | PASS | PASS (18) | PASS | PASS |
| `loom-present` | yes | PASS | PASS (5) | PASS | PASS |
| `loom-photo` | yes | PASS | PASS (4) | PASS | PASS |
| `loom-motion` | yes | PASS | PASS (5) | PASS | PASS |
| `loom-video` | yes | PASS | PASS (4) | PASS | PASS |
| `loom-studio` | yes | PASS | PASS (4) | PASS | PASS |
| `loom-encode` | yes | PASS | PASS (3) | PASS | PASS |
| `loom-vision` | yes | PASS | PASS (72) | PASS | PASS |
| `loom-plugin-sdk` | yes | PASS | PASS (57) | PASS | PASS |

The counts above come from the extracted-package verification logs in
`loom-bootstrap/.work/verify-test-loom-*.log`, not source-file presence. All
11 extracted Cargo workspaces passed metadata and locked offline tests: 276
tests in total. Missing workspaces, missing binaries, and zero-test workspaces
make the orchestration gate incomplete/failing instead of producing a false
pass.

## Application and visual evidence

- All 8 declared application binaries built in release mode and all 8 exited
  `0` under `scripts/run-apps.sh`.
- Fresh visual captures: 16/16. Baselines compared: 16/16 with
  `metric=0.000000`, 0 diffs, 0 size mismatches, and 0 capture/comparison
  failures.
- The default light/dark visual gate is **PASS**. The full design-bible matrix
  (high contrast, text scale, reduced motion, locales, component states, and
  error states) remains unrun.
- All 16 screenshots were visually inspected in the Docker capture set. They
  show the eight sample applications with coherent light/dark layouts and
  visible default content/selection states.

## Interrupted Docker CI run

- Fresh Docker format and clippy gates completed for 11/11 workspaces, with
  zero Loom-crate clippy issues.
- The Docker test phase passed `loom-core`, `loom-writer`, `loom-sheets`, and
  `loom-present` before it was interrupted when photo testing began. It is not
  a completed aggregate Docker test result; the complete test result above is
  from the extracted package verifier.

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

The package gate is:

```bash
bash scripts/package.sh
bash scripts/verify-package.sh
```

Docker/offline results are recorded separately only after the corresponding
command completes; an interrupted run, image-build failure, or package
verification failure is not converted into PASS prose.
