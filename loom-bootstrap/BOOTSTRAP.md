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
| ImageMagick (`compare`) **or** python3+PIL | any | visual QA comparison | |
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

Prerequisites: app binaries built (release or debug) and baselines in
`../loom-design-bible/baselines/<app>/<app>-{light,dark}.png`.

```sh
bash scripts/visual-qa-all.sh
```

Per app: runs `<bin> --screenshot <work>/screenshots/<app>-light.png
--size 1280x800`, attempts the dark variant with `--theme dark` (noted as
unsupported if it fails), then compares each screenshot against its baseline
with `scripts/img-compare.sh` (ImageMagick `compare -metric RMSE`, falling
back to python3+PIL). Result table → `../visual-qa-report.md`. Exit 1 when a
comparison is available and exceeds tolerance (default 0.02, override via
`VISUAL_QA_TOLERANCE`).

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

## 6. Smoke launch

```sh
bash scripts/run-apps.sh   # each existing binary runs --smoke for 5s
```

A binary that exits cleanly or stays alive for the full 5 seconds counts as
launched. No binary → reported as missing, not a failure.

## 7. Packaging (ZIP delivery)

```sh
bash scripts/package.sh
```

Creates `../Loom-Complete.zip` from the suite root:

- includes every `loom-*` repo,
- excludes `target/`, `.git/`, `.DS_Store`, `.work/`, `__pycache__`,
- deterministic: sorted file list piped to `zip -X -r -@`,
- writes `../Loom-Complete.zip.sha256`.

Limitation: zip embeds file mtimes, so two runs at different times are not
byte-identical even with identical content; ordering is deterministic.

## 8. Verification of extraction

```sh
bash scripts/verify-package.sh
```

Extracts the zip into `.work/verify-extract/` and re-runs from the extracted
tree: `env-check.sh`, a quick `cargo metadata --no-deps` parse of `loom-core`,
and `cargo test --workspace` in `loom-core` (test-all-lite). Reports success
or the failing step.

## 9. Status reporting

```sh
bash scripts/generate-status-report.sh   # → ../VERIFICATION_REPORT.md
```

Walks every `loom-*` repo and records: exists / cargo / builds / tests-pass
(from the last `.work/test-*.log`) / status keywords from `TASKS.md` or
`FEATURE_STATUS.md` / missing items.

## 10. Docker environment

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

## 11. GitHub Actions

`.github/workflows/ci.yml` runs on ubuntu-24.04: single checkout (this
monorepo workspace), Rust stable with rustfmt+clippy, cargo cache, then
env-check, fmt, clippy, test, release build. Test logs are uploaded as
artifacts (14-day retention). In real deployment, swap the single checkout
for per-repo checkouts pinned via `COMPATIBILITY.toml` revs.
