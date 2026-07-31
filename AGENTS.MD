# LOOM — AUTONOMOUS END-TO-END BUILD DIRECTIVE

## 0. Execution Contract

You are the autonomous engineering organization responsible for designing and building **Loom**, a professional, local-first creative software suite.

This is an execution task, not merely a planning or documentation exercise.

You must:

1. Create the complete multi-repository project structure.
2. Produce all architectural, product, design, implementation, testing, and agent documentation.
3. Implement the shared platform and each application.
4. Build runnable Linux applications using Rust and Slint.
5. Create automated tests, visual regression tests, performance tests, fuzzing targets, and CI workflows.
6. Run the applications inside an Ubuntu Docker environment for automated visual QA.
7. Fix problems discovered during compilation, testing, or visual review.
8. Package the resulting repositories into a verified ZIP archive.
9. Continue autonomously until all feasible deliverables and quality gates are complete.
10. Report incomplete functionality honestly rather than representing scaffolding or placeholders as finished work.

Do not stop after creating a roadmap.

Do not stop after creating documentation.

Do not stop after generating empty project scaffolds.

Do not ask the user to choose routine technical details. Make the most defensible engineering decision, document it in an ADR or RFC, and proceed.

Ask the user only when progress is impossible without information that cannot be inferred, such as unavailable credentials. Missing credentials must not prevent offline functionality from being implemented and tested.

Use multiple Sol subagents where available. Assign each subagent clear file and subsystem ownership. The coordinating agent remains responsible for integration, consistency, code review, testing, and final delivery.

“One-shot” means that this single directive should initiate and govern the complete autonomous workflow. It does not mean compressing the entire project into one model response. Continue working through the repository until the deliverables are generated and verified.

---

# 1. Product Mission

Build **Loom**, an original, open-source, professional creative suite for desktop computers.

Loom must combine:

* A calm and minimal interface.
* Professional depth.
* Excellent typography.
* Direct manipulation.
* Smooth, meaningful animation.
* High-performance native execution.
* Local-first storage.
* Offline-first operation.
* Local computer vision and machine learning.
* Strong accessibility.
* Documented open formats.
* Extensible, sandboxed plugins.
* Cohesion across applications.

Loom should provide the polish, simplicity, responsiveness, progressive disclosure, and workflow coherence associated with the best modern desktop creative applications.

However, Loom must be an original product.

Do not copy proprietary:

* Source code.
* Icons.
* Illustrations.
* Templates.
* Sounds.
* sample media.
* UI screenshots.
* Exact layouts.
* Exact control measurements.
* Exact animation choreography.
* Branding.
* Product names.
* Document formats.
* Trade dress.
* Undocumented behavior.

Study broad interaction principles and professional workflows, then design an independent implementation and visual identity.

The objective is not to reproduce another vendor’s application. The objective is to create an original suite that reaches an equivalent level of usability, capability, and refinement.

---

# 2. Non-Negotiable Product Principles

## 2.1 Local First

Every core workflow must function without an internet connection.

Users must be able to:

* Create files.
* Open files.
* Edit files.
* Save files.
* Export files.
* Search local files.
* Run supported computer-vision features.
* Use installed local models.
* Recover unsaved work.
* Render projects.
* Transcode media.
* Install local plugin packages.
* Read all bundled help material.

No mandatory account may exist.

No cloud service may be required.

No cloud synchronization architecture should be included in the initial implementation.

Do not create placeholder cloud directories, cloud interfaces, remote collaboration APIs, analytics services, remote model APIs, or account systems “for later.”

A future cloud capability may only be introduced as an optional external plugin after the local platform is complete. The current architecture must not depend on it.

## 2.2 Privacy

Loom must have:

* No telemetry by default.
* No advertising.
* No hidden network requests.
* No user profiling.
* No remote crash upload.
* No remote model inference.
* No automatic upload of documents or media.
* No mandatory update checks.

Local diagnostic logs must be understandable, redactable, and under user control.

Test the applications in a network-disabled container. A core workflow that fails because the network is unavailable is a release-blocking defect.

## 2.3 AI Is Optional

All conventional editing features must remain fully usable without an AI model.

AI and computer vision must enhance tools rather than replace the underlying tools.

Examples:

* Loom Photo must include ordinary manual masks even when semantic segmentation is unavailable.
* Loom Video must support manual subtitles even when transcription is unavailable.
* Loom Writer must support conventional document editing without a language model.
* Loom Sheets must calculate formulas without an AI model.
* Loom Motion must support manual keyframing and rotoscoping without tracking models.

## 2.4 User Ownership

Use documented, versioned file formats.

Users must be able to:

* Keep files permanently.
* Back them up using ordinary filesystem tools.
* Inspect package contents.
* Export to common formats.
* Move projects between computers.
* Recover data from partially damaged packages where practical.

## 2.5 Performance

Loom must feel responsive under normal professional workloads.

Long-running work must be asynchronous, cancellable, observable, and recoverable.

Never block the UI thread with:

* Media decoding.
* File parsing.
* Model inference.
* Autosave.
* Export.
* Thumbnail generation.
* Waveform generation.
* Proxy generation.
* Font scanning.
* Plugin discovery.
* Search indexing.

## 2.6 Accessibility

Accessibility is a release requirement, not a future enhancement.

Every application must support:

* Complete keyboard navigation.
* Visible keyboard focus.
* Screen-reader labels.
* Logical focus order.
* High-contrast operation.
* Scalable UI.
* Reduced-motion mode.
* Non-color status indicators.
* Configurable shortcuts.
* Accessible error reporting.
* Accessible canvas and timeline navigation where technically possible.

---

# 3. Product Family

Create the following applications.

## 3.1 Loom Writer

A professional word processor and page-layout application.

Primary capabilities:

* Rich text editing.
* Continuous and paginated document modes.
* Advanced paragraph and character styles.
* Page styles.
* Master pages.
* Sections.
* Columns.
* Tables.
* Lists.
* Footnotes and endnotes.
* Headers and footers.
* Automatic tables of contents.
* Citations and bibliographies.
* Cross-references.
* Change tracking.
* Comments.
* Structured document navigation.
* Shapes and media.
* Text wrapping.
* Anchored and floating objects.
* Templates.
* Form fields.
* Mail merge.
* Print layout.
* PDF export.
* EPUB export.
* DOCX import and export where feasible.
* ODT import and export.
* Markdown import and export.
* Plain-text import and export.
* OCR-assisted scanned-document import.
* Local document search.
* Crash recovery and version snapshots.

## 3.2 Loom Sheets

