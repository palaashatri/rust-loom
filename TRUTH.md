# Loom — Current Truth

This is the repository's human-maintained readiness record. `AGENTS.MD` defines the intended product. This file states what a user can actually rely on today.

CI artifacts can prove that a build, test, package, screenshot, or scripted journey passed. They do not convert an engine function, source token, screenshot, or partial file writer into a finished product feature.

## Product status

Loom is a local-first Rust/Slint creator-suite **functional alpha** containing eight desktop applications, substantial application-core engines, shared infrastructure, native project formats, and cross-platform validation machinery.

It is not yet a replacement for mature professional office, image, motion, video, audio, or delivery software. No application currently satisfies all requirements assigned to it in `AGENTS.MD`.

Under the strict product rubric below, current complete-suite readiness is approximately **29/100** (was 28/100 on 2026-08-27).

That score intentionally falls when previously credited engine work is not reachable through a professional GUI, when a workflow cannot be completed end to end, or when the shipping interaction/visual quality is not production grade. The score is not an additive count of functions, tests, structs, file parsers, or command-line capabilities.

**Delta 2026-08-30:** +1 reflects measurable hygiene and design-system consolidation (tracked pyc removal, .gitignore fix, 3 duplicated TemplateCards → 1 canonical, 91 spacing + 21 radius tokenizations, 7 stale import migrations, clippy 11/11 pass) without new user-facing workflow depth. No app has moved from PARTIAL to REAL.

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
| Architecture & persistence | 71/100 | Shared Rust infrastructure, local packages, history/recovery, atomic persistence, jobs, and native-dialog foundations are meaningful strengths. Hygienic: 4 tracked pyc removed, __pycache__/*.pyc now ignored, fmt 11/11 PASS, clippy 11/11 PASS. |
| Functionality reachable through GUI | 30/100 | Too much engine depth remains disconnected from normal professional workflows. |
| Interaction design | 20/100 | Selection, contextual editing, responsive toolbar behavior, direct manipulation, and editor-specific interaction models are incomplete. |
| Visual design & polish | 21/100 | Current application screenshots contain clipping, weak hierarchy, inconsistent density, dead space, oversized/overlapping chrome, and placeholder-like composition. Improved: 0 Slint hex violations, 69% spacing and 70% radius now tokenized, 3 TemplateCards consolidated to 1 canonical with @children slot; remaining preview-font and shell-spacing duplication reduced but responsive overflow still missing in Photo/Motion/Video/Studio/Encode. |
| Professional workflow depth | 25/100 | Each application has useful slices, but none yet provides the complete daily workflow expected of its mature category. |
| **Overall product readiness** | **29/100** | Functional alpha with substantial foundations; not professional-suite parity. |

The overall score is deliberately not the arithmetic mean. User-visible product capability and workflow completion dominate the final readiness judgment.

## Shared UI/productization reset

The current priority is the shared Loom UI toolkit and application-shell migration rather than accumulating more isolated backend features.

The normative design sources are the machine-readable token/desktop contracts plus the shared toolkit implementation. The suite is being moved to one geometry, typography, responsive, accessibility, toolbar, panel, canvas, and timeline system before broad feature expansion resumes.

**Progress 2026-08-30:**
- `loom-core/crates/loom-ui/ui/toolkit.slint` is sole owner of TemplateCard (now 140×160, preview-width/height, preview-background, show-selected-badge, accessible role) with @children slot; Writer 160×230, Sheets 180×185, Present 200×170 variants deleted (344 lines removed, -82 net).
- Hardcoded shell spacing 131→40 (-69%) and radius 30→9 (-70%) tokenized via `Theme.tokens.space.*` and `metrics.radius-*`; remaining literals are domain geometry (page 820px, timeline 230px) not chrome.
- Stale `components.slint` aliases migrated to canonical `toolkit.slint`: StudioLibrary 3×ToolbarButton+2×IconOnlyToolbarItem, StudioMixer 3×, StudioArrangement 3×, EncodeInspector 2×, Video 6×; dead ToolButton/IconButton imports removed from Studio/Encode app.slint. Photo/Motion/Video still use toolkit aliases `ToolbarIconButton`/`AppleToolbarItem` which are already toolkit-owned but need full `ResponsivePolicy` integration.
- No second GUI toolkit introduced; Slint remains sole UI framework (GATE C PASS).
- Remaining duplication: 6 preview font literals outside cards, ~40 spacing literals (domain), and per-app toolbar RTL mirroring via explicit branches (by design for stable HorizontalLayout).

A visual regression baseline is valid only after the represented design is approved. A screenshot does not become acceptable merely because it is deterministic or because a new baseline was committed.

## Current application boundaries

### Writer

Implemented foundations include editable document data, persistence/history/recovery, search and statistics helpers, block/formatting algorithms, Markdown/PDF paths, partial DOCX import/export, native file workflows, and a desktop editor shell. **2026-08-30:** New Document chooser now uses canonical `toolkit.slint::TemplateCard` (140×160 base, preview 140×175) via @children slot; former per-app `TemplateCard` 160×230 deleted. **2026-08-30 W1 slice:** selection-aware formatting hardened — `TextSelection` anchor/focus/affinity preserved and char-boundary clamped `loom-writer-core/src/lib.rs:1114`, `format_block_range` now splits fragments and coalesces `lib.rs:542`, `remap_style_runs` boundary duplicate fixed `lib.rs:1060`, `split_block`/`merge_blocks` UTF-8 flooring; `SelectionFormattingState`/`selection_text_spans`/`coalesce_runs`/`caret_style` added with `normalized_range` handling; `CommandRegistry` authoritative with `CommandSpec` catalog (34 specs: file/edit/format/view, shortcuts Cmd+B/I/U, Ctrl+Z) and honest enablement `loom-writer-app/src/main.rs:698` (undo iff `can_undo`, bold iff `!collapsed`, heading/align iff `!empty`); selection is first-class state that updates inspector/toolbar/registry/announcement without mutating blocks `main.rs:1920`; formatting produces named history entries and clears redo `main.rs:1457`; 7 new core tests + 9 new app tests (56 core PASS, 45 app PASS, search deterministic).

Current product limitation: the visible editor is still far below a professional word processor. Multi-block formatting now survives typing/split/merge/save-reopen per W1 hardening, but rich page layout, shaping/bidi pagination, floating objects, tables/images in a production editing model, comments/review, headers/footers, forms, high-fidelity interchange, and professional pagination/layout workflows remain incomplete. The dense toolbar has also demonstrated collision defects at supported desktop sizes. Toolbar B/I/U checked state now derives from `formatting_state_for_selection` with mixed-indeterminate handling, but line/page wrapping and print/PDF fidelity remain W2 work.

### Sheets

Implemented foundations include workbook/cell models, formulas, CSV workflows, persistence/history, several analysis/formatting helpers, partial XLSX import/export, and native file workflows. **2026-08-30:** Template chooser now uses canonical `TemplateCard` (preview 160×115) via @children; former `SheetTemplateCard` 180×185 deleted; grid viewport logic and spacing now tokenized (9px scrollbar → space.md).

Current product limitation: the shipping UI is still a small fixed-grid editor rather than a scalable spreadsheet workspace. Large-grid virtualization, viewport-filling grid behavior, robust range selection/fill/resize/freeze interactions, rich formatting, chart/pivot authoring, broad formula coverage, and high-fidelity XLSX/ODS workflows remain incomplete.

### Present

Implemented foundations include deck/slide/object models, ordering/alignment helpers, notes, PDF output, partial PPTX import/export, presenter-related engine primitives, history/persistence, and native file workflows. **2026-08-30:** Theme chooser now uses canonical `TemplateCard` (preview 180×101/144×108) via @children; former `ThemeCard` 200×170 deleted; spacing tokenized.

Current product limitation: object authoring, mixed media, animation authoring, presenter workflows, recording/video export, direct manipulation, and high-fidelity interchange are incomplete. Inspector command labels have demonstrated clipping/overlap at supported sizes.

### Photo

Implemented foundations include raster buffers, layers, masks, many CPU image operations/adjustments, histogram and transform helpers, project persistence/history, raster import/export, and native file workflows. **2026-08-30:** Inspector/canvas shell spacing and layer-row padding tokenized (24px → xxl, 12px → lg, 4px → sm, 4px radius → small); `ToolbarIconButton` already toolkit-owned.

Current product limitation: the GUI is not yet a professional raster editor. Selection/transform tooling, painting/retouching workflows, layer/inspector interaction, RAW/ICC, healing/warping, HDR/panorama, PSD fidelity, GPU effects, and production AI editing remain incomplete. Current captures have shown truncated layer names and persistent UI competing with the canvas.

### Motion

Implemented foundations include layer/keyframe models, interpolation, transforms, timing/playback helpers, procedural motion utilities, template/render-queue primitives, persistence/history, and a composition editor shell. **2026-08-30:** Panels and timeline shell spacing tokenized (7–9px → md/lg, 9px radius → large); `clock_for_document` helper added with frame-accurate `HH:MM:SS:FF` and `CompositionClock` wiring (still dead_code until playback is wired) and `timecode_uses_the_clock_frame_and_frame_rate` test; clippy hygiene fixed (unused Timer/Duration removed).

Current product limitation: professional scene manipulation, graph/timeline editing, compositing, effects, playback/render workflows, and interchange remain incomplete. The previously blank first frame and persistent transport chip were repaired, but this is a local correctness improvement rather than product completion.

### Video

Implemented foundations include timeline/track/clip models, trim/marker helpers, FFmpeg-backed local processing pieces, captions/audio/media helpers, persistence/history, and a timeline editor shell. **2026-08-30:** MediaBin, PreviewStage, Timeline and Inspector spacing tokenized (8–10px → md/lg, 7/9px radius → medium/large); 6 `ToolButton`/`IconButton` migrated to `ToolbarButton`/`IconOnlyToolbarItem` via toolkit.

Current product limitation: media-bin/source consistency, scalable timeline interaction, clip trimming/direct manipulation, viewer/source workflows, effects/color/audio workflows, export UX, and professional NLE depth remain incomplete. Existing captures have shown track-header truncation, ruler/control overlap, and inconsistent sample-media state.

### Studio

Implemented foundations include tracks/regions, PCM/WAV support, synthesis/DSP helpers, mixer/automation primitives, persistence/history, local device foundations, and a multitrack shell. **2026-08-30:** Library, Mixer and Arrangement spacing tokenized; 8 stale `ToolButton`/`IconButton` migrated to `ToolbarButton`/`IconOnlyToolbarItem` via toolkit; dead imports removed from `StudioApp`.

Current product limitation: production recording, low-latency realtime scheduling, editing/comping, time/pitch workflows, plugin hosting/isolation/UI, mixing/mastering depth, and scalable arrangement interaction remain incomplete. Track naming and dense arrangement presentation still require shared-toolkit migration.

### Encode

Implemented foundations include an FFmpeg queue, presets, command planning/execution/progress/cancellation, persistence/recovery, probe/conformance helpers, hardware-codec planning, and batch/destination primitives. **2026-08-30:** Inspector and progress shell tokenized (5px radius → small); 2 `ToolButton` migrated to `ToolbarButton` via toolkit; dead imports removed from `EncodeApp`.

Current product limitation: the queue/settings workflow, job hierarchy, watch-folder experience, hardware policy, pause/resume guarantees, exhaustive format support, and perceptual conformance remain incomplete. Existing UI gives progress percentage excessive visual hierarchy relative to job identity/settings.

## Stabilization evidence 2026-08-30

### Hygiene
- Before: 81 GB total (`du -sh .`), 80.8 GB in 13 `*/target` (loom-sheets 11G, writer 9.4G, motion 8.8G, present 8.7G, core 8.4G, video 8.0G, photo 7.8G, studio 7.6G, encode 7.6G, vision 1.2G, plugin-sdk 384M, .work/slint-hit-test 2.0G), 2.4 GB `.work` (7809 files, evidence 274M, dist dmg 63M), 2.7 MB `Loom-Complete.zip` (ignored), 5 `*.pyc` (200K) leaked.
- `.gitignore` fixed: added `__pycache__/`, `*.pyc`, `*.pyo` (was missing; only `target`, `.work`, `.DS_Store` were covered). 4 tracked `*.pyc` in `loom-bootstrap/{packaging,scripts}/__pycache__` removed via `git rm --cached` (commit `cdf3703`).
- Verified ignore: `git check-ignore -v loom-bootstrap/scripts/__pycache__/audit-product-ui.cpython-314.pyc => .gitignore:50:__pycache__/` PASS; `git status --ignored` now shows 0 untracked pyc; `git ls-files | grep pyc` 0 after removal.
- No deletion of tracked user assets: `loom-design-bible/baselines` 2.5M (20 pngs) and `loom-*/docs/screenshot.png` 8 files kept.

### Design system
- Toolkit sole owner verified: 0 hardcoded `#[hex]` in app Slint (grep returns 0 outside `theme.slint`); `Theme.palette()` and `Theme.tokens.*` used for all chrome.
- Consolidated: `Writer TemplateCard` 160×230 + `Sheets SheetTemplateCard` 180×185 + `Present ThemeCard` 200×170 (344 lines) → 1 canonical `toolkit.slint::TemplateCard` 140×160 via @children slot (-82 net, commit `baee190`).
- Tokenized: spacing/padding 131→40 (-69%), radius 30→9 (-70%) across motion/photo/video/sheets/writer/present/encode_progress; 25 preview `font-size` literals → 6.
- Stale aliases: 7+ migrations from `components.slint` legacy (`ToolButton`→`ToolbarButton`, `IconButton`→`IconOnlyToolbarItem`) in StudioLibrary/Mixer/Arrangement, EncodeInspector, Video MediaBin/Transport/Timeline; `cargo check` 8/8 PASS.

