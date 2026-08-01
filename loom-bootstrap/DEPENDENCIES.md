# Loom Bootstrap — dependency report

This repository itself has no Rust dependencies (it contains no crates). It
orchestrates repositories that do. The information below describes how the
suite manages dependencies and what is currently pinned.

## Toolchain

| Tool | Version | Required by |
|------|---------|-------------|
| Rust (rustc/cargo) | stable, MSRV 1.80 | all cargo workspaces |
| Slint | 1.17.1 | app UI layer (pinned via COMPATIBILITY.toml) |
| just | any recent | optional task runner |
| docker / compose | any recent | optional visual-QA + offline containers |
| zip / unzip | any recent | packaging and verification |

## Runtime dependencies by repo

Per-repo dependency lists live in each repository's `DEPENDENCIES.md` and
`Cargo.lock`. The authoritative cross-suite pins live in `COMPATIBILITY.toml`
in this repository.

### Shared platform (loom-core, crates/)

| Crate | License | Notes |
|-------|---------|-------|
| loom-package | MIT OR Apache-2.0 | container/package format |
| loom-document | MIT OR Apache-2.0 | document model |
| loom-color | MIT OR Apache-2.0 | color pipeline |
| loom-jobs | MIT OR Apache-2.0 | async jobs |
| loom-command | MIT OR Apache-2.0 | command system |
| loom-history | MIT OR Apache-2.0 | undo/redo |
| loom-text | MIT OR Apache-2.0 | text foundation |
| loom-storage | MIT OR Apache-2.0 | storage |

External third-party dependencies are resolved by each repo's `Cargo.lock`.
A complete inventory (cargo-deny / cargo-license output) is generated at audit
time; see `LICENSE_POLICY.md` for the release gate.

## Dependency audit process

1. `cargo tree -e all` per repo to enumerate direct and transitive deps.
2. `cargo-deny check licenses` (or equivalent) to verify license compliance.
3. Review security advisories via `cargo audit`.
4. Record findings in the suite-wide `DEPENDENCY_REPORT.md` at the parent
   level before release.

## Replacement strategy

Critical dependencies must have a documented replacement strategy in the
consuming repo's ADR, per the suite directive. This repository's role is to
enforce the pins and to validate lockfiles at package time.