A professional spreadsheet and data-analysis application.

Primary capabilities:

* Large virtualized grid.
* Formula engine.
* Dependency graph.
* Incremental recalculation.
* Named ranges.
* Structured tables.
* Sorting and filtering.
* Conditional formatting.
* Validation rules.
* Charts.
* Pivot tables.
* Data grouping.
* Freeze panes.
* Multiple sheets.
* Cell comments.
* Rich cell formatting.
* Dates, times, durations, currencies, and units.
* CSV and TSV import/export.
* XLSX import/export where feasible.
* ODS import/export.
* Local data connectors for files and databases.
* Formula auditing.
* Error tracing.
* Goal seeking.
* Basic statistical and financial functions.
* Photograph-to-table import through Loom Vision.
* Receipt and invoice table extraction.
* Accessible spreadsheet navigation.

Do not execute arbitrary network queries from spreadsheet cells.

## 3.3 Loom Present

A professional presentation authoring application.

Primary capabilities:

* Slide canvas.
* Themes.
* Master slides.
* Layouts.
* Guides.
* Alignment and distribution.
* Smart grouping.
* Text.
* Tables.
* Charts.
* Shapes.
* Images.
* Audio.
* Video.
* Equations.
* Speaker notes.
* Slide transitions.
* Object animations.
* Timeline-based animation editing.
* Presenter display.
* Rehearsal timing.
* Presentation recording.
* PDF export.
* Video export.
* PPTX import/export where feasible.
* ODP import/export.
* Accessible presentation structures.
* Local presenter-background removal.
* Presenter tracking and automatic reframing where supported.

## 3.4 Loom Photo

A professional nondestructive image-editing application.

Primary capabilities:

* Layer-based editing.
* Pixel layers.
* Vector layers.
* Text layers.
* Adjustment layers.
* Fill layers.
* Groups.
* Masks.
* Clipping.
* Blend modes.
* Nondestructive filters.
* RAW development.
* Color management.
* ICC profiles.
* Curves.
* Levels.
* White balance.
* Exposure.
* Selective color.
* Gradients.
* Brushes.
* Clone and healing tools.
* Content-aware repair.
* Crop and perspective correction.
* Transform and warp.
* Liquify.
* Panorama workflows.
* HDR workflows.
* Batch processing.
* PSD import/export where practical.
* OpenRaster support.
* TIFF, PNG, JPEG, WebP, AVIF, EXR, and other appropriate formats.
* Semantic subject selection.
* Background removal.
* Portrait matting.
* Object-aware masks.
* Local inpainting where a compatible local model is installed.
* Local super-resolution where a compatible model is installed.

All AI-assisted selections must remain editable as ordinary masks.

## 3.5 Loom Motion

A motion-graphics, animation, and compositing application.

Primary capabilities:

* Layer-based compositions.
* Timeline.
* Keyframes.
* Curves and graph editor.
* Parenting.
* Constraints.
* Masks.
* Vector shapes.
* Text animation.
* Image sequences.
* Video layers.
* Audio reference tracks.
* Cameras.
* Lights where supported.
* 2.5D scenes.
* Transform hierarchy.
* Blend modes.
* Filters and effects.
* Particle systems.
* Replicators.
* Behaviors.
* Motion paths.
* Chroma keying.
* Rotoscoping.
* Planar tracking.
* Point tracking.
* Object tracking through Loom Vision.
* Stabilization.
* Optical-flow retiming.
* Motion blur.
* Render queue.
* Template export for Loom Video and Loom Present.

## 3.6 Loom Video

A professional nonlinear video editor.

Primary capabilities:

* Media library.
* Event and project organization.
* Metadata.
* Keyword collections.
* Favorites and rejects.
* Proxy and optimized media.
* Background transcoding.
* Timeline editing.
* Connected clips or an independently designed relationship-based editing model.
* Track-oriented compatibility where required.
* Ripple, roll, slip, slide, blade, trim, and overwrite operations.
* Compound clips.
* Nested timelines.
* Multicam.
* Synchronized clips.
* Audio lanes and roles.
* Transitions.
* Titles.
* Generators.
* Effects.
* Keyframes.
* Speed changes.
* Optical-flow retiming.
* Stabilization.
* Color correction.
* Scopes.
* LUT support.
* HDR-aware processing.
* Captions and subtitles.
* Local transcription.
* Scene detection.
* Subject tracking.
* Automatic reframing.
* Background removal where feasible.
* Export through Loom Encode.
* XML/EDL/AAF interchange where feasible and legally appropriate.
* Autosave.
* Project backups.
* Media relinking.
* Offline media workflows.

## 3.7 Loom Studio

A digital audio workstation serving both approachable music creation and professional production.

Provide two progressive-disclosure workspace modes over the same engine:

### Quick Workspace

Designed for new musicians and rapid creation.

Include:

* Loop browser.
* Simplified track creation.
* Software instruments.
* Audio recording.
* MIDI recording.
* Drummer or procedural rhythm tools using original implementations and assets.
* Smart controls.
* Chord and scale assistance.
* Basic effects.
* Easy arrangement.
* Guided mixing.
* Simple export.

### Pro Workspace

Designed for professional production.

Include:

* Multitrack audio.
* MIDI.
* Piano roll.
* Drum editor.
* Score editor where feasible.
* Automation.
* Comping.
* Take folders.
* Flex-time style nondestructive time editing using an original implementation.
* Pitch editing.
* Mixer.
* Buses.
* Sends.
* Sidechains.
* Plugin hosting.
* Instrument hosting.
* Sample editing.
* Looping.
* Markers.
* Tempo maps.
* Meter changes.
* Surround and spatial audio architecture where feasible.
* Mastering tools.
* Loudness metering.
* Local source separation where compatible local models are installed.
* Local speech and noise enhancement.
* Beat, tempo, key, and transient analysis.

Support suitable Linux plugin standards, with an architecture capable of adding additional platform-specific standards later. Sandboxing is mandatory for third-party plugins where practical.

## 3.8 Loom Encode

A professional media transcoding and delivery application.

Primary capabilities:

* Batch queue.
* Source inspection.
* Presets.
* Custom encoding settings.
* Video filters.
* Audio channel mapping.
* Subtitle handling.
* Frame-rate conversion.
* Scaling.
* Cropping.
* Color-space conversion.
* HDR metadata handling.
* Image sequences.
* Audio-only output.
* Multi-destination jobs.
* Job dependencies.
* Retry and recovery.
* Pause and resume.
* Hardware-accelerated encoding when available.
* Software fallback.
* Watch folders as an optional local feature.
* CLI operation.
* Integration with Loom Video, Motion, Present, Photo, and Studio.
* Deterministic preset files.
* Content-aware analysis performed locally.
* Perceptual quality metrics and comparison tools.

