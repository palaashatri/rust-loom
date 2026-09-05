# Loom — Current Truth

`AGENTS.MD` defines what Loom must become. This file records what is verified today and which work is currently allowed.

## Current product state

Loom is a local-first Rust + Slint creative-suite **functional alpha** with eight desktop applications, substantial domain engines, persistence/history infrastructure, native project formats, cross-platform packaging machinery, and a large amount of unfinished product/UI code.

It is **not** yet a professional replacement for mature office, image, motion, video, audio, or encoding software. No application currently passes the acceptance definition in `AGENTS.MD`.

Current complete-suite readiness remains approximately **29/100**.

The score is intentionally frozen during the UI-foundation reset unless verified user-facing capability materially changes. Repository cleanup, smaller code, better CI, and stronger governance are valuable, but they do not by themselves increase professional product readiness.

## Active gate

```text
ACTIVE PHASE: WRITER
FOUNDATION STATUS: ACCEPTED
ACTIVE APPLICATION: WRITER (IN_PROGRESS)
LOCKED APPLICATIONS: PRESENT, PHOTO, MOTION, VIDEO, STUDIO, ENCODE
```

Loom Sheets is certified ACCEPTED. Loom Writer is currently the active application undergoing deep functional, interaction, and visual redesign to align with professional desktop standards (e.g. Apple Pages). Present, Photo, Motion, Video, Studio, and Encode remain strictly LOCKED per AGENTS.MD Section 4 until Writer passes its complete acceptance gate.

## Why the reset is necessary

The repository has accumulated useful engines together with excessive agent-generated structure and UI duplication. Several application entrypoints and core modules are extremely large; generic controls exist both in shared and application-local Slint files; old plans and reports compete for agent attention; and CI has been spending substantial compute on all-workspace release builds and cross-platform packaging even when the active work is a shared UI edit.

The visible result is below the desired product bar. Current captures have demonstrated clipping, weak hierarchy, dense or redundant chrome, dead space, inconsistent control grammar, fixed-size workspaces, and prototype-like composition. Passing deterministic screenshot tests does not make those designs acceptable.

## Strict readiness scorecard

| Dimension | Current | Current truth |
|---|---:|---|
| Core/backend engineering | 65/100 | Many meaningful models, algorithms, persistence paths, parsers/exporters, and media helpers exist. |
| Architecture & persistence | 70/100 | Shared history/recovery/storage/jobs/platform foundations are useful, but oversized modules and duplicated host logic remain significant debt. |
| Functionality reachable through GUI | 30/100 | A large amount of core capability is still disconnected from a coherent professional desktop workflow. |
| Interaction design | 20/100 | Selection, direct manipulation, context-sensitive commands, responsive chrome, and app-specific editor semantics remain incomplete. |
| Visual design & polish | 21/100 | Existing application UI is not accepted as production-quality and is explicitly frozen as legacy reference only. |
| Professional workflow depth | 25/100 | Useful slices exist in every app, but none completes the mature daily workflow expected of its category. |
| **Overall product readiness** | **29/100** | Functional alpha with substantial foundations; professional-suite parity is not established. |

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

Status: `ACCEPTED` (Application Acceptance Gate Satisfied per `AGENTS.MD` Section 13)

Verified capabilities:
- Shared UI foundation adopted: zero app-local generic control forks; 100% token discipline, native palette & menu bar integration.
- Full daily spreadsheet workflow executable via GUI & keyboard: cell selection, ranges, Shift/Arrow navigation, Select All, Formula Bar input/cancellation/commit, Fill Down, Copy/Cut/Paste (cells and matrices), Delete/Backspace clearing.
- Cell formatting & styling: Number formatting (Raw, Currency $, Percent %), Alignment (Left, Center, Right).
- Table & sheet mutations with full undo/redo: Add/Delete rows, Add/Delete columns, Inspector step sizing (row height, column width), Ascending and Descending sort by active column with snapshot transactions, Freeze/Unfreeze panes with snapshot transactions.
- Multi-sheet workbook management: Tab strip creation (+), switching, renaming, deletion, with isolated per-sheet undo/redo stacks.
- Native file persistence & export: Native `.loomtable` save/open, CSV import/export with dialect sniffing and RFC 4180 multiline quote handling, XLSX workbook export.
- Charts & Visualization: Embedded chart dialog with Bar, Line, and Pie views, automatic series normalization and responsive SVG rendering.
- Mechanical & Visual Gates:
  - 114 unit/integration tests passing (55 in `loom-sheets-app`, 59 in `loom-sheets-core`).
  - Headless journeys (keyboard journey, sparse workbook journey) passing.
  - 4 automated audits passing: code-structure, governance, assets, UI foundation.
  - 0 warnings with `-D warnings` on Clippy.
  - Native macOS screenshot evidence captured across all viewports (1024×720, 1280×800, 1440×900, 1920×1200) and themes (light, dark).

### Writer

Status: `IN_PROGRESS` (Visual and Functional Quality Audit in Progress; Present, Photo, Motion locked back per user directive)

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
