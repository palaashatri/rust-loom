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

- **Writer:** editable plain-text-oriented document surface, Loom package
  persistence, Markdown-oriented workflows, and basic PDF output. It is not yet
  a complete rich-text/page-layout engine.
- **Sheets:** formula engine, package persistence, CSV workflows, selected-cell
  editing, and a small visible grid. Large-grid virtualization, advanced charts,
  pivots, and broad office-format compatibility are incomplete.
- **Present:** deck/slide model, selection, notes, layouts, persistence, and basic
  PDF output. The canvas is not yet a full direct-manipulation presentation
  editor and animation/rendering remains limited.
- **Photo:** layer metadata, adjustment parameters, persistence, and an
  interactive UI shell. Real pixel decoding, painting, masks, compositing, and
  nondestructive rendering are not complete.
- **Motion:** layer/keyframe metadata, persistence, timeline controls, and an
  interactive UI shell. Real composition rendering, interpolation, playback,
  tracking, and effects are incomplete.
- **Video:** track/clip metadata, persistence, timeline controls, and an
  interactive UI shell. Real media decode, synchronized playback, proxying,
  editing semantics, and export are incomplete.
- **Studio:** track/region metadata, persistence, mixer controls, and an
  interactive UI shell. A real-time audio engine, recording, MIDI instruments,
  plugin hosting, and bounce are incomplete.
- **Encode:** queue/preset metadata, persistence, and interactive queue controls.
  Real transcoder invocation, hardware acceleration, progress parsing, and
  output validation are incomplete.
- **Vision:** provider/model-pack architecture and reference capabilities exist;
  production-quality OCR, segmentation, tracking, and model distribution remain
  incomplete.
- **Plugin SDK:** manifest, package, permission, and host foundations exist;
  production sandboxing and broad application integration remain incomplete.

## Current UI and functional audit

- All eight apps share the graphite/copper token system, but only Present,
  Photo, Motion, Video, Studio, and Encode now use the common application header.
  Writer and Sheets retain their specialized document/grid chrome for now.
- Open now reloads each media-app project's documented local save file instead
  of exposing an unwired control. Native file-picker integration remains future
  work.
- Photo can add and persist an adjustment-layer record, but the value is not yet
  rendered into pixels.
- Present Undo/Redo is visibly disabled until a real presentation history stack
  exists.
- Photo, Video, and Studio export actions are visibly disabled until their real
  render/media/audio engines exist.
- Video and Studio transport controls no longer imply real playback or recording
  when no decoder/audio engine is connected.
- Encode no longer invents 75% progress or claims an encoder started. Progress is
  derived from persisted job states and the start control remains disabled until
  a transcoder backend is integrated.
- CI now checks callback wiring, production panic patterns, all Cargo workspaces,
  release builds, and distinct light/dark/high-contrast screenshots.

## Measured product score policy

- `loom-bootstrap/scripts/audit-product-readiness.py` reports UI and functionality
  separately on a ten-point evidence scale. The score is derived from source,
  tests, native packaging, and screenshot workflows; it is not manually declared.
- Ten out of ten is reserved for complete adaptive user journeys, native platform
  integration, production engines, interoperability, accessibility, and measured
  reliability. A passing regression floor is not equivalent to a 10/10 product.
- Windows x86-64, macOS Apple silicon, and macOS Intel now build release binaries,
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

The repository now contains executable reference implementations for the
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
  layers, deterministic compositing, and PPM export.
- Motion: ordered keyframes, easing/interpolation, frame sampling, layer
  reordering, validation, and bounded render ranges.
- Video: trim/split/speed operations, ripple moves/removal, markers, captions,
  overlap detection, render plans, and documented EDL output.
- Studio: validated PCM buffers, WAV export, oscillator/MIDI synthesis,
  automation interpolation, region moves, project validation, and stereo mixing.
- Encode: local FFmpeg discovery, deterministic command planning, progress
  parsing, interruption recovery, real process execution, and truthful statuses.
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
