# ADR-0002 — Path-Based Pinning of Shared Crates During Development

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

During development, `loom-core` crates are not published. Applications
need to consume them without version churn on every change.

## Decision

- Applications depend on shared crates by **path dependency** during
  development, e.g. `loom-writer/Cargo.toml`:
  `loom-document = { path = "../loom-core/crates/loom-document" }`.
- Path pins are dev-only and never published; tagged releases use version
  requirements with lockfiles (`COMPATIBILITY_POLICY.md` §4).
- `COMPATIBILITY.toml` in `loom-bootstrap` records pinned revisions and
  expected crate versions so bootstrap validates workspace consistency
  (`RFC-0001-Repository-and-Versioning-Strategy.md`).

## Consequences

- Any change to a shared crate is immediately visible to consumers —
  catch contract breaks early, keep semver discipline anyway
  (`COMPATIBILITY_POLICY.md` §2).
- Builds only work inside the checkout; CI and Docker build from the
  pinned workspace, so this is acceptable.
- Consumers must not copy shared code into their own trees; the pin is the
  single source.

## Verification

- Bootstrap CI builds the full pinned workspace on every shared-crate
  change; a consumer build against the workspace is part of the gate
  (`IMPLEMENTATION_GUIDE.md` §6).
