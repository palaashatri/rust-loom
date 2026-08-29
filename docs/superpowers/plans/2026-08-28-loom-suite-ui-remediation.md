# Loom Suite UI Remediation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn every Loom application into an honest, native-feeling desktop editor whose visual hierarchy, interactions, and contextual controls are backed by real document state.

**Architecture:** Keep shared UI limited to semantic tokens, native desktop integration, command state, selection/undo contracts, and composable workspace primitives. Each application owns its editing surface and one complete vertical slice; no generic `DocumentChrome -> Toolbar -> Canvas -> Inspector -> Status` scaffold is imposed where it does not fit.

**Tech Stack:** Rust stable, Slint 1.17, native macOS `winit-femtovg` QA, existing Loom core/app workspaces, Cargo tests/Clippy/rustfmt.

**Spec:** `.work/evidence/ui/macos-suite-baseline-20260828/REMEDIATION_PLAN.md`

## Global Constraints

- Preserve the current `cline-implementation` integration branch and unrelated working-tree changes.
- Build and visually inspect on macOS; a software-rendered PNG is regression evidence, never native visual acceptance.
- Do not copy Apple branding, assets, icons, or layouts; use the supplied Pages, Numbers, and Keynote captures only as quality references.
- Do not enable a control without a real command, selection semantics, undo, persistence, keyboard/accessibility action, and recoverable failure behaviour.
- Keep generated screenshots, logs, and status evidence under `.work/`; do not add root-level status files.
- Every implemented slice must pass changed-workspace tests, `cargo fmt --check`, relevant Clippy, `git diff --check`, and native macOS visual inspection.

## 2026-08-29 Numbers-quality audit and execution priority

The supplied Numbers, Pages, and Keynote captures are quality references only;
they do not authorize copying Apple branding, assets, icons, or undocumented
layout. The exact-head audit and findings are recorded in
`.work/evidence/ui/20260829-numbers-audit/QA.md`.

### Current self-score

The honest suite score remains **28/100**. The current scorecard is:

| Dimension | Score | Evidence-backed interpretation |
| --- | ---: | --- |
| Core/backend engineering | 65/100 | Useful models, persistence, history, parsers, exporters, and media helpers exist. |
| Architecture & persistence | 70/100 | Shared Rust infrastructure and local package/recovery foundations are meaningful strengths. |
| Functionality reachable through GUI | 30/100 | Too much engine depth is disconnected from professional GUI workflows. |
| Interaction design | 20/100 | Selection-led editing, direct manipulation, and editor-specific interaction remain incomplete. |
| Visual design & polish | 18/100 | The severe inspector overlap is fixed, but hierarchy, density, compact context, and truthful content remain below professional-suite quality. |
| Professional workflow depth | 25/100 | Each app has useful slices; none yet provides its category's complete daily workflow. |
| **Overall product readiness** | **28/100** | Functional alpha, not Numbers/Pages/Keynote parity. |

The layout repair is a defect reduction, not a score increase. Passing the
software-rendered theme matrix proves distinct renders and absence of the
specific overlap in the captured states; it does not prove native macOS chrome,
keyboard operation, screen-reader semantics, persistence, or professional
workflow completion.

### What “closer to Numbers” means for Loom

The target is the same product discipline visible in the reference: native
window ownership, quiet toolbar hierarchy, one dominant editable surface,
selection-driven context, deliberate width budgets, and a live inspector. Loom
keeps its own orange accent and design language. The implementation must not
turn a static screenshot into a Numbers-like facade: every enabled control still
needs a typed command, selection semantics, undo, persistence, keyboard and
accessibility access, and recoverable failure behaviour.

### Priority change: prove the Sheets slice first

The existing plan's shared-contract work remains first, but the first
user-facing vertical slice after those contracts is Sheets because the supplied
reference is Numbers. This gives the suite one measurable reference surface
before the same primitives are rolled through Writer, Present, media, and
production apps.

1. **Shared contract gate.** Stabilize semantic palette/typography/spacing/
   metrics, toolbar variants, inspector flow, focus, overflow, appearance, and
   native desktop command projection. Add static guards for zero overlap and
   zero label clipping at 1024×720, 1280×800, 1440×900, and 1920×1200; keep
   150% text, high contrast, reduced motion, and RTL in the matrix.
