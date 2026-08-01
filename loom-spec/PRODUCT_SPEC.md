# Loom Product Specification

## 1. Product mission

Loom is an original, open-source, professional creative suite for desktop
computers: word processing, spreadsheets, presentations, photo editing, motion
graphics, video editing, audio production, and media transcoding — one calm,
cohesive product family.

Loom combines a minimal and calm interface, professional depth, excellent
typography, direct manipulation, smooth meaningful animation, high-performance
native execution, local-first storage, offline-first operation, local computer
vision and machine learning, strong accessibility, documented open formats,
and extensible sandboxed plugins.

Loom must be an **original** product. No proprietary source, icons, layouts,
templates, sounds, sample media, or branding from other creative suites may be
copied. Loom studies interaction principles and professional workflows, then
implements an independent design and visual identity (see
`../loom-design-bible/`).

## 2. Non-negotiable product principles

These bind every application and every release. A violation is release-blocking.

### 2.1 Local first

Every core workflow must function without an internet connection: create,
open, edit, save, export, search local files, run supported computer-vision
features with installed local models, recover unsaved work, render projects,
transcode media, install local plugin packages, and read bundled help.
No mandatory account, no required cloud service. Cloud synchronization is out
of scope entirely; the architecture must not depend on it.

### 2.2 Privacy

No telemetry by default, no advertising, no hidden network requests, no user
profiling, no remote crash upload, no remote model inference, no automatic
upload of documents or media, no mandatory update checks. Local diagnostic
logs must be understandable, redactable, and under user control. Core
workflows must be verified in a network-disabled container (see
`../loom-bootstrap/`).

### 2.3 AI is optional

All conventional editing features remain fully usable without any AI model.
AI/computer vision enhances tools, never replaces them: manual masks without
segmentation, manual subtitles without transcription, conventional document
editing without a language model, formulas without an AI model, manual
keyframing without tracking.

### 2.4 User ownership

Documented, versioned file formats (see `FILE_FORMAT_FAMILY.md`). Users can
keep files permanently, back up with ordinary filesystem tools, inspect
package contents, export to common formats, move projects between computers,
and recover data from partially damaged packages where practical.

### 2.5 Performance

Responsive under professional workloads; long work is asynchronous,
cancellable, observable, and recoverable. Never block the UI thread with media
decoding, file parsing, model inference, autosave, export, thumbnail
generation, waveform generation, proxy generation, font scanning, plugin
discovery, or search indexing. Budgets and tiers: see
`../loom-design-bible/PERFORMANCE.md` and `RELEASE_CRITERIA.md`.

### 2.6 Accessibility

Accessibility is a release requirement: complete keyboard navigation, visible
focus, screen-reader labels, logical focus order, high-contrast operation,
scalable UI, reduced-motion mode, non-color status indicators, configurable
shortcuts, accessible error reporting, and accessible canvas/timeline
navigation where technically possible. Design authority:
`../loom-design-bible/ACCESSIBILITY.md`.

## 3. The applications

All applications are Rust + Slint desktop apps in separate repositories.
Status of every capability is in `FEATURE_MATRICES.md`; the phase plan is in
`ROADMAP.md`. As of this revision, engines exist for Writer and Sheets;
no application has a GUI yet (Slint UI is the documented follow-on per
`docs/rfcs/RFC-0003-Slint-Integration-Model.md`).

### 3.1 Loom Writer (`../loom-writer/`)

Professional word processor and page-layout application: rich text editing,
continuous and paginated modes, paragraph/character styles, page styles,
master pages, sections, columns, tables, lists, footnotes/endnotes, headers
and footers, automatic tables of contents, citations, cross-references,
change tracking, comments, structured navigation, shapes and media, text
wrapping, anchored and floating objects, templates, form fields, mail merge,
print layout, PDF/EPUB export, DOCX/ODT/Markdown/plain-text import and export,
OCR-assisted scanned-document import, local document search, crash recovery
and version snapshots. Implemented now: headless document model, `.loomdoc`
save/load, Markdown and plain-text export, CLI.

