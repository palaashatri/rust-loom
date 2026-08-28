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