2. **Sheets/Numbers vertical slice.** In `loom-sheets-core/src/lib.rs`, add a
   viewport projection and explicit grid selection model. In
   `loom-sheets/crates/loom-sheets-app/{ui/app.slint,src/main.rs}`, render only
   visible rows/columns, bind the sheet tab, name box, formula bar, selection
   outline/fill handle, and Table/Cell inspector to that state, and route
   keyboard/mouse edits through typed reversible commands. Test scroll, range
   selection, formula commit/cancel, fill/copy, undo, save/reopen, and
   coordinate/value/formula announcements.
3. **Office rollout.** Complete Writer's selection-aware paginated editing and
   Present's real scene selection/direct manipulation using the same shared
   command, history, and inspector contracts. Their acceptance journeys must
   be type/select/format or add/select/transform → undo/redo → save/reopen →
   export, not palette-only smoke tests.
4. **Media rollout.** Replace Photo's calibration-like stand-in, Motion's static
   stage/timeline assumptions, and Video's synthetic/external-player path with
   real selected content, pan/zoom or clocked playback, direct manipulation,
   and contextual inspectors. Each slice must include cancellation/error and
   persistence evidence.
5. **Production rollout.** Make Studio regions/mixer and Encode queue/source/
   destination properties directly editable, reorderable, undoable, and
   recoverable. Native file panels and device/backend failures must be visible
   and actionable.
6. **Native release proof.** Rebuild from the exact commit, capture genuine
   macOS windows (traffic lights, menus, focus, file panels where applicable),
   inspect compact/reference/large light/dark/high-contrast/large-text/reduced-
   motion states, run one complete keyboard/mouse/persistence/failure journey
   per app, and only then reconsider the score.

### Hard acceptance gates for every phase

- No text, field, button, tab, inspector section, or timeline lane overlaps or
  clips at a required viewport; a safe elision must retain an accessible name.
- The primary workspace remains the largest region; sidebars/inspectors collapse
  or overflow before it becomes unusable.
- A selection change updates visible properties, command enablement, focus,
  undo labels, persistence, and accessibility output from one source of truth.
- Theme changes alter semantic tokens throughout the surface; no literal color
  or one-off padding is introduced to hide a broken layout.
- A screenshot may support a visual claim only. Product-completion claims also
  require executable workflow, failure, persistence, undo, and accessibility
  evidence at the exact SHA.

---

## File ownership map

| Tranche | Primary files | Boundaries |
| --- | --- | --- |
| Shared desktop | `loom-core/crates/loom-ui/ui/{theme,toolkit,components,icons}.slint`, `loom-core/crates/loom-desktop/src/{lib,menu}.rs`, `loom-bootstrap/scripts/audit-product-ui.py` | Tokens, workspace primitives, native menu/appearance bridge, static guards; no app document mutation |
| Writer | `loom-writer/crates/loom-writer-app/{ui/app.slint,src/main.rs,src/document_formatting.rs}`, `loom-writer-core/src/lib.rs` | Text selection, page layout, formatting/history/persistence |
| Sheets | `loom-sheets/crates/loom-sheets-app/{ui/app.slint,src/main.rs}`, `loom-sheets-core/src/lib.rs` | Virtual grid, range selection, formulas and cell operations |
| Present | `loom-present/crates/loom-present-app/{ui/app.slint,src/main.rs}`, `loom-present-core/src/lib.rs` | Scene rendering, object selection and transforms |
| Photo | `loom-photo/crates/loom-photo-app/{ui/product_workspace_v4.slint,src/main.rs}`, `loom-photo-core/src/lib.rs` | Image/layer rendering, selection and transforms |
| Motion | `loom-motion/crates/loom-motion-app/{ui/product_workspace_v2.slint,src/main.rs}`, `loom-motion-core/src/lib.rs` | Composition clock, keyframes and timeline state |
| Video | `loom-video/crates/loom-video-app/{ui/product_workspace_v4.slint,src/main.rs}`, `loom-video-core/src/lib.rs` | In-window playback, clip/range edits and timeline state |
| Studio | `loom-studio/crates/loom-studio-app/{ui/app.slint,src/main.rs,src/audio_io.rs}`, `loom-studio-core/src/lib.rs` | Region selection/editing, mixer and audio jobs |
| Encode | `loom-encode/crates/loom-encode-app/{ui/app.slint,src/main.rs}`, `loom-encode-core/src/lib.rs` | Queue selection/reorder, native source/destination and job lifecycle |

