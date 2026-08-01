# Loom Roadmap

Phase plan with honest current status. Statuses are maintained in
`FEATURE_MATRICES.md`; prose here must not contradict the matrices.

## Phase 0 — Workspace bootstrap

**Status: DONE** (with known gaps)

- Parent `loom/` workspace with all 15 repositories created; every repository
  is a git repo; shared licensing (MIT OR Apache-2.0) and root `AGENTS.md`
  in place; task ledger and ownership map defined in the root directive.
- Gaps: `loom-bootstrap` has only empty `ci/`, `docker/`, `scripts/`
  directories — no README, no justfile, no `COMPATIBILITY.toml`, no CI
  workflows yet (Phase 2/6 work). `loom-design-bible` docs directory is
  empty (Phase 1).

## Phase 1 — Foundation specifications

**Status: IN PROGRESS**

- This repository (`loom-spec`) documents product scope, architecture,
  terminology, file formats, release criteria, compatibility policy,
  roadmap, feature matrices, implementation guide, cross-app workflows,
  and 12 accepted RFCs + 7 ADRs.
- Remaining: RFC-0004 (GPU renderer), RFC-0009 (plugin ABI and sandboxing),
  RFC-0012 (media framework), RFC-0014 (accessibility), RFC-0016 (cross-repo
  compatibility), RFC-0017 (application command system), RFC-0019 (local
  search), RFC-0020 (localization) — PROPOSED, not yet drafted.
- `loom-design-bible` visual/motion/interaction/accessibility documents are
  not yet written.
- The first cross-repository contracts frozen so far: package container and
  manifest (`loom-package`), command/history/jobs/storage/text/color crate
  boundaries, vision provider model, plugin manifest schema.

## Phase 2 — Shared platform

**Status: PARTIAL (8 core crates exist with tests; no UI crate yet)**

Implemented in `loom-core/crates/` (all `0.1.0`, MSRV 1.80):

| Crate | Purpose | Tests |
|---|---|---|
| `loom-package` | manifest schema + ZIP container, checksums, security limits | 19 |
| `loom-document` | block tree document model (Block, BlockTree, Mutation, Offset, Text, TextEdit) | 6 |
| `loom-text` | paragraph/character styles, style runs | 10 |
| `loom-color` | color types and conversion | 8 |
| `loom-jobs` | async job framework: progress, cancellation, priority | 5 |
| `loom-command` | command identifiers, enablement | 5 |
| `loom-history` | transactional undo/redo history | 7 |
| `loom-storage` | storage paths and transactional file operations | 7 |

Not yet implemented: `loom-app-runtime`, `loom-ui` (Slint component
library — `RFC-0003-Slint-Integration-Model.md`), `loom-render`,
`loom-animation`, `loom-autosave`, `loom-recovery`, `loom-media`,
`loom-fonts`, `loom-search`, `loom-settings`, `loom-accessibility`,
`loom-shortcuts`, `loom-clipboard`, `loom-diagnostics`, `loom-test-support`,
plugin host integration with core, component gallery, visual test harness.

## Phase 3 — Loom Vision foundation

**Status: DONE** (vertical slice to Loom Vision's own spec)

- `loom-vision-core`: capability traits (`CapabilityProvider`,
  `ProviderDescriptor`, `RunContext` with cancellation/progress),
  `ProviderRegistry`/`CapabilityRegistry` with best-provider selection,
  model-pack manifest validation with SHA-256 checksums and path-traversal
  protection (24 tests), image interchange (`LumaImage`, `ProviderInput`,
  `ProviderOutput`, `BBox`).
- CPU reference providers implemented and tested: **QR decode**
  (`QrCodeProvider`) and **image statistics** (`ImageStatsProvider`).
- `loom-vision-cli` headless commands for provider workflows.
- Not started: OCR, segmentation, tracking, transcription, ONNX/Candle
  backends, hardware acceleration, semantic search, benchmark harness.
- No application consumes Loom Vision yet (Phase 4+).

## Phase 4 — Application vertical slices

**Status: IN PROGRESS** — Writer and Sheets engines complete; no GUI anywhere yet.

- **Loom Writer** (`loom-writer-core` + CLI, 6 tests): rich-text block
  document model, `.loomdoc` save/load, Markdown and plain-text export,
  `create`/`info`/`export-md`/`validate` CLI commands. Slint GUI NOT_STARTED.
- **Loom Sheets** (`loom-sheets-core` + CLI, 12 tests): formula engine
  (tokenizer, recursive-descent parser, A1 cell references, dependency-graph
  evaluator with topological ordering and cycle detection), CSV import/export,
  `.loomtable` JSON round-trip. Slint GUI NOT_STARTED.
- **Present, Photo, Motion, Video, Studio, Encode**: no code yet
  (empty repositories). Their vertical slices are planned after the shared
  platform and UI foundation land.

## Phase 5 — Professional feature expansion

**Status: NOT_STARTED.** Expand per `FEATURE_MATRICES.md` priority order;
do not sacrifice architectural integrity for checkbox counts.

## Phase 6 — Integration

**Status: NOT_STARTED.** Shared clipboard formats, cross-app drag and drop,
linked assets, Motion templates into Video/Present, Encode integration,
Vision integration, shared commands/shortcuts/components, consistent file
dialogs and recovery (see `CROSS_APP_WORKFLOWS.md`).

## Phase 7 — Hardening

**Status: NOT_STARTED.** Full suite tests, deterministic mutation fuzzing
(`docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`), dependency and
license audits, offline tests, corruption and crash-recovery tests, memory
and performance tests, visual regression, accessibility audits, packaging
tests.

## Phase 8 — Release packaging

**Status: NOT_STARTED.** Development builds, Linux release binaries, AppImage
where practical, Flatpak manifests where practical, distribution-neutral
archives, checksums, SBOM, license notices, build provenance, sample projects
(`../loom-samples/`), documentation, source archives, verified
`Loom-Complete.zip` per the root directive §26.

## Current milestone focus

1. Finish Phase 1 specs (remaining RFCs, design bible).
2. Bootstrap `loom-bootstrap` orchestration (CI, Docker, COMPATIBILITY.toml).
3. Land `loom-ui` Slint component library and the first application GUI
   (Writer), completing its vertical slice end-to-end.
4. Wire Loom Vision into the Photo/S sheets workflows.
5. Then proceed app by app through Phase 4.
