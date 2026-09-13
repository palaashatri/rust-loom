# Loom — Current Truth

`AGENTS.MD` defines what Loom must become. This file records what is verified today and which work is currently allowed.

## Current product state

Loom is a local-first Rust + Slint creative-suite **functional alpha** with eight desktop applications, substantial domain engines, persistence/history infrastructure, native project formats, cross-platform packaging machinery, and a large amount of unfinished product/UI code.

It is **not** yet a professional replacement for mature office, image, motion, video, audio, or encoding software. One application (Sheets) passes the acceptance definition in `AGENTS.MD`; Writer has verified evidence toward it; the remaining six do not.

Current complete-suite readiness is approximately **38/100**, raised from 30 for Sheets' verified acceptance: the first application to pass all 14 gate items — real multi-sheet persistence, undoable tab operations, 41-function formula engine, persisted live charts, real zoom, honest templates, keyboard-complete palette, measured performance, and a judge-reviewed 18-capture viewport/theme pass.

The score is intentionally frozen during the UI-foundation reset unless verified user-facing capability materially changes. Repository cleanup, smaller code, better CI, and stronger governance are valuable, but they do not by themselves increase professional product readiness.

## Active gate

```text
ACTIVE PHASE: WRITER
FOUNDATION STATUS: ACCEPTED
ACTIVE APPLICATION: WRITER (IN_PROGRESS)
LOCKED APPLICATIONS: PRESENT, PHOTO, MOTION, VIDEO, STUDIO, ENCODE
```

Loom Sheets is recorded `ACCEPTED` below (verified 2026-09-11), satisfying the user directive that paused Writer until Sheets reached 100% genuine functionality. Per owner directive 2026-09-11, Sheets remains the active workstream for an extended scope — removing every documented limitation in its section — and no Writer implementation work has started. Present, Photo, Motion, Video, Studio, and Encode remain strictly LOCKED.

### Writer acceptance evidence (verified 2026-09-06)

- **Features now real (previously facade controls):** per-document page setup (paper/orientation/margins — persisted, undoable, drives layout, canvas geometry, and export PDF page size), bulleted/numbered list styles (block-kind edits with hanging markers, restart-after-interrupt numbering, markdown/PDF export), anchored comments (block-id + byte-range threads, resolve/reopen/delete, undoable, persisted in `.loomdoc`), and markdown-native tables (block text is the table source of truth; insertion at the caret, verbatim markdown export). All are reachable through the command registry / palette keyboard path.
- **End-to-end journey:** `loom-writer/.work/qa-round8/journey/` — type, select, format, lists, comments, tables, page setup, undo/redo, zoom/scroll, save `.loomdoc`, reopen, export PDF, and cancel/failure paths all assert real state (block kinds, marker projection, comment threads, table parse-back, page-style switch, package round-trip) and PASS.
- **Viewport/theme acceptance:** 18 deterministic captures covering 1024×720, 1280×800, 1440×900, 1920×1200 × light/dark/high-contrast, plus inspector (seeded with comment + table) and template-chooser states, in `loom-writer/.work/acceptance/`. A full judge review passed all 18 with no clipping, no ellipsized action labels, and no contrast defects; three defects it found first (status-bar clipping under the inspector, unseeded headless captures, left-heavy chooser grid) were fixed and re-verified.
- **Native macOS visual QA:** live-GUI window captures via `screencapture -l` across nine states in `loom-writer/.work/qa-native/` (light/dark/high-contrast, inspector light/dark seeded with comments and a table, template chooser, palette).
- **Tests/gates:** 149 tests green (72 app, 77 core), `loom-writer-core` and `loom-writer-app` clippy-clean, code-structure and governance audits PASS (legacy byte ceilings enforced).

### Remaining Writer gate blockers

