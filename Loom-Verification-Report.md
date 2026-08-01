# Loom Verification Report — Package

Generated: 2026-08-01T09:30:00Z (final packaging session)

## Archive

| Artifact | Path | Checksum (SHA-256) |
|----------|------|--------------------|
| Package | `/Users/palaashatri/Code/loom/rust-loom/Loom-Complete.zip` (752 KB, 229 files) | `85c2916afc76c09c28a6b74d61481d85c6005f02aba753019568b3130ffa4670` |
| Checksum file | `Loom-Complete.zip.sha256` | — |
| Manifest | `Loom-Manifest.json` | — |

## Verification performed

1. `scripts/package.sh` — deterministic file ordering (`sort`), excludes `target/`,
   `.git/`, `.work/`, `.DS_Store`, `__pycache__`; PASS.
2. `scripts/verify-package.sh` — extracted into a clean directory and ran from the
   extracted tree:
   - `env-check.sh` — PASS (toolchain, timeout, python3+PIL)
   - `cargo metadata` on extracted loom-core — PASS
   - `cargo test --workspace` on extracted loom-core — PASS (84 tests; log in
     `loom-bootstrap/.work/verify-lite-test.log`)
3. Scope note: the ZIP contains the 15 `loom-*/` repositories. The parent-root reports
   (LOOM_MASTER_INDEX.md, FEATURE_STATUS.md, KNOWN_LIMITATIONS.md, LICENSE_REPORT.md,
   DEPENDENCY_REPORT.md, SECURITY_REPORT.md, ACCESSIBILITY_REPORT.md,
   PERFORMANCE_REPORT.md, VISUAL_QA_ALL.md, TEST_ALL.md, RUN_ALL.md, BUILD_ALL.md,
   REPOSITORY_MAP.md, VERIFICATION_REPORT.md, visual-qa-report.md, Loom-Manifest.json)
   live beside the archive in the parent workspace and are listed in the manifest.

## Gate results used as evidence

- fmt: PASS (5 cargo repos) | clippy `-D warnings`: PASS (0 diagnostics)
- tests: PASS — loom-core 84, loom-writer 6, loom-sheets 12, loom-vision, loom-plugin-sdk
- apps: `run-apps.sh` PASS (writer, sheets smoke); 6 apps skipped (no binary)
- visual QA: PASS — writer/sheets light+dark, metric 0.000000 (4 comparisons)
- offline (`--network none` container): PASS for writer, sheets, vision, plugin-sdk;
  loom-core's only failure is the font-dependent visual baseline (see KNOWN_LIMITATIONS.md)

## Honest status

The archive contains the implemented suite as of this session. Six application
repositories are documentation-only; per-repository git history is not initialized.
No secrets, credentials, or model files are included (exclusion rules verified).
