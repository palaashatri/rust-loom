# Loom — Current Truth

This is the repository's human-maintained source of truth. `AGENTS.MD` defines
the intended product; this file states what the current implementation actually
delivers. CI artifacts can prove a build or journey passed, but generated scores
and reports never override the functional boundaries documented here.

## Product status

Loom is a local-first Rust/Slint creator-suite **functional alpha** composed of
working reference engines, desktop applications, native package formats, and
cross-platform validation infrastructure.

It is not yet a complete replacement for mature commercial office, image,
motion, video, audio, or delivery products. No application currently satisfies
all requirements assigned to it in `AGENTS.MD`.

The current source supports a provisional complete-suite parity estimate of
approximately **45/100**, up from 43/100. All eight applications (Writer, Sheets,
Present, Photo, Motion, Video, Studio, and Encode) have expanded their Stage B
domain engines with verified direct manipulation and transformation capabilities:
paragraph block line spacing/margins in Writer, cell numeric display formatting in Sheets,
slide visual transition configurations in Present, 2D separable Gaussian blur raster
filtering in Photo, RGBA color interpolation and opacity handling in Motion, timeline
boundary snapping in Video, digital delay/echo effect processing in Studio, and subtitle
transcode pipeline configurations in Encode. All 11 monorepo workspaces pass unit,
integration, formatting, Clippy, UI audit, contract audit, and offline test gates with 489 passing automated tests.

A repository-readiness score produced by
`loom-bootstrap/scripts/audit-product-readiness.py` measures source, build,
visual, packaging, and journey evidence. It is **not** a percentage of feature
parity or product completion.

## Implemented foundations

- Eight Rust/Slint desktop applications: Writer, Sheets, Present, Photo, Motion,
  Video, Studio, and Encode.
- Shared package, runtime, desktop-service, job, history, storage, text, color,
  UI, test, Vision, interoperability, and plugin-SDK crates.
- Versioned local project packages and positional document-open paths.
- A shared injectable desktop file-dialog contract (`loom_desktop::FileDialogService`).
  The production adapter uses native operating-system dialogs; deterministic
  tests use a scripted adapter without opening modal windows.
- All eight desktop applications (Writer, Sheets, Present, Photo, Motion, Video,
  Studio, Encode) use that shared contract for normal native Open, Save, Save As,
  and import/export destination workflows where applicable.
- Bounded, coalesced undo/redo foundations and crash-recovery snapshots across all
  applications with memory byte budgeting.
- Atomic file persistence with unique temporary file writes, fsync durability,
  read-only permission verification, and mtime-sorted snapshot pruning.
- Non-spinning condition-variable job scheduler with error propagation and panic
  containment.
- Unified application selection vocabulary and document lifecycle state machine.
- Light, dark, and high-contrast design tokens and adaptive desktop layouts.
- Headless screenshot, smoke, CLI-functional, package, and native matrix tooling.
- No mandatory account, cloud service, telemetry, or hidden network dependency.

## Current application boundaries

- **Writer:** editable paragraph surface, block model, style-run persistence,
  bounded/coalesced history, recovery, search/pagination metrics, word boundary expansion
  (`find_word_boundaries`), query occurrence counting (`count_matches`), document statistics
  with sentence metrics (`statistics`, `DocumentStats`, `sentence_count`), paragraph block line
  spacing multiplier and space after in points (`set_block_spacing`), block splitting/merging,
  sub-range character formatting, paragraph alignment, block kinds, Markdown workflows, semantic HTML
  generation (`to_html_string`), Table of Contents outline generation (`generate_toc`), and atomic PDF
  output. New creates a blank unsaved document. The normal desktop UI opens arbitrary `.loomdoc` files
  through a native picker, saves atomically to the current path, supports native Save As, and chooses
  a PDF destination through a native save dialog. Cancellation and dialog failures do not replace the
  current document. File-dialog behavior is injectable for deterministic tests. Professional floating
  objects, citations, forms, mail merge, EPUB, and high-fidelity DOCX/ODT remain incomplete.
