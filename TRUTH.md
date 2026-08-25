# Loom — Current Truth

This is the repository's human-maintained readiness record. `AGENTS.MD` defines the intended product. This file states what a user can actually rely on today.

CI artifacts can prove that a build, test, package, screenshot, or scripted journey passed. They do not convert an engine function, source token, screenshot, or partial file writer into a finished product feature.

## Product status

Loom is a local-first Rust/Slint creator-suite **functional alpha** containing eight desktop applications, substantial application-core engines, shared infrastructure, native project formats, and cross-platform validation machinery.

It is not yet a replacement for mature professional office, image, motion, video, audio, or delivery software. No application currently satisfies all requirements assigned to it in `AGENTS.MD`.

Under the strict product rubric below, current complete-suite readiness is approximately **28/100**.

That score intentionally falls when previously credited engine work is not reachable through a professional GUI, when a workflow cannot be completed end to end, or when the shipping interaction/visual quality is not production grade. The score is not an additive count of functions, tests, structs, file parsers, or command-line capabilities.

## Strict product rubric

A capability receives full product credit only when all of these are true:

1. the engine behavior exists and is tested;
2. the normal desktop GUI exposes it through a coherent workflow;
3. editing behavior is direct, selection-aware, undoable where applicable, and persistent;
4. failure/cancellation/error states are truthful and recoverable;
5. realistic user content can complete an acceptance task end to end;
6. the UI passes the shared mechanical design contract without overlap, clipping, placeholder state, or inaccessible controls;
7. supported import/export claims have evidence proportional to the fidelity claimed;
8. the workflow passes on the platforms for which readiness is claimed.

Core-only functionality earns foundation credit, not parity credit.

## Current scorecard

| Dimension | Current | What the score means |
|---|---:|---|
| Core/backend engineering | 65/100 | Many useful data models, algorithms, persistence primitives, parsers, exporters, and media helpers exist. |
| Architecture & persistence | 70/100 | Shared Rust infrastructure, local packages, history/recovery, atomic persistence, jobs, and native-dialog foundations are meaningful strengths. |
| Functionality reachable through GUI | 30/100 | Too much engine depth remains disconnected from normal professional workflows. |
| Interaction design | 20/100 | Selection, contextual editing, responsive toolbar behavior, direct manipulation, and editor-specific interaction models are incomplete. |
| Visual design & polish | 18/100 | Current application screenshots contain clipping, weak hierarchy, inconsistent density, dead space, oversized/overlapping chrome, and placeholder-like composition. |
| Professional workflow depth | 25/100 | Each application has useful slices, but none yet provides the complete daily workflow expected of its mature category. |
| **Overall product readiness** | **28/100** | Functional alpha with substantial foundations; not professional-suite parity. |

The overall score is deliberately not the arithmetic mean. User-visible product capability and workflow completion dominate the final readiness judgment.

## Shared UI/productization reset

The current priority is the shared Loom UI toolkit and application-shell migration rather than accumulating more isolated backend features.

The normative design sources are the machine-readable token/desktop contracts plus the shared toolkit implementation. The suite is being moved to one geometry, typography, responsive, accessibility, toolbar, panel, canvas, and timeline system before broad feature expansion resumes.

A visual regression baseline is valid only after the represented design is approved. A screenshot does not become acceptable merely because it is deterministic or because a new baseline was committed.

## Current application boundaries

### Writer

Implemented foundations include editable document data, persistence/history/recovery, search and statistics helpers, block/formatting algorithms, Markdown/PDF paths, partial DOCX import/export, native file workflows, and a desktop editor shell.

Current product limitation: the visible editor is still far below a professional word processor. Formatting behavior and document structure are not yet uniformly selection/caret driven; rich page layout, floating objects, tables/images in a production editing model, comments/review, headers/footers, forms, high-fidelity interchange, and professional pagination/layout workflows remain incomplete. The dense toolbar has also demonstrated collision defects at supported desktop sizes.

### Sheets

Implemented foundations include workbook/cell models, formulas, CSV workflows, persistence/history, several analysis/formatting helpers, partial XLSX import/export, and native file workflows.

Current product limitation: the shipping UI is still a small fixed-grid editor rather than a scalable spreadsheet workspace. Large-grid virtualization, viewport-filling grid behavior, robust range selection/fill/resize/freeze interactions, rich formatting, chart/pivot authoring, broad formula coverage, and high-fidelity XLSX/ODS workflows remain incomplete.

### Present

Implemented foundations include deck/slide/object models, ordering/alignment helpers, notes, PDF output, partial PPTX import/export, presenter-related engine primitives, history/persistence, and native file workflows.

Current product limitation: object authoring, mixed media, animation authoring, presenter workflows, recording/video export, direct manipulation, and high-fidelity interchange are incomplete. Inspector command labels have demonstrated clipping/overlap at supported sizes.

### Photo

Implemented foundations include raster buffers, layers, masks, many CPU image operations/adjustments, histogram and transform helpers, project persistence/history, raster import/export, and native file workflows.

Current product limitation: the GUI is not yet a professional raster editor. Selection/transform tooling, painting/retouching workflows, layer/inspector interaction, RAW/ICC, healing/warping, HDR/panorama, PSD fidelity, GPU effects, and production AI editing remain incomplete. Current captures have shown truncated layer names and persistent UI competing with the canvas.

### Motion

