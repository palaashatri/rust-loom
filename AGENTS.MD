# LOOM — 23 → 100 PRODUCTISATION DIRECTIVE

## 0. Authority, scope, and current truth

This file is the engineering execution authority for **Loom**, the local-first Rust + Slint creative suite in this repository.

Loom is not a greenfield project. It is a working functional alpha containing eight desktop applications, shared platform crates, native package formats, CI, test infrastructure, and reference engines. All work must improve this repository as it exists now.

The active integration branch is:

```text
chatgpt/loom-ui-functional-audit-2
```

Do not create long-lived implementation branches, duplicate roadmaps, issue sprawl, or parallel sources of truth. Parallel agents may work in isolated local worktrees or produce patch sets, but the coordinator must serialize reviewed commits onto the active integration branch.

Root prose is intentionally limited:

- `AGENTS.MD` — target product, architecture, roadmap, execution rules, and acceptance gates.
- `TRUTH.md` — what the current source actually delivers and what remains incomplete.
- `README.md` — user and contributor entry point.

Generated reports, task ledgers, screenshots, benchmarks, compatibility matrices, accessibility evidence, and verification records belong under `.work/` or in CI artifacts. Do not add new root-level status Markdown files.

### 0.1 Baseline

As of 2026-08-06, the honest complete-suite parity score against the product depth, desktop usability, integration, and polish expected from Apple Creator Studio-class software is:

```text
23 / 100
```

This score is **not** the source-only readiness score produced by `audit-product-readiness.py`. Repository-readiness evidence and product parity are separate measurements.

Current approximate product parity:

| Area | Current range |
|---|---:|
| Writer | 30–35 |
| Sheets | 25–30 |
| Present | 20–25 |
| Photo | 20–25 |
| Motion | 10–15 |
| Video | 15–20 |
| Studio | 15–20 |
| Encode | 35–40 |
| Shared desktop integration | 20–25 |
| Cross-application workflows | 5–10 |
| Overall suite | **23** |

These values may only increase when user-verifiable behavior, tests, native evidence, and release evidence improve. Code volume, controls, callbacks, screenshots, or passing shallow smoke tests are not sufficient.

---

# 1. What 100/100 means

The objective is an original Loom product with equivalent professional usefulness, reliability, desktop coherence, and refinement to the best commercial creator suites. It is not a demand to copy proprietary source, UI layouts, icons, assets, branding, formats, or undocumented behavior.

A 100/100 score requires all of the following:

1. **Professional completion:** users can finish substantial real projects in every application without switching to another editor for missing core operations.
2. **Desktop authenticity:** normal file dialogs, drag and drop, menus, clipboard, recent documents, printing where applicable, file associations, OS services, installers, updates, and crash recovery work naturally on Windows, macOS, and Linux.
3. **Interaction authenticity:** every visible control has real semantics; selection, direct manipulation, undo, persistence, error behavior, keyboard operation, and accessibility all agree.
4. **Interoperability:** supported external formats are measured with semantic corpora and documented loss budgets, not merely detected by extension or ZIP structure.
5. **Media correctness:** image, video, motion, and audio workflows use production render, playback, colour, timing, cache, and export pipelines.
6. **Reliability:** corruption, cancellation, low disk, missing assets, unavailable devices, plugin crashes, model failures, and process termination are handled without losing user work.
7. **Accessibility and localization:** keyboard-only operation, screen-reader journeys, scalable text, high contrast, reduced motion, RTL, pseudolocalization, and input methods are proven in real applications.
8. **Performance:** representative professional projects meet published latency, frame pacing, memory, startup, save, render, and audio budgets.
9. **Release quality:** signed or verifiable packages, reproducible builds, SBOMs, license inventory, migration tests, and clean-machine installation evidence exist for supported platforms.
10. **Truthfulness:** no feature is called complete without engine, UI, persistence, undo, failures, tests, and end-to-end evidence.

## 1.1 Weighted parity score

The coordinator must maintain the following scoring model in generated CI evidence:

| Dimension | Weight |
|---|---:|
| Core application workflow depth | 25 |
| UI/UX and direct manipulation | 15 |
| Media, rendering, text, and audio correctness | 15 |
| Native desktop integration | 10 |
| Interoperability and export fidelity | 10 |
| Reliability, undo, autosave, and recovery | 10 |
| Accessibility and localization | 7 |
| Cross-application integration | 5 |
| Performance and resource discipline | 2 |
| Security, packaging, and release engineering | 1 |
| **Total** | **100** |

A weighted average alone cannot produce 100. The following caps apply:

- Any critical visible placebo behavior caps the suite at **49**.
- Missing native open/save workflows in a majority of apps caps desktop integration at **4/10**.
- Missing operation-level undo or crash replay for critical edits caps reliability at **5/10**.
- External-format detection without semantic round-trip tests caps interoperability at **3/10**.
- Unsynchronized video playback or non-realtime audio behavior caps the relevant app below **50**.
- No real screen-reader journeys caps accessibility at **3/7**.
- An app with a professional core workflow marked incomplete cannot score above **79**.
- Any release-blocking data-loss defect prevents a score above **89**.
- 100 requires every application to score at least **95**, every shared dimension to score at least **98% of its weight**, and no open severity-1 or severity-2 release defect.

---

# 2. Non-negotiable execution rules

## 2.1 Keep the existing product direction

- Rust stable.
- Slint for application UI.
- Original Loom design identity.
- Local-first and offline-capable.
- No mandatory account, telemetry, hidden upload, or remote inference.
- Optional local Vision providers and model packs.
- Versioned, inspectable project formats.
- Cross-platform support for Windows x86-64, macOS x86-64, macOS arm64, and Linux x86-64.
- Preserve the repository license unless the owner explicitly changes it.

## 2.2 No facade-first development

Do not add a visible control until one of these is true:

1. the complete operation is implemented and tested; or
2. the control is clearly disabled with an accurate explanation and no claim that the feature exists.

A feature is not complete because:

- a callback exists;
- a status string changes;
- a screenshot looks convincing;
- a model type exists;
- an exporter writes a syntactically valid but semantically incomplete file;
- a CLI journey exercises a narrower path than the GUI implies;
- a sample project contains hard-coded output;
- a test only verifies launch, presence, or string matching.