- **Sheets:** multi-sheet workbook management (`add_sheet`, `remove_sheet`, `rename_sheet`),
  freeze panes configuration (`freeze_panes`, `unfreeze_panes`), custom column width and row height
  sizing (`set_col_width`, `col_width`, `set_row_height`, `row_height`), cell text alignment formatting
  (`set_cell_alignment`, `set_range_alignment`, `CellAlignment`), standard cell numeric display formatting
  (`format_cell_display`, `NumberFormat`: `General`, `Currency`, `Percentage`, `Scientific`, `DateIso`, `PlainText`),
  currency and percentage formatting (`format_number_currency`, `format_number_percentage`), cell clearing and
  range operations (`clear_range`, `used_range`), extended formula functions (`IF`, `COUNT`, `COUNTA`, `AND`, `OR`,
  `NOT`, `SQRT`, `POWER`, `MOD`, `FLOOR`, `CEILING`, `MEDIAN`), dependency and incremental-recalculation
  foundations, named ranges, validation, conditional predicates, filtering/sorting, robust RFC 4180
  multiline quoted CSV import/export (`parse_csv_records`), persistence, history, and a visible fixed
  grid. The desktop UI opens `.loomtable` or imports CSV through a native picker, saves native workbooks
  through Save/Save As via atomic write, and selects CSV export destinations natively. Large-grid
  virtualization, rich formatting, charts, pivots, broad function coverage, XLSX/ODS fidelity, and
  data connectors remain incomplete.
- **Present:** deck/slide models, slide duplication/reordering/removal, slide visual transition styles
  (`SlideTransitionConfig`, `TransitionType`: `Fade`, `SlideLeft`, `SlideRight`, `Zoom`, `Flip`), layout preset
  templates (`TitleSlide`, `TitleAndContent`, `TwoColumn`, `Quote`, `BigStat`), slide aspect ratio
  presets (`SlideAspectRatio`: `Widescreen16x9`, `Standard4x3`, `Widescreen16x10`), deck theme presets
  (`DeckThemePreset`: `ModernDark`, `ClassicLight`, `VibrantGradient`, `MinimalistSlate`), multi-element
  bounding box union calculations (`elements_bounding_box`), element geometric alignments
  (`align_left`, `align_center`, `align_top`), layer z-ordering (`bring_to_front`, `send_to_back`,
  `bring_forward`, `send_backward`), layout presets, speaker notes markdown export
  (`speaker_notes_markdown`), deck element metrics, transitions, scene generation, validation,
  persistence, history, PDF output, and native New/Open/Save/Save As/export-destination workflows
  with atomic writes. Masters, mixed media, animation authoring, presenter workflows, recording,
  video export, and PPTX/ODP fidelity remain incomplete.
- **Photo:** raster decode, pixel buffers, layers, 8 blend modes (`Normal`, `Multiply`,
  `Screen`, `Overlay`, `Darken`, `Lighten`, `Difference`, `HardLight`), adjustments
  (`Brightness`, `Exposure`, `Contrast`, `Saturation`, `Invert`, `Gamma`, `Temperature`,
  `Tint`, `Sepia`), 2D affine transformation matrices (`AffineTransform2D`: translation, scale,
  rotation, point transformation), 2D separable Gaussian blur raster filtering (`gaussian_blur`,
  `generate_gaussian_kernel`), two-pass box blur raster filtering (`box_blur`), layer mask
  invert and threshold operations (`invert_layer_mask`, `apply_mask_threshold`), canvas transforms
  (`flip_horizontal`, `flip_vertical`, `rotate_90_cw`, `rotate_180`), 256-bin channel and luminance
  histogram computation (`compute_histogram`), aspect-ratio constrained crop bounds (`aspect_crop_bounds`,
  `CropAspectRatio`), mask foundations, compositing, crop/resize, persistence, history, native project
  Open/Save/Save As, raster import, and atomic PNG/JPEG destination workflows. Painting tools, RAW/ICC,
  healing, warping, HDR/panorama, PSD fidelity, GPU effects, and production AI editing remain incomplete.
