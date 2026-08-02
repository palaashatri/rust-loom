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