## 2.3 Vertical-slice definition

Every shipped feature must include:

```text
Domain model
Command or operation
Selection semantics
UI access
Keyboard access
Accessibility semantics
Persistence and migration
Undo and redo
Autosave and crash recovery
Cancellation where relevant
Failure behavior
Unit tests
Integration tests
Real in-application journey
Visual evidence
Performance measurement where relevant
Documentation or user-facing explanation
```

If one item is missing, classify the feature as `FUNCTIONAL_WITH_LIMITATIONS`, `EXPERIMENTAL`, `SCAFFOLDED`, or `NOT_STARTED` in `TRUTH.md`.

## 2.4 One branch, controlled parallelism

Agents must receive exclusive subsystem ownership for a tranche. Shared contracts are modified only by the coordinator or by one explicitly assigned shared-platform agent.

Recommended ownership boundaries:

```text
loom-writer/**      Writer agent
loom-sheets/**      Sheets agent
loom-present/**     Present agent
loom-photo/**       Photo agent
loom-motion/**      Motion agent
loom-video/**       Video agent
loom-studio/**      Studio agent
loom-encode/**      Encode agent
loom-core/**        Shared-platform agent
loom-vision/**      Vision agent
loom-plugin-sdk/**  Plugin agent
loom-bootstrap/**   QA/release agent
```

Agents must not weaken audits, lower thresholds, delete failing tests, replace real journeys with string checks, or mark unsupported behavior complete.

---

# 3. Target architecture

The repository is a monorepo containing independent Cargo workspaces. Preserve that structure.

```text
rust-loom/
├── loom-core/
├── loom-vision/
├── loom-plugin-sdk/
├── loom-writer/
├── loom-sheets/
├── loom-present/
├── loom-photo/
├── loom-motion/
├── loom-video/
├── loom-studio/
├── loom-encode/
└── loom-bootstrap/
```

## 3.1 Dependency direction

```text
Application UI
    ↓
Application controller / commands
    ↓
Application domain engine
    ↓
Narrow loom-core crates
    ↓
Platform adapters and optional backends
```

Rules:

- Slint components must not own domain truth.
- UI callbacks dispatch typed commands; they must not perform complex file, media, audio, or model work directly.
- Domain engines must remain testable without creating a window.
- Platform-specific code must sit behind narrow traits.
- Optional FFmpeg, CPAL, GPU, model, and plugin backends must not contaminate package schemas.
- Avoid an undifferentiated shared `common` crate.
- Break monolithic application `main.rs` files into `app_state`, `commands`, `controller`, `desktop`, `journeys`, and feature modules as functionality grows.

## 3.2 Standard application layout

Each app should converge on:

```text
crates/
├── loom-<app>-core/          # domain model, operations, persistence
├── loom-<app>-app/           # Slint host, controllers, desktop adapters
├── loom-<app>-cli/           # deterministic headless workflows
└── loom-<app>-interop/       # external formats when substantial
```

Inside the desktop app:

```text
src/
├── main.rs                   # startup only
├── app_state.rs
├── controller.rs
├── commands.rs
├── desktop_services.rs
├── recovery.rs
├── jobs.rs
├── journeys.rs
└── features/
```

---

# 4. Shared implementation contracts

## 4.1 Typed commands and honest enablement

Every meaningful user action must be represented by a typed command whose enablement is derived from domain state.

```rust
pub trait AppCommand<D>: Send + 'static {
    fn id(&self) -> &'static str;
    fn label(&self, document: &D) -> String;
    fn is_enabled(&self, document: &D, selection: &Selection) -> bool;
    fn execute(
        &self,
        document: &mut D,
        selection: &mut Selection,
        services: &dyn AppServices,
    ) -> Result<CommandOutcome, CommandError>;
}

pub struct CommandOutcome {
    pub history: Option<HistoryRecord>,
    pub invalidated_regions: Vec<Invalidation>,
    pub follow_up_jobs: Vec<JobRequest>,
    pub announcement: Option<String>,
}
```

Toolbar buttons, menus, keyboard shortcuts, context menus, command palette entries, accessibility actions, and plugin commands must use the same command registry.

## 4.2 Shared selection vocabulary

Selection must be explicit and application-aware rather than inferred from whichever widget has focus.

```rust
pub enum Selection {
    None,
    Text(TextSelection),
    Grid(GridSelection),
    Scene(SceneSelection),
    Timeline(TimelineSelection),
    Layers(LayerSelection),
    Audio(AudioSelection),
    Queue(QueueSelection),
}

pub struct TextSelection {
    pub anchor: TextPosition,
    pub focus: TextPosition,
    pub affinity: CaretAffinity,
}
```

Selection changes are first-class state transitions. They must update inspectors, command enablement, accessibility announcements, and keyboard behavior without mutating the document.

## 4.3 Operation-level undo

Whole-document snapshots may remain as a fallback for small reference documents, but production editing must use reversible operations or bounded deltas.

```rust
pub trait ReversibleOperation<D>: Send {
    fn name(&self) -> &str;
    fn apply(&mut self, document: &mut D) -> Result<(), OperationError>;
    fn revert(&mut self, document: &mut D) -> Result<(), OperationError>;
    fn merge(&mut self, newer: &dyn ReversibleOperation<D>) -> bool;
    fn estimated_bytes(&self) -> usize;
}
```

History requirements:

- named atomic operations;
- compound transactions;
- typing, drag, brush, slider, trim, and automation coalescing;
- memory and disk budgets;
- deterministic replay;
- persistence boundaries;
- plugin operation participation;
- recovery journal compatibility;
- no undo entry for failed or cancelled operations.

## 4.4 Asynchronous jobs

Long-running work must use one shared cancellable job framework.

```rust
pub trait LoomJob: Send + 'static {
    type Output: Send + 'static;

    fn kind(&self) -> &'static str;
    fn estimate(&self) -> ResourceEstimate;
    fn run(
        self,
        context: JobContext,
        progress: ProgressSink,
        cancel: CancellationToken,
    ) -> Result<Self::Output, JobError>;
}
```

Mandatory users include import, export, save compaction, thumbnail and waveform generation, media decode, proxy creation, Vision inference, font scanning, plugin discovery, indexing, conformance analysis, and package migration.

