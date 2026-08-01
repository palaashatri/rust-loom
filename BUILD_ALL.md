# Build All

Commands to build every Loom repository. Requirements: Rust >= 1.80, Slint 1.17.1 (pinned in lockfiles).

## One command (orchestrated)

```bash
cd loom-bootstrap
bash scripts/build-all.sh          # builds every existing cargo workspace (release)
bash scripts/env-check.sh          # verifies toolchain and QA tools
```

## Per-repo (host)

```bash
cd loom-core    && cargo build --workspace
cd loom-writer  && cargo build --workspace
cd loom-sheets  && cargo build --workspace
cd loom-vision  && cargo build --workspace
cd loom-plugin-sdk && cargo build --workspace
```

Application binaries (debug): `loom-writer/target/debug/loom-writer`, `loom-sheets/target/debug/loom-sheets`.

## Docker

```bash
cd loom-bootstrap
bash scripts/docker-build.sh       # builds the Ubuntu 24.04 toolchain image
bash scripts/docker-test.sh        # runs fmt/clippy/test/build inside the container
```

## Status

- loom-core: PASS (builds, fmt, clippy -D warnings, 84 tests)
- loom-writer, loom-sheets: PASS (builds + app binaries)
- loom-vision, loom-plugin-sdk: PASS
- present/photo/motion/video/studio/encode: NOT_STARTED — no Cargo workspace yet; build-all.sh reports SKIP (expected).
