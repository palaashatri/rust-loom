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
approximately **24/100**, up from 23/100. The increase is intentionally small:
Writer and Sheets now use real native file workflows, New creates blank projects,
and Sheets no longer advertises non-persistent multi-sheet and formatting
commands through the command palette. The score remains provisional until the
fresh four-platform native matrix passes on the final milestone head. Epic 1 is
not complete; Video, Studio, and Encode still require the shared native desktop workflow migration.

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
  native operating-system dialogs; deterministic tests use a scripted adapter
  without opening modal windows.
- Writer, Sheets, Present, Photo, and Motion use that shared contract for normal
  native Open, Save/Save As, import/export destination workflows where applicable.
- Bounded undo/redo foundations and crash-recovery snapshots in all application
  front ends, with depth varying by application.
- Light, dark, and high-contrast design tokens and adaptive desktop layouts.
- Headless screenshot, smoke, CLI-functional, package, and native matrix tooling.
- No mandatory account, cloud service, telemetry, or hidden network dependency.

## Current application boundaries

- **Writer:** editable paragraph surface, block model, style-run persistence,
  bounded/coalesced history, recovery, search/pagination foundations, Markdown
  workflows, and PDF output. New creates a blank unsaved document. The normal
  desktop UI opens arbitrary `.loomdoc` files through a native picker, saves to
  the current path, supports native Save As, and chooses a PDF destination
  through a native save dialog. Cancellation and dialog failures do not replace
  the current document. File-dialog behavior is injectable for deterministic
  tests. Writer does not yet provide recent-document UI, platform menus,
  drag/drop, printing, asynchronous large-document I/O, or atomic save through
  the new desktop service. Toolbar formatting is still document-wide rather
  than selection-aware. Professional page layout, floating objects, citations,
  forms, mail merge, EPUB, and high-fidelity DOCX/ODT remain incomplete.
- **Sheets:** formulas, dependency and incremental-recalculation foundations,
  named ranges, validation, conditional predicates, filtering/sorting, CSV
  workflows, persistence, history, and a visible fixed grid. New creates a
  blank single-sheet workbook. The desktop UI opens `.loomtable` or imports CSV
  through a native picker, saves native workbooks through Save/Save As, and
  selects CSV export destinations natively. Imported CSV paths are deliberately
  not reused as native package save targets. The command palette no longer
  exposes Add Sheet, Go To Sheet, or cell-format actions that previously changed
  UI state without persisted workbook semantics. Multi-sheet storage, large-grid
  virtualization, rich formatting, charts, pivots, broad function coverage,
  XLSX/ODS fidelity, data connectors, recent documents, menus, drag/drop,
  asynchronous I/O, and atomic save remain incomplete.
- **Present:** deck/slide models, layouts, notes, transitions, scene generation,
  validation, persistence, history, PDF output, and native New/Open/Save/Save As/
  export-destination workflows are wired. The Phase 0 re-audit does **not** promote
  its score: semantic round-trip assertions remain narrow, writes are non-atomic,
  PDF output is not independently validated in the desktop journey, and recent
  documents/import, full direct manipulation, masters, mixed media, animation
  authoring, presenter workflows, recording, video export, and PPTX/ODP fidelity
  remain incomplete.
- **Photo:** raster decode, pixel buffers, layers, blend modes, adjustment and
  mask foundations, compositing, crop/resize, persistence, history, native project
  Open/Save/Save As, raster import, and native PNG/JPEG destination workflows are
  wired. The Phase 0 re-audit does **not** promote its score: persistence/export
  writes are non-atomic, exported files are not independently decoded in the desktop
  journey, recent documents are absent, and tool selection still includes status-only
  modes rather than complete canvas interaction. Painting, production masks/selections,
  RAW/ICC, healing, warping, HDR/panorama, PSD fidelity, GPU effects, and production
  AI editing remain incomplete.
- **Motion:** layer/keyframe models, interpolation, transform manipulation, ordering,
  validation, persistence, bounded history, frame sampling, SVG frame export, and
  native New/Open/Save/Save As/export destination workflows are wired. The repaired
  native slice has a genuinely blank New composition, exact model round-trip equality,
  repeated-open idempotence, Save→Save As path coverage, cancellation/error coverage,
  read-only and non-UTF-8 path coverage where supported, and responsive startup smoke
  checks. This still does **not** justify a score increase: writes remain non-atomic,
  recent documents and professional render validation are absent, and production
  compositing/playback, cameras/lights, particles, effects, tracking, stabilization,
  optical flow, and render-queue breadth remain incomplete.
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

### First desktop-authenticity milestone

The shared `loom-desktop` crate has unit tests for filter validation, scripted
open/save results, cancellation, response exhaustion, and unsafe suggested file
names. Writer and Sheets have deterministic controller tests for dialog request
construction and current-directory behavior. Sheets additionally tests that CSV
imports do not become native workbook save targets. Both affected applications
passed formatting, strict Clippy, unit tests, and release builds after the final
blank-project and status-semantics cleanup.

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
score therefore remains approximately **24/100**.

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
  `chatgpt/loom-ui-functional-audit-2` until this programme is integrated.