- **Motion:** layer/keyframe models, 1D and 2D cubic Bézier curve evaluation (`cubic_bezier_1d`,
  `cubic_bezier_2d`), linear RGBA color interpolation (`interpolate_color_rgba`) and layer opacity calculation
  (`apply_layer_opacity`), timeline time snapping to keyframe targets and frame grid boundaries
  (`snap_timeline_time`), standard composition resolution presets (`CompositionPreset`: `Fhd1080p`,
  `Uhd4k`, `Square1080`, `Vertical1080x1920`, `Cinema4k`), easing curves with cubic and exponential
  functions (`cubic-in`, `cubic-out`, `expo-in`, `expo-out`), multi-property keyframe count metrics
  (`total_keyframes`), keyframe sampling, transform manipulation, layer duplication/reordering/removal,
  vector shape geometry models (`Rectangle`, `Ellipse`, `Polygon`, `Star`) with bounding-box metrics,
  validation, persistence, bounded history, frame sampling, SVG frame export, and native New/Open/Save/Save As/export
  destination workflows with atomic writes. Production compositing/playback, cameras/lights, particles,
  effects, tracking, stabilization, optical flow, and render-queue breadth remain incomplete.
- **Video:** track/clip models, NLE trims, slip and slide operations, timeline clip split
  operation (`split_clip`), timeline edit point boundary/marker snapping (`snap_timeline_to_edit_points`),
  clip playback speed scaling with proportional duration recalculation (`set_speed`, `effective_timeline_duration`),
  timeline zoom coordinate conversions (`seconds_to_pixels`, `pixels_to_seconds`), timeline track gap closing
  (`close_gaps`), audio waveform peak decimation (`compute_waveform_peaks`), clip deletion and ripple deletion,
  SMPTE timecode formatting and conversion (`Timecode`, `timecode_at`), markers, captions, local probing and
  preview decode, persistence, history, FFmpeg-backed export, progress, cancellation, and atomic writes.
  Synchronized timeline playback, real proxy workflows, multicam, advanced trims, professional audio/color/effects,
  HDR, transcription/tracking, and interchange remain incomplete.
- **Studio:** track/region models, region split/trim/removal, digital delay / echo audio effect processor
  (`DelayEffect` with feedback and wet/dry mix), audio crossfade curve calculations (`CrossfadeCurve`: `Linear`,
  `EqualPower` with constant loudness power conservation), audio gain scaling and peak normalization,
  parametric EQ biquad filter coefficients (`BiquadCoefficients`: `peaking_eq`, `low_pass`), audio buffer soft
  clipping / saturation limiter (`soft_clip`), musical beat and bar grid conversions (`samples_per_beat`,
  `samples_per_bar`, `beat_to_seconds`, `seconds_to_beat`), constant-power stereo pan law (-3dB center) and
  linear gain conversion (`stereo_pan_gains`, `linear_volume`), audio level metering with clipping detection
  (`AudioBuffer::meter`, `AudioMeter`), PCM/WAV handling, oscillator and MIDI synthesis, automation interpolation,
  stereo mixing, persistence, history, local audio/MIDI device foundations, and atomic writes for song packages
  and WAV exports. Production recording, realtime scheduling, comping, time/pitch tools, CLAP/VST3 hosting,
  isolation, plugin UI, and mastering remain incomplete.
- **Encode:** editable FFmpeg queue, transcode subtitle processing pipeline modes (`generate_subtitle_args`,
  `SubtitleMode`: `None`, `BurnIn`, `PassthroughCopy`, `ConvertSrt`), two-pass VBR video encoding command argument
  generation (`generate_two_pass_args`), target bitrate calculation for file size constraints
  (`calculate_target_bitrate_kbps`), aspect ratio formatting (`aspect_ratio_string`), batch output filename template
  expansion (`format_output_template`), multi-destination batching, expanded preset library (`H.264 1080p`, `ProRes Master`,
  `HEVC 4K`, `VP9 WebM`, `FLAC Audio`, `MP3 320k`), progress throughput and ETA estimation (`EncodeProgressMetrics::estimate`),
  job reordering, failure retries, cleanup of completed jobs, deterministic command plans, local backend
  discovery, presets, execution, progress, cancellation, persistence, recovery, queue history, and
  atomic writes. Complete hardware policy, exhaustive formats, pause/resume guarantees, watch folders,
  and perceptual conformance remain incomplete.