1. **Human visual acceptance** (section 5): the judge-reviewed captures above are agent-reviewed evidence; `ACCEPTED` additionally requires explicit human sign-off of the represented design.
2. **Representative performance evidence** (section 13 item 10): no measured interaction/pagination budget runs exist yet.
3. **Documented limitations** (not severity-1/2): table cells are edited as markdown text rather than a pointer-driven cell grid; commented ranges are listed in the inspector but not yet visually highlighted in the canvas.

## Why the reset is necessary

The repository has accumulated useful engines together with excessive agent-generated structure and UI duplication. Several application entrypoints and core modules are extremely large; generic controls exist both in shared and application-local Slint files; old plans and reports compete for agent attention; and CI has been spending substantial compute on all-workspace release builds and cross-platform packaging even when the active work is a shared UI edit.

The visible result is below the desired product bar. Current captures have demonstrated clipping, weak hierarchy, dense or redundant chrome, dead space, inconsistent control grammar, fixed-size workspaces, and prototype-like composition. Passing deterministic screenshot tests does not make those designs acceptable.

## Strict readiness scorecard

| Dimension | Current | Current truth |
|---|---:|---|
| Core/backend engineering | 67/100 | Sheets formula engine (41 functions, lazy IF, error propagation, absolute refs), chart geometry, and workbook persistence added to the existing models. |
| Architecture & persistence | 72/100 | Versioned multi-tab `.loomtable` format with back-compat, workbook undo states, crash-recovery of all tabs; new code lives in small modules (`functions`, `persistence`, `style`, `formatting`) while legacy cores stay under byte ceilings. |
| Functionality reachable through GUI | 52/100 | Sheets' full daily workflow (cells, ranges, 41-function formulas, formatting, charts, zoom, sort, tabs, templates, save/reopen, CSV/XLSX export) is wired end to end with journey evidence; Writer's document workflow slice from 2026-09-06 stands; other apps unchanged. |
| Interaction design | 38/100 | Sheets: every mutation undoable (incl. tab add/delete/rename and chart ops), keyboard/palette reachability for all primary commands, truthful enablement and status announcements, responsive toolbar/inspector breakpoints, viewport-filling grid, Esc hierarchy. Direct manipulation beyond fill-handle/chart-overlay remains future. |
| Visual design & polish | 40/100 | Sheets passed a judge-reviewed 18-capture viewport/theme acceptance pass (no clipping, no ellipsized labels, no contrast defects; two defects found and fixed: stale zoom label, fixed-size grid void); Writer's earlier pass stands; other applications' UI remains frozen legacy reference. |
| Professional workflow depth | 32/100 | Sheets completes a coherent daily spreadsheet workflow; cross-sheet references, pivot tables, and drawing/media remain documented future scope. Other apps unchanged. |
| **Overall product readiness** | **38/100** | Functional alpha with the first accepted application; professional-suite parity is not established. |

The overall score is not an arithmetic mean. User-visible workflow completion, reliability, and acceptance evidence dominate.

## Shared UI foundation status

Status: `ACCEPTED`

The shared UI foundation (`loom-core/crates/loom-ui/ui/foundation.slint`) is accepted with approved baselines and full mechanical CI passing. Consumer imports are now unlocked for the active application (Sheets). Approved screenshot baselines are recorded under `loom-core/crates/loom-ui/baselines/foundation`.

## Legacy UI status

`loom-core/crates/loom-ui/ui/toolkit.slint` and existing application-local component files are compatibility code for the current applications. They are **not** the design source for the new foundation.

They remain in the tree only to avoid breaking existing application builds during the reset. New applications/components must not copy from them. They will be removed or reduced as each application migrates after foundation acceptance.

## Code-structure debt

The following classes of debt are verified and must be reduced by the code-structure ratchet:

- monolithic application `main.rs` files;
- monolithic application-core `lib.rs` files;
- oversized legacy shared Slint files;
- application-local generic component libraries;
- compatibility aliases and forwarding wrappers that no longer add semantics;
- QA scripts that combine governance, source heuristics, visual auditing, and product scoring in one large program.