## Milestone 1: shared UI honesty and native visual gate

### Task 1: Stabilize the shared visual contracts

**Files:**
- Modify: `loom-core/crates/loom-ui/ui/theme.slint`
- Modify: `loom-core/crates/loom-ui/ui/toolkit.slint`
- Modify: `loom-core/crates/loom-ui/ui/components.slint`
- Modify: `loom-core/crates/loom-ui/ui/icons.slint`
- Modify: `loom-design-bible/contracts/desktop-ui.toml`
- Test: `loom-bootstrap/scripts/audit-product-ui.py`

- [ ] Add static-audit assertions for one tokenized implementation of each toolbar, icon, inspector, and status primitive.
- [ ] Remove binding loops, fixed icon viewports, duplicate simulated macOS traffic lights, and geometry that permits labels to paint over neighbours.
- [ ] Make compact mode choose icon-only controls or an accessible overflow menu before the primary workspace loses its minimum width.
- [ ] Verify the audit fails against a temporary deliberate contract violation, restore the contract, and run it successfully.
- [ ] Capture Writer, Sheets, Present, Photo, Motion, Video, Studio, and Encode at 1024x720 and 1440x900 in light, dark, high contrast, 150% text, and reduced motion using real macOS windows.
- [ ] Commit: `fix(ui): establish honest responsive desktop primitives`.

### Task 2: Connect native desktop state to shared commands

**Files:**
- Modify: `loom-core/crates/loom-desktop/src/lib.rs`
- Modify: `loom-core/crates/loom-desktop/src/menu.rs`
- Modify: app `src/main.rs` only where command registration is missing
- Test: existing desktop menu and app command tests

- [ ] Define one command-state projection that supplies visible label, enabled state, checked state, keyboard action, and accessibility default action to menu and toolbar adapters.
- [ ] Route New, Open, Save, Save As, Undo, and Redo through that projection in every document-bearing app.
- [ ] Verify disabled commands cannot mutate state, keyboard and menu dispatch execute the same command, and native open/save cancellation leaves the document untouched.
- [ ] Commit: `feat(desktop): share command state across native menus and toolbars`.

## Milestone 2: office editing surfaces

### Task 3: Writer selection-aware page editor

**Files:** ownership-map Writer files.

- [ ] Write core tests for grapheme-safe caret/range movement, mixed-style insertion, selection formatting, and undo restoring both content and selection.
- [ ] Implement the smallest structured page layout model that exposes pages, visible ranges, selection rectangles, zoom, and scroll position independently of Slint widgets.
- [ ] Bind `WriterApp` text editing, toolbar and inspector controls to the same `DocumentSelection`; remove any document-wide formatting path from visible controls.
- [ ] Implement save/reopen, native file-panel cancellation, PDF export failure, and accessibility announcements for caret/selection changes.
- [ ] Inspect native macOS type -> select -> bold/italic/heading -> undo/redo -> save/reopen -> export journey.
- [ ] Commit: `feat(writer): make page editing selection-aware`.

### Task 4: Sheets virtual workbook workspace

**Files:** ownership-map Sheets files.

- [ ] Write core tests for viewport row/column projection, range selection, formula commit/cancel, fill/copy, and reversible range edits.
- [ ] Implement a virtual visible-cell model driven by scroll offsets and workbook dimensions; stop rendering a fixed 8x6 table.
- [ ] Bind sheet tabs, table name, formula bar, selection outline/fill affordance, and a minimal Table/Cell inspector to live workbook state.
- [ ] Add keyboard coordinate navigation and accessible coordinate/value/formula announcements.
- [ ] Inspect a 1,000-row sparse workbook journey: scroll -> range select -> formula -> fill -> undo -> save/reopen.
- [ ] Commit: `feat(sheets): add virtual range-aware workbook editing`.

### Task 5: Present direct scene manipulation

**Files:** ownership-map Present files.

- [ ] Write core tests for hit testing, single/multi/marquee selection, snap calculation, transform operations, and undo/reopen.
- [ ] Render scene-model objects from their geometry rather than only title/body sample strings.
- [ ] Implement pointer capture, resize/rotate/move handles, keyboard nudge, guides, inspector synchronization, and focused accessible selection names.
- [ ] Inspect add -> select -> move/resize/rotate/snap -> undo -> save/reopen -> present/export.
- [ ] Commit: `feat(present): make slide objects directly manipulable`.

