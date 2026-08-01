# Loom Cross-Application Workflows

End-to-end workflows that span applications. Each workflow names the shared
contracts it relies on and its current status (see `FEATURE_MATRICES.md`
§12). All are local-first; none require a network. Statuses: most workflows
are NOT_STARTED because the consuming applications do not exist yet; the
contracts they depend on are specified here so implementation can proceed
independently.

## 1. Photo → Sheets: photograph-to-table extraction

**Status: NOT_STARTED** (needs Vision table detection + Sheets import path)

1. User opens a photo of a receipt, invoice, or data table in Loom Photo and
   selects "Extract table…".
2. Photo submits the image to a Loom Vision job with capability
   `table_detection`; the job is cancellable with progress
   (`loom-jobs`, `RunContext`).
3. A provider (future; model-pack based) returns table regions and cell
   structure (rows/columns/confidence). No provider installed → clear
   "no compatible provider" state; the user can still frame the table
   manually (AI is optional, `PRODUCT_SPEC.md` §2.3).
4. Photo opens the extracted grid as an interactive preview; edits apply
   before transfer (merge cells, drop columns, type fixes).
5. On confirm, Photo hands the grid to Loom Sheets through the shared
   clipboard format (below) or a direct "open in Sheets" command.
6. Sheets inserts it as a structured table. The extracted data and the
   source photo remain linked metadata; both packages are unchanged on disk
   until the user saves.

Contracts: Vision capability `table_detection` + provider interface;
Sheets import command; shared clipboard table payload; job + progress.

## 2. Video export via Loom Encode

**Status: NOT_STARTED**

1. In Loom Video, the user clicks "Send to Encode" (or Encode CLI with a
   job descriptor).
2. Video writes a `.loomencode` job package: sources (media references,
   proxies resolved to full-res), timeline render spec, output presets,
   destinations, dependencies.
3. Encode renders/transcodes as cancellable, resumable, retryable jobs with
   per-job progress; hardware acceleration when available with software
   fallback.
4. Completion surfaces back in Video via the job's completion state; output
   paths are user-visible.
5. Encode's CLI mode supports the same descriptor for scripting
   (deterministic preset files).

Contracts: `.loomencode` package schema (`FILE_FORMAT_FAMILY.md`),
loom-jobs persistence, render API in the future renderer.

## 3. Motion templates reused in Present (and Video)

**Status: NOT_STARTED**

1. A Motion composition (or part of it — keyframed title, animated badge)
   is exported as a template package (`.loommotion` with a
   `template` marker in the manifest, or the future dedicated template
   package).
2. The template declares editable properties (text, color, duration,
   keyframes exposed as parameters).
3. In Loom Present, the user inserts the template; Present hosts the
   animation via the shared animation contract and exposes the declared
   parameters in the inspector.
4. Video consumes the same template for titles/generators.
5. Templates are versioned with the package format; incompatible template
   versions are rejected with an import report.

Contracts: template marker in manifest, parameterized animation contract,
shared timeline/keyframe model (Phase 6 integration).

## 4. Studio stems into Video

**Status: NOT_STARTED**

1. In Loom Studio, the user exports stems (per-role audio: dialogue, music,
   effects, ambience) as external audio files plus a `.loomstudio` stem
   descriptor.
2. In Loom Video, "Import Studio stems" attaches the stems as audio lanes/
   roles on the timeline, time-aligned by the descriptor.
3. Round trip: Video sends audio regions to Studio for mixing or
   mastering; Studio returns a new mixdown, which Video relinks.

Contracts: stem descriptor format (part of `.loomstudio` schema), audio
role model (`TERMINOLOGY.md`), linked-asset relinking.

## 5. Shared clipboard formats

**Status: NOT_STARTED**

- One Loom clipboard format carrying typed payloads with a MIME-like type
  tag: rich text, table/grid, image with layers, selection masks, media
  references, vector shapes, animation snippets.
- Copying in any app writes the Loom payload plus standard fallback types
  (plain text, PNG) for external targets.
- Paste inside Loom negotiates the richest type the target supports; the
  target converts with an import report where fidelity is limited.
- Payloads are validated before acceptance (malformed clipboard input is a
  fuzz target per `IMPLEMENTATION_GUIDE.md`).

Contracts: clipboard payload schema (loom-clipboard crate, future),
per-app converters.

## 6. Linked assets

**Status: NOT_STARTED**

- Media-heavy packages (Photo, Motion, Video, Studio) reference assets by
  relative path + checksum in the manifest `media` section
  (`FILE_FORMAT_FAMILY.md` §7).
- Missing media shows an offline placeholder; the relink workflow searches
  user-chosen folders and matches by checksum, then updates the package.
- Linked assets shared by multiple packages are never duplicated on save;
  packages store references, keeping user-owned folders as the single copy.

## 7. Shared workspace conventions

- Commands, shortcuts, and the inspector behave identically across apps
  (Phase 6): same command palette, same selection model, same drag and drop.
- The file dialogs, recovery browser, and error reporting are shared
  components from `loom-ui`.

## Implementation order

Phase 6 of `ROADMAP.md` implements §5 and §6 first (no vision dependency),
then §1 (needs Vision), then §2–§4 as the apps exist. Each workflow needs
its own integration test and, where visual, a golden baseline.