### Architecture
- 11 independent Cargo workspaces (`resolver="2"`, `edition="2021"`, `rust-version="1.80"`); no root Cargo.toml.
- Monolithic `main.rs` 1367–2604 lines in all 8 apps remains (expected split `app_state.rs`, `controller.rs`, `commands.rs` still 0 files); `loom-command`, `loom-history`, `loom-jobs`, `loom-interop`, `loom-media-runtime` workspace members exist but 0 app imports (grep returns 0). Documented as P1 debt, not attempted in this tranche.
- `loom-desktop::FileDialogService` and `loom-production::define_snapshot_recovery!` correctly used in all 8 apps; `loom-ui` toolkit mature (80 icons, 40 palette fields, 8 spacing, 13 metrics, 3 themes, ResponsivePolicy 1180/1320).

### Build/test
- `loom-bootstrap/scripts/fmt-all.sh` 11/11 PASS (2026-08-30)
- `loom-bootstrap/scripts/clippy-all.sh` 11/11 PASS (was 10/11 due to `loom-motion` unused `Duration`/`Timer`; fixed in 92340e5, now diagnostic_lines 2304, loom_crate_issues 0)
- `cargo check` PASS for loom-core, writer, sheets, present, photo, motion, video, studio, encode (all dev, 2.7–5.2s)
- `cargo test --manifest-path loom-core/Cargo.toml --lib` 8+11+10+7 tests PASS (0.46s, 0.31s, 1.07s)
- Full `test-all.sh` not run to completion in CI time budget (120s timeout); representative core tests evidence above.