## Milestone 3: visual media editing surfaces

### Task 6: Photo layer and transform workflow

**Files:** ownership-map Photo files.

- [ ] Write core tests for imported layer identity, canvas transforms, crop/selection geometry, adjustment operation history, and save/reopen.
- [ ] Render actual imported/sample image payloads with pan/zoom and selected-layer bounds; remove the calibration-gradient stand-in from the primary journey.
- [ ] Bind layer, transform, selection, crop, and adjustment inspectors to selected layer state.
- [ ] Inspect import -> select -> transform/crop/adjust -> undo -> save/reopen -> export including failed import/export feedback.
- [ ] Commit: `feat(photo): establish real layer transform editing`.

### Task 7: Motion clocked stage and keyframe editing

**Files:** ownership-map Motion files.

- [ ] Write core tests for `CompositionClock`, time-normalized keyframe insertion, seek/play state, transform-at-playhead, undo, and persistence.
- [ ] Render current-time layer content on the stage and wire the timeline to keyframe lanes rather than static sample rows.
- [ ] Bind selected-layer transform/timing inspector changes to the current playhead and provide pointer/keyboard keyframe selection.
- [ ] Inspect keyframe -> play/seek -> transform at time -> undo -> save/reopen -> render/export/cancel.
- [ ] Commit: `feat(motion): edit keyframed layers against a live clock`.

### Task 8: Video in-window timeline and playback

**Files:** ownership-map Video files.

- [ ] Write core tests for clip selection, trim/move/split operations, timeline time conversion, undo, save/reopen, and cancellation.
- [ ] Use in-window decoded preview with audio as the master clock when an audio stream is present; leave external players diagnostic-only.
- [ ] Add draggable clips, trim handles, zoom/scroll, waveform/thumbnail cache state, and selection-driven clip inspector.
- [ ] Inspect import -> trim/move -> undo -> save/reopen -> synchronized play/seek -> export/cancel.
- [ ] Commit: `feat(video): establish in-window clip editing and playback`.

## Milestone 4: production workspaces

### Task 9: Studio arrangement and mixer editing

**Files:** ownership-map Studio files.

- [ ] Write core tests for `TimelineSelection`, move/trim/split/delete region operations, mixer changes, undo, persistence, and device failures.
- [ ] Bind arrangement regions to pointer capture, edit handles, playhead, loop range, zoom/scroll, keyboard/context commands, and the selected mixer channel.
- [ ] Move import, record, mix, and bounce onto cancellable jobs with durable progress, cancellation, and error state.
- [ ] Inspect native WAV import/drop -> edit -> undo -> save/reopen -> play/device failure -> bounce/cancel.
- [ ] Commit: `feat(studio): make arrangement regions and mixer state editable`.

### Task 10: Encode queue and job lifecycle

**Files:** ownership-map Encode files.

- [ ] Write core tests for queue selection, reorder/multi-select, destination collision policy, cancellation, retry, partial-output cleanup, and save/reopen.
- [ ] Add visible native source/output buttons and drop targets; replace typed-path-only paths with file panels.
- [ ] Bind Source, Video, Audio, Subtitles, Metadata, and Destination inspector sections to typed job fields.
- [ ] Distinguish queued/running/cancelled/failed/retry/completed states with actionable UI and final conformance report.
- [ ] Inspect add -> configure -> reorder -> undo -> save/reopen -> run -> cancel/fail/retry -> report.
- [ ] Commit: `feat(encode): make queue configuration and lifecycle durable`.

## Milestone 5: suite acceptance

### Task 11: Native macOS suite proof

**Files:** `.work/evidence/ui/<exact-sha>/` only; update `TRUTH.md` only when evidence changes supported feature state.

- [ ] Build every application from the exact commit.
- [ ] Run all changed-workspace tests, formatter, Clippy, and static UI audit.
- [ ] Capture and inspect every required state from Milestone 1 for all eight apps.
- [ ] Run one complete mouse/keyboard/persistence/failure journey per app and retain logs/screenshots next to its exact commit.
- [ ] Reject a milestone for overlap, clipping, stale inspector values, theme inconsistency, fake controls, or an unsupported completion claim.
- [ ] Commit: `test(ui): record native suite remediation evidence`.
