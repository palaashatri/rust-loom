# Loom Performance Report

Status: architecture + qualitative verification; automated benchmarks pending.

## Architecture (built in)

- No UI-thread blocking: capture platform renders headless; jobs, snapshots, PDF export
  run as functions invoked off critical paths; long tasks are structured for
  cancellation/pause (loom-jobs: pause, cancel, priority).
- Software renderer for deterministic tests; GPU path (wgpu/Vulkan) is future work.
- Bounded work: snapshot pipeline has fixed budgets; archive extraction has limits.

## Verified this session

- Writer/sheets smoke and screenshots complete in well under a second each (debug build)
  including PNG save — consistent with the warm-launch budget.
- The default visual QA gate covers 16 screenshots and 16 comparisons; no
  benchmark timing is recorded for the harness.
- Extracted-package verification covers all 11 Cargo workspaces and 276 tests;
  elapsed time is retained in the command logs but is not a performance
  benchmark.

## Budgets (targets from the directive)

| Workload | Budget (mainstream HW) | Status |
|----------|------------------------|--------|
| Input feedback within one frame | — | architecture supports; not instrumented |
| UI animation 60 FPS | — | motion tokens defined; no heavy UI yet |
| Warm app launch < 1 s | < 1 s | met for writer/sheets (smoke < 1 s) |
| Cold launch < 2 s | < 2 s | debug build meets; release expected better |
| No synchronous I/O on UI thread | hard rule | enforced by design; apps' file ops are blocking callbacks today — KNOWN limitation |
| Autosave non-intrusive | — | not yet wired into apps |

## Honest status

No automated benchmark harness exists yet (task ledger item). The heaviest workloads in
the directive (timeline scrubbing, waveform generation, model inference, large-doc
scrolling) apply to applications not yet implemented. Current apps' file save/open call
`std::fs` synchronously in the callback path — acceptable at this scale, must move to
jobs before the first release.