Implemented foundations include layer/keyframe models, interpolation, transforms, timing/playback helpers, procedural motion utilities, template/render-queue primitives, persistence/history, and a composition editor shell.

Current product limitation: professional scene manipulation, graph/timeline editing, compositing, effects, playback/render workflows, and interchange remain incomplete. The previously blank first frame and persistent transport chip were repaired, but this is a local correctness improvement rather than product completion.

### Video

Implemented foundations include timeline/track/clip models, trim/marker helpers, FFmpeg-backed local processing pieces, captions/audio/media helpers, persistence/history, and a timeline editor shell.

Current product limitation: media-bin/source consistency, scalable timeline interaction, clip trimming/direct manipulation, viewer/source workflows, effects/color/audio workflows, export UX, and professional NLE depth remain incomplete. Existing captures have shown track-header truncation, ruler/control overlap, and inconsistent sample-media state.

### Studio

Implemented foundations include tracks/regions, PCM/WAV support, synthesis/DSP helpers, mixer/automation primitives, persistence/history, local device foundations, and a multitrack shell.

Current product limitation: production recording, low-latency realtime scheduling, editing/comping, time/pitch workflows, plugin hosting/isolation/UI, mixing/mastering depth, and scalable arrangement interaction remain incomplete. Track naming and dense arrangement presentation still require shared-toolkit migration.

### Encode

Implemented foundations include an FFmpeg queue, presets, command planning/execution/progress/cancellation, persistence/recovery, probe/conformance helpers, hardware-codec planning, and batch/destination primitives.

Current product limitation: the queue/settings workflow, job hierarchy, watch-folder experience, hardware policy, pause/resume guarantees, exhaustive format support, and perceptual conformance remain incomplete. Existing UI gives progress percentage excessive visual hierarchy relative to job identity/settings.

## Evidence boundaries

### Build and unit-test evidence

Passing formatting, Clippy, unit, integration, or release-build gates proves the corresponding source revision meets those automated checks. It does not prove professional workflow completeness or visual quality.

### Native package evidence

A native package counts only after independent inspection of artifact provenance, executable architecture, all required application payloads, and document registrations. A package filename or successful packaging command alone is not readiness evidence.

### Keyboard journeys

The current command-palette journey machinery dispatches real key events for typing, filtering, navigation, Return, and Escape. Current Slint public APIs cannot inject the Ctrl/Cmd modifier, so the palette-open step uses the same host hook used by the shortcut. The current journeys also verify only selected command effects.

Therefore these journeys **do not prove complete keyboard-only application operation** or complete command semantics.

### Native UI matrix

Native UI captures can prove that binaries render deterministic nonblank states under selected themes/sizes and can expose selected overlays. They do not prove that the resulting design is good. Baselines containing known overlap, clipping, hierarchy, or workflow defects are invalid product-quality evidence.

### Functional matrix

CLI/native functional journeys prove only the operations they execute and the output invariants they validate. They do not substitute for normal in-application editing journeys.

### Interoperability corpus

The committed minimal Office/ODF/PSD/text fixtures **exercise content-based format detection** and selected partial readers/writers. **They are not round-trip fidelity** evidence for layout, formulas, animation, layered-image semantics, fonts, or professional interchange unless a format-specific conformance test explicitly proves those properties.

## UI acceptance policy

A migrated shared component/application shell is accepted only when:

- geometry comes from the shared contract/tokens;
- action/control labels do not clip or ellipsize;
- unintentional overlap is zero;
- toolbars remain one row and apply deterministic priority/overflow behavior;
- required pointer/keyboard/accessibility states work;
- required viewport/theme/text-scale matrices pass;
- realistic content, empty state, and error state are exercised;
- a human-approved baseline exists for intended visual output;
- no known layout defect is hidden by replacing a baseline.

A tool with no vision capability may enforce geometry, state, responsive, accessibility, command, screenshot-diff, and approved-baseline contracts. It may not declare an unreviewed changed screenshot attractive merely because the pixels are stable.

## Productization order

The current implementation order is:

1. make shared tokens/runtime theme mechanically consistent;
2. make the shared UI toolkit the sole owner of standard controls and chrome;
3. build the deterministic shared-component reference surface and state matrix;
4. enforce contract/baseline/accessibility/responsive gates;
5. replace legacy title/toolbar/panel shells across the suite;
6. build professional document/grid primitives for Writer and Sheets;
7. build shared canvas/layer/inspector/direct-manipulation primitives for Photo and Present;
8. build shared scene/timeline primitives for Motion and Video;
9. migrate Studio arrangement/mixer primitives;
10. migrate Encode queue/settings primitives;
11. resume broad feature expansion through the shared toolkit only.

A migration is complete when the legacy app-local implementation is removed, not when a new component exists beside it.

## Audit policy

- Root-level prose remains limited to `AGENTS.MD`, `README.md`, and `TRUTH.md`.
- Generated reports belong in CI artifacts or working output, not as competing truth documents.
- Audit thresholds must not be weakened to make a failing implementation look complete.
- Source-token counts, test counts, callback counts, or engine-function counts are never direct product-parity percentages.
- Product readiness rises only with evidenced user-facing capability and quality.

## Non-negotiable direction

- Rust + Slint; no framework rewrite for the application suite.
- Local-first and offline-capable.
- Original Loom visual identity rather than proprietary assets or source.
- Shared toolkit before eight independent UI implementations.
- No fabricated progress or placeholder behavior represented as complete.
- Implementation continues on the current programme branch until integrated.
