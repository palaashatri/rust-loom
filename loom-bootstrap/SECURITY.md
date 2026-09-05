# Security

This repository orchestrates the Loom suite; it contains no application code,
but it defines security-relevant build, packaging, and verification behavior.

## Principles

- **Local-first:** no telemetry, no account systems, no hidden network calls.
  The offline test (`scripts/offline-test.sh`) enforces that core workflows
  work with the network disabled.
- **Supply chain:** every repo pins dependencies via `Cargo.lock`; the package
  step verifies extraction from a clean tree (`scripts/verify-package.sh`).
- **No secrets:** packaging excludes VCS metadata and build artifacts; before
  any release the archive must be scanned for credentials and absolute
  personal paths.
- **Sandboxing:** plugins (loom-plugin-sdk) and model packs are sandboxed and
  capability-declared; model packs must pass checksum and provenance
  validation before loading (enforced in loom-vision).

## Archive hardening

- `scripts/package.sh` excludes `target/`, `.git/`, `.DS_Store` and produces a
  deterministic file ordering plus a SHA-256 checksum.
- Archive extraction in `scripts/verify-package.sh` is limited to the suite
  package; extraction of untrusted archives (document/plugin/model packages)
  must enforce size/entry limits in the consuming crate (loom-package).

## Reporting

Security issues in Loom should be reported privately to the maintainers —
do not open a public issue with exploit details. Reports should include the
affected repository, version/commit, and a minimal reproduction.

## CI

The GitHub Actions workflow runs fmt, clippy, and the full test suite on every
push; it is the first gate for security-relevant changes (unsafe code,
filesystem access, parser code).