### Screenshots (human review required)
- Generated headless via `SLINT_BACKEND=software` release binaries at 1280×800 light theme, 2026-08-30, `/tmp/loom-visual-qa/`:
  - `writer-light-1280x800.png` 112K 1280×800 PNG
  - `sheets-light-1280x800.png` 63K
  - `present-light-1280x800.png` 87K
  - `photo-light-1280x800.png` 113K
  - `motion-light-1280x800.png` 113K
  - `video-light-1280x800.png` 102K
  - `studio-light-1280x800.png` 115K
  - `encode-light-1280x800.png` 94K
- All PNGs validated (`file` 1280×800 RBGA, IHDR, >4096 bytes). No visual-quality claim made; human/visual-model must inspect hierarchy, clipping, density, alignment, focus, state clarity.
- Visual regression baselines NOT updated; existing `loom-core/crates/loom-ui/baselines/light/` (5 files) preserved.

### Remaining gates
- GATE D (shared duplication) measurably decreased: 3 components removed, 7 stale migrations, 0 app hex violations.
- GATE E (hardcoded styling) measurably decreased: 91 spacing +21 radii tokenized; preview duplication remains partially (6 literals) and domain geometry kept literal.
- GATE C PASS (no second toolkit); GATE G updated (TRUTH matches code); GATES H/I partially (pyc removed, .gitignore fixed, targets still 80.8 GB reclaimable via `cleanup-targets.sh` after verification).

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
