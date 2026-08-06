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

The current honest complete-suite parity score against the target defined in
`AGENTS.MD` is still approximately **23/100**. The Writer desktop-file tranche
is a real improvement, but one application adopting native dialogs is not large
enough to move the rounded suite-wide score. It does not complete Epic 1.

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
- A shared injectable desktop file-dialog contract. The production adapter uses
  native operating-system dialogs; deterministic tests can use a scripted
  adapter without opening modal windows.
- Bounded undo/redo foundations and crash-recovery snapshots in all application
  front ends, with depth varying by application.
- Light, dark, and high-contrast design tokens and adaptive desktop layouts.
- Headless screenshot, smoke, CLI-functional, package, and native matrix tooling.
- No mandatory account, cloud service, telemetry, or hidden network dependency.

## Current application boundaries

- **Writer:** editable paragraph surface, block model, style-run persistence,
  bounded/coalesced history, recovery, search/pagination foundations, Markdown
  workflows, and PDF output. The normal desktop UI now opens arbitrary
  `.loomdoc` files through a native picker, saves to the current path, supports
  native Save As, and chooses a PDF destination through a native save dialog.
  Cancellation and dialog failures are surfaced without replacing the current
  document. File-dialog behavior is injectable for deterministic tests. Writer
  does not yet provide recent-document UI, platform menus, drag/drop, printing,
  asynchronous large-document I/O, or atomic save through the new desktop
  service. Toolbar formatting is still document-wide rather than a complete
  selection-aware rich-text path. Professional page layout, floating objects,
  citations, forms, mail merge, EPUB, and high-fidelity DOCX/ODT remain
  incomplete.
- **Sheets:** formulas, dependency and incremental-recalculation foundations,
  named ranges, validation, conditional predicates, filtering/sorting, CSV
  workflows, persistence, history, and a visible grid. Large-grid
  virtualization, rich formatting, charts, pivots, broad function coverage,
  XLSX/ODS fidelity, data connectors, and native desktop file workflows remain
  incomplete.
- **Present:** deck/slide models, layouts, notes, transitions, scene generation,
  validation, persistence, history, and PDF output. Full direct manipulation,
  masters, mixed media, animation authoring, presenter workflows, recording,
  video export, PPTX/ODP fidelity, and native desktop file workflows remain
  incomplete.
- **Photo:** raster decode, pixel buffers, layers, blend modes, adjustment and
  mask foundations, compositing, crop/resize, persistence, history, and
  PNG/JPEG export. Painting, production masks/selections, RAW/ICC, healing,
  warping, HDR/panorama, PSD fidelity, GPU effects, production AI editing, and
  native desktop file workflows remain incomplete.
- **Motion:** layer/keyframe models, interpolation, transform manipulation,
  ordering, validation, persistence, bounded history, frame sampling, and SVG
  frame export. A production compositor, synchronized playback, cameras/lights,
  particles, effects, tracking, stabilization, optical flow, render-queue
  breadth, and native desktop file workflows remain incomplete.
- **Video:** track/clip models, trim/split/speed/ripple operations, markers,
  captions, local probing and preview decode, persistence, history, FFmpeg-backed
  export, progress, and cancellation. Synchronized timeline playback, real proxy
  workflows, multicam, advanced trims, professional audio/color/effects, HDR,
  transcription/tracking, interchange, and native desktop file workflows remain
  incomplete.
- **Studio:** track/region models, PCM/WAV handling, oscillator and MIDI synthesis,
  automation interpolation, stereo mixing, persistence, history, and local
  audio/MIDI device foundations. Production recording, realtime scheduling,
  complete editing/mixing, comping, time/pitch tools, CLAP/VST3 hosting,
  isolation, plugin UI, spatial audio, mastering, and native desktop file
  workflows remain incomplete.
- **Encode:** editable FFmpeg queue, deterministic command plans, local backend
  discovery, presets, execution, progress, cancellation, retry, persistence,
  recovery, and queue history. Complete source controls, hardware policy,
  exhaustive formats, pause/resume guarantees, watch folders, multi-destination
  dependency workflows, perceptual conformance, and native desktop file
  workflows remain incomplete.

## Evidence boundaries

### Writer native file workflow

The shared `loom-desktop` crate has unit tests for filter validation, scripted
open/save results, cancellation, response exhaustion, and unsafe suggested file
names. Writer's affected workspace passed formatting, strict Clippy, unit tests,
and a release build before the migration was committed. This proves the
controller and backend contracts compile and their deterministic behavior is
tested. A fresh four-platform native run is still required before native dialog
appearance and interaction can be called verified on every target.

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
A palette screenshot proves overlay rendering only.

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

The branch builds the complete suite on Linux x86-64, Windows x86-64, macOS
Apple silicon, and macOS Intel in the native matrix. Packaging source uses WiX
v4-compatible package metadata and bounded retry for transient macOS `hdiutil`
failures. Every new implementation head still requires its own fresh native
matrix before those packages can be called verified for that exact source.

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
  `chatgpt/loom-ui-functional-audit-2` until this programme is integrated.