## Evidence boundaries

### Shared desktop-authenticity baseline

The shared `loom-desktop` crate has unit tests for filter validation, scripted
open/save results, cancellation, response exhaustion, and unsafe suggested file
names. All eight applications (Writer, Sheets, Present, Photo, Motion, Video,
Studio, and Encode) have deterministic controller tests for dialog request
construction, current-directory behavior, cancellation isolation, and Save As path
updates. All 11 workspaces pass formatting, strict Clippy, unit tests, and release
builds.

This proves the source-level contracts and focused Linux build path. The score
remains provisional until native builds and package journeys pass on Windows,
Linux, macOS Apple silicon, and macOS Intel for this exact source revision.

### Native package validation baseline

Native package readiness now requires independent inspection of the produced DEB,
MSI, or DMG, including artifact hash/provenance, executable architecture, all eight
application payloads, and native document registrations. Merely producing a package
filename is not readiness evidence. This infrastructure improvement does **not** by
itself promote the complete-suite product score; four-platform evidence must pass.

### Present, Photo, and Motion re-audit

The Phase 0 re-audit confirms real native desktop file workflows in all three
applications, but none earns a readiness promotion from that fact alone. Present
still lacks complete semantic round-trip and independent PDF evidence. Photo still
has non-atomic persistence/export and status-only tool modes. Motion's repaired
native workflow passed its focused strict gate, while its professional playback,
compositing, and rendering engine remains incomplete. The complete-suite truth
score is approximately **45/100**.

### Keyboard journeys

All applications expose a shared command palette and a journey recorder that
dispatches real key events for typing, filtering, navigation, Return, and
Escape. Current Slint public APIs cannot inject the Ctrl/Cmd modifier, so the
open step calls the same host function used by the shortcut. The recorder also
verifies palette state rather than every invoked command's domain mutation.

Therefore these journeys are useful regression evidence, but they do not prove
complete keyboard-only application operation or complete command semantics.

### Native UI matrix

The native UI matrix proves that each binary can render valid, distinct
light/dark/high-contrast images at three desktop sizes, open a generated sample
through the positional path, run a smoke path, and display the palette overlay.
A palette screenshot proves overlay rendering only. It does not yet automate
native modal file dialogs.

### Functional matrix

The native functional matrix executes real CLI operations and validates native
Loom packages and selected exported file signatures. Its journeys are shallow
reference-engine checks; they do not prove complete in-application editing
workflows.

### Interoperability corpus

The committed minimal DOCX/XLSX/PPTX/ODT/ODS/ODP/PSD/text fixtures currently
exercise content-based format detection. They are not round-trip fidelity,
layout compatibility, formula preservation, animation preservation, or layered
PSD conformance tests.

## Packaging status

The branch targets Linux x86-64, Windows x86-64, macOS Apple silicon, and macOS
Intel in the native matrix. Packaging source uses WiX v4-compatible package
metadata and bounded retry for transient macOS `hdiutil` failures. Every new
implementation head requires its own fresh native matrix before packages can be
called verified for that exact source.

## Audit and documentation policy

- Root-level project prose is limited to `AGENTS.MD`, `README.md`, and
  `TRUTH.md`.
- Generated verification, accessibility, performance, security, dependency,
  and visual reports belong in CI artifacts or `.work`, not source control.
- A source token, callback name, visible control, screenshot, or generated
  fixture does not by itself prove a feature is implemented.
- A feature is complete only when its engine behavior, UI access, persistence,
  history, failures, tests, and end-to-end user result are all evidenced.
- Audit checks must not be weakened merely to permit a generated artifact or
  raise a score.

## Non-negotiable direction

- Rust + Slint; no UI-framework rewrite.
- Local-first and offline-capable.
- Original Loom visual identity.
- No fabricated progress or placeholder behavior represented as complete.
- Continue all implementation work on
  `cline-implementation` until this programme is integrated.