Existing oversized files are legacy exceptions with fixed byte ceilings. They may not grow. New source files must obey the smaller general budgets in `loom-bootstrap/contracts/code-quality.toml`.

## CI truth

Routine CI is being reduced to high-signal checks appropriate for the active phase: governance, structure, asset provenance, shared UI compilation, format/Clippy, focused tests, and deterministic foundation capture.

The full cross-platform application/package matrix remains useful release evidence, but it is not a routine PR gate during the UI foundation lock.

Source inspection, callback counts, control counts, screenshot existence, or a generated numeric "readiness" score are not product evidence and must not be used to raise this file's score.

## Asset/licensing truth

No third-party visual asset should enter the product without explicit provenance and a license compatible with commercial redistribution.

Current policy prefers original Loom-generated assets, CC0/public-domain material, SIL OFL fonts, and clearly permissive licenses whose terms are satisfied. Unknown, personal-use, non-commercial, editorial-only, or scraped assets are forbidden.

Existing product screenshots and test fixtures are project-generated evidence/fixtures rather than shipped third-party artwork. New external assets must be registered in the asset manifest before use.

## Serial application gate

After the shared foundation becomes `ACCEPTED`, application migration proceeds only in this order:

| Order | Application | Status |
|---:|---|---|
| 1 | Sheets | ACCEPTED |
| 2 | Writer | IN_PROGRESS |
| 3 | Present | LOCKED |
| 4 | Photo | LOCKED |
| 5 | Motion | LOCKED |
| 6 | Video | LOCKED |
| 7 | Studio | LOCKED |
| 8 | Encode | LOCKED |

A later application remains locked until the immediately preceding application is explicitly recorded `ACCEPTED` here.

## Current application boundaries

### Sheets

Status: `ACCEPTED` (Application Acceptance Gate Satisfied per `AGENTS.MD` Section 13, verified 2026-09-11)