## 4.5 Native desktop services

Replace hard-coded filenames and typed filesystem paths with a shared platform service.

```rust
pub trait DesktopServices: Send + Sync {
    fn open_file(&self, request: OpenFileRequest) -> DesktopFuture<Option<PathBuf>>;
    fn save_file(&self, request: SaveFileRequest) -> DesktopFuture<Option<PathBuf>>;
    fn reveal_in_file_manager(&self, path: &Path) -> Result<(), DesktopError>;
    fn open_external(&self, target: &str) -> Result<(), DesktopError>;
    fn recent_documents(&self, app_id: &str) -> Result<Vec<RecentDocument>, DesktopError>;
    fn set_recent_document(&self, app_id: &str, path: &Path) -> Result<(), DesktopError>;
}
```

Backends:

- Linux: XDG Desktop Portal first, native fallback only when justified.
- Windows: `IFileOpenDialog`, `IFileSaveDialog`, shell recent documents, file associations.
- macOS: `NSOpenPanel`, `NSSavePanel`, document types, recent documents, services.

The shared shell must also provide menus, clipboard, drag and drop, notifications, printing hooks, URL opening, application data paths, crash-recovery locations, and platform appearance integration.

## 4.6 Render graph

Photo, Motion, Video, Present, and parts of Writer require a shared scene and render architecture.

```rust
pub struct RenderRequest {
    pub surface: SurfaceDescriptor,
    pub time: TimelineTime,
    pub region: DamageRegion,
    pub quality: RenderQuality,
    pub color_context: ColorContext,
}

pub trait RenderNode: Send + Sync {
    fn bounds(&self, context: &RenderContext) -> Bounds;
    fn dependencies(&self) -> &[NodeId];
    fn render(&self, context: &mut RenderContext) -> Result<RenderOutput, RenderError>;
}
```

Requirements:

- CPU reference path for deterministic tests;
- `wgpu` production path;
- damage tracking and tile invalidation;
- colour-managed intermediate surfaces;
- linear-light compositing where required;
- predictable cache keys;
- device-loss recovery;
- no silent CPU/GPU output divergence beyond measured tolerance;
- shared effect parameter and animation model.

## 4.7 Media clock and synchronized playback

Video and Motion must render inside Loom; Studio must provide realtime audio. External players are diagnostic helpers, not the product playback path.

```rust
pub trait PlaybackClock: Send + Sync {
    fn state(&self) -> TransportState;
    fn position(&self) -> MediaTime;
    fn seek(&self, target: MediaTime) -> Result<(), PlaybackError>;
    fn play(&self) -> Result<(), PlaybackError>;
    fn pause(&self) -> Result<(), PlaybackError>;
}

pub struct PlaybackFrame {
    pub presentation_time: MediaTime,
    pub image: VideoFrame,
    pub dropped_before: u64,
}
```

For video, audio is the default master clock when audio exists. Decode, effects, scaling, presentation, and audio buffering must use explicit time bases. Measure A/V drift, seek latency, dropped frames, underruns, and cache pressure.

## 4.8 Interoperability reports

Every import must be capable of reporting substitutions and losses.

```rust
pub struct InteropReport {
    pub source_format: FormatId,
    pub source_version: Option<String>,
    pub preserved: Vec<FeatureId>,
    pub substituted: Vec<Substitution>,
    pub dropped: Vec<Loss>,
    pub warnings: Vec<InteropWarning>,
    pub unsupported: Vec<FeatureId>,
}
```

Compatibility tests must compare semantic structures, rendered pages/frames where appropriate, formulas, timing, fonts, media references, and round-trip loss. Extension detection or successful parsing is not compatibility evidence.

## 4.9 Feature evidence record

Generated parity evidence must use a structure equivalent to:

```rust
pub struct FeatureEvidence {
    pub feature_id: String,
    pub state: FeatureState,
    pub engine_tests: Vec<TestRef>,
    pub ui_journeys: Vec<JourneyRef>,
    pub persistence_tests: Vec<TestRef>,
    pub undo_tests: Vec<TestRef>,
    pub failure_tests: Vec<TestRef>,
    pub accessibility_evidence: Vec<ArtifactRef>,
    pub performance_evidence: Vec<BenchmarkRef>,
    pub known_limitations: Vec<String>,
}
```

A feature marked `COMPLETE` must have evidence in all applicable fields.

---

# 5. Suite-wide 23 → 100 roadmap

Do not attempt every advanced feature simultaneously. Complete the following stages in order while parallelizing independent application work inside each stage.

## Stage A — 23 → 35: Desktop authenticity and semantic honesty

Goal: remove development-tool behavior and visible placebo interactions.

Required work:

1. Shared native open, save, save-as, export, and recent-document services.
2. Platform menus and a unified command registry in all eight apps.
3. Real clipboard, drag and drop, and file-association paths.
4. Refactor fixed filenames and typed path fields out of primary workflows.
5. Replace Motion transport status changes with a real composition clock, even if initially CPU-only.
6. Replace Video external-player-only playback with in-window single-source playback and synchronized transport foundations.
7. Make Writer formatting selection-aware; remove or disable unsupported sample claims.
8. Add actual zoom semantics to Writer, Present, Motion, Photo, and Video.
9. Audit every visible control for engine mutation, persistence, undo, and failure behavior.
10. Add real keyboard command-side-effect journeys, not only palette overlay checks.

Exit gates:

- No enabled critical control merely changes a label or status string.
- Every app opens and saves arbitrary user-selected paths.
- A create/edit/save/close/reopen/export journey passes through the GUI on all supported native targets.
- All unsupported introductory text and tooltips are removed.
- Suite parity may not exceed 35 until these gates pass.

## Stage B — 35 → 50: Direct manipulation and professional editing surfaces

Goal: make each workspace behave like the editor it visually represents.

Required work:

