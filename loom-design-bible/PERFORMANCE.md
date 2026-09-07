# Performance

UI performance budgets and the discipline around them. These are design
contracts: the design language assumes them (motion, selection, scrubbing,
typing all specify one-frame latency), so the budgets are binding.

## 1. Hardware tiers

Budgets are defined per tier; applications report against the tier they
target (mainstream is the default gate).

| Tier | Example | UI target |
|---|---|---|
| Baseline integrated | Intel/AMD iGPU laptop, 1080p | Functional 30 fps UI, responsive input |
| Mainstream desktop | Mid-range dGPU, 1440p | **60 fps UI, one-frame input** |
| High-performance workstation | Top dGPU, 4K HDR | 60 fps at 4K, 120 Hz-friendly architecture |
| CPU-only compatibility | Software rendering in CI/VM | Deterministic renders, animations complete in wall-clock time |

## 2. Input and interaction budgets

* **Input feedback within one display frame** (16.7 ms @ 60 Hz, 8.3 ms @
  120 Hz): hover, press, selection change, slider thumb, caret move.
* **Animations run at 60 fps**: 200 ms standard animations render at ≤
  16.7 ms/frame on mainstream; no frame-budget overrun may cause visible
  stutter on the budget tier (frame-drop policy per `MOTION.md` §5).
* **No UI-thread blocking, ever**: file I/O, media decode, parsing,
  inference, indexing, autosave, export, thumbnail/waveform generation run
  off-thread (jobs framework, `loom-core`). A blocking path that exceeds
  16.7 ms in a known critical path is a release-blocking defect
  (`loom-spec` release criteria).
* **Scrolling and virtualized surfaces allocate zero per frame**:
  timelines, spreadsheets, lists, and canvases reuse buffers; GC/alloc
  pauses are not acceptable in scroll paths (alloc-profile gate).
* **Cancellation feedback appears immediately**: cancel input → visible
  acknowledgement within one frame; the underlying job stops within 100 ms
  of the acknowledgement (best effort) and reports cancellation
  (`NOTIFICATIONS.md` §6).

## 3. Startup budgets

* **Warm launch < 1 s** for lightweight apps (Sheets, Writer, Encode) from
  app start to usable main window; **< 2 s cold launch where feasible**
  (cold = no caches).
* Heavy apps (Video, Studio, Photo, Motion): main window interactive
  within 1.5 s; project/media loading continues asynchronously with
  progress (never a blocking splash).
* Startup must not require network access; offline startup is a test gate
  (`loom-bootstrap` offline suite).

## 4. Memory budgets

* Bounded memory via documented cache policies: thumbnail caches (LRU with
  byte budget), waveform caches, decoded-frame pools, undo history budgets
  (memory + disk-backed modes per `loom-core` history contract).
* Representative workloads define the budgets per app (`PERFORMANCE.md`
  benchmark projects in `loom-samples`): e.g. a 1 M-cell sheet, a 10,000-
  clip timeline, a 4 GB photo folder — each app documents its workload
  budget in its PERFORMANCE.md.
* Leak detection: long-session tests (open/close 100 documents, run 500
  undo cycles) assert RSS returns to baseline within 5%.
* GPU memory: bounded by cache policies; the renderer reports GPU memory
  use in diagnostics.

## 5. Background-task interference

* Background tasks (autosave, thumbnails, proxies, indexing, model
  inference) yield to direct manipulation: interactive-frame budget wins;
  tasks are cancellable and prioritized (`loom-core` jobs contract).
* Autosave never visibly interrupts editing (silent path,
  `NOTIFICATIONS.md` §6); typing latency is unaffected during autosave
  (verified in the perf suite).
* Background-task progress is observable: status bar + jobs panel; a task
  that starves input is a defect.

## 6. Measurement and gates

* Benchmarks run in the Docker environment on the mainstream tier profile
  (and CI-software tier for determinism): cold/warm launch, window
  creation, document open, large-document scroll, recalc, scrub, waveform
  generation, thumbnail generation, export, save, undo, search index,
  memory, GPU memory, background interference.
* Regressions beyond configured thresholds fail CI or require a reviewed
  waiver (`loom-bootstrap` perf gate).
* Budgets are declared per workload in each application's
  `PERFORMANCE.md`; the Bible fixes the UI-level budgets above, which
  override nothing and are overridden by nothing less strict.
