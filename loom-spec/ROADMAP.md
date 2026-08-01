# Loom Roadmap

Phase plan with honest current status. Statuses are maintained in
`FEATURE_MATRICES.md`; prose here must not contradict the matrices.

## Phase 0 — Workspace bootstrap

**Status: DONE** (with known gaps)

- Parent `loom/` workspace with all 15 repositories created; shared licensing
  (MIT OR Apache-2.0), root `AGENTS.md`, task ledger, and ownership map are in
  place. The parent tracks the sibling repositories; they do not currently
  have independent `.git` histories.
- `loom-bootstrap` now has README/BOOTSTRAP guidance, scripts, Docker files,
  `COMPATIBILITY.toml`, and CI workflow scaffolding. `loom-design-bible`
  contains the visual, motion, interaction, accessibility, and baseline
  specifications; the remaining gap is application baseline coverage.

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
  written; six application light/dark baseline pairs are still missing.
- The first cross-repository contracts frozen so far: package container and
  manifest (`loom-package`), command/history/jobs/storage/text/color crate
  boundaries, vision provider model, plugin manifest schema.

## Phase 2 — Shared platform

**Status: PARTIAL (core crates, a shared UI crate, and app showcase slices exist)**

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

Not yet implemented: `loom-app-runtime`, `loom-render`,
`loom-animation`, `loom-autosave`, `loom-recovery`, `loom-media`,
`loom-fonts`, `loom-search`, `loom-settings`, `loom-accessibility`,
`loom-shortcuts`, `loom-clipboard`, `loom-diagnostics`, `loom-test-support`,
plugin host integration with core, richer component gallery, visual test
harness, and the full editing/runtime command layer. The `loom-ui` Slint
component crate and shared app styling are present and used by the eight
application slices.

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

**Status: IN PROGRESS** — all eight application repositories now contain
tested model/CLI/package slices and limited Slint showcase UIs. Full editing,
rendering, media, and export capabilities remain incomplete per the matrices.

- **Loom Writer** (`loom-writer-core` + CLI, 6 tests): rich-text block
  document model, `.loomdoc` save/load, Markdown and plain-text export,
  `create`/`info`/`export-md`/`validate` CLI commands and a Slint quick-start
  showcase; the UI is not yet a full text editor.
- **Loom Sheets** (`loom-sheets-core` + CLI, 12 tests): formula engine
  (tokenizer, recursive-descent parser, A1 cell references, dependency-graph
  evaluator with topological ordering and cycle detection), CSV import/export,
  `.loomtable` JSON round-trip and a Slint grid showcase; cells are not yet
  editable in the UI.
- **Present, Photo, Motion, Video, Studio, Encode**: model/CLI/package
  round-trips plus populated Slint showcase UIs now exist. Their professional
  editing engines and media pipelines remain planned work.

## Phase 5 — Professional feature expansion

**Status: NOT_STARTED.** Expand per `FEATURE_MATRICES.md` priority order;
do not sacrifice architectural integrity for checkbox counts.

## Phase 6 — Integration

**Status: NOT_STARTED.** Shared clipboard formats, cross-app drag and drop,
linked assets, Motion templates into Video/Present, Encode integration,
Vision integration, shared commands/shortcuts/components, consistent file
dialogs and recovery (see `CROSS_APP_WORKFLOWS.md`).

## Phase 7 — Hardening

**Status: PARTIAL.** The suite has deterministic unit tests, dependency and
license reports, host build/test/lint gates, smoke tests, package extraction
tests, and a visual regression harness. Remaining hardening includes mutation
fuzzing (`docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`), offline
coverage, corruption/crash-recovery tests, memory/performance tests,
accessibility audits, and the missing application visual baselines.

## Phase 8 — Release packaging

**Status: PARTIAL.** Host release builds, checksums, sample projects,
documentation, a deterministic `Loom-Complete.zip`, and extracted-package
verification exist. Linux release binaries, AppImage/Flatpak manifests,
SBOM/build provenance, and a final visual-baseline-complete release remain.

## Current milestone focus

1. Finish Phase 1 specs (remaining RFCs and design-bible baselines).
2. Harden the current eight app vertical slices: real editing interactions,
   persistence UX, and application-level regression coverage.
3. Complete the required light/dark visual baselines for all eight apps.
4. Wire Loom Vision into Photo/Sheets workflows and then proceed with the
   professional capabilities app by app.