- Shared pointer capture, transform handles, snapping, guides, marquee selection, multi-selection, keyboard nudging, context menus, and inspector binding.
- Writer: range selection, caret model, selection-aware styles, page zoom and scrolling.
- Sheets: virtualized scrolling grid, range selection, edit mode, fill handle, row/column resize, freeze panes.
- Present: true scene graph rendering and direct move/resize/rotate/multi-select.
- Photo: transform tool, selections, masks, brushes, gradients, layer groups.
- Motion: real layer content, playhead, keyframe lanes, graph editor basics.
- Video: draggable clips, real trim handles, ripple/roll/slip/slide foundations, waveform/thumbnails.
- Studio: draggable and trimmable regions, playhead, loop range, mixer interaction.
- Encode: reorderable jobs, native source/output selection, editable stream controls.

Exit gates:

- Every application has at least one complete, substantial mouse-and-keyboard editing journey.
- Direct manipulation produces named undo entries and survives save/reopen.
- Inspector state follows selection without stale or contradictory values.
- 200-step randomized interaction tests produce no crash or invalid document state.

## Stage C — 50 → 65: Core production engines

Goal: replace reference engines with robust primary workflows.

Required shared engines:

- professional text shaping/layout;
- virtualized data grid and formula recalculation scheduling;
- scene graph and GPU render graph;
- colour pipeline and ICC handling;
- media demux/decode/cache/playback;
- realtime audio graph and device negotiation;
- operation-level history and recovery journals;
- import/export framework with semantic reports.

Exit gates:

- Writer completes a long-form paginated document.
- Sheets completes a multi-sheet analytical workbook.
- Present completes a designed presentation with reusable masters.
- Photo completes a multilayer masked composition.
- Motion completes and renders a keyframed title sequence.
- Video completes a synchronized short-form edit.
- Studio records, edits, mixes, and bounces a multitrack session.
- Encode completes a multi-destination batch with recovery.

## Stage D — 65 → 80: Professional feature breadth

Goal: cover the operations professionals use daily.

Required work is application-specific and defined in section 7. Prioritize depth over checkbox count. Every tranche must finish one coherent workflow with UI, engine, persistence, undo, tests, failures, accessibility, and performance evidence.

Exit gates:

- Each app passes three representative professional project journeys.
- No app relies on hard-coded sample content for its acceptance evidence.
- Performance corpora include large documents, workbooks, images, compositions, timelines, sessions, and queues.
- All core workflows remain functional with the network disabled.

## Stage E — 80 → 90: Interoperability and suite integration

Goal: make Loom practical in existing creative and office environments.

Required work:

- semantic DOCX/ODT, XLSX/ODS, PPTX/ODP, PSD/OpenRaster, media, MIDI, subtitle, and interchange corpora;
- cross-app clipboard and drag/drop formats;
- shared assets and relinking;
- Motion templates consumed by Video and Present;
- Video, Motion, Present, Photo, and Studio submission to Encode;
- Vision results converted to ordinary editable selections, masks, text, tables, captions, markers, or automation data;
- unified colour, font, media, job, and recovery behavior;
- migration compatibility across at least three package schema versions.

Exit gates:

- Every supported external format has published generated loss reports and regression fixtures.
- Cross-app workflows preserve editability rather than flattening prematurely.
- Missing fonts, plugins, models, and media produce actionable relink/substitution UI.

## Stage F — 90 → 97: Accessibility, reliability, and performance hardening

Required work:

- real screen-reader journeys on Windows, macOS, and Linux where platform support permits;
- complete keyboard-only journeys;
- high-contrast, large-text, reduced-motion, RTL, pseudolocale, and IME validation;
- crash injection at every save, journal, import, export, render, and recording boundary;
- low-memory, low-disk, device-loss, media-corruption, plugin-crash, and model-failure tests;
- memory leak and cache-bound tests;
- startup, input latency, frame pacing, seek, render, save, recalculation, and audio benchmarks;
- security fuzzing and sandbox validation.

Exit gates:

- No severity-1 or severity-2 data-loss, accessibility-blocking, or security defect.
- Representative projects can be edited continuously for eight-hour soak tests.
- Recovery restores the last durable state after injected termination.
- Performance budgets pass on the defined baseline hardware tier.

## Stage G — 97 → 100: Release proof

Required work:

- clean-machine installation and uninstall evidence;
- signed or verifiably checksummed packages;
- file associations and launch-from-document behavior;
- update and rollback mechanism or a documented manual release policy;
- reproducible build comparison;
- SBOM and dependency/license inventory;
- migration from previous public Loom versions;
- final professional user acceptance projects;
- independent visual, accessibility, performance, security, and interoperability review.

100/100 is awarded only after the full checklist in section 14 passes. It is not awarded because the roadmap is exhausted.

---

# 6. Shared UI/UX productisation programme

## 6.1 Design-system completion

Consolidate and test:

- semantic colour tokens for all states;
- typography roles and platform font fallback;
- density modes without clipping;
- window, toolbar, sidebar, inspector, canvas, timeline, grid, modal, popover, menu, tooltip, progress, error, and empty-state components;
- keyboard focus and accessibility actions;
- reduced-motion behavior;
- light, dark, and high-contrast parity;
- compact, reference, large, and scaled-text layouts.

Do not chase visual novelty while primary interactions remain fake or incomplete.

## 6.2 Interaction quality gates

Every interaction must define:

```text
Pointer behavior
Keyboard equivalent
Touchpad/scroll behavior
Selection result
Snapping behavior
Cursor feedback
Focus result
Inspector update
Undo grouping
Persistence result
Accessibility announcement
Cancellation behavior
Error behavior
Animation and reduced-motion behavior
```

## 6.3 Animation policy

Animations must be interruptible and communicate continuity. Use existing Loom motion tokens, but measure rather than merely declare smoothness.

- pointer feedback within one frame where practical;
- direct manipulation must never wait for decorative animation;
- panel transitions must preserve focus;
- selection and transform overlays must track content without visible lag;
- reduced-motion mode must remove spatial motion while preserving state communication;
- animation frame pacing must be captured in native journeys.

## 6.4 Visual QA

For every app and theme, capture:

- empty state;
- representative project;
- active selection;
- direct manipulation in progress;
- command palette;
- menu and context menu;
- native dialog handoff where capturable;
- inspector states;
- progress and cancellation;
- recoverable error;
- inaccessible backend or missing dependency;
- compact, reference, large, 150% text, high contrast, RTL, and reduced motion.

