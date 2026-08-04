# Loom — Performance Report

## Budgets (documented)

Performance budgets are defined by hardware tier in
`loom-spec/RELEASE_CRITERIA.md` and `loom-design-bible/PERFORMANCE.md`:

- Baseline integrated-GPU system
- Mainstream desktop system
- High-performance workstation
- CPU-only compatibility environment

Mainstream targets include: input feedback within one display frame,
60 FPS UI animations, no synchronous file/media operations on the UI thread,
warm launch below one second, bounded memory via cache policies, immediate
cancellation feedback, and autosave that never visibly interrupts editing.

## Architecture guarantees (implemented)

- Long-running work (media decode, file parse, model inference, autosave,
  export, thumbnail/waveform generation, search indexing) runs off the UI
  thread through the shared job framework with progress, cancellation,
  priority, and persistence (`loom-core/crates/loom-jobs`).
- UI-thread blocking in known critical paths is a release-blocking condition
  and is guarded by the architecture; the record-keyboard-journey and
  screenshot harnesses render and dispatch through the real event loop.
- All 358 tests complete quickly; the functional matrix (create/validate/
  export journeys) runs in seconds per app; the UI matrix renders 72+ native
  captures deterministically in minutes on a CPU-only software renderer.

## Measured evidence (2026-08-04)

- Offline gate: full suite builds and tests with no network.
- Native UI matrix: 8 apps x 3 sizes x 3 themes captured via the software
  renderer with deterministic output (byte-distinct per theme).
- No sustained load benchmarks, frame-time measurements, memory profiles, or
  GPU measurements have been run in this audit.

## Honest status

Performance architecture and budgets are documented and enforced by design
(no UI-thread blocking paths in the audited critical flows), but
benchmark-driven budgets per workload have **not** been executed; measured
startup/scroll/export timings and memory figures are not yet published. That
work is the next milestone (see `KNOWN_LIMITATIONS.md`).