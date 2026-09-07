# Loom File Format Family

This document is the authoritative specification for Loom's document package
formats. The implemented schema lives in `loom-core/crates/loom-package`
(`manifest.rs`, `zip.rs`); when the crate and this document disagree, the
crate is the implementation and this document must be corrected in the same
effort.

## 1. Package container

Every Loom document is a ZIP archive with one extension per application:

| Application | Extension | PackageKind |
|---|---|---|
| Writer | `.loomdoc` | `document` |
| Sheets | `.loomtable` | `table` |
| Present | `.loomdeck` | `deck` |
| Photo | `.loomphoto` | `photo` |
| Motion | `.loommotion` | `motion` |
| Video | `.loomvideo` | `video` |
| Studio | `.loomstudio` | `studio` |
| Encode | `.loomencode` | `encode` |

A package contains:

```text
manifest.json       versioned manifest (required, first entry)
content/            document content in application-defined form
assets/             embedded media (optional; see §7)
previews/           preview images (optional)
metadata/           user metadata, keywords, collections (optional)
history/            undo/redo journal snapshots (optional)
recovery/           autosave and crash-recovery snapshots (optional)
```

`manifest.json` carries at minimum:

- `format_version` — schema version of the package (see §3);
- `id` — stable identifier (UUID) and `revision` — monotonic edit revision;
- `created` / `modified` timestamps (ISO 8601, UTC);
- `app` — creating application and version (informational);
- `checksums` — SHA-256 per entry, plus a package-level digest;
- `entries` — table of entry paths, sizes, kinds, and MIME types.

PackageKind, schema versioning, checksums, and entry types are implemented in
`loom-package`; the exact JSON keys follow the crate's `Manifest` type.

## 2. Integrity and corruption handling

- Every entry is checksummed (SHA-256); a mismatch marks the package
  corrupted.
- A partially damaged package can still open if `manifest.json` parses and the
  `content/` entries verify; missing `assets/` degrades gracefully with a
  relink prompt (media-heavy apps) or embedded fallback.
- Readers must validate checksums before use and report precisely which
  entries failed.
- Packages failing manifest validation are rejected with a structured error;
  the application must never write over a corrupt package without user
  confirmation.

## 3. Schema versioning and migration

- `format_version` is an integer, currently `1`. Bump only on breaking
  schema change.
- Version `1` packages open on all future versions with a forward-compatible
  strategy: readers ignore unknown optional entries and unknown manifest
  fields, and warn.
- Migration: a version `N` reader migrates `format_version < N` packages in
  memory and saves the current version; the original file is only replaced
  after a successful save. Unsupported future versions are opened read-only
  with an explanatory error.
- Every schema change requires a migration test, a fixture, and an RFC/ADR
  note (see `RFC-0006-File-Package-Format.md`).

## 4. Security limits (archive-bomb protection)

ZIP readers must enforce, before extraction:

- maximum entry count (e.g. 10,000);
- maximum uncompressed total size (e.g. 4 GiB default, configurable);
- maximum per-entry size (e.g. 1 GiB);
- maximum compression ratio (e.g. 1000:1) to defeat zip bombs;
- path traversal rejection: every entry path must resolve inside the package
  root (no `..`, no absolute paths, no symlink escapes);
- duplicate entry rejection.

Violations fail package open with a security error, never partial extraction.
These limits are enforced in `loom-package`'s ZIP reader and covered by
fuzz targets (see `IMPLEMENTATION_GUIDE.md`).

## 5. Content serialization

Each application defines its content inside `content/`:

- **Writer** — block/paragraph model with text runs and styles, serialized as
  JSON (implemented: `loom-writer-core` rich-text blocks; Markdown and
  plain-text export in addition to `.loomdoc` save/load).
- **Sheets** — workbook JSON: sheets, cells, values, formulas, styles
  (implemented: `sheet_to_json`/`sheet_from_json` round-trip in
  `loom-sheets-core`; CSV import/export).
- **Present/Photo/Motion/Video/Studio/Encode** — application-defined JSON
  models, to be specified by each application's `FILE_FORMAT.md` when their
  vertical slices begin.

Deterministic serialization: stable key order, no timestamps inside content
JSON, fixed float formatting — required for reproducible archives and
golden-file tests.

## 6. History and recovery entries

- `history/` holds optional undo/redo journal snapshots for large projects;
  disk-backed history is NOT_STARTED (`RFC-0007-Undo-and-Transaction-System.md`).
- `recovery/` holds autosave snapshots and operation journals written
  transactionally (temp file + atomic rename); the recovery browser is
  NOT_STARTED (`RFC-0018-Autosave-and-Recovery.md`).
- Recovery entries must never be modified in place; readers reconcile
  newest-valid-wins.

## 7. Embedded vs external media policy

Per application, stated as default policy:

| Application | Default | Notes |
|---|---|---|
| Writer | embed | images and fonts embedded; large assets may be external |
| Sheets | embed | no large media expected |
| Present | embed | media embedded; video may be external |
| Photo | external by default | embedded optional; RAW sidecars external |
| Motion | external | media referenced, relink supported |
| Video | external | proxy and full-res referenced; relink required |
| Studio | external | audio files referenced |
| Encode | n/a | job descriptors reference sources; outputs external |

External references are stored as relative paths with a `media` manifest
section; missing media degrades to offline placeholders with a relink
workflow (NOT_STARTED). Linked-asset workflows are specified in
`CROSS_APP_WORKFLOWS.md`.

## 8. Testing requirements

- Round-trip property tests (save → load → save equality);
- schema fixtures and a golden compatibility corpus;
- fuzz targets for the package reader and manifest parser;
- corruption tests (truncation, bit flips, missing entries, bad checksums,
  zip bombs, path traversal);
- migration tests for every schema version bump.