Visual review must inspect hierarchy, clipping, density, alignment, legibility, contrast, focus, state clarity, and whether the screenshot implies behavior that the app does not implement.

---

# 7. Application roadmaps

## 7.1 Loom Writer — target 100

### W1. Selection-aware editor

- Replace document-wide formatting with stable text ranges and caret positions.
- Preserve style runs through insertion, deletion, split, merge, paste, and undo.
- Implement grapheme, word, line, paragraph, and document navigation.
- Add IME composition and bidirectional cursor tests.
- Bind toolbar, inspector, shortcuts, and command palette to the same selection-aware commands.

Acceptance: create mixed-style paragraphs using mouse and keyboard; save/reopen preserves exact styles; undo restores selection and content.

### W2. Professional text layout

- Integrate shaping, bidi, font fallback, OpenType features, line breaking, hyphenation, justification, tabs, and baseline metrics.
- Build deterministic layout objects separate from editor widgets.
- Add pagination invalidation so edits relayout only affected regions.

Acceptance: multilingual 200-page corpus renders deterministically without missing glyphs or cursor drift.

### W3. Page model

- Page size, margins, orientation, sections, columns, page and section breaks.
- Headers, footers, page numbering, footnotes, endnotes, widow/orphan control.
- Continuous and paginated modes using the same document model.

### W4. Structured content

- Lists, tables, styles, outlines, TOC, bookmarks, citations, bibliography, cross-references, comments, tracked changes.
- Named style editing and style inheritance.

### W5. Objects and layout

- Images, shapes, text frames, anchored/floating objects, text wrap, captions, object selection, resize, crop, alignment, layering.

### W6. Print and export

- Print preview and platform printing.
- PDF with embedded fonts, links, outlines, tagged structure where supported.
- EPUB, Markdown, HTML, text, and image export.

### W7. Interoperability

- Semantic DOCX and ODT import/export with per-document loss reports.
- Fixture corpus covering styles, sections, tables, lists, headers, footnotes, images, tracked changes, and complex scripts.

### W8. Scale and accessibility

- 500-page editing benchmark.
- document outline and accessible structure tree;
- screen-reader navigation by heading, paragraph, table, comment, and revision.

## 7.2 Loom Sheets — target 100

### S1. Virtual workbook grid

- Multi-sheet workbook model.
- Row/column virtualization independent of total sheet size.
- Frozen panes, hidden rows/columns, variable dimensions, merged cells.
- Range, row, column, multi-range, and discontiguous selection.

Acceptance: smooth navigation of a 1,000,000-row sparse sheet with bounded memory.

### S2. Editing and clipboard

- Cell edit mode, formula bar, fill handle, move/copy ranges, paste-special, autofill, undo grouping.
- Rich formatting, number formats, dates, time, duration, currency, units, styles, comments.

### S3. Formula engine

- Expand function registry and type system.
- Cross-sheet references, structured references, array formulas, date/time semantics.
- Parallel incremental recalculation with deterministic dependency scheduling.
- Cycle diagnostics, tracing, precedents/dependents, goal seek.

### S4. Tables, filters, and validation

- Structured tables, filter UI, sorting, grouping, subtotals, conditional formatting, validation messages.

### S5. Charts

- Shared chart scene model, axes, legends, data labels, accessible descriptions, selection, resizing, themes, and export.

### S6. Pivot and analysis

- Pivot model, aggregation, grouping, calculated fields, refresh, drill-down, local file/database connectors.

### S7. Interoperability

- XLSX and ODS semantic import/export.
- Preserve formulas, shared strings, styles, number formats, charts where supported, print settings, names, and sheet structure.

### S8. Accessibility and scale

- keyboard navigation comparable to a serious spreadsheet;
- screen-reader announcements for coordinates, values, formulas, errors, headers, and selection ranges;
- million-row and wide-workbook performance corpus.

## 7.3 Loom Present — target 100

### P1. Real scene canvas

- Render every element from its actual geometry.
- Move, resize, rotate, multi-select, marquee, align, distribute, snap, group, lock, order, and keyboard nudge.
- Shared transform overlay and inspector binding.

### P2. Themes and masters

- Master slides, layouts, placeholders, theme fonts/colours, reusable styles, guides, background inheritance.

### P3. Content breadth

- Rich text, tables, charts, shapes, images, audio, video, equations, links, masks, crop, and object effects.

### P4. Animation

- Object animation timeline, keyframes, builds, ordering, easing, motion paths, preview, reduced-motion authoring feedback.

### P5. Presenter workflows

- Multi-display presenter view, notes, next slide, timer, navigation, rehearsal timings, recording, pointer and annotation.

### P6. Export

- PDF, image sequence, video with synchronized audio and animation, accessible structure where supported.

### P7. Interoperability

- PPTX and ODP corpora for masters, themes, charts, tables, media, notes, transitions, and animations.

### P8. Accessibility

- reading order editor, alt text, contrast warnings, keyboard authoring, accessible presentation output.

## 7.4 Loom Photo — target 100

### PH1. Tile compositor and canvas

- GPU and CPU tile renderers, damage tracking, zoom pyramid, pan, rotate canvas, proofing, device-loss recovery.

### PH2. Layer model

- Pixel, vector, text, fill, adjustment, group, smart/reference, mask, clipping, blend modes, isolation, lock states.

### PH3. Selection and transform

- Rectangle, ellipse, lasso, polygon, colour range, magic selection, feather, grow/shrink, refine edge.
- Interactive move, scale, rotate, skew, perspective, warp, crop, and nondestructive transform.

### PH4. Painting and retouching

- Brush engine with spacing, pressure, tilt, smoothing, opacity, flow, blending, eraser, clone, heal, dodge/burn, gradients.
- Stroke-level undo and tile invalidation.

### PH5. Adjustments and effects

- Levels, curves, exposure, white balance, HSL, selective colour, channel mixer, LUT, blur, sharpen, noise, distort, lighting.
- Shared effect graph with Motion and Video where appropriate.

### PH6. Colour and RAW

- ICC profiles, display conversion, soft proofing, 8/16/32-bit processing, linear-light pipeline, HDR, RAW development.

### PH7. Formats

- PNG, JPEG, TIFF, WebP, AVIF, EXR, OpenRaster, layered project export, measured PSD support, metadata preservation.

