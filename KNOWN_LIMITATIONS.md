# Loom — Known Limitations

Honest inventory of what is not yet implemented or not yet verified.
Derived from the 2026-08-04 readiness audit, `TRUTH.md`, and the
verification gates. Nothing here is hidden or dressed up as complete.

## Readiness audit blockers (8)

1. **Native shell integration (UI 0.75/1.00)** — native menus, file
   pickers, drag/drop, recent documents, and OS services are incomplete;
   only positional open + file associations + a package exist.
2. **Undo/redo breadth (functionality 0.85)** — operation-level undo and
   crash-replay tests are not complete (front ends expose history).
3. **Import/export breadth (0.85)** — round-trip fidelity and
   destructive-loss reporting need broader conformance corpora.
4. **Media engines (0.80)** — GPU effects, synchronized low-latency
   playback, professional codecs, and plugin hosting are incomplete.
5. **Document engines (0.80)** — professional typography, layout,
   recalculation breadth, and format fidelity are incomplete.
6. **Vision productionisation (0.75)** — redistributable production models
   and measured application integration are incomplete (CPU reference
   capabilities exist).
7. **Plugin productionisation (0.75)** — production native CLAP/VST3
   isolation and complete host UI ABI are incomplete.
8. **Interoperability fidelity (0.65)** — broad round-trip conformance,
   compatibility reports, and loss budgets are incomplete.

## Verified-not-measured

- **Performance** — budgets documented by tier; no benchmarked startup/
  scroll/export timings or memory profiles published yet.
- **Accessibility** — keyboard + semantics + high-contrast verified;
  screen-reader integration, reduced-motion automation, text-scale and RTL
  stress captures not yet executed.
- **Security** — offline gate, package safety, plugin bounds verified;
  fuzz campaigns, cargo-audit runs, and Miri checks not yet executed.
- **Visual** — native matrix + theme smoke + journeys verified; golden
  baseline diffing (renderer-pinned) and component/error/locale state sets
  not yet executed.

## Per-application breadth gaps

- **Writer**: not yet a complete rich-text/page-layout engine (tables,
  styles, master pages, citations, change tracking, DOCX/ODT/EPUB breadth).
- **Sheets**: large-grid virtualization, advanced charts, pivots, broad
  XLSX/ODS compatibility pending.
- **Present**: canvas is not yet a complete direct-manipulation editor;
  video export and presenter display pending.
- **Photo**: painting, masks, advanced selection, RAW workflows, GPU
  effects pending.
- **Motion**: GPU composition, real-time playback, tracking, particles,
  professional effects pending.
- **Video**: full synchronized playback, proxies, GPU effects, broad
  codec/container coverage pending.
- **Studio**: production plugin hosting, process isolation, recording
  workflows, real-time DAW behavior pending.
- **Encode**: hardware-acceleration policy, exhaustive codec/container
  coverage, production conformance testing pending.
- **Vision**: production OCR/segmentation/tracking models and acceleration
  backends pending.
- **Plugin SDK**: complete host ABIs, production UI embedding, signing
  infrastructure pending.

## Architecture commitments that must not regress

- No mandatory account, cloud service, telemetry, or remote inference.
- No UI-thread blocking in known critical paths.
- AI features must never replace the underlying manual tools.
- All AI-assisted selections stay editable as ordinary masks.
- No `unsafe` without justification, tests, and safety comments.