---

# 4. Loom Vision

Create a shared platform named **Loom Vision**.

Loom Vision is an original, local-first computer-vision and perception framework for Loom applications and third-party plugins.

It must not be hard-wired to a single model, vendor, runtime, or hardware backend.

## 4.1 Required Capability Areas

Document and text:

* Printed-text OCR.
* Layout-aware OCR.
* Handwriting-recognition provider interface.
* Document boundary detection.
* Perspective correction.
* Dewarping architecture.
* Table detection.
* Table-structure extraction.
* Form-field detection.
* Barcode recognition.
* QR-code recognition.
* Mathematical-expression recognition provider interface.
* PDF page analysis.
* Reading-order detection.
* Language detection.
* Local full-text indexing.

Images:

* Image classification.
* Object detection.
* Semantic segmentation.
* Instance segmentation.
* Salient-object segmentation.
* Portrait segmentation.
* Alpha matting.
* Face detection.
* Face landmarks.
* Face quality and orientation analysis.
* Body-pose estimation.
* Hand-pose estimation.
* Depth-estimation provider interface.
* Image embeddings.
* Similar-image search.
* Duplicate detection.
* Local reverse-image search across user-selected libraries.
* Scene classification.
* Image captioning provider interface.
* Super-resolution provider interface.
* Denoising provider interface.
* Inpainting provider interface.

Video:

* Object tracking.
* Point tracking.
* Planar tracking.
* Optical flow.
* Shot-boundary detection.
* Scene grouping.
* Subject tracking.
* Camera-motion estimation.
* Stabilization analysis.
* Automatic reframing.
* Subtitle timing.
* Local speech transcription.
* Speaker diarization provider interface.
* Thumbnail selection.
* Video embeddings.
* Local semantic search.

Audio perception:

* Speech recognition.
* Voice activity detection.
* Noise classification.
* Beat detection.
* Tempo estimation.
* Key estimation.
* Transient detection.
* Source-separation provider interface.
* Speech-enhancement provider interface.
* Audio embeddings.

## 4.2 Provider Architecture

Define stable capability traits rather than exposing model-specific APIs.

Examples:

* `OcrProvider`
* `DocumentAnalysisProvider`
* `ObjectDetectionProvider`
* `SegmentationProvider`
* `MattingProvider`
* `PoseProvider`
* `EmbeddingProvider`
* `TrackingProvider`
* `OpticalFlowProvider`
* `SpeechRecognitionProvider`
* `AudioAnalysisProvider`
* `ImageGenerationProvider`
* `InpaintingProvider`
* `SuperResolutionProvider`

Each provider must declare:

* Capability identifier.
* Input types.
* Output schema.
* Supported media formats.
* Supported languages.
* Required memory.
* Estimated latency.
* Hardware backends.
* License metadata.
* Model provenance.
* Determinism characteristics.
* Batch support.
* Streaming support.
* Cancellation support.
* Progress support.

## 4.3 Local Model Packs

Do not require online model downloads.

Implement a local model-pack format that users can install from a file.

Each model pack must contain:

* Manifest.
* Model files.
* Checksums.
* License information.
* Provenance.
* Capability declarations.
* Runtime requirements.
* Preprocessing definition.
* Postprocessing definition.
* Test vectors.
* Optional sample inputs.
* Version and compatibility range.

Do not bundle models whose licenses do not allow redistribution.

The application may support importing model packs from disk. A remote marketplace or account system is out of scope.

## 4.4 Hardware Backends

Design provider support for:

* CPU.
* Vulkan compute.
* ONNX Runtime.
* Candle.
* CUDA where optional and license-compatible.
* TensorRT as an optional external backend.
* ROCm as an optional backend.
* OpenVINO as an optional backend.
* DirectML in future Windows builds.
* Core ML in future macOS builds.

Every capability must either have a CPU fallback or clearly report that no compatible provider is installed.

Never silently transmit a request to a remote service.

---

# 5. Technology Direction

Use:

* Rust stable.
* Slint for the application UI.
* `wgpu` for custom GPU rendering and compute where appropriate.
* Vulkan as the primary Linux graphics target.
* A documented CPU rendering or software-rendering path for deterministic tests.
* Tokio or another justified async runtime for background orchestration.
* Structured concurrency and cancellation.
* A versioned message or command architecture between UI and engines.
* Cargo workspaces inside each repository.
* Semantic versioning for shared crates.
* Reproducible dependency lockfiles.
* Explicit feature flags for optional backends.

Do not use:

* Qt.
* GTK as an application UI toolkit.
* Electron.
* A browser UI as the primary desktop interface.
* Java.
* A required web server.
* Mandatory cloud APIs.
* Proprietary dependencies that prevent compliant redistribution.

A non-UI system library may be used where justified. For example, FFmpeg or GStreamer may be evaluated for media support, but the selection must include a written licensing, packaging, maintenance, and sandboxing analysis.

Before selecting any dependency:

1. Verify the current official documentation.
2. Verify active maintenance.
3. Verify Linux support.
4. Verify license compatibility.
5. Verify thread-safety and performance characteristics.
6. Record the decision in an ADR.
7. Pin a compatible version.
8. Add a replacement strategy for critical dependencies.

Choose and document a minimum supported Rust version after verifying the current ecosystem requirements.

---

# 6. Repository Architecture

The applications must live in separate repositories, while shared code remains versioned and reusable.

Create the following repository directories inside one generated parent workspace:

```text
loom/
├── loom-bootstrap/
├── loom-spec/
├── loom-design-bible/
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
└── loom-samples/
```

## 6.1 `loom-bootstrap`

The umbrella orchestration repository.

Responsibilities:

* Clone or locate all Loom repositories.
* Pin compatible repository revisions.
* Build the complete suite.
* Run all tests.
* Launch applications.
* Build Docker images.
* Run visual QA.
* Produce packages.
* Produce the final ZIP.
* Generate a suite compatibility manifest.
* Validate shared dependency versions.

Include:

* `README.md`
* `AGENTS.md`
* `BOOTSTRAP.md`
* `COMPATIBILITY.toml`
* `justfile` or equivalent task runner
* build scripts
* test scripts
* release scripts
* Docker Compose configuration where justified
* deterministic archive script

## 6.2 `loom-spec`

The authoritative product and engineering specification.

It defines:

* Product scope.
* Application capabilities.
* Cross-application workflows.
* Shared terminology.
* File format family.
* Release criteria.
* Compatibility policy.
* Roadmap.
* Feature matrices.
* Application-level implementation guides.

It must reference `loom-design-bible`, `loom-core`, `loom-vision`, and `loom-plugin-sdk` rather than duplicating their contracts.

## 6.3 `loom-design-bible`

The authoritative visual, motion, interaction, and accessibility specification.

## 6.4 `loom-core`

Shared platform crates.

Candidate crates include:

* `loom-app-runtime`
* `loom-ui`
* `loom-render`
* `loom-animation`
* `loom-command`
* `loom-history`
* `loom-document`
* `loom-package`
* `loom-storage`
* `loom-autosave`
* `loom-recovery`
* `loom-jobs`
* `loom-media`
* `loom-color`
* `loom-text`
* `loom-fonts`
* `loom-search`
* `loom-settings`
* `loom-accessibility`
* `loom-shortcuts`
* `loom-clipboard`
* `loom-diagnostics`
* `loom-test-support`

Do not create a large undifferentiated “common” crate.

Each shared crate must have a narrow responsibility, explicit API boundary, documentation, tests, and semantic version.

## 6.5 `loom-plugin-sdk`

Defines the sandboxed extension system.

Prefer a WebAssembly/WASI-based plugin model where technically appropriate.

The SDK must support:

* Commands.
* Importers.
* Exporters.
* Effects.
* Generators.
* Inspectors.
* Metadata providers.
* Vision providers.
* Document processors.
* Media processors.
* Controlled filesystem access.
* Declared capabilities.
* Version negotiation.
* Resource limits.
* Crash isolation.
* Plugin signing architecture without requiring a central authority.
* Local plugin-package installation.

## 6.6 Application Repositories

Each application repository must build independently against released or pinned shared crate versions.

Do not use unversioned source copying between repositories.

Do not introduce circular repository dependencies.

---

# 7. Original Design Language

The Loom interface must feel:

* Calm.
* Precise.
* Minimal.
* Warm.
* Professional.
* Fast.
* Predictable.
* Approachable.
* Capable.
* Timeless.

Minimal does not mean featureless.

Professional features must be available through progressive disclosure rather than permanently filling the screen with controls.

## 7.1 Content First

The document, canvas, spreadsheet, timeline, composition, or media is the visual focus.

Application chrome should support the work without dominating it.

Use:

* Restrained toolbars.
* Context-sensitive inspectors.
* Collapsible sidebars.
* Searchable commands.
* Direct manipulation.
* High-quality typography.
* Carefully controlled spacing.
* Subtle depth.
* Clear selection.
* Strong focus indication.
* Meaningful motion.

Avoid:

* Dense permanent ribbons.
* Excessive borders.
* Unnecessary gradients.
* Decorative animation.
* Deeply nested modal dialogs.
* Tiny unlabeled icons.
* Hover-only functionality.
* Hidden destructive behavior.
* Excessive floating windows.
* Feature duplication across multiple unrelated surfaces.

## 7.2 Progressive Disclosure

A new user should be able to start useful work immediately.

An expert must still be able to reach advanced functionality quickly.

Use layered disclosure:

1. Direct manipulation.
2. Context toolbar.
3. Context inspector.
4. Menus and command palette.
5. Advanced workspace or panel.
6. Scripting or plugin extension where appropriate.

## 7.3 Inspector Model

Develop a consistent contextual inspector shared across the suite.

The inspector must:

* Update smoothly when the selection changes.
* Show only applicable properties.
* Preserve discoverability.
* Support search.
* Support keyboard operation.
* Distinguish object, style, document, metadata, and advanced sections.
* Allow frequently used controls to be pinned where appropriate.
* Avoid spawning many transient property dialogs.

## 7.4 Motion

Animation communicates:

* Origin.
* Destination.
* State change.
* Hierarchy.
* Continuity.
* Completion.
* Cancellation.
* Error.
* Relationship.

Every animation must answer a usability question.

Define:

* Duration tokens.
* Easing tokens.
* Spring models.
* Enter and exit transitions.
* Panel transitions.
* Selection transitions.
* Zoom behavior.
* Drag feedback.
* Reordering feedback.
* Progress transitions.
* Reduced-motion behavior.
* Interruption rules.
* Frame-dropping behavior.

Animations must be interruptible and input-responsive.

Never delay user input merely to finish an animation.

## 7.5 Typography

Typography is a defining capability.

Implement and document:

* Unicode text.
* Bidirectional text.
* Script shaping.
* Font fallback.
* Variable fonts.
* OpenType features.
* Ligatures.
* Kerning.
* Baseline alignment.
* Optical sizing.
* Hyphenation.
* Justification.
* Line breaking.
* Language-aware behavior.
* Paragraph styles.
* Character styles.
* Baseline grids.
* Vertical text architecture where feasible.
* High-DPI rendering.
* Deterministic layout.

Evaluate modern Rust text libraries and shaping engines. Record the selected architecture in an RFC.

## 7.6 Original Assets

Create an original Loom icon family.

Do not use proprietary system symbol sets or trace commercial icons.

Icons must:

* Share consistent stroke, geometry, and optical weight.
* Remain legible at small sizes.
* Have accessible labels.
* Support light and dark themes.
* Be provided as editable source assets.
* Include snapshot tests.

Create original:

* App icons.
* Toolbar icons.
* Empty-state illustrations.
* Template previews.
* Sample documents.
* Sample media.
* Sounds.
* Instrument presets where included.

Only use assets created within the project or under verified compatible licenses.

---

# 8. Loom Design Bible Deliverables

The `loom-design-bible` repository must include at least:

```text
README.md
AGENTS.md
DESIGN_BIBLE.md
DESIGN_PRINCIPLES.md
DESIGN_TOKENS.md
TYPOGRAPHY.md
COLOR.md
ICONOGRAPHY.md
LAYOUT.md
SPACING.md
MOTION.md
COMPONENTS.md
WINDOWS.md
TOOLBARS.md
SIDEBARS.md
INSPECTORS.md
MENUS.md
COMMAND_PALETTE.md
DIALOGS.md
NOTIFICATIONS.md
SELECTION.md
CANVAS.md
TIMELINE.md
SPREADSHEET.md
DOCUMENT_EDITOR.md
DRAG_AND_DROP.md
KEYBOARD.md
POINTER_AND_PEN.md
ACCESSIBILITY.md
THEMING.md
VISUAL_QA.md
PERFORMANCE.md
ANTI_PATTERNS.md
DESIGN_REVIEW.md
UX_ACCEPTANCE_CHECKLIST.md
```

Also create:

* Machine-readable design tokens.
* Slint component examples.
* Original component gallery application.
* Screenshot baselines.
* Interaction-state matrices.
* Accessibility annotations.
* Motion demonstrations.
* Light and dark themes.
* High-contrast theme.
* Reduced-motion theme.
* RTL test screens.
* Large-font test screens.
* Localization stress-test screens.

The component gallery must be runnable and tested in Docker.

---

# 9. File Format Family

Design documented, versioned Loom package formats.

Suggested extensions:

* `.loomdoc`
* `.loomtable`
* `.loomdeck`
* `.loomphoto`
* `.loommotion`
* `.loomvideo`
* `.loomstudio`
* `.loomencode`

Use an inspectable package container such as ZIP where appropriate.

A package should contain:

```text
manifest.json
content/
assets/
previews/
metadata/
history/
recovery/
```

Requirements:

* Versioned schema.
* Stable identifiers.
* Deterministic serialization.
* Forward-compatibility strategy.
* Backward migration.
* Incremental save architecture where appropriate.
* External asset references.
* Embedded asset support.
* Checksums.
* Corruption detection.
* Partial recovery.
* Preview images.
* Human-readable metadata.
* Clear MIME types.
* Security limits against archive bombs and malformed content.
* Fuzz testing.
* Schema fixtures.
* Golden compatibility corpus.

Large media projects may use package directories or reference external media rather than embedding everything.

Document each application’s storage trade-offs.

---

# 10. Shared Platform Requirements

## 10.1 Command System

Every user action must map to a command identifier when practical.

Commands must support:

* Menus.
* Shortcuts.
* Command palette.
* Context menus.
* Accessibility invocation.
* Scripting or plugins where permitted.
* Enablement state.
* Checked state.
* Undo description.
* Localization.

## 10.2 Undo and Redo

Build a transactional history framework.

Requirements:

* Atomic edits.
* Compound operations.
* Coalescing.
* Named undo actions.
* Memory budgets.
* Disk-backed history for large media projects where needed.
* Branch handling policy.
* Autosave interaction.
* Crash recovery interaction.
* Plugin operation integration.
* Deterministic replay tests.

## 10.3 Jobs

Use a shared asynchronous job framework.

Every long task must support:

* Progress.
* Cancellation.
* Pausing where possible.
* Priority.
* Dependencies.
* Resource estimates.
* Error reporting.
* Retry where safe.
* Cleanup.
* Persistence where appropriate.
* Crash recovery.

## 10.4 Autosave and Recovery

Implement:

* Transactional writes.
* Temporary-file safety.
* Periodic autosave.
* Operation-journal architecture.
* Recovery after crashes.
* Recovery browser.
* Explicit recovered-document handling.
* Storage limits.
* Privacy-preserving diagnostics.

## 10.5 Search and Indexing

All search remains local.

Support:

* Document content.
* Project metadata.
* Media metadata.
* OCR text.
* Transcript text.
* Tags.
* Comments.
* Command search.
* Help search.
* Optional semantic search using local embeddings.

Index only user-approved directories or explicitly opened projects.

## 10.6 Color Management

Build a professional color pipeline.

Address:

* ICC profiles.
* Display profiles.
* Working spaces.
* Conversion.
* Linear-light processing.
* HDR.
* Wide color.
* Rendering intents.
* Soft proofing.
* Image, video, and print differences.
* GPU precision.
* Deterministic reference tests.

## 10.7 Media Framework

Define shared abstractions for:

* Demuxing.
* Decoding.
* Encoding.
* Frames.
* Audio buffers.
* Time bases.
* Color metadata.
* Hardware acceleration.
* Caches.
* Proxies.
* Thumbnails.
* Waveforms.
* Cancellation.
* Sandboxing.
* Corrupt media handling.

---

# 11. Documentation Requirements

Documentation is part of the product.

Every repository must include:

```text
README.md
AGENTS.md
ARCHITECTURE.md
IMPLEMENTATION_GUIDE.md
BUILDING.md
TESTING.md
VISUAL_QA.md
PERFORMANCE.md
SECURITY.md
ACCESSIBILITY.md
ROADMAP.md
TASKS.md
CONTRIBUTING.md
LICENSE_POLICY.md
DEPENDENCIES.md
CHANGELOG.md
docs/rfcs/
docs/adrs/
```

Application repositories must additionally include:

```text
PRODUCT_SPEC.md
UX_SPEC.md
WORKFLOWS.md
DOCUMENT_MODEL.md
FILE_FORMAT.md
RENDERING.md
IMPORT_EXPORT.md
PLUGIN_INTEGRATION.md
UNDO_REDO.md
KEYBOARD_SHORTCUTS.md
ERROR_MODEL.md
RECOVERY.md
FEATURE_MATRIX.md
```

## 11.1 Writing for Less-Capable Agents

Assume future implementation tasks may be assigned to less-capable coding agents.

Do not rely on implication.

Each feature specification must define:

* Purpose.
* User story.
* Non-goals.
* Preconditions.
* Inputs.
* Outputs.
* State model.
* Data structures.
* Interfaces.
* Error behavior.
* Threading model.
* Persistence behavior.
* Undo behavior.
* Accessibility behavior.
* Security considerations.
* Performance budget.
* Unit tests.
* Integration tests.
* Visual tests.
* Failure cases.
* Acceptance criteria.
* Dependencies.
* Files expected to change.
* Example usage.

## 11.2 Task Format

Each implementation task must include:

```text
ID
Title
Owner subsystem
Purpose
Dependencies
Files or modules
Required behavior
Non-goals
Implementation steps
Acceptance tests
Visual QA
Performance budget
Security considerations
Completion evidence
```

Tasks must be small enough for one coding agent to complete and verify independently.

Do not generate thousands of vague tasks such as “Implement editor.”

Use concrete tasks such as:

* Implement immutable paragraph-style value object.
* Implement UTF-16 cursor mapping tests for bidirectional text.
* Implement cancel-safe thumbnail job.
* Implement inspector animation interruption behavior.
* Implement dependency-graph cycle reporting for formulas.

---

# 12. Agent Organization

Use available Sol subagents as an engineering organization.

Suggested roles:

## Architecture

* Chief architect.
* Shared-platform architect.
* File-format architect.
* Security architect.
* Accessibility architect.
* Performance architect.

## Design

* Design-system lead.
* Motion lead.
* Typography lead.
* Interaction lead.
* Visual-QA lead.

## Loom Vision

* Runtime and provider lead.
* OCR and document analysis lead.
* Segmentation and image analysis lead.
* Tracking and video analysis lead.
* Audio perception lead.
* Model-pack and benchmarking lead.

## Applications

* Writer lead.
* Sheets lead.
* Present lead.
* Photo lead.
* Motion lead.
* Video lead.
* Studio lead.
* Encode lead.

## Quality

* Build and CI lead.
* Test-infrastructure lead.
* Fuzzing lead.
* Packaging lead.
* Documentation consistency reviewer.
* Licensing reviewer.

Rules:

1. Each agent receives explicit ownership.
2. Two agents must not make conflicting edits to the same contract simultaneously.
3. Shared interfaces must be proposed through RFCs.
4. Application agents consume shared contracts; they do not redefine them.
5. The coordinator reviews every cross-repository contract.
6. Agents must run tests before reporting completion.
7. Claims of completion require evidence.
8. Documentation-only completion does not satisfy implementation tasks.
9. Unimplemented work must remain visible in the task ledger.
10. The coordinator resolves architecture drift immediately.

---

# 13. RFC and ADR Process

Create numbered RFCs for cross-cutting architecture.

Initial RFC candidates:

* RFC-0001 Repository and versioning strategy.
* RFC-0002 UI and engine separation.
* RFC-0003 Slint integration model.
* RFC-0004 GPU renderer architecture.
* RFC-0005 Text shaping and layout.
* RFC-0006 File package format.
* RFC-0007 Undo and transaction system.
* RFC-0008 Async job framework.
* RFC-0009 Plugin ABI and sandboxing.
* RFC-0010 Loom Vision provider model.
* RFC-0011 Model-pack format.
* RFC-0012 Media framework.
* RFC-0013 Color management.
* RFC-0014 Accessibility architecture.
* RFC-0015 Visual regression system.
* RFC-0016 Cross-repository compatibility.
* RFC-0017 Application command system.
* RFC-0018 Autosave and recovery.
* RFC-0019 Local search and indexing.
* RFC-0020 Localization.

An RFC must include:

* Context.
* Goals.
* Non-goals.
* Proposed design.
* Alternatives.
* Trade-offs.
* Security.
* Performance.
* Compatibility.
* Migration.
* Testing.
* Open questions.
* Final status.

Use ADRs for smaller implementation decisions.

Do not silently change accepted contracts.

---

# 14. Testing Strategy

## 14.1 Unit Tests

Require unit tests for:

* Parsers.
* Serializers.
* Formula evaluation.
* Text layout.
* Time calculations.
* Coordinate transforms.
* Undo operations.
* Migration.
* Model-pack validation.
* Color conversion.
* Plugin permissions.
* Command state.
* Caches.
* Job cancellation.

## 14.2 Property Tests

Use property-based tests for:

* Serialization round trips.
* Undo/redo invariants.
* Spreadsheet formulas.
* Timeline edits.
* Transform composition.
* Package migration.
* Unicode cursor movement.
* Range operations.
* Media timestamp conversion.

## 14.3 Fuzzing

Create fuzz targets for:

* Loom package readers.
* Importers.
* Media metadata parsers.
* Subtitle parsers.
* Formula parser.
* Rich-text parser.
* Plugin manifest parser.
* Model-pack manifest parser.
* Clipboard input.
* Project recovery journals.

## 14.4 Integration Tests

Test complete workflows:

* Create, edit, save, close, reopen.
* Crash during save and recover.
* Import and export.
* Undo and redo.
* Copy and paste.
* Drag and drop.
* Plugin installation and removal.
* Model-pack installation.
* Offline startup.
* Offline editing.
* Offline export.
* Low-disk handling.
* Missing media.
* Missing fonts.
* Corrupt files.
* Cancelled background work.

## 14.5 End-to-End UI Tests

Automate:

* Menus.
* Keyboard shortcuts.
* Inspector changes.
* Canvas editing.
* Timeline editing.
* Spreadsheet navigation.
* Accessibility focus.
* Dialog interaction.
* Export.
* Recovery.
* Theme changes.
* Reduced motion.
* Localization layouts.

## 14.6 Visual Regression

Create deterministic golden-image testing.

Capture:

* Component states.
* Full application windows.
* Empty states.
* Menus.
* Dialogs.
* Error states.
* Selection.
* Hover.
* Focus.
* Drag and drop.
* Zoom levels.
* Dark theme.
* Light theme.
* High contrast.
* Large fonts.
* RTL layouts.
* Reduced motion final states.

Use perceptual diffing with documented tolerances.

Store:

* Baseline.
* Actual.
* Diff.
* Metadata.
* Font configuration.
* Renderer information.
* Commit identifier.

Do not automatically approve new baselines.

## 14.7 Performance Tests

Measure:

* Cold startup.
* Warm startup.
* Window creation.
* Document opening.
* Large-document scrolling.
* Spreadsheet recalculation.
* Timeline scrubbing.
* Waveform generation.
* Thumbnail generation.
* Model inference.
* Export.
* Save and autosave.
* Undo.
* Search indexing.
* Memory use.
* GPU memory.
* Background-task interference.

Create representative benchmark projects and budgets.

---

# 15. Docker Ubuntu Visual-QA Environment

Create a Docker-based Ubuntu environment for builds and self-visual QA.

The environment must support:

* Stable pinned Ubuntu base.
* Rust toolchain.
* Slint dependencies.
* Vulkan software renderer.
* Optional GPU passthrough.
* Deterministic fonts.
* Locale configuration.
* Xvfb or a headless Wayland compositor.
* Screenshot capture.
* Input automation.
* Audio test devices or virtual sinks where needed.
* Sample project generation.
* Network-disabled test mode.
* Artifact collection.

Provide at least:

```text
docker/Dockerfile.dev
docker/Dockerfile.ci
docker/Dockerfile.visual
docker/compose.yaml
scripts/docker-build.sh
scripts/docker-test.sh
scripts/docker-visual-qa.sh
scripts/docker-offline-test.sh
```

Visual QA must not consist solely of checking whether a window launched.

For each application:

1. Open representative sample projects.
2. Exercise key workflows.
3. Capture screenshots.
4. Compare with baselines.
5. Inspect severe diffs.
6. Fix rendering or layout defects.
7. Repeat until visual gates pass.

When GPU access is unavailable, use a deterministic software Vulkan implementation for baseline tests.

Create an optional hardware test path for NVIDIA, AMD, and Intel GPUs.

---

# 16. Performance Budgets