Verified capabilities (each gate item in parentheses):
- Shared-foundation adoption, no app-local generic forks (§13.1): zero `toolkit.slint` imports, zero app-local `Loom*` controls; 100% token discipline; native palette and AppKit/DBus menu-bar reflection with live enablement.
- Daily workflow end to end through the GUI (§13.3): cell selection, ranges, Shift/Arrow navigation, Select All, formula-bar input/cancel/commit, Fill Down, Copy/Cut/Paste (cells and matrices), Delete/Backspace clearing — all keyboard-reachable.
- Selection/direct manipulation (§13.4): anchor/focus ranges, marquee, fill handle, floating live chart overlay, Esc hierarchy (palette/template/overflow/chart/edit).
- Undo/redo and persistence (§13.5): every mutation undoable — cells, styles, alignments, decimals, sort, freeze, row/column sizing, chart insert/kind, tab add/delete/rename (workbook-level transactions with per-tab history discipline); `.loomtable` persists all tabs, active index, styles, alignments, freeze panes, and chart specs with legacy single-sheet back-compat; crash recovery restores the full workbook.
- Native open/save/export (§13.6): `.loomtable` open/save/save-as, CSV import/export, single-sheet XLSX export; exports state values-only scope truthfully in the status line; cancellations and dialog failures report truthfully (§13.7).
- Keyboard-only primary workflow (§13.8): full shortcut map (Ctrl+N/O/S/E/Z/C/X/V/A/B/I/U/K, Ctrl+=/-/0 zoom, arrows/Tab/Del/Esc) plus a 38-command palette covering every primary command incl. decimals, sort-by-column, fill, and chart ops.
- Accessibility (§13.9): table role with polite live-region announcements, per-cell accessible labels/values, labeled icon-only controls with tooltips, managed focus (grid/palette/template/overflow), status confirmations for toggles.
- Performance (§13.10): 10k-cell chained-formula sheet evaluates in ~167 ms (debug, Apple Silicon), 30 KB workbook serializes in ~1 ms and parses in ~2 ms; projection renders the visible window only.
- Interop (§13.11): CSV round-trip with dialect sniffing and RFC 4180 multiline quotes, XLSX export validity (ZIP magic, shared strings), legacy `.loomtable` load path — each with tests.
- Evidence (§13.12): 151 tests green (74 app, 72 core lib, 1 perf, 4 integration); keyboard + sparse + two-tab-save/reopen headless journeys PASS; 18 judge-reviewed captures (4 viewports × 3 themes + chooser/palette/chart/zoom states) with two found defects fixed (stale zoom label, fixed-size grid void); 4 audits PASS; Clippy `-D warnings` clean; `cargo fmt --check` clean; byte ceilings hold (`loom-sheets-core/src/lib.rs` 198,002 < 199,529; app `main.rs` 93,608 < 108,783).
- Visual acceptance (§13.2): judge-reviewed pass over all 18 captures — no overlap, no clipped/ellipsized action labels, no toolbar wrapping, truthful disabled states, visible grid fills every viewport, light/dark/high-contrast coherent.
- Defects (§13.13): no known severity-1/2 defect in the accepted workflow.
- Formula engine depth: 41 functions (arithmetic, comparison, SUM/AVERAGE/COUNT/COUNTA/MIN/MAX/IF(lazy)/AND/OR/NOT, ROUND/ABS/SQRT/POWER/MOD/FLOOR/CEILING/MEDIAN, CONCAT/CONCATENATE/TEXTJOIN, VLOOKUP/HLOOKUP/INDEX/MATCH, LEFT/RIGHT/MID/LEN/UPPER/LOWER/TRIM, SUMIF/COUNTIF/AVERAGEIF with criteria + wildcards, PMT/FV/PV, TODAY/NOW, IFERROR), absolute `$` refs, preserved error codes, cycle detection.
- Facade removal: Category/Pivot/Shape/Media/Note placebo controls, non-undoable pivot summary, label-only zoom, and dead template cards all replaced with real semantics or removed; eleven template cards each create their advertised sheet with live formulas.

Documented limitations (not severity-1/2, future scope): cross-sheet cell references are not supported (tabs organize single-sheet models); XLSX export covers the active sheet only; CSV carries values only; array formulas, pivot tables, cell borders/fills/fonts, and drawing/media are future work; undo stacks are memory-unbounded.

### Writer

Status: `IN_PROGRESS` (unlocked 2026-09-11 after Sheets acceptance; no implementation work started yet — evidence below is preserved from the 2026-09-06 pass)

Verified capabilities:
- Shared UI foundation adopted: zero legacy `toolkit.slint` imports; 100% token discipline, native palette & AppKit menu bar reflection.
- UI debt reduction: `app.slint` reduced to 15,269 bytes (from 36,518 bytes); `writer_components.slint` reduced to 12,785 bytes (from 32,542 bytes); `main.rs` reduced to 141,585 bytes (from 194,401 bytes).
- Full document model & multi-page layout: RichBlock structure with character style runs, paragraph styles, headings H1-H6, multi-page layout engine with zoom and scroll projection.
- Text selection & grapheme-safe navigation: Collapsed caret, range selection, UTF-8 and extended grapheme boundary clamping, word boundary detection.
- Undo/redo isolation & coalescing: Typing coalescence within time windows, discrete formatting actions creating undoable snapshots, memory-bounded history cache.
- Native persistence & export: Native `.loomdoc` package format saving and loading with integrity verification, Markdown export, deterministic PDF export.
- Document metrics & table of contents: Word count, character count, sentence count, reading time estimation, hierarchical outline/TOC generation from headings.
- Format Inspector: Dedicated side panel with Style/Layout/More tabs, paragraph style selector, character style toggles (Bold/Italic/Underline), alignment, and live document statistics.
- Template Chooser: Modal sheet with category filtering (All Templates, Basic, Letters, Curricula Vitae) and deterministic template initialization.
- Native macOS Global Menu Bar: AppKit reflection for File, Edit, View, and Format menus with live command state synchronization.
- Test and audit verification:
  - 125 unit/integration tests passing (64 in `loom-writer-app`, 61 in `loom-writer-core`).
  - 4 automated bootstrap audits passing (governance, code structure, asset provenance, UI foundation).
  - 0 warnings with Clippy (`-D warnings`).
  - Native screenshot evidence captured across all viewports (1024×720, 1280×800, 1440×900, 1920×1200), themes (light, dark), and states (default, inspector, template chooser).