### PH8. Advanced workflows

- panorama, HDR merge, focus stack, batch processing, macros, asset relinking, large-canvas memory budgets.

### PH9. Vision integration

- subject/object selection, portrait matting, background removal, inpainting, denoise, super-resolution.
- Every result becomes an ordinary editable layer, mask, or selection; model packs are optional and locally installed.

## 7.5 Loom Motion — target 100

### M1. Production composition model

- Real text, vector, raster, video, audio-reference, nested composition, camera, light, and generator layers.
- Parenting, constraints, anchors, transform hierarchy, visibility, solo, lock, blend, masks.

### M2. Realtime compositor

- Shared GPU render graph, CPU reference renderer, render cache, colour pipeline, frame scheduling, device recovery.

### M3. Timeline and keyframes

- Real playhead and transport, layer bars, in/out points, keyframe lanes, graph editor, interpolation, easing, motion paths, copy/paste, retiming.

### M4. Direct manipulation

- Canvas transforms, rotation, anchor editing, path editing, guides, snapping, multi-selection, 2.5D controls.

### M5. Effects and behaviors

- Effect stack, parameter animation, masks, matte, keying, particles, replicators, behaviors, text animators, motion blur.

### M6. Tracking and analysis

- Point/planar/object tracking, stabilization, optical flow, rotoscoping, result correction, caching, failure diagnostics.

### M7. Audio and preview

- Synchronized reference audio, scrubbing, waveform, loop playback, adaptive preview quality, dropped-frame reporting.

### M8. Rendering

- Image sequence and video render queue, resumable jobs, background rendering, alpha, colour/HDR metadata, Encode integration.

### M9. Templates

- Versioned editable templates for Video and Present with safe parameter exposure, fonts/media dependencies, and migration.

## 7.6 Loom Video — target 100

### V1. Media library

- Events/bins, metadata, tags, keywords, favourites/rejects, duplicate detection, thumbnails, waveforms, background analysis, relinking.

### V2. Media engine

- In-process demux/decode, explicit time bases, cache, hardware acceleration with software fallback, corrupt-media handling.

### V3. Synchronized timeline playback

- Audio-master clock, video presentation queue, seek, scrub, shuttle, loop, frame-step, preroll, dropped-frame and drift metrics.
- Eliminate external FFplay as the product playback surface.

### V4. Editing model

- Select, blade, ripple, roll, slip, slide, trim, overwrite, insert, lift, range selection, linked/related clips, snapping, markers.
- Every edit is operation-level undoable and survives relink/save/reopen.

### V5. Timeline structure

- Compound/nested clips, synchronized clips, multicam, auditions/alternates if retained as an original design, captions, roles/lanes.

### V6. Audio

- Waveforms, gain, fades, pan, roles, submixes, keyframes, meters, loudness, channel mapping, sync correction, Studio handoff.

### V7. Titles, transitions, and effects

- Shared render graph, Motion templates, parameter keyframes, masks, tracking, generators, effect browser, background rendering.

### V8. Colour

- Primary/secondary correction, curves, wheels, LUTs, scopes, colour management, HDR, render/export consistency.

### V9. Analysis and Vision

- Transcription, caption generation, scene detection, subject tracking, reframing, stabilization, background removal with editable results.

### V10. Interchange and delivery

- Encode integration, XML/EDL/AAF where support is defensible, subtitle formats, relinking, archive/consolidate, proxy and optimized-media workflows.

## 7.7 Loom Studio — target 100

### ST1. Realtime engine

- Lock-free or bounded realtime-safe audio graph.
- Device negotiation, sample-rate conversion, buffer-size control, transport clock, underrun/overrun metrics.
- No allocation, file I/O, logging, locks, or UI work on the audio callback.

### ST2. Recording

- Multi-track audio and MIDI recording, pre-roll, count-in, punch, loop recording, take folders, crash-safe recording files.

### ST3. Arrangement editing

- Regions with drag, trim, split, slip, duplicate, loop, fades, crossfades, snapping, markers, tempo and meter maps.

### ST4. MIDI and instruments

- Piano roll, drum editor, note expression, quantization, velocity, controllers, scale/chord tools, software instrument API, original starter instruments.

### ST5. Mixer

- Channel strips, buses, sends, sidechains, groups, automation, meters, routing graph, latency compensation.

### ST6. Plugin hosting

- CLAP first, VST3 where redistribution and SDK terms permit.
- Discovery cache, validation, out-of-process hosting, crash recovery, resource limits, state persistence, UI embedding/bridging, version migration.

### ST7. Editing and analysis

- Sample editor, comping, transient detection, time stretch, pitch correction, beat/key analysis, noise cleanup.

### ST8. Quick workspace

- Loop browser, guided track creation, simplified controls, original instruments and rhythm tools, templates that use the same Pro engine.

### ST9. Mastering and delivery

- Loudness, true peak, dynamics, EQ, limiting, dithering, stems, surround/spatial architecture where supported, Encode integration.

### ST10. Reliability and latency proof

- Device hot-plug, plugin crash, recording interruption, project recovery, long-session soak tests, baseline and professional latency tiers.

## 7.8 Loom Encode — target 100

### E1. Source inspection

- FFprobe-backed stream model, chapters, attachments, metadata, colour/HDR, subtitles, audio layouts, image sequences.

### E2. Deterministic plan model

- Explicit video, audio, subtitle, metadata, scaling, crop, frame-rate, colour, HDR, filter, and destination settings.
- Command preview is generated from a typed plan rather than concatenated UI strings.

### E3. Queue orchestration

- Reorder, dependencies, multi-destination jobs, concurrency limits, priority, retry, persistent progress, restart recovery, safe cancellation.

### E4. Hardware policy

- Detect available encoders, test capability, choose policy explicitly, expose fallback, measure quality/performance, report unavailable features.

### E5. Pause and resume

- Implement true resume where the format and backend allow it; otherwise expose restart/checkpoint semantics honestly.

### E6. Integration

- Receive typed jobs from Video, Motion, Present, Photo, and Studio while allowing destination overrides without mutating source projects.

### E7. Conformance

- Decode verification, duration/stream/layout checks, frame and audio sampling, perceptual metrics, loudness, output-size and metadata validation.

