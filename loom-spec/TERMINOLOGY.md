# Loom Terminology

The shared glossary. Every Loom document must use these terms as defined here.
Statuses in parentheses are implementation states; see `FEATURE_MATRICES.md`.

## Platform concepts

- **Command** — a named, undoable user action with a stable identifier; the
  single path for menus, shortcuts, palette, accessibility, and plugins
  (`loom-core/crates/loom-command`).
- **Job** — a cancellable unit of async work with progress, priority, and
  optional dependencies; never runs on the UI thread
  (`loom-core/crates/loom-jobs`).
- **Document** — the in-memory model of user content for an application
  (Writer document, workbook, deck, composition, …); headless and testable.
- **Package** — the on-disk ZIP container format for a document
  (`.loomdoc`, `.loomtable`, …); implemented in `loom-core/crates/loom-package`.
- **Manifest** — the `manifest.json` inside a package: format version, id,
  timestamps, checksums, entry table.
- **Schema version** — the `format_version` of a package; governs migration
  rules. See `FILE_FORMAT_FAMILY.md`.
- **Checksum** — SHA-256 digest of an entry, recorded in the manifest for
  corruption detection.
- **Autosave** — periodic transactional save of the current document state.
- **Recovery** — reopening and reconciling a document after a crash, from
  autosave and operation journals.
- **History** — the undo/redo transaction stack
  (`loom-core/crates/loom-history`).
- **Workspace** — the arrangement of panels/windows for an application
  (also "work-space"; a mode in Loom Studio).
- **Inspector** — the context-sensitive property panel that updates with the
  selection (`../loom-design-bible/INSPECTORS.md`).
- **Preset** — a named, reusable settings bundle (export preset, effect
  preset, instrument preset).
- **Template** — a reusable document/deck/composition starting point.
- **Theme** — a named set of design tokens applied to a deck or document.
- **Provider** — an implementation of a capability behind a capability trait
  (`loom-vision`).
- **Capability** — a named perception ability (OCR, segmentation, tracking, …)
  identified by `CapabilityId`.
- **Model pack** — an installable local directory of model files with
  `manifest.json`, checksums, license, and capability declarations.
- **Reference provider** — a small CPU-only provider proving a capability
  contract (QR decode, image statistics).
- **Plugin** — a sandboxed extension packaged as `.loomplugin` (WASM-based
  design); declares capabilities and permissions in its manifest
  (`loom-plugin-sdk`).
- **Sandbox** — the runtime boundary that isolates plugins from the host.
- **Permission** — a declared plugin capability grant checked at call time.
- **Inspector section** — a group of inspector properties (object, style,
  document, metadata, advanced).
- **Progressive disclosure** — revealing advanced controls only when needed.

## Editing concepts

- **Layer** — an element in a stack (Photo/Motion) with order, blend mode,
  and opacity; may be pixel, vector, text, adjustment, or fill.
- **Mask** — a grayscale channel controlling visibility; AI-assisted masks
  must remain editable as ordinary masks.
- **Adjustment layer** — a layer that applies a nondestructive tonal/color
  change.
- **Blend mode** — how a layer composites with layers below.
- **Clip** — a media segment placed on a timeline.
- **Track** — a timeline lane grouping related clips (video, audio role).
- **Timeline** — the time-based editing surface.
- **Keyframe** — a value-anchor in time; animation interpolates between
  keyframes.
- **Curve** (graph editor) — the interpolation shape between keyframes.
- **Parenting** — linking a layer's transform to another layer.
- **Composition** — a self-contained animated scene (Motion).
- **Compound clip / nested timeline** — a clip that contains a timeline.
- **Multicam** — synchronized multiple angles cut together.
- **Proxy** — a low-resolution stand-in for a media file, replaced by the
  original at output.
- **Transcode** — converting media between formats/codecs (Loom Encode).
- **Stem** — a per-role audio mix (e.g. drums, vocals) exported separately.
- **MIDI** — the note/control protocol used by Studio instruments.
- **Sample** — (a) an audio snippet; (b) a sample project in `loom-samples`.
  Context disambiguates.
- **Master slide / layout** — presentation templates for slides and their
  placeholders.
- **Guide** — a non-printing alignment reference on a canvas.
- **Rule-based structure** — shared across apps: styles, tables, sections,
  columns, headers/footers, footnotes/endnotes, cross-references, change
  tracking, comments (Writer).

## Data and interchange

- **Cell** — a spreadsheet coordinate (`CellRef`, 0-based row/column).
- **Formula** — an expression evaluated by the Sheets engine over the
  dependency graph.
- **Dependency graph** — the directed graph of formula precedents/dependents;
  topological evaluation with cycle detection.
- **Named range** — a named cell region usable in formulas.
- **Pivot table** — a re-aggregation of tabular data.
- **Import report** — the report a converter emits explaining substitutions
  or losses during import.
- **Interchange** — importing/exporting non-Loom formats (ODT, DOCX, XLSX,
  PPTX, PDF, CSV, …) with documented fidelity.
- **Linked asset** — external media referenced by a package, with relinking
  support.
- **Clipboard format** — the Loom typed payload format for cross-application
  copy/paste.

## Vision and media

- **Luma image** — a grayscale image interchange type used by providers.
- **Bounding box (BBox)** — an object-detection result rectangle.
- **OCR** — optical character recognition (capability).
- **Segmentation** — per-pixel labeling of an image (semantic/instance/
  salient-object/portrait).
- **Tracking** — following objects/points/planes across video frames.
- **Optical flow** — per-pixel motion between frames.
- **Transcription** — speech-to-text (local, provider-based).
- **Embedding** — a fixed-size vector representing image/video/audio content
  for similarity search.
- **Index** — the local search index over approved directories and opened
  projects (no network).

## Quality and process

- **Vertical slice** — one complete user workflow (UI, engine, persistence,
  tests, accessibility, visual QA) before broad feature expansion.
- **Quality gate** — a mandatory automated check (fmt, clippy, tests,
  visual, license, …). See `../loom-bootstrap/`.
- **Golden baseline** — a committed reference screenshot for visual
  regression.
- **Perceptual diff** — image comparison with documented tolerance.
- **Pseudolocale** — a locale variant that stresses layout expansion for
  localization testing.
- **MSRV** — minimum supported Rust version (1.80).
- **COMPATIBILITY.toml** — the suite compatibility manifest owned by
  `loom-bootstrap`.
- **Status words** — `COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`,
  `EXPERIMENTAL`, `SCAFFOLDED`, `NOT_STARTED`, `BLOCKED`.

## File extensions

`.loomdoc` (Writer) · `.loomtable` (Sheets) · `.loomdeck` (Present) ·
`.loomphoto` (Photo) · `.loommotion` (Motion) · `.loomvideo` (Video) ·
`.loomstudio` (Studio) · `.loomencode` (Encode) · `.loomplugin` (plugin
package).
