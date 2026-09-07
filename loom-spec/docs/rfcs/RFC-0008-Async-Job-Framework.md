# RFC-0008 — Async Job Framework

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-jobs`, all applications

## Context

Loom's performance principle (§2.5 of `PRODUCT_SPEC.md`) forbids UI-thread
blocking for media decoding, parsing, inference, autosave, export,
thumbnails, waveforms, proxies, font scanning, plugin discovery, and search
indexing. The root directive requires a shared job framework with progress,
cancellation, priority, dependencies, resource estimates, retry, cleanup,
persistence, and crash recovery where appropriate.

## Goals

- One job framework in `loom-core/crates/loom-jobs` used by every app,
  Vision, and Encode.
- Structured concurrency and cancellation: cancel propagates and cleanup
  runs.
- Observable progress; immediate cancellation feedback.
- Priorities and dependencies (e.g. thumbnail jobs yield to direct
  manipulation; encode jobs chain on source jobs).

## Non-goals

- A full scheduler/executor replacement — Tokio (or justified equivalent)
  remains the async runtime.
- Distributed or networked job execution.

## Proposed design

- `loom-jobs` defines: `Job` (unit of work), job ids, `JobHandle`
  (progress + cancellation), `JobSpec` (priority, dependencies, resource
  estimate), and a registry of running/completed jobs for observability.
  Implemented: core with progress, cancellation, and priorities; 5 tests.
- Cancellation: cooperative — jobs poll a cancel flag and check it at
  structured checkpoints; long loops must check per-item. Cancel handlers
  run cleanup (temp files, resources) via RAII guards.
- Priorities: a simple scheduler ordering where lower-priority background
  work (thumbnails, indexing, waveforms) yields to user-facing work; no
  preemption — cooperation only, which is sufficient because jobs never run
  on the UI thread.
- Dependencies: a job may declare completion prerequisites (e.g. "transcode
  after all sources copied"); the scheduler starts dependents when
  prerequisites finish.
- Retry: explicit, per-job policy (e.g. export retries on transient IO);
  never auto-retry user-visible failures without feedback.
- Persistence (NOT_STARTED): durable job records for recovery after app
  restart (Encode queue), designed as a storage-backed journal in
  `loom-storage`.
- Observability: the registry exposes state for the jobs panel
  (progress, errors, cancellation); logs are privacy-safe
  (`PRODUCT_SPEC.md` §2.2).

## Alternatives

- **Ad-hoc per-app threading**: duplicated cancellation/progress and
  inconsistent UX; rejected.
- **Crate-heavy async frameworks (rayon, tokio-rs)**: Tokio as runtime is
  already accepted; `loom-jobs` layers semantics (progress/priority/
  dependencies) on top rather than replacing it.
- **OS threads per task**: acceptable for some I/O work, but the framework
  must present one API regardless of backend.

## Trade-offs

Cooperative cancellation requires discipline (every long loop checks);
enforced by tests and clippy-friendly patterns. A centralized registry adds
a little overhead per job; negligible against real work (media, inference).

## Security

Jobs touch user files and model packs; cleanup must remove partial
artifacts; cancellation must not leave half-written packages
(`RFC-0006`). Job inputs from plugins are permission-checked
(`loom-plugin-sdk`).

## Performance

No UI-thread blocking by construction; the registry must not contend on
hot paths (lock-free reads where practical); cancellation checkpoints must
be cheap.

## Compatibility

Job contracts are internal; persisted job records (future) versioned with
`loom-storage` schemas.

## Migration

None yet; adopted as apps integrate jobs (autosave, export, vision, encode).

## Testing

- Cancellation tests: cancel mid-loop, assert cleanup ran and no partial
  output.
- Priority tests: background job yields to foreground.
- Dependency tests: dependent job starts only after prerequisites.
- Progress monotonicity tests; retry policy tests; crash-recovery tests for
  persisted records (future).

## Open questions

- Scheduler thread-pool sizing policy (resolved at first app integration).

## Final status

ACCEPTED. Core implemented in `loom-jobs` (5 tests); persistence and
registry UI NOT_STARTED.
