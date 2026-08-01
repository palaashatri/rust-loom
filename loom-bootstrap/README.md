# loom-bootstrap

The umbrella orchestration repository for the **Loom** creative suite.

Loom is a multi-repository project: each application and shared platform lives
in its own sibling repository with its own cargo workspace and `Cargo.lock`.
This repository builds, tests, visually QAs, packages, and verifies the whole
suite. It holds no application code — only orchestration.

## Layout

```
loom-bootstrap/
├── COMPATIBILITY.toml          cross-suite pins (toolchain, slint, repo status/revs)
├── justfile                    optional task runner (just is not required)
├── scripts/                    the actual engine: bash, POSIX-safe
│   ├── env-check.sh            toolchain vs MSRV 1.80, optional tools
│   ├── build-all.sh            cargo build (--release default) per repo
│   ├── test-all.sh             cargo test --workspace per repo, aggregate report
│   ├── fmt-all.sh              cargo fmt --check per repo
│   ├── clippy-all.sh           clippy -D warnings; fails only on Loom crates
│   ├── run-apps.sh             launch each existing binary with --smoke for 5s
│   ├── visual-qa-all.sh        --screenshot capture + baseline comparison
│   ├── img-compare.sh          tiny compare helper (ImageMagick or PIL)
│   ├── offline-test.sh         suite tests with network disabled (proxy-off / docker --network none)
│   ├── package.sh              deterministic Loom-Complete.zip + .sha256
│   ├── verify-package.sh       extract + env-check + metadata + lite tests
│   ├── generate-status-report.sh → ../VERIFICATION_REPORT.md
│   └── docker-*.sh             thin docker compose wrappers
├── docker/                     Dockerfile.ci / .dev / .visual, compose.yaml
└── .github/workflows/ci.yml    GitHub Actions: fmt, clippy, test, build
```

Sibling repos: `../loom-core`, `../loom-writer`, `../loom-sheets`,
`../loom-present`, `../loom-photo`, `../loom-motion`, `../loom-video`,
`../loom-studio`, `../loom-encode`, `../loom-vision`, `../loom-plugin-sdk`,
`../loom-design-bible`, `../loom-spec`, `../loom-samples`.

## Quick start

```sh
# 1. Verify the toolchain (rustc/cargo >= 1.80; docker is optional)
bash scripts/env-check.sh

# 2. Format, lint, test, build — per existing repo
bash scripts/fmt-all.sh
bash scripts/clippy-all.sh
bash scripts/test-all.sh
bash scripts/build-all.sh          # release
bash scripts/build-all.sh --debug  # debug

# 3. Visual QA (needs built binaries + baselines in ../loom-design-bible/baselines)
bash scripts/visual-qa-all.sh

# 4. Offline check (mode A: proxies off; mode B: docker --network none)
bash scripts/offline-test.sh

# 5. Package and verify
bash scripts/package.sh            # ../Loom-Complete.zip + .sha256
bash scripts/verify-package.sh     # extract and re-verify

# Or use just, if installed (same recipes):
just test
```

## Behavior with incomplete repos

The suite is built incrementally. Not every sibling repo has a cargo workspace
or a binary yet. All scripts **detect what exists** and report
SKIP/FAIL/missing items explicitly instead of failing hard:

- no `Cargo.toml` → skipped by build/test/fmt/clippy, logged as `SKIP`
- no built `loom-<app>` binary → `run-apps.sh` and `visual-qa-all.sh` report
  "no binary" and continue
- no baselines in `loom-design-bible/baselines/` → screenshot captured,
  comparison skipped and noted
- missing `compare`/PIL → images copied, marked "compare unavailable"

Scripts exit 1 only when a step that was actually executed fails.

## Artifacts

| Path | Meaning |
|------|---------|
| `.work/` | logs, screenshots, diffs (gitignored) |
| `../visual-qa-report.md` | latest visual QA report |
| `../VERIFICATION_REPORT.md` | per-repo status stub |
| `../Loom-Complete.zip` | packaged suite |
| `../Loom-Complete.zip.sha256` | its checksum |

See [BOOTSTRAP.md](BOOTSTRAP.md) for the full procedure and
[COMPATIBILITY.toml](COMPATIBILITY.toml) for the cross-suite contract.
