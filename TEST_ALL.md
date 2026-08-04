# Loom — Test All

## Commands

```bash
bash loom-bootstrap/scripts/test-all.sh            # full suite
bash loom-bootstrap/scripts/test-all.sh --offline  # offline (locked) variant
bash loom-bootstrap/scripts/clippy-all.sh
bash loom-bootstrap/scripts/fmt-all.sh
bash loom-bootstrap/scripts/env-check.sh
bash loom-bootstrap/scripts/offline-test.sh --mode-b  # network-disabled container
```

## Latest verified results (2026-08-04)

| Gate | Result |
|---|---|
| `env-check.sh` | PASS (rustc/cargo 1.97.1 >= MSRV 1.80) |
| `fmt-all.sh` | PASS 11/11 workspaces |
| `clippy-all.sh` | PASS 11/11 workspaces, `loom_crate_issues=0` |
| `test-all.sh` | PASS 11/11 workspaces, 0 failures (358 tests) |
| `offline-test.sh` (container `--network none`) | PASS 11/11 with `cargo --offline` |

## Test counts per workspace

| Workspace | Tests |
|---|---|
| loom-core | 112 |
| loom-writer | 21 |
| loom-sheets | 22 |
| loom-present | 9 |
| loom-photo | 10 |
| loom-motion | 10 |
| loom-video | 9 |
| loom-studio | 10 |
| loom-encode | 8 |
| loom-vision | 83 |
| loom-plugin-sdk | 64 |
| **Total** | **358** |

## Offline verification

The suite is verified to build and test with no network: the container runs
with `--network none` and every workspace passes `cargo test --offline
--locked --workspace` (dependency resolution from the local registry cache
only). A network-dependent workflow would fail loudly; none did.

## Package verification

`bash loom-bootstrap/scripts/verify-package.sh` extracts `Loom-Complete.zip`
into a clean directory, verifies checksum/integrity/no-symlink safety, parses
every workspace, and runs `cargo test --locked --offline` from the extracted
tree: 11/11 PASS.

Logs: `loom-bootstrap/.work/test-<repo>.log`, `build-<repo>.log`,
`verify-test-<repo>.log`.
