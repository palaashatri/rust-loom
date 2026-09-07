# Changelog

All notable changes to this repository (loom-bootstrap) are recorded here.
Format follows [Keep a Changelog](https://keepachangelog.com/); versioning
follows Semantic Versioning for the orchestration contract
(`COMPATIBILITY.toml` schema_version and script behavior).

## [Unreleased]

### Added

- Initial bootstrap repository for the Loom suite.
- Orchestration scripts: env-check, build-all, test-all, fmt-all, clippy-all,
  run-apps, visual-qa-all, offline-test, package, verify-package,
  generate-status-report, docker-build/test/visual-qa/offline-test.
- Image comparison helper (`scripts/img-compare.sh`) with ImageMagick or
  python3+PIL backends.
- `COMPATIBILITY.toml` cross-suite manifest (schema_version 1, MSRV 1.80,
  Slint 1.17.1, per-repo status/rev pins).
- `justfile` task recipes (just is optional; scripts run standalone).
- Docker compose environment with `ci`, `dev`, `visual`, and `offline`
  (`network_mode: none`) services.
- GitHub Actions workflow: fmt, clippy, test, release build, artifact upload.
- Documentation: README, AGENTS, BOOTSTRAP, LICENSE_POLICY, DEPENDENCIES,
  SECURITY, ADR-0001.
