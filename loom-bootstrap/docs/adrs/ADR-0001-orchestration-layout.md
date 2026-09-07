# ADR-0001: Orchestration layout and development dependency strategy

- Status: Accepted
- Date: 2026-08-01
- Owner: build-and-CI lead (loom-bootstrap)

## Context

The Loom suite is a multi-repository project: shared crates live in
`loom-core`, each application is its own repo, and the specification/design
repos are documentation-only. The suite directive requires a single
orchestration repository that builds, tests, visually verifies, and packages
all repos, and it requires that application repos build against pinned or
versioned shared crates without source copying or circular dependencies.

Two questions needed a decision:

1. Where does orchestration live, and how is it executed?
2. How do application workspaces depend on `loom-core` crates during
   development?

## Decision

### 1. Orchestration lives in a sibling repository, `loom-bootstrap`, executed via scripts

- `loom-bootstrap` is a normal sibling repo (its own git repo, no cargo
  workspace) containing `scripts/*.sh`, `docker/`, `COMPATIBILITY.toml`, and
  `.github/workflows/ci.yml`.
- The execution interface is **portable bash scripts** (`set -euo pipefail`,
  POSIX-safe) that invoke `cargo`/`docker` per repo. `just` is an optional
  thin wrapper over the same scripts.
- CI and Docker compose call the scripts directly; there is no central
  build server, no Makefile magic, and no dependency on `just` being installed.

Alternatives considered:

- **A Makefile in each repo**: duplicated orchestration, no single gate.
- **CI-only orchestration (GitHub Actions as the only driver)**: impossible
  for local/offline development; the directive requires local runs.
- **A cargo workspace spanning all repos**: forbidden by the directive
  (independent workspaces with own lockfiles; avoids a shared-target
  monolith and enables independent rev pins).
- **just-only**: just is not guaranteed installed on all hosts; scripts are
  the floor, just is the ergonomic layer.

### 2. Relative path dependencies during development; tag-pinned versions at release

- During development, application workspaces reference shared crates via
  relative paths, e.g. `loom-document = { path = "../loom-core/crates/loom-document" }`.
- `COMPATIBILITY.toml` records `rev = "local"` for every repo while
  development is path-pinned.
- Before a release, `rev` entries switch to tagged versions, applications
  move to `git`/registry dependencies on released shared crates, and
  `verify-package.sh` re-runs the gates from the packaged tree to prove the
  packaging works without the development path pins.

Alternatives considered:

- **Published crates from day one**: publication friction blocks lockstep
  changes across the suite and slows iteration.
- **Vendored source copies**: forbidden — no unversioned copying between
  repos; would create divergence.
- **git dependencies on unpublished repos**: equivalent to path deps but
  requires network and complicates offline builds.

## Consequences

Positive:

- One command per gate (`bash scripts/<x>-all.sh`), identical locally, in
  Docker, and in CI.
- Missing repos degrade gracefully (SKIP + report) instead of failing hard,
  which matches the suite's incremental build order.
- Path deps keep the whole suite compiling in lockstep during development.
- The suite stays fully offline-capable: no registry publication is needed to
  develop.

Negative / risks:

- Path deps make each repo's `Cargo.lock` tied to sibling paths; the package
  + verify step is therefore mandatory before any delivery.
- Scripts must be kept POSIX-safe (no zsh-isms, no bash-3.2-incompatible
  syntax) so they run on macOS and in the Ubuntu containers alike.
- Drift risk between `COMPATIBILITY.toml` statuses and reality — mitigated by
  `generate-status-report.sh` and the CI gates.

## References

- Suite directive §6 (repository architecture), §11 (docs), §23 (quality gates),
  §26 (ZIP delivery).
