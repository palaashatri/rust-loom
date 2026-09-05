# RFC-0002 — UI and Engine Separation

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: all applications

## Context

Loom applications need professional depth and testability. Slint UIs and
GPU canvases are hard to exercise in headless CI and Docker; document
engines must be verifiable without a display. The vertical-slice strategy
(Phase 4) requires engines first, UIs later.

## Goals

- Every application engine is a headless library, fully testable without a
  display, with a CLI harness for scripting and Docker visual QA.
- The UI layer (Slint) is a thin consumer of engine services.
- UI↔engine communication goes through commands and change notifications
  with defined interfaces.
- Engines, UI, and persistence are independently testable.

## Non-goals

- A UI toolkit abstraction layer; Slint is the toolkit (RFC-0003).
- Mixing rendering logic into document models.

## Proposed design

- Each application repository: `crates/<app>-core` (engine), optional
  `crates/<app>-ui` (Slint, future), `crates/<app>-cli` (headless harness).
  Precedent: `loom-writer-core`/`loom-writer-cli`, `loom-sheets-core`/
  `loom-sheets-cli`.
- The engine owns: document model, persistence (via `loom-package`),
  commands (via `loom-command`), history (via `loom-history`), and jobs
  (via `loom-jobs`).
- The UI owns: Slint components (from `loom-ui`), rendering of canvases
  (future `loom-render`/wgpu), input mapping to commands.
- Engines expose change notifications; UI subscribes and re-renders. UI
  never mutates documents directly — it invokes commands, which update
  history atomically.
- Selection, clipboard, and drag/drop cross the boundary as typed payloads,
  not toolkit objects.
- Engines must compile and pass tests on any platform Slint can target and
  on the CPU-only compatibility tier.

## Alternatives

- **Fat UI with embedded logic**: faster prototyping, but untestable logic
  and UI-thread blocking risks; rejected.
- **Full MVC frameworks**: unnecessary ceremony for Slint's declarative
  model; rejected.

## Trade-offs

Some indirection is required (command plumbing, notification fan-out) in
exchange for deterministic tests, headless QA, and the ability to reuse
engines in CLIs. Engines that need rendering (Photo/Motion/Video) define a
render-service interface consumed by the UI; the engine stays display-free
by treating the renderer as a dependency-injected service (not yet
implemented).

## Security

A narrower UI surface means untrusted input (clipboard, drag/drop, files)
is validated at the engine boundary with fuzz targets
(`IMPLEMENTATION_GUIDE.md` §5).

## Performance

Commands are cheap indirection; hot paths (scrolling, scrubbing) bypass
command dispatch through dedicated render streams where needed
(`../loom-design-bible/PERFORMANCE.md`).

## Compatibility

UI and engine can evolve at different paces; the command/notification
contract is versioned via `loom-command` (`COMPATIBILITY_POLICY.md` §2).

## Migration

Engines already built headless; no migration. Future UI crates add
dependencies upward only.

## Testing

- Engine: unit, property, fuzz, integration tests (headless).
- UI: E2E tests plus visual regression via software renderer
  (`RFC-0015-Visual-Regression-System.md`).
- CLI: golden-file tests for scripts and Docker pipelines.

## Open questions

- Change-notification batching strategy for high-frequency edits
  (deferred to the `loom-ui` design task).

## Final status

ACCEPTED. Existing engines conform; new application crates must follow this
separation.
