# Loom — Current Truth

This file is the concise, human-maintained statement of what the repository
actually contains. Build logs and CI results are stronger evidence than this
file and must override it when they disagree.

## Implemented foundation

- Rust/Slint application binaries for Writer, Sheets, Present, Photo, Motion,
  Video, Studio, and Encode.
- Shared package, command, job, history, storage, text, color, UI, testing,
  Vision, and plugin-SDK crates.
- Versioned Loom project packages and sample projects.
- Headless smoke/screenshot paths and suite orchestration scripts.
- Light, dark, and high-contrast design tokens.
- Local-first architecture with no mandatory account or cloud service.

## Functional application boundaries

- **Writer:** editable document surface, bounded/coalesced undo and redo, crash
  recovery, Loom package persistence, Markdown-oriented workflows, and PDF
  output. It is not yet a complete rich-text/page-layout engine.
- **Sheets:** formula engine, package persistence, CSV workflows, selected-cell
  editing, undo and redo, and a visible grid. Large-grid virtualization,
  advanced charts, pivots, and broad office-format compatibility are incomplete.
- **Present:** deck/slide model, selection, notes, layouts, bounded undo and redo,
  persistence, transitions, validation, and PDF output. The canvas is not yet a
  complete direct-manipulation presentation editor.
- **Photo:** local raster decoding, layers, blend modes, adjustments, compositing,
  undo and redo, persistence, and PNG/JPEG export. Painting, masks, advanced
  selection tools, RAW workflows, and GPU effects remain incomplete.
- **Motion:** layer and keyframe editing, interpolation, persisted transforms,
  bounded undo and redo, crash recovery, timeline controls, and deterministic
  SVG frame export. GPU composition rendering, synchronized real-time playback,
  tracking, particles, and professional effects remain incomplete.
- **Video:** track and clip editing, local media probing/preview decoding,
  timeline operations, undo and redo, persistence, FFmpeg-backed export, and
  cancellation. Full synchronized playback, proxy workflows, GPU effects, and
  broad professional codec/container coverage remain incomplete.
- **Studio:** local project editing, track and region operations, PCM/WAV handling,
  MIDI/oscillator synthesis, stereo mixing, undo and redo, persistence, and local
  audio/MIDI device foundations. Production plugin hosting, process isolation,
  recording workflows, and complete real-time DAW behavior remain incomplete.
- **Encode:** editable FFmpeg-backed batch queue, presets, truthful progress,
  cancellation, retry, persisted crash recovery, and queue-edit undo and redo.
  Hardware acceleration policy, exhaustive codec/container coverage, distributed
  encoding, and production output conformance testing remain incomplete.
- **Vision:** provider/model-pack architecture and CPU reference capabilities
  exist; production-quality OCR, segmentation, tracking, evaluated model packs,
  and redistributable acceleration backends remain incomplete.
- **Plugin SDK:** manifest, package, permission, defensive WebAssembly validation,
  trust, update, and rollback foundations exist; complete host ABIs, production
  UI embedding, signing infrastructure, and broad application integration remain
  incomplete.

## Current UI and functional audit

- All eight apps use the shared graphite/copper token system, common application
  header, semantic light/dark/high-contrast palettes, and shared professional
  workspace components.
- Shared buttons, tabs, segmented controls, sliders, transport controls, and
  workspace rows expose keyboard and accessibility behavior through the Slint
  component layer.
- Applications accept an associated document path as the first positional
  argument as well as through `--open`. Native file-picker/menu integration
  remains incomplete.
- Motion transform controls mutate persisted keyframes rather than status text;
  its history coalesces repeated edits and its SVG export serializes a sampled
  composition frame.
- Encode locks queue editing and history controls while FFmpeg owns the queue;
  user queue edits can be undone and redone independently of runtime progress.
- CI checks callback wiring, production panic patterns, product-readiness scores,
  all Cargo workspaces, release builds, and distinct light/dark/high-contrast
  screenshots.

## Measured product score policy

- `loom-bootstrap/scripts/audit-product-readiness.py` reports UI and functionality
  separately on a ten-point evidence scale. The score is derived from source,
  tests, native packaging, and screenshot workflows; it is not manually declared.
- Ten out of ten is reserved for complete adaptive user journeys, native platform
  integration, production engines, interoperability, accessibility, and measured
  reliability. A passing regression floor is not equivalent to a 10/10 product.
- Windows x86-64, macOS Apple silicon, and macOS Intel build release binaries,
  render all eight apps in light/dark/high-contrast, run native smoke paths, build
  MSI/DMG validation packages, and upload those packages and screenshots for review.
- Native document associations are emitted by Linux, Windows, and macOS packages;
  every application accepts an associated document path as its first positional
  argument as well as through `--open`.

## Non-negotiable direction

- Rust + Slint; no UI-framework rewrite.
- Linux first, cross-platform architecture.
- Local-first and offline-capable.
- No telemetry, mandatory account, or hidden network access.
- Original Loom visual identity: graphite surfaces, warm copper accent, compact
  professional controls, progressive disclosure, and functional motion.
- Static mockups and fake progress never count as implemented features.
- Keep `AGENTS.MD`, `README.md`, and `TRUTH.md` as the root-level project prose.
  Put durable technical details next to code, schemas, tests, and fixtures.

## End-to-end implementation tranche (August 2026)

The repository contains executable reference implementations for the
cross-suite foundations that were previously only described in specifications:

- `loom-runtime`: deterministic settings, platform paths, atomic writes,
  crash-recovery snapshots, bounded/redactable diagnostics, shortcut conflict
  management, multi-format clipboard payloads, recent files, and autosave timing.
- Writer: document search/replace, generated table of contents, deterministic
  pagination, comments, tracked revisions, bookmarks, and table models.
- Sheets: named ranges, validation, conditional-format predicates, row filters,
  range sorting, dependency graphs, and incremental recalculation.
- Present: bounded undo/redo sessions, slide duplication/reordering, element
  transforms, transitions, normalized render scenes, and deck validation.
- Photo: validated RGBA buffers, crop/resize, masks, blend modes, adjustment
  layers, deterministic compositing, and raster export.
- Motion: ordered keyframes, easing/interpolation, frame sampling, transform
  history, SVG-frame export, layer reordering, validation, and bounded ranges.
- Video: trim/split/speed operations, ripple moves/removal, markers, captions,
  overlap detection, render plans, preview decoding, and FFmpeg-backed export.
- Studio: validated PCM buffers, WAV export, oscillator/MIDI synthesis,
  automation interpolation, region moves, project validation, and stereo mixing.
- Encode: local FFmpeg discovery, deterministic command planning, queue history,
  progress parsing, interruption recovery, process execution, and truthful states.
- Loom Vision: CPU reference QR, statistics, threshold segmentation, document
  region detection, image embeddings, and audio analysis, all local and
  cancellable where work is iterative.
- Plugin SDK: defensive package installation, bounded WebAssembly binary
  validation, capability/path checks, entry-export validation, and optional
  local Wasmtime execution with time and output limits.

These are functional, testable reference engines and APIs. They do **not** yet
constitute complete commercial parity with mature office, image, motion, video,
audio, transcoding, or ML products. Hardware-accelerated render graphs, full
codec/container coverage, rich interchange compatibility, production OCR and
learned vision models, full plugin ABI host functions, and exhaustive UI wiring
remain subsequent engineering work. No placeholder or sample-only path should
be represented as those capabilities.
