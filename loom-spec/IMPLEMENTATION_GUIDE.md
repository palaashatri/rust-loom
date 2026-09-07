# Loom Implementation Guide

How a new capability moves from idea to verified implementation across the
Loom suite. This document is for specification writers and coding agents;
quality gates are executed by `../loom-bootstrap/`.

## 1. Workflow overview

```text
Specify → Task → Implement → Test → Verify → Mark complete (with evidence)
```

Unimplemented work stays visible in the task ledger; nothing is "complete"
without acceptance evidence (`AGENTS.md` §2, `RELEASE_CRITERIA.md`).

## 2. Specify

- Decide whether the change is **product scope** (this repo, `PRODUCT_SPEC.md`
  + `FEATURE_MATRICES.md`), **design** (`loom-design-bible`), **platform
  contract** (`loom-core` docs + crate), or **application contract**
  (application repo).
- Cross-cutting changes require an RFC here (`docs/rfcs/`); small decisions
  use an ADR (`docs/adrs/`). Accepted contracts are normative — never change
  them silently (`AGENTS.md` §4).
- A feature specification must define, per root `AGENTS.md` §11.1: purpose,
  user story, non-goals, preconditions, inputs, outputs, state model, data
  structures, interfaces, error behavior, threading model, persistence,
  undo, accessibility, security, performance budget, unit/integration/visual
  tests, failure cases, acceptance criteria, dependencies, files expected to
  change, example usage.

## 3. Task

- Decompose into tasks small enough for one coding agent to complete and
  verify independently. Concrete tasks (examples from the suite):
  "Implement immutable paragraph-style value object", "Implement UTF-16
  cursor mapping tests for bidirectional text", "Implement cancel-safe
  thumbnail job", "Implement dependency-graph cycle reporting for formulas".
- Task format (root `AGENTS.md` §11.2): ID, Title, Owner subsystem, Purpose,
  Dependencies, Files or modules, Required behavior, Non-goals,
  Implementation steps, Acceptance tests, Visual QA, Performance budget,
  Security considerations, Completion evidence.
- Assign one owner subsystem per task; two agents must not edit the same
  contract simultaneously.

## 4. Implement

- Follow the owning repository's conventions: rustfmt, clippy, no `unsafe`
  without safety comments + tests + justification (`RELEASE_CRITERIA.md`
  §1.11), engines headless and display-free (`ARCHITECTURE.md` §7).
- Engines consume shared contracts (`loom-core`, `loom-vision`,
  `loom-plugin-sdk`); they never redefine them. Applications never import
  each other.
- New shared behavior goes into the owning shared crate with its own tests;
  do not copy code between repositories (`COMPATIBILITY_POLICY.md`).

## 5. Test

Tests required per `ROADMAP.md` Phase 7 hardening and the root directive §14:

- **Unit tests** — parsers, serializers, formula evaluation, text layout,
  time/coordinate math, undo operations, migration, model-pack validation,
  color conversion, plugin permissions, command state, caches, job
  cancellation.
- **Property tests** — serialization round trips, undo/redo invariants,
  formulas, timeline edits, transform composition, package migration,
  Unicode cursor movement, range operations, media timestamp conversion.
- **Fuzzing** — deterministic mutation fuzz targets integrated into normal
  tests (no cargo-fuzz on stable; see
  `docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`): package readers,
  importers, media metadata and subtitle parsers, formula/rich-text/manifest
  parsers, clipboard input, recovery journals.
- **Integration tests** — create/edit/save/close/reopen; crash during save +
  recover; import/export; undo/redo; copy/paste; drag and drop; plugin and
  model-pack install/removal; offline startup/edit/export; low disk; missing
  media/fonts; corrupt files; cancelled background work.
- **End-to-end UI tests** — menus, shortcuts, inspector, canvas/timeline/
  spreadsheet navigation, accessibility focus, dialogs, export, recovery,
  themes, reduced motion, localization layouts.
- **Visual regression** — software-renderer screenshots vs golden baselines
  from pinned Docker images
  (`docs/rfcs/RFC-0015-Visual-Regression-System.md`,
  `docs/adrs/ADR-0003-Headless-Screenshots.md`). No auto-approval of new
  baselines.
- **Performance tests** — startup, window creation, open, large-document
  scroll, recalculation, scrubbing, waveform/thumbnail generation, model
  inference, export, save/autosave, undo, indexing, memory/GPU memory,
  background-task interference. Budgets per app supersede broad targets
  (`RELEASE_CRITERIA.md` §3).

## 6. Verify

Quality gates executed by `loom-bootstrap` (targeted matrix when optional
backends conflict; documented in the repo):

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
documentation link checks
schema validation
visual regression tests
offline integration tests (network-disabled container)
license audit
dependency audit
package smoke tests
```

A task is complete only when its acceptance tests pass, its visual QA is
reviewed, and `FEATURE_MATRICES.md`/`ROADMAP.md` reflect the new state with
evidence linked. Update `CHANGELOG.md` and `COMPATIBILITY.toml` when
contracts change.

## 7. Ownership map (current)

- Shared platform: `loom-core` maintainers.
- Vision: `loom-vision` maintainers (runtime/providers, OCR, segmentation,
  tracking, audio, model packs).
- Apps: one lead per application repository.
- Quality: `loom-bootstrap` (build/CI, test infra, fuzzing, packaging,
  licensing, doc consistency).
- Contracts: this repository reviews cross-repository contract changes;
  architecture drift is resolved by the coordinator per root `AGENTS.md` §12.
