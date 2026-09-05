# RFC-0001 — Repository and Versioning Strategy

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: whole suite

## Context

Loom is a suite of applications sharing platform code. The root directive
mandates separate repositories per application, versioned shared crates, and
no unversioned source copying. We need a repository layout and versioning
strategy that keeps contracts single-sourced while allowing each repository
to build independently.

## Goals

- Each application repository builds independently against pinned shared
  crate versions.
- Shared code is versioned (semantic) and reused, never copied.
- No circular repository dependencies.
- Reproducible builds with lockfiles; cross-repo state is verifiable.

## Non-goals

- A monorepo or a workspace spanning repositories.
- Publishing to a public registry in the initial milestone (later, tagged
  releases only).
- Cloud or networked versioning services as a build requirement.

## Proposed design

- One repository per concern: `loom-core`, `loom-vision`, `loom-plugin-sdk`,
  `loom-writer`, `loom-sheets`, `loom-present`, `loom-photo`, `loom-motion`,
  `loom-video`, `loom-studio`, `loom-encode`, `loom-bootstrap`,
  `loom-design-bible`, `loom-samples`, and this repository (`loom-spec`).
- Dependency direction: `loom-bootstrap` → all; applications →
  `loom-core`/`loom-vision`/`loom-plugin-sdk`; shared repos → external
  crates only (`ARCHITECTURE.md` §1).
- Development pinning: path dependencies into `loom-core` (see
  ADR-0002) plus a suite compatibility manifest `COMPATIBILITY.toml` in
  `loom-bootstrap` recording each repository's pinned revision and expected
  crate versions (`COMPATIBILITY_POLICY.md`).
- Releases (future): `loom-core`, `loom-vision`, `loom-plugin-sdk` tag and
  publish semver crates; applications depend on version ranges with
  lockfiles; `loom-bootstrap` validates a release manifest.
- All workspaces: Cargo workspace per repository, edition 2021, MSRV 1.80,
  MIT OR Apache-2.0.

## Alternatives

- **Single monorepo**: simpler cross-repo refactors, but conflicts with the
  root directive's separate-repository architecture and makes independent
  versioning and ownership harder.
- **Publish-only (no path deps)**: forces registry publication before any
  inter-repo work is possible; premature at 0.1.0.
- **Versioned source copying**: rejected — drift and double maintenance.

## Trade-offs

Path deps make builds only valid inside the checkout; mitigated by
`COMPATIBILITY.toml` validation and by treating path pins as dev-only
(ADR-0002). Cross-repo refactors cost more than in a monorepo; mitigated by
narrow contracts owned by single repositories.

## Security

Path deps never leave the local checkout; release artifacts use tagged
versions with lockfiles. No supply-chain mechanism changes otherwise.

## Performance

No runtime impact; only build/versioning concerns.

## Compatibility

Every repository pins its own lockfiles; `COMPATIBILITY.toml` is the
cross-repo contract (`COMPATIBILITY_POLICY.md` §5).

## Migration

From current state (core crates exist at 0.1.0, writer/sheets consume via
path deps): no migration needed; this strategy formalizes existing practice.

## Testing

Bootstrap validates: all repos build against pinned revisions; lockfiles
match the compatibility manifest; no repository depends on another
application.

## Open questions

- Registry choice when tagged releases begin (crates.io vs private).
- Whether `loom-spec`/`loom-design-bible` receive tags in the release
  manifest (likely yes, informational).

## Final status

ACCEPTED. Current practice (path pins, per-repo workspaces, MSRV 1.80,
lockfiles) is the norm; release tagging is deferred to `ROADMAP.md` Phase 8.
