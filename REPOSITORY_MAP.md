# Loom Repository Map

Ownership and dependency direction. Dependencies always point from an application
toward shared code; shared code never depends on applications.

```
loom-bootstrap ── orchestrates ──> every repo (build/test/QA/package scripts)
loom-spec ──────> references (never duplicates) design-bible, core, vision, plugin-sdk contracts
loom-design-bible ──> supplies baselines to loom-bootstrap visual QA
loom-core ──────> no deps on other loom repos (platform foundation)
loom-vision ────> loom-core
loom-plugin-sdk ──> loom-core
loom-writer ────> loom-core, loom-package, loom-pdf, loom-ui, loom-test-support
loom-sheets ────> loom-core, loom-package, loom-ui, loom-test-support
loom-present/photomotion/video/studio/encode ──> (planned: loom-core, loom-vision, ...)
```

## Ownership

| Repo | Owner | Boundaries |
|------|-------|------------|
| loom-bootstrap | orchestration only | must never contain application code; locates siblings at runtime |
| loom-core | shared platform | narrow crates; each has explicit API boundary, tests, semver |
| loom-vision | perception | capability traits + provider registry; no app hard-coding |
| loom-plugin-sdk | extension system | sandboxed plugins; declared capabilities |
| each loom-<app> | application lead | builds against pinned shared crate versions; no source copying |

## Cross-repo contracts

- `loom-bootstrap/COMPATIBILITY.toml` pins MSRV, Slint version, per-repo status/rev.
- Shared interfaces change only through RFCs (loom-core/docs/rfcs).
- Application repos consume shared contracts; they never redefine them.
- The coordinator (loom-bootstrap) resolves architecture drift immediately.

## Direction rules

1. Never a dependency edge from shared code to an application.
2. No circular repository dependencies.
3. No unversioned source copying between repositories.
4. Two agents must not edit the same contract simultaneously.