Define hardware tiers instead of making universal timing promises.

Create at least:

* Baseline integrated-GPU system.
* Mainstream desktop system.
* High-performance workstation.
* CPU-only compatibility environment.

Initial targets for mainstream hardware:

* Input feedback visible within one display frame where practical.
* UI animations capable of smooth 60 FPS.
* 120 Hz-friendly interaction architecture.
* No synchronous file or media operations on the UI thread.
* Warm application launch target below one second for lightweight apps.
* Cold launch target below two seconds where feasible.
* Scrolling must not allocate excessively per frame.
* Background tasks must yield to direct manipulation.
* Memory use must remain bounded through documented cache policies.
* Cancellation feedback must appear immediately.
* Autosave must not visibly interrupt editing.

Application teams must replace broad targets with workload-specific benchmark budgets.

Performance regressions beyond configured thresholds must fail CI or require a reviewed waiver.

---

# 17. Security Requirements

Implement:

* Safe temporary-file handling.
* Symlink and path traversal protection.
* Archive extraction limits.
* Media parser isolation where practical.
* Plugin sandboxing.
* Plugin capability permissions.
* Model checksum verification.
* Model provenance display.
* Dependency audit.
* Supply-chain policy.
* Lockfiles.
* Reproducible build strategy.
* Fuzzing.
* Secure crash recovery.
* Sensitive data redaction in logs.
* Clipboard input validation.
* Untrusted font handling strategy.
* Untrusted document handling strategy.
* Malformed project handling.
* Resource exhaustion limits.

Avoid `unsafe` Rust unless required for FFI or measurable performance.

Every unsafe block must have:

* A safety comment.
* Tests.
* Encapsulation.
* A justification in the relevant module.
* Miri or equivalent checking where applicable.

---

# 18. Licensing Policy

Prepare all original Loom code for permissive dual licensing under:

* MIT
* Apache-2.0

Before finalizing this choice, verify all direct and transitive dependencies.

Create:

* Dependency inventory.
* License report.
* Notice files.
* Asset-license report.
* Model-license policy.
* Codec patent and redistribution notes.
* Optional-feature license matrix.

Do not introduce a dependency that forces an incompatible license across the entire suite without explicit documentation and isolation.

Where system media frameworks or optional codecs have licensing implications, isolate them behind features and document package variants.

Do not claim that the suite is legally safe merely because it compiles. Produce a factual dependency and license audit.

---

# 19. Import, Export, and Interoperability

Use clean-room, standards-based implementations.

Prioritize documented open standards.

Candidate support:

Writer:

* PDF.
* ODT.
* DOCX where feasible.
* EPUB.
* Markdown.
* HTML.
* Plain text.
* RTF where justified.

Sheets:

* CSV.
* TSV.
* ODS.
* XLSX where feasible.
* PDF.
* JSON import.
* Local SQL sources.

Present:

* PDF.
* ODP.
* PPTX where feasible.
* Image sequence.
* Video.

Photo:

* PNG.
* JPEG.
* TIFF.
* WebP.
* AVIF.
* OpenRaster.
* EXR.
* PSD where feasible.
* Common RAW formats through a justified backend.

Motion and Video:

* Common image, audio, and video formats through the chosen media backend.
* Image sequences.
* Subtitle formats.
* EDL/XML interchange where documented and feasible.

Studio:

* WAV.
* AIFF where appropriate.
* FLAC.
* Ogg formats where appropriate.
* MIDI.
* Standard project interchange where feasible.

Encode:

* Container and codec support determined by backend and licensing configuration.

Create compatibility fixtures and round-trip tests.

Never promise perfect fidelity where the source format contains unsupported proprietary behavior. Produce import reports that explain substitutions or losses.

---

# 20. Localization

Architect for localization from the beginning.

Requirements:

* No hard-coded user-facing strings.
* Message catalogs.
* Pluralization.
* Parameterized messages.
* Date, time, number, and currency formatting.
* RTL support.
* Bidirectional text.
* Input-method support.
* Shortcut localization policy.
* Layout expansion testing.
* Pseudolocalization.
* Font fallback.
* Language-aware line breaking and hyphenation.

Ship at least a pseudolocale and automated layout-stress tests.

---

# 21. Sample Content

Create original sample projects for every application.

Examples:

* Writer report, brochure, résumé, and long-form document.
* Sheets budget, dataset, dashboard, and scientific workbook.
* Present product presentation and educational deck.
* Photo multilayer composition and RAW-style sample.
* Motion title sequence and tracked composition.
* Video short documentary edit and multicam sample.
* Studio song project in Quick and Pro workspaces.
* Encode batch and delivery preset examples.
* Vision OCR, segmentation, tracking, and search examples.

All sample content must be original or generated from verified permissive assets.

Do not use copyrighted commercial sample material.

---

# 22. Implementation Phases

The coordinating agent must execute these phases in order while parallelizing independent work inside a phase.

## Phase 0 — Workspace Bootstrap

* Create repository directories.
* Initialize Git repositories.
* Create root orchestration.
* Add license policy.
* Add shared terminology.
* Add task ledger.
* Add decision system.
* Add compatibility manifest.
* Add initial CI.
* Verify toolchain.

## Phase 1 — Foundation Specifications

Complete and review:

* Product specification.
* Design Bible.
* Core architecture.
* Vision architecture.
* Plugin SDK architecture.
* File-format architecture.
* Security model.
* Accessibility model.
* Testing architecture.
* Docker visual-QA architecture.

Freeze initial cross-repository contracts before widespread application implementation.

## Phase 2 — Shared Platform

Implement:

* App runtime.
* Commands.
* Jobs.
* Settings.
* Logging.
* Storage.
* Package format.
* Autosave.
* Recovery.
* Undo.
* Rendering foundation.
* Text foundation.
* Color foundation.
* Plugin host foundation.
* Visual test harness.
* Component gallery.

## Phase 3 — Loom Vision Foundation

Implement:

* Capability traits.
* Provider registry.
* Model-pack loader.
* Manifest validation.
* CPU reference provider examples.
* ONNX provider integration where justified.
* Image and tensor interchange.
* Batch jobs.
* Cancellation.
* Benchmark harness.
* Sample OCR or detection vertical slice using a redistributable or user-installed model.

Do not hardcode Loom applications directly into Loom Vision.

## Phase 4 — Application Vertical Slices

For each application, implement one complete workflow before broad feature expansion.

Examples:

Writer:

* Create document.
* Type styled text.
* Insert image.
* Save.
* Reopen.
* Export PDF.
* Undo.
* Recover after simulated crash.