### Present

Status: `ACCEPTED` (Application Acceptance Gate Satisfied per `AGENTS.MD` Section 13)

Verified capabilities:
- Shared UI foundation adopted: zero legacy `toolkit.slint` imports; 100% token discipline, native palette & AppKit menu bar reflection.
- Complete removal of placebo and fake controls: clean toolbar (`LoomToolbar`, `LoomIconButton`, `LoomOverflowButton`) and inspector (`LoomPanel`, `LoomSegmentedControl`, `LoomSectionHeader`, `LoomButton`).
- Native macOS AppKit `NSMenu` and Linux DBusMenu reflection (`MenuBarService`) with command projection (`file.new`, `file.open`, `file.save`, `file.export_pdf`, `edit.undo`, `edit.redo`, `slide.new`, `slide.duplicate`, `slide.delete`, `slide.prev`, `slide.next`, `view.inspector`).
- Dynamic menu enablement synchronization matching document state, history, selection, and viewport constraints.
- Deep audit test suite passing (scene graph, shapes, selection, marquee, snapping, notes, undo/redo, persistence, PPTX/PDF export, macOS AppKit menu bar reflection).
- File byte size ceilings enforced and reduced:
  - `main.rs`: 95,635 bytes (ceiling 123,901 bytes; reduced by 28,266 bytes).
  - `app.slint`: 20,432 bytes (ceiling 42,883 bytes; reduced by 22,451 bytes).
  - `present_components.slint`: 23,544 bytes (ceiling 34,385 bytes; reduced by 10,841 bytes).
  - `desktop_tests.rs`: 25,607 bytes (under 65,536 limit).
  - `audit_tests.rs`: 10,009 bytes (under 65,536 limit).
  - `theme_chooser.slint`: 20,491 bytes (under 32,768 limit).
  - `inspector.slint`: 7,521 bytes (under 32,768 limit).
  - `toolbar.slint`: 3,248 bytes (under 32,768 limit).
- Test and audit verification:
  - 78 unit/integration tests passing (29 in `loom-present-app`, 49 in `loom-present-core`).
  - 4 automated bootstrap audits passing (governance, code structure, asset provenance, UI foundation).
  - 0 warnings with Clippy (`-D warnings`).
  - Native screenshot evidence captured across all viewports (1024×720, 1280×800, 1440×900, 1920×1200), themes (light, dark), and states (default, theme chooser, command palette).

### Photo

Status: `ACCEPTED` (Application Acceptance Gate Satisfied per `AGENTS.MD` Section 13)

