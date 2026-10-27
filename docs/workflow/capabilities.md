# Recorded capability inventory

The single live quality/work table is in [current.md](current.md). This inventory preserves previously recorded capabilities and limitations; its old test results are not fresh verification. Read the named source and current evidence before relying on a claim.

## Existing capability inventory — preserve these while repairing

These capabilities describe the current implementation and historical work, not blanket acceptance. Do not remove working features to make a defect disappear. Old performance and cross-platform numbers are historical, not rerun measurements.

### Sheets

The implementation includes sparse multi-sheet workbooks, formulas and cross-sheet ranges, absolute references, lazy conditionals, lookup/text/aggregate/date/financial functions, dynamic-array spills, formula-backed summaries, cell style/formatting, freeze and row/column sizing, charts, anchored shapes/images, tab operations, templates, native packages, CSV and XLSX paths, a command palette, undo, and recovery. The P1 repair run covers lossless text/recovery, bounded workbook history, valid rich XLSX chart output, embedded recovery images, and live imported formulas. The 2026-09-24 follow-up fixes XLSX sheet-name collisions and array-spill orientation, removes formula-reader overhead when no array functions exist, reuses calculated values during view-only updates, and routes formula-bar commits through a revision-safe background worker that preserves the user's current viewport and formula draft. CODE-23 reports known lossy XLSX features and stages both picker and startup imports until confirmation; the shared command dispatcher blocks native-menu and palette actions during the decision, including queued commands, the empty `<definedNames/>` case does not warn, and Escape cancels after Tab navigation. The maintenance split moved CLI, headless rendering, and XLSX import flow into modules; it divided the app tests and Slint dialogs and split core XLSX handling into focused modules. The latest workspace run passes 330 tests: 206 app, 2 cache integration, 104 core unit, 13 formula, 1 performance fixture, and 4 range tests. Full-workspace all-target Clippy and the production app build pass on the async export/close changes. A fresh native capture shows the menu row and CODE-23 warning dialog; popup interactions, dialog focus, and keyboard behavior remain unverified because the available computer-use interface exposes no native-app controls. The latest optimized 10,000-formula run calculated in 88.5 ms on this low-end local host; a LibreOffice Calc 24.2.7.2 XLSX→ODS→XLSX→Loom round-trip imported four sheets, formulas, tested formatting, one chart, and one shape, with a custom row/column-size warning. The opt-in release test projected 60 viewports across 1,000,000 pseudorandomly placed cells in a 2,048×2,048 sheet at 0.17 ms p95 and 0.26 ms maximum; its test process peaked at 90,308 KiB RSS with no swap. A test-profile app run timed only edit preparation and mailbox submission (0.086 ms combined), not the full UI callback or a visible frame. It then took 2.56 seconds to evaluate, 12.72 seconds to create a recovery package, and 43.05 seconds to write the recovery journal; its process peaked at 2,612,252 KiB RSS with no swap. These are CPU/test-process measurements, not native frame rate or whole-app memory. This is not complete acceptance. UI-02/04/08 full visual evidence, live native menu/import-warning/close interactions, complete visual coverage, broader accessibility, very slow recovery after large edits, non-cell full-workbook copying, full file callback/export-scale performance, native million-cell scrolling, a numeric app memory budget, and wider interoperability remain. A fresh code-structure audit reports 18 findings outside the active Sheets source changes. Single-series charts and cached PivotTable import remain boundaries; unsupported OOXML must be disclosed. Do not label these boundaries as proof that all imports are safe.

### Writer

The implementation includes rich blocks/style runs, multi-page layout, selection, undo/coalescing, native packages, Markdown/PDF export, outline/metrics, page setup, list styles, comments, Markdown-native tables, an inspector, templates, and command/menu projection. The P1 repair run now covers complete PDF pagination, guarded Open, and launch recovery. Drifting comment anchors, pointer-driven table editing, visible comment highlighting, measured representative performance, and explicit human acceptance remain incomplete/unverified.

### Present

The implementation includes slides, scene objects, selection/manipulation helpers, snapping, notes, undo, native persistence, PPTX/PDF paths, themes, transitions, and native command/menu projection. CODE-04 New/Open replacement is guarded; duplicate identities (CODE-16) and non-undoable transitions (CODE-19) still block acceptance. Older source-cleanup and test-count claims are not evidence that these user workflows are correct.

### Photo

The implementation includes image/layer models, pixel and adjustment operations, selection/editing tools, native persistence, export paths, history, and a desktop canvas/inspector. CODE-04 New/Open replacement and the default fit geometry (UI-16) are repaired. Layer ID collisions, the remaining compact/precision checks, and a real import-edit-save-reopen-export journey still block approval.

### Motion

Layer/keyframe models, interpolation, transforms, timing/playback helpers, procedural utilities, render-queue primitives, persistence/history, and a composition shell exist. Professional scene manipulation, graph/timeline editing, compositing/effects, playback/render workflows, and interchange remain incomplete or unverified.

### Video

Timeline/track/clip models, trim/marker helpers, local processing, caption/audio/media helpers, persistence/history, and a timeline shell exist. Scalable interaction, media/source consistency, source/viewer workflows, effects/color/audio depth, export UX, and professional editing behavior remain incomplete or unverified.

### Studio

Tracks/regions, PCM/WAV support, synthesis/DSP, mixer/automation primitives, persistence/history, local device foundations, and a multitrack shell exist. CODE-10 precision persistence is repaired in code; production recording, realtime scheduling, comping/time/pitch, plugin isolation/UI, mixing/mastering, and scalable arrangement interaction remain incomplete or unverified.

### Encode

FFmpeg queue/preset planning, execution/progress/cancellation, persistence/recovery, probe/conformance helpers, hardware-codec planning, and destination primitives exist. CODE-09 final no-overwrite protection and the UI-23/25 readiness paths are repaired in code; filesystem review, real encode fixtures, queue/settings hierarchy, watch-folder experience, hardware policy, pause/resume guarantees, exhaustive format support, and perceptual conformance remain incomplete or unverified.

