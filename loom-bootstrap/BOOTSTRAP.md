# BOOTSTRAP.md — full bootstrap procedure

This document describes how to build, test, visually verify, package, and
deliver the complete Loom suite from `loom-bootstrap`.

## 0. Toolchain requirements

| Tool | Version | Required | Purpose |
|------|---------|----------|---------|
| `rustup` + stable rustc/cargo | >= 1.80 (MSRV) | yes | all cargo workspaces |
| `git` | any | yes | VCS (dev only; excluded from packages) |
| `just` | any recent | optional | nicer task runner over scripts/ |
| `docker` + compose plugin | any recent | optional | visual QA + offline containers |
| `zip`, `unzip` | any | packaging + verification | |
| python3 + PIL | any | contract-aligned RGBA visual comparison | |
| `timeout` (coreutils) | any | recommended | smoke/screenshot timeouts (fallback built in) |

Verify with:

```sh
bash scripts/env-check.sh
```

It checks rustc/cargo against MSRV 1.80 and reports optional tools as WARN
(docker, just, zip, unzip, timeout). It exits 1 only if the toolchain is
missing or below MSRV.

## 1. Layout

```
<suite-root>/                    (parent of loom-bootstrap)
├── loom-bootstrap/              this repo — orchestration
├── loom-core/                   shared crates (crates/loom-*)
├── loom-writer/ ... loom-encode/  applications
├── loom-vision/  loom-plugin-sdk/ loom-design-bible/ loom-spec/ loom-samples/
```

Repos are path-pinned siblings during development: app workspaces reference
`../loom-core/crates/*` via relative paths. `COMPATIBILITY.toml` records
`rev = "local"` for every repo; before release, revs are pinned to tags and
the manifest updated.

## 2. Build

```sh
bash scripts/build-all.sh            # cargo build --release, per existing repo
bash scripts/build-all.sh --debug    # debug profile
```

Repos without `Cargo.toml` are reported as `SKIP` (not a failure). Logs:
`.work/build-<repo>.log`. Exit 1 if any repo that *is* a workspace fails.

## 3. Test

```sh
bash scripts/fmt-all.sh      # cargo fmt --check per repo
bash scripts/clippy-all.sh   # clippy -D warnings; fails only on Loom crates
bash scripts/test-all.sh     # cargo test --workspace per repo, aggregate PASS/FAIL
```

`clippy-all.sh` reports diagnostics in third-party code without failing;
warnings/errors in Loom's own crates fail the run.

## 4. Visual QA

Prerequisites: app binaries built (release or debug). The harness captures the
default light and dark screens for each app. It compares when a committed
baseline exists, reports missing baselines as incomplete, and never writes a
baseline automatically. This is only the default light/dark slice; it does
not run the design-bible high-contrast, text-scale, reduced-motion, locale,
component/state, or error-state matrix.

```sh
bash scripts/visual-qa-all.sh
```

Per app: runs `<bin> --screenshot <work>/screenshots/<app>-light.png
--size 1280x800`, attempts the dark variant with `--theme dark`, then
compares each available screenshot against its baseline with
`scripts/img-compare.sh` (Python 3 + PIL, RGBA mean absolute error < 1.0 and
one-pixel-eroded differing-pixel ratio < 0.01). ImageMagick alone is not a
valid fallback because it cannot evaluate both contract gates. Result table →
`../visual-qa-report.md`. Exit 1 for a diff, missing baseline, missing binary,
screenshot failure, or size mismatch; exit 2 when comparison tooling or input
is unavailable. Override the size with `--size WIDTHxHEIGHT`.

Inside Docker:

```sh
bash scripts/docker-build.sh
bash scripts/docker-visual-qa.sh     # xvfb-run inside the visual service
```

## 5. Offline test

Mode A (host, no network): unset proxy variables, run the test suite with
`cargo --offline`:

```sh
bash scripts/offline-test.sh
```

Mode B (docker, hard network isolation):