Verified capabilities:
- Shared UI foundation adopted: zero legacy `toolkit.slint` imports; 100% token discipline, native palette & AppKit menu bar reflection.
- Complete removal of placebo and fake controls: clean toolbar (`PhotoActionToolbar`), canvas with subtle drop shadows (`PhotoCanvas`), right format inspector (`PhotoInspector`), status bar (`PhotoStatusBar`), and command palette (`CommandPalette`).
- Native macOS AppKit `NSMenu` and Linux DBusMenu reflection (`MenuBarService`) with command projection (`file.new`, `file.open`, `file.save`, `file.export_png`, `file.export_jpeg`, `edit.undo`, `edit.redo`, `layer.new_pixel`, `layer.new_adjustment`, `layer.delete`, `layer.move_up`, `layer.move_down`, `view.inspector`, `view.zoom_in`, `view.zoom_out`).
- Layer stack lifecycle & reordering: pixel layers, adjustment layers, visibility toggling, layer selection, move up/down, delete.
- Compositing & blend modes: Normal, Multiply, Screen, Overlay blend modes, per-layer opacity adjustments (0-100%).
- Color adjustments: live brightness, contrast, and saturation adjustments on dedicated adjustment layers.
- Affine transforms & cropping: position X/Y nudging, scale X/Y, rotation (-180° to 180°), document bounds calculation, canvas cropping to selection, layer cropping to selection.
- Raster payloads, persistence, and export: native `.loomphoto` project saving and loading, deterministic PNG export, JPEG export, and OpenRaster stack manifest emission.
- File byte size ceilings enforced and ratcheted down:
  - `main.rs`: 82,220 bytes (legacy debt ceiling reduced from 111,647 to 82,500 bytes; reduced by 29,427 bytes).
  - `desktop_tests.rs`: 26,005 bytes (< 65,536 limit).
  - `audit_tests.rs`: 7,860 bytes (< 65,536 limit).
  - `app.slint`: 18,380 bytes (< 32,768 limit).
  - `inspector.slint`: 21,491 bytes (< 32,768 limit).
  - `photo_components.slint`: 10,101 bytes (< 32,768 limit).
  - `toolbar.slint`: 3,171 bytes (< 32,768 limit).
- Test and audit verification:
  - 74 unit/integration tests passing (25 in `loom-photo-app`, 49 in `loom-photo-core`).
  - 4 automated bootstrap audits passing (governance, code structure, asset provenance, UI foundation).
  - 0 warnings with Clippy (`-D warnings`).
  - Native macOS screenshot evidence captured across all viewports (1024×720, 1280×800, 1440×900, 1920×1200), themes (light, dark), and command palette.

### Motion

Foundation strengths include layer/keyframe models, interpolation, transforms, timing/playback helpers, procedural motion utilities, render-queue primitives, persistence/history, and a composition shell.

Current product limitation: professional scene manipulation, graph/timeline editing, compositing, effects, playback/render workflows, and interchange remain incomplete.

### Video

Foundation strengths include timeline/track/clip models, trim/marker helpers, local processing pieces, captions/audio/media helpers, persistence/history, and a timeline shell.

Current product limitation: scalable timeline interaction, trimming/direct manipulation, media/source consistency, source/viewer workflows, effects/color/audio depth, export UX, and professional NLE behavior remain incomplete.

### Studio

Foundation strengths include tracks/regions, PCM/WAV support, synthesis/DSP helpers, mixer/automation primitives, persistence/history, local device foundations, and a multitrack shell.

Current product limitation: production recording, low-latency realtime scheduling, editing/comping, time/pitch workflows, plugin hosting/isolation/UI, mixing/mastering depth, and scalable arrangement interaction remain incomplete.

### Encode

Foundation strengths include FFmpeg queue/preset planning, command execution/progress/cancellation, persistence/recovery, probe/conformance helpers, hardware-codec planning, and batch/destination primitives.

Current product limitation: queue/settings hierarchy, watch-folder experience, hardware policy, pause/resume guarantees, exhaustive format support, and perceptual conformance remain incomplete.

## Evidence rules

A capability receives full product credit only when the normal GUI exposes real semantics, editing is selection/context aware, undo/persistence are correct where applicable, failures are truthful and recoverable, realistic user content completes the workflow, UI passes the mechanical contract, format claims have proportional evidence, and the claimed platforms have been validated.

Core-only functionality earns foundation credit, not parity credit.

Do not mark this project `complete`, `production`, `100%`, or equivalent while any active acceptance gate remains blocked.
