# Loom — Build All

Build every Cargo workspace in the suite. The canonical build environment is
the pinned Ubuntu container (`loom-bootstrap/docker/`), but the same commands
run on any host with Rust >= 1.80.

## Prerequisites

- Rust stable >= 1.80 (`bash loom-bootstrap/scripts/env-check.sh` verifies)
- Container (optional): `docker compose -f loom-bootstrap/docker/compose.yaml build ci`

## Commands

```bash
# Full suite, release profile (default)
bash loom-bootstrap/scripts/build-all.sh

# Full suite, debug profile
bash loom-bootstrap/scripts/build-all.sh --debug

# Single workspace
cd loom-writer && cargo build --release --workspace
```

## Workspaces

`loom-core`, `loom-writer`, `loom-sheets`, `loom-present`, `loom-photo`,
`loom-motion`, `loom-video`, `loom-studio`, `loom-encode`, `loom-vision`,
`loom-plugin-sdk` (11 Cargo workspaces).

## Latest verified result (2026-08-04)

`build-all.sh --release`: `SUMMARY build: pass=11 skip=0 fail=0 -> RESULT: PASS`
(binaries for all eight applications plus CLIs produced).

## In-container variant

```bash
docker run --rm -it \
  -e CARGO_HOME=/cargo -e CARGO_TARGET_DIR=/targets -e CARGO_INCREMENTAL=0 \
  -v loom-dev-cargo-registry:/cargo -v loom-dev-targets:/targets \
  -v "$PWD":/workspace loom-bootstrap-ci bash -lc \
  "bash loom-bootstrap/scripts/build-all.sh"
```

Build logs: `loom-bootstrap/.work/build-<repo>.log`.