```sh
bash scripts/offline-test.sh --mode-b   # docker run --network none
# or
bash scripts/docker-offline-test.sh     # compose 'offline' service (network_mode: none)
```

Note: `--offline` can only resolve dependencies that are already in the local
cargo registry or vendored. Repos without a `Cargo.lock` are reported
explicitly. A core workflow failing without network is a release-blocking
defect.

## 6. Cleanup

Inspect generated output before cleanup, especially when preserving visual
evidence:

```sh
bash scripts/cleanup-targets.sh --dry-run
bash scripts/cleanup-targets.sh
```

The allowlist covers sibling Cargo `target/` directories, the documented
plugin fixture target, and package-verification temporary paths. Logs,
screenshots, reports, historical run directories, and arbitrary
`CARGO_TARGET_DIR` paths are preserved. Pass `--visual-diffs` only when the
current diff images are no longer needed.

## 7. Smoke launch

```sh
bash scripts/run-apps.sh   # each existing binary runs --smoke for 5s
```

A binary that exits cleanly or stays alive for the full 5 seconds counts as
launched. No binary → reported as missing, not a failure.

## 8. Packaging (ZIP delivery)

```sh
bash scripts/package.sh
```

Creates `../Loom-Complete.zip` from the suite root:

- includes every `loom-*` repo,
- excludes `target/`, `.git/`, `.DS_Store`, `.work/`, `__pycache__`,
- excludes symbolic links deliberately; `find -P ... -type f` never follows
  untrusted link targets,
- deterministic ordering: sorted regular-file list piped to `zip -X -q -@`,
- writes `../Loom-Complete.zip.sha256`.

Checksum generation is fatal. A previous zip and checksum sidecar are removed
before packaging so a failed run cannot leave a stale `.sha256` paired with a
new or partial archive.

Limitation: zip embeds file mtimes, so two runs at different times are not
byte-identical even with identical content; ordering is deterministic.

## 9. Verification of extraction

```sh
bash scripts/verify-package.sh
```

Extracts the zip into `.work/verify-extract/` and re-runs from the extracted
tree: `env-check.sh`, `cargo metadata --no-deps` for each expected workspace,
and the full `cargo test --locked --offline --workspace` suite for each
expected workspace. The temporary extracted tree and verification target are
removed on startup and exit, using exact allowlisted paths only. Reports
success or the failing step.

## 10. Status reporting

```sh
bash scripts/generate-status-report.sh   # → ../VERIFICATION_REPORT.md
```

Walks every `loom-*` repo and records: exists / cargo / builds / tests-pass
(from the last `.work/test-*.log`) / status keywords from `TASKS.md` or
`FEATURE_STATUS.md` / missing items.

## 11. Docker environment

```sh
bash scripts/docker-build.sh [service]   # build ci, visual, offline images
bash scripts/docker-test.sh              # fmt + clippy + test in the ci service
bash scripts/docker-visual-qa.sh         # visual QA headlessly
bash scripts/docker-offline-test.sh      # offline test with network_mode: none
```

`docker/compose.yaml` mounts the suite root at `/workspace`; logs and
screenshots written under `.work/` land on the host. Images:

- `Dockerfile.ci` — ubuntu:24.04, rustup stable, X11/font/GL dev libraries,
  fonts-noto-core, xvfb, imagemagick + python3-pil, zip/unzip; locale
  C.UTF-8; `RUSTFLAGS=""` default.
- `Dockerfile.dev` — ci + git/bash/curl/vim/jq.
- `Dockerfile.visual` — ci + xdotool, mesa-utils.

## 12. GitHub Actions

`.github/workflows/ci.yml` runs on ubuntu-24.04: single checkout (this
monorepo workspace), Rust stable with rustfmt+clippy, cargo cache, then
env-check, fmt, clippy, test, release build. Test logs are uploaded as
artifacts (14-day retention). In real deployment, swap the single checkout
for per-repo checkouts pinned via `COMPATIBILITY.toml` revs.