---

# 8. Cross-application workflows

These workflows are required for suite parity:

1. Photo layer or composition placed in Present and retained as a linked, refreshable asset.
2. Motion template installed once and edited parametrically inside Video and Present.
3. Video timeline sent to Encode as a typed job with destination presets.
4. Studio mix or stems refreshed in Video without destructive reimport.
5. Writer table copied to Sheets and pasted back as either editable data or a linked table.
6. Sheets chart placed in Writer or Present with editable source data and update semantics.
7. Vision OCR converted into editable Writer text with source-region provenance.
8. Vision table extraction converted into an editable Sheets range.
9. Vision segmentation converted into Photo masks and Motion/Video mattes.
10. Shared media relink and font substitution UI across all applications.

Cross-app clipboard payloads must include a Loom-native editable representation plus standard fallbacks such as text, image, HTML, or file URLs.

---

# 9. Interoperability programme

For each format family, build a committed minimal corpus and a larger generated CI corpus. Include clean, malformed, adversarial, multilingual, high-complexity, and round-trip fixtures.

Required comparison methods:

- document structure tree;
- computed formula values and formula text;
- styles and number formats;
- slide/layer geometry;
- animation and transition timing;
- image layer/mask/blend semantics;
- media stream/timestamp/channel metadata;
- MIDI notes, tempo, and controller data;
- rendered page/frame perceptual comparison;
- import and export loss reports.

A format support label must be one of:

```text
DETECT_ONLY
READ_PARTIAL
READ_SUPPORTED
WRITE_PARTIAL
ROUND_TRIP_SUPPORTED
CONFORMANCE_VALIDATED
```

Do not use a generic “supported” label.

---

# 10. Accessibility and localization programme

Every application must provide:

- complete tab and spatial keyboard navigation;
- command access without pointer use;
- logical focus restoration after dialogs and overlays;
- accessible names, values, roles, states, and actions;
- non-colour state communication;
- screen-reader announcements for selection, edits, errors, progress, and completion;
- high-contrast and scalable text without clipped controls;
- reduced-motion behavior;
- RTL and bidirectional layout where applicable;
- IME composition for text entry;
- pseudolocalized stress captures.

Canvas, grid, timeline, layer, and mixer accessibility must expose a navigable semantic model rather than a single anonymous surface.

Accessibility completion requires recorded native journeys and manual review notes in CI artifacts. Static labels alone are insufficient.

---

# 11. Performance budgets

Define benchmark tiers in generated evidence. Initial mainstream targets:

| Workflow | Target |
|---|---:|
| Lightweight warm launch | < 1.0 s where feasible |
| Lightweight cold launch | < 2.0 s where feasible |
| Pointer/keyboard feedback | visible within one display frame where practical |
| 60 Hz interaction | p95 frame < 16.7 ms |
| 120 Hz-ready critical manipulation | architecture capable of p95 < 8.3 ms on capable hardware |
| Autosave | no visible input stall |
| Cancellation feedback | < 100 ms |
| Video seek feedback | preview response < 150 ms when cached, measured separately when uncached |
| Audio transport start | bounded and measured per device tier |
| A/V drift | < 20 ms sustained target, with visible diagnostics outside tolerance |
| Spreadsheet edit recalculation | interactive cells < 50 ms for representative workbook |
| Recovery journal durability | documented maximum lost-operation window |

Application-specific corpora:

- Writer: 500 pages, mixed scripts, images, tables, footnotes.
- Sheets: one million sparse rows, 100k formulas, charts, cross-sheet dependencies.
- Present: 200 slides, media, charts, and animations.
- Photo: 16k image, 100 layers, masks, adjustment stack.
- Motion: 4K composition, 100 layers, animated effects.
- Video: 4K long-GOP and intraframe media, multicam, effects, proxies.
- Studio: 100 tracks, plugins, automation, low-buffer playback.
- Encode: multi-job 4K batch with hardware and software backends.

Budgets must be measured on real native builds. Headless CI numbers are regression signals, not universal user promises.

---

# 12. Testing and evidence gates

## 12.1 Required test layers

1. Unit tests for domain invariants.
2. Property tests for serialization, history, range, timeline, transform, and formula behavior.
3. Fuzz tests for package, importer, media, plugin, model-pack, subtitle, formula, and recovery inputs.
4. Integration tests for save/reopen, migration, import/export, recovery, cancellation, low disk, missing dependency, and corruption.
5. In-application journeys that operate real controls and verify domain side effects.
6. Native keyboard, accessibility, and desktop-service journeys.
7. Visual regression with reviewed baselines.
8. Performance and soak tests.
9. Clean-machine packaging tests.

## 12.2 Definition of done

A task is complete only when all applicable checks pass:

```text
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
package round-trip tests
corruption and resource-limit tests
operation undo/redo tests
recovery replay tests
GUI journey with domain assertion
keyboard journey
accessibility evidence
visual matrix
performance budget
native target matrix
```

## 12.3 Failure injection

Add deterministic failpoints for:

- before and after temp-file creation;
- during package member write;
- before atomic rename;
- during journal append;
- during import and migration;
- during render/export finalization;
- during recording flush;
- during proxy/cache commit;
- during plugin state save;
- during model-pack install.

Every critical persistence path must prove that the prior valid state survives interruption.

---

# 13. Agent execution protocol

## 13.1 Tranche workflow

For each tranche:

1. Read `AGENTS.MD` and `TRUTH.md`.
2. Inspect the active engine, UI wiring, tests, CI, and generated evidence.
3. Define a coherent vertical slice and exact acceptance journey.
4. Implement engine behavior first or in lockstep with UI.
5. Add operation-level undo and recovery semantics.
6. Add failures and cancellation.
7. Add GUI, keyboard, and accessibility access.
8. Run focused tests.
9. Run the affected workspace release build.
10. Run suite contracts and audits.
11. Update `TRUTH.md` only to reflect proven behavior.
12. Report the new truth score and why it changed.

## 13.2 Commit discipline

- One coherent subsystem or vertical slice per commit.
- No drive-by audit weakening.
- No generated reports committed as prose.
- No dead alternative UI files.
- No disabled tests without an explicit source comment and tracked repair in this roadmap.
- No large dependency without license, maintenance, platform, security, and replacement analysis.
- No `unsafe` without a safety comment, containment, and tests.