Sheets:

* Enter data.
* Evaluate formulas.
* Format cells.
* Create chart.
* Save.
* Reopen.
* Export CSV/PDF.

Present:

* Create slides.
* Add content.
* Apply theme.
* Add transition.
* Present.
* Export PDF/video where available.

Photo:

* Open image.
* Add adjustment layer.
* Add mask.
* Paint.
* Save nondestructive project.
* Export PNG/JPEG.

Motion:

* Create composition.
* Add layers.
* Animate keyframes.
* Preview.
* Export frames/video.

Video:

* Import media.
* Create timeline.
* Trim clips.
* Add title.
* Add transition.
* Export through Encode.

Studio:

* Create tracks.
* Record or import audio.
* Add MIDI notes.
* Mix.
* Save.
* Export audio.

Encode:

* Add source.
* Select preset.
* Run job.
* Pause or cancel.
* Inspect output.

Each vertical slice must include UI, engine, persistence, tests, accessibility, and visual QA.

## Phase 5 — Professional Feature Expansion

Expand application capability according to feature matrices and prioritized milestones.

Do not sacrifice architectural integrity merely to increase checkbox counts.

## Phase 6 — Integration

Implement:

* Shared clipboard formats.
* Cross-application drag and drop.
* Linked assets.
* Motion templates in Video and Present.
* Encode integration.
* Vision integration.
* Shared color and font behavior.
* Shared commands and shortcuts.
* Shared component library.
* Consistent file dialogs and recovery.

## Phase 7 — Hardening

Run:

* Full test suite.
* Fuzzing.
* Dependency audit.
* License audit.
* Offline tests.
* Corruption tests.
* Crash recovery tests.
* Memory tests.
* Performance tests.
* Visual regression.
* Accessibility audits.
* Packaging tests.

## Phase 8 — Release Packaging

Produce:

* Development builds.
* Debug-symbol packages.
* Linux release binaries.
* AppImage where practical.
* Flatpak manifests where practical.
* Distribution-neutral archives.
* Checksums.
* SBOM.
* License notices.
* Build provenance.
* Sample projects.
* Documentation.
* Source archives.

---

# 23. Quality Gates

A repository or feature is not complete merely because files exist.

The final quality process must run:

```text
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --workspace --all-features
documentation link checks
schema validation
visual regression tests
offline integration tests
license audit
dependency audit
package smoke tests
```

Use targeted commands when a full `--all-features` build is incompatible with optional system backends, and document the supported test matrix.

Release-blocking conditions include:

* Compilation failure.
* Fake or empty tests.
* Critical path placeholders.
* Network dependency in a core workflow.
* UI-thread blocking in a known critical path.
* Unrecoverable save corruption.
* Undocumented unsafe code.
* Missing license information.
* Severe accessibility regression.
* Severe visual regression.
* Claimed feature without working implementation.
* Unbounded memory behavior in representative workloads.

---

# 24. Honesty and Completion Reporting

Never mark an item complete unless its acceptance evidence exists.

Classify implementation state as:

* `COMPLETE`
* `FUNCTIONAL_WITH_LIMITATIONS`
* `EXPERIMENTAL`
* `SCAFFOLDED`
* `NOT_STARTED`
* `BLOCKED`

Do not use “complete” for:

* Empty UI screens.
* Hard-coded demonstrations.
* Interfaces without implementations.
* Tests that only assert `true`.
* Exporters that write placeholder files.
* Fake AI outputs.
* Screenshots that do not come from the real application.
* Documentation describing code that does not exist.

Create:

* `BUILD_STATUS.md`
* `FEATURE_STATUS.md`
* `KNOWN_LIMITATIONS.md`
* `UNIMPLEMENTED.md`
* `VERIFICATION_REPORT.md`

The final report must distinguish clearly between architecture, prototype, functional implementation, and production readiness.

---

# 25. Required Final Directory Documentation

At the generated parent root, create:

```text
LOOM_MASTER_INDEX.md
REPOSITORY_MAP.md
BUILD_ALL.md
RUN_ALL.md
TEST_ALL.md
VISUAL_QA_ALL.md
LICENSE_REPORT.md
DEPENDENCY_REPORT.md
SECURITY_REPORT.md
ACCESSIBILITY_REPORT.md
PERFORMANCE_REPORT.md
VERIFICATION_REPORT.md
FEATURE_STATUS.md
KNOWN_LIMITATIONS.md
```

`LOOM_MASTER_INDEX.md` must link to every important document and repository.

`REPOSITORY_MAP.md` must explain ownership and dependency direction.

---

# 26. Final ZIP Delivery

After generating and verifying the project:

1. Remove build caches and unnecessary temporary files.
2. Preserve source, documentation, tests, fixtures, sample projects, lockfiles, Docker files, and release scripts.
3. Verify that no secrets, credentials, absolute personal paths, or model files with incompatible licenses are included.
4. Run the archive verification script.
5. Create:

```text
Loom-Complete.zip
```

The ZIP must contain the parent `loom/` directory with every repository.

Also generate:

```text
Loom-Complete.zip.sha256
Loom-Manifest.json
Loom-Verification-Report.md
```

The manifest must include:

* Repository names.
* Commit hashes.
* Version identifiers.
* Build status.
* Test status.
* Included binaries.
* Included documents.
* Known limitations.
* Checksums.

Verify the ZIP by extracting it into a clean temporary directory and running the documented bootstrap validation from the extracted copy.

Do not merely create an archive without testing extraction.

---

# 27. Final Response Requirements

When the autonomous run is finished, provide:

1. The exact path to `Loom-Complete.zip`.
2. The exact path to the checksum file.
3. A brief repository summary.
4. Build and run commands.
5. Test results.
6. Visual-QA results.
7. Offline-test results.
8. Honest implementation status for each application.
9. Important known limitations.
10. The recommended next engineering milestone.

Do not respond with only a plan.

Do not ask whether you should begin.

Begin immediately.

---

# 28. First Actions

Execute these actions now:

1. Inspect the available environment and tools.
2. Create the parent `loom/` workspace.
3. Initialize the orchestration and specification repositories.
4. Create the task ledger and agent ownership map.
5. Delegate foundation documents and RFC research to separate Sol subagents.
6. Produce and review the Design Bible, core architecture, Loom Vision architecture, file-format architecture, and testing architecture.
7. Resolve cross-document conflicts.
8. Begin implementing the shared platform and application vertical slices.
9. Build and test continuously rather than postponing integration.
10. Continue through packaging and verified ZIP creation.

Start execution now.
