# Loom — Repository Map

Ownership and dependency direction across the suite. Every application
repository builds against pinned/released shared crates; there is no
unversioned source copying between repositories and no circular dependency.

## Repositories

| Repository | Owns | Depends on |
|---|---|---|
| `loom-bootstrap` | Orchestration only: builds, tests, visual QA, offline verification, packaging, `COMPATIBILITY.toml` | All repos at runtime; never application code |
| `loom-spec` | Product scope, file format family, release criteria, feature matrices | References design bible, core, vision, plugin SDK contracts |
| `loom-design-bible` | Visual, motion, interaction, accessibility specification, tokens | None (references core token implementation) |
| `loom-core` | Shared platform crates (`loom-*`) | Published crates only |
| `loom-vision` | Vision provider framework, model packs, benchmarking | `loom-core` (image/tensor interchange) |
| `loom-plugin-sdk` | Plugin manifest, host, CLI | `loom-core` |
| `loom-writer` | Writer app + core + CLI | `loom-core`, `loom-plugin-sdk` |
| `loom-sheets` | Sheets app + core + CLI | `loom-core`, `loom-plugin-sdk` |
| `loom-present` | Present app + core + CLI | `loom-core`, `loom-plugin-sdk` |
| `loom-photo` | Photo app + core + CLI | `loom-core`, `loom-vision`, `loom-plugin-sdk` |
| `loom-motion` | Motion app + core + CLI | `loom-core`, `loom-vision`, `loom-plugin-sdk` |
| `loom-video` | Video app + core + CLI | `loom-core`, `loom-vision`, `loom-encode`, `loom-plugin-sdk` |
| `loom-studio` | Studio app + core + CLI | `loom-core`, `loom-plugin-sdk` |
| `loom-encode` | Encode app + core + CLI | `loom-core`, `loom-plugin-sdk` |
| `loom-samples` | Sample projects, conformance fixtures, original media | None (consumed by apps and matrices) |

## Dependency rules

1. Shared contracts (commands, packages, jobs, vision providers, plugin ABI)
   are proposed through RFCs in `loom-bootstrap`/`loom-core` and frozen before
   broad application work.
2. Application agents consume shared contracts; they do not redefine them.
3. Two agents must not edit the same contract simultaneously.
4. Cross-repo compatibility is enforced by `loom-bootstrap/COMPATIBILITY.toml`
   (MSRV, Slint version, per-repo status/rev).
5. Cargo workspace members stay inside their own repository; inter-repo
   dependencies use published versions pinned in `Cargo.lock`.

## Current ownership state

All application repositories are functional reference implementations with
bounded undo/redo, persistence, recovery, export paths, and tests. Detailed
per-app boundaries are documented in `TRUTH.md` and `FEATURE_STATUS.md`.