### 3.2 Loom Sheets (`../loom-sheets/`)

Professional spreadsheet and data-analysis application: large virtualized
grid, formula engine, dependency graph, incremental recalculation, named
ranges, structured tables, sorting/filtering, conditional formatting,
validation, charts, pivot tables, grouping, freeze panes, multiple sheets,
comments, rich cell formatting, dates/times/durations/currencies/units,
CSV/TSV import-export, XLSX/ODS where feasible, local data connectors,
formula auditing, error tracing, goal seeking, statistics/financial
functions, photograph-to-table import through Loom Vision, receipt/invoice
extraction. No arbitrary network queries from cells. Implemented now: formula
engine (tokenizer, parser, dependency graph with cycle detection), CSV
import/export, `.loomtable` JSON round-trip, CLI.

### 3.3 Loom Present (`../loom-present/`)

Presentation authoring: slide canvas, themes, master slides, layouts, guides,
alignment/distribution, smart grouping, text/tables/charts/shapes/images/
audio/video/equations, speaker notes, transitions, object animations,
timeline-based animation editing, presenter display, rehearsal timing,
presentation recording, PDF/video export, PPTX/ODP where feasible, local
presenter background removal and tracking where supported. Status: not started.

### 3.4 Loom Photo (`../loom-photo/`)

Nondestructive image editing: layer-based editing, pixel/vector/text/
adjustment/fill layers, groups, masks, clipping, blend modes, nondestructive
filters, RAW development, color management, ICC profiles, curves, levels,
white balance, exposure, selective color, gradients, brushes, clone/heal,
content-aware repair, crop/perspective, transform/warp, liquify, panorama,
HDR, batch processing, PSD where practical, OpenRaster, TIFF/PNG/JPEG/WebP/
AVIF/EXR, semantic subject selection, background removal, portrait matting,
object-aware masks, local inpainting/super-resolution where compatible local
models are installed. All AI-assisted selections must remain editable as
ordinary masks. Status: not started.

### 3.5 Loom Motion (`../loom-motion/`)

Motion graphics, animation, compositing: layer-based compositions, timeline,
keyframes, curve/graph editor, parenting, constraints, masks, vector shapes,
text animation, image sequences, video layers, audio reference tracks,
cameras, lights where supported, 2.5D scenes, transform hierarchy, blend
modes, filters/effects, particles, replicators, behaviors, motion paths,
chroma key, rotoscoping, planar/point/object tracking through Loom Vision,
stabilization, optical-flow retiming, motion blur, render queue, template
export for Loom Video and Loom Present. Status: not started.

### 3.6 Loom Video (`../loom-video/`)

Nonlinear video editor: media library, events/projects, metadata, keyword
collections, favorites/rejects, proxies, background transcoding, timeline
editing, connected-clip editing model, track compatibility where required,
ripple/roll/slip/slide/blade/trim/overwrite, compound clips, nested timelines,
multicam, synchronized clips, audio lanes and roles, transitions, titles,
generators, effects, keyframes, speed changes, optical-flow retiming,
stabilization, color correction, scopes, LUTs, HDR-aware processing, captions,
local transcription, scene detection, subject tracking, automatic reframing,
background removal where feasible, export through Loom Encode, XML/EDL/AAF
interchange where feasible and legally appropriate, autosave, project
backups, media relinking, offline media workflows. Status: not started.

### 3.7 Loom Studio (`../loom-studio/`)

Digital audio workstation with two progressive-disclosure workspaces over one
engine.

*Quick Workspace*: loop browser, simplified tracks, software instruments,
audio and MIDI recording, procedural rhythm tools with original
implementations and assets, smart controls, chord/scale assistance, basic
effects, easy arrangement, guided mixing, simple export.

*Pro Workspace*: multitrack audio, MIDI, piano roll, drum editor, score editor
where feasible, automation, comping, take folders, nondestructive time
editing (original implementation), pitch editing, mixer, buses, sends,
sidechains, plugin hosting, instrument hosting, sample editing, looping,
markers, tempo maps, meter changes, surround architecture where feasible,
mastering tools, loudness metering, local source separation/speech-noise
enhancement where compatible local models are installed, beat/tempo/key/
transient analysis. Linux plugin standards supported with sandboxing where
practical. Status: not started.

### 3.8 Loom Encode (`../loom-encode/`)

Media transcoding and delivery: batch queue, source inspection, presets,
custom encoding settings, video filters, audio channel mapping, subtitle
handling, frame-rate conversion, scaling, cropping, color-space conversion,
HDR metadata handling, image sequences, audio-only output, multi-destination
jobs, job dependencies, retry/recovery, pause/resume, hardware acceleration
when available with software fallback, optional watch folders, CLI operation,
integration with Video/Motion/Present/Photo/Studio, deterministic preset
files, local content-aware analysis, perceptual quality metrics. Status:
not started.

## 4. Loom Vision

A shared, local-first computer-vision and perception platform
(`../loom-vision/`), not hard-wired to any model, vendor, runtime, or
hardware backend. Capability areas: document and text (OCR, layout-aware OCR,
handwriting provider interface, boundary detection, perspective correction,
dewarping, table detection/structure, form-field detection, barcode/QR,
math-expression provider interface, PDF page analysis, reading order,
language detection, local full-text indexing); images (classification,
object detection, segmentation, salient-object/portrait segmentation, alpha
matting, face detection/landmarks/quality, body/hand pose, depth provider
interface, embeddings, similar/duplicate search, scene classification,
captioning/super-resolution/denoising/inpainting provider interfaces); video
(object/point/planar tracking, optical flow, shot boundaries, scene grouping,
subject tracking, camera motion, stabilization analysis, reframing, subtitle
timing, speech transcription, speaker diarization provider interface,
thumbnails, embeddings, semantic search); audio (speech recognition, VAD,
noise classification, beat/tempo/key/transient detection, source-separation/
speech-enhancement provider interfaces, embeddings).

Providers are selected through capability traits (`CapabilityProvider`,
`ProviderDescriptor`, `RunContext` — see `../loom-vision/ARCHITECTURE.md`),
never model-specific APIs. CPU reference providers exist today (QR decode,
image statistics); OCR, segmentation, tracking, transcription, and all model
packs are not yet implemented. Model packs are installed from files, verified
by checksum, never downloaded automatically.

## 5. Cross-application workflows

Specified end-to-end in `CROSS_APP_WORKFLOWS.md`; implemented state there.

- **Photo → Sheets table import**: Loom Vision detects a table region in a
  photo, extracts cell structure, and imports rows into a Sheets workbook.
  Requires Vision table detection (NOT_STARTED).
- **Motion templates → Video and Present**: Motion exports composition
  templates consumed by Video titles/generators and Present animations.
  Template format is part of the file-format family (NOT_STARTED).
- **Encode integration**: Video, Motion, Present, Photo, and Studio hand
  export jobs to Loom Encode through shared job and package contracts
  (NOT_STARTED).
- **Studio → Video**: Studio exports stems (per-role audio) into Video
  projects; Video exports audio to Studio for mixing (NOT_STARTED).
- **Shared clipboard**: cross-application copy/paste via a Loom clipboard
  format carrying typed payloads (content, selection masks, assets)
  (NOT_STARTED).
- **Linked assets**: shared external media referenced by multiple packages
  with relinking (NOT_STARTED).

## 6. Out of scope

- Cloud accounts, cloud sync, remote collaboration, analytics, remote model
  APIs, mandatory update checks, cloud-backed storage (now or "for later").
- A future cloud capability may exist only as an optional external plugin
  after the local platform is complete.
- Network-driven formula queries in Sheets cells.
- Proprietary formats, assets, and behavior copied from other products
  (see `../loom-design-bible/` anti-patterns).