## 13.3 Truth score reporting

After every major milestone report:

```text
Previous suite score
New suite score
Changed weighted dimensions
Implemented user journeys
Evidence produced
Remaining caps
Known limitations
Next highest-leverage milestone
```

Do not increase a score solely because a plan, interface, model type, or disabled control was added.

---

# 14. Immediate implementation order

The next work should proceed in this order because these items unlock all applications and remove the largest score caps.

## Epic 1 — Shared native desktop services

Primary locations:

```text
loom-core/crates/loom-app-runtime/
loom-core/crates/loom-storage/
loom-core/crates/loom-clipboard/
loom-core/crates/loom-command/
loom-core/crates/loom-ui/
```

Deliver native file dialogs, save-as, recent files, menus, clipboard, drag/drop, and platform data paths. Convert Writer first, then Sheets, Present, Photo, Motion, Video, Studio, and Encode.

## Epic 2 — Unified command registry

Replace per-app duplicated palette dispatch with shared command metadata, enablement, shortcuts, menu placement, accessibility invocation, and side-effect journey hooks.

## Epic 3 — Shared selection and transform framework

Implement text/range/scene/timeline/layer selections, pointer capture, handles, snapping, guides, keyboard nudging, and operation coalescing.

## Epic 4 — Writer selection-aware editing

This is the fastest path to one genuinely useful complete desktop application. Finish selection-aware rich text, page zoom, file dialogs, tables/lists, pagination, print/PDF, and long-document tests before broad advanced features.

## Epic 5 — Sheets virtual grid

Build virtualization and multi-sheet storage before adding charts or more toolbar controls.

## Epic 6 — Shared render graph

Create CPU reference and `wgpu` paths, then adopt in Photo, Motion, Present, and Video.

## Epic 7 — Video playback engine

Implement in-window decoded playback with an audio-master clock, cache, seek, and frame presentation. Remove the external-player product dependency.

## Epic 8 — Studio realtime graph

Separate the realtime callback from UI/project code; prove playback, recording, and recovery before plugin breadth.

## Epic 9 — Operation-level history and journals

Migrate high-frequency edits away from whole-document snapshots, beginning with Writer typing, Sheets cells, Photo strokes, Motion transforms, Video trims, and Studio regions.

## Epic 10 — Interoperability harness

Build semantic corpus runners and `InteropReport`; stop awarding meaningful parity for detection-only fixtures.

## Epic 11 — Accessibility harness

Record real focus, keyboard, text scaling, high contrast, reduced motion, and screen-reader evidence on native targets.

## Epic 12 — Encode conformance

Bring Encode to production usability first: native paths, stream controls, hardware policy, multi-destination queue, recovery, and decoded output verification.

---

# 15. Final 100/100 acceptance checklist

The suite may be declared 100/100 only when every item below is evidenced.

## Product

- [ ] All eight applications complete at least five substantial professional projects from blank state to final deliverable.
- [ ] No critical workflow requires a different application because Loom lacks a core operation.
- [ ] No enabled visible control is decorative, simulated, or misleading.
- [ ] All user-facing claims match `TRUTH.md` and executable behavior.

## UI/UX

- [ ] Direct manipulation, selection, inspectors, keyboard commands, menus, and undo agree in every app.
- [ ] Light, dark, high contrast, reduced motion, large text, compact, large, RTL, and pseudolocalized layouts pass review.
- [ ] Native desktop conventions work on Windows, macOS, and Linux.
- [ ] Professional users complete benchmark tasks without hidden development-only steps or typed filesystem paths.

## Engines

- [ ] Professional text layout, virtual grid, render graph, colour pipeline, media playback, realtime audio, jobs, history, and recovery are production validated.
- [ ] Video A/V synchronization and Studio realtime safety meet published budgets.
- [ ] GPU and CPU reference outputs stay within documented tolerances.

## Interoperability

- [ ] Every advertised format has an explicit support level.
- [ ] Round-trip-supported formats pass semantic and rendered corpora.
- [ ] Imports report substitutions, drops, and unsupported features.
- [ ] Cross-app data remains editable where the workflow promises editability.

## Reliability

- [ ] Atomic save and recovery pass injected termination at every critical boundary.
- [ ] Eight-hour edit/playback/recording/render soak tests pass representative projects.
- [ ] Low disk, missing assets, device loss, corrupt files, plugin crashes, and model failures are recoverable or safely reported.
- [ ] No open severity-1 or severity-2 data-loss defect.

## Accessibility

- [ ] All core journeys are keyboard-completable.
- [ ] Native screen-reader journeys pass or documented platform limitations have accessible alternatives.
- [ ] Canvas, grid, timeline, layer, queue, and mixer structures expose meaningful semantics.
- [ ] Tagged/accessible exports are produced where the target format supports them.

## Performance

- [ ] Representative project budgets pass on baseline hardware.
- [ ] Memory and GPU caches remain bounded.
- [ ] Input, scrolling, direct manipulation, playback, and audio remain responsive under background work.
- [ ] Performance regressions have enforced CI thresholds.

## Security and release

- [ ] Package, importer, media, plugin, model-pack, clipboard, and recovery fuzzing is clean to the configured budget.
- [ ] Plugins are permissioned and isolated; crashes do not terminate the host or corrupt projects.
- [ ] Model packs are checksummed, licensed, versioned, and reproducible.
- [ ] Install, uninstall, file association, launch-from-document, update/rollback policy, SBOM, license inventory, and clean-machine validation pass.
- [ ] Supported native packages are downloadable and independently verifiable.

## Final scoring rule

Passing this checklist permits an independent final audit. It does not automatically set the score to 100. The final score must be derived from executable evidence and a fresh professional workflow review.

---

# 16. Start and continuation directive

Do not respond to this file by writing another roadmap.

Inspect the current branch, choose the highest-priority incomplete epic from section 14 whose dependencies are satisfied, implement a complete vertical slice, test it, run the affected native evidence, update `TRUTH.md`, and report the new truth score.

Continue on `chatgpt/loom-ui-functional-audit-2` until the programme is integrated or the repository owner explicitly changes the branch policy.
