# RFC-0006 — File Package Format

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-package`, all applications

## Context

Loom documents must be user-owned: documented, versioned, inspectable,
backup-able with ordinary filesystem tools, portable between computers, and
partially recoverable. The root directive specifies a ZIP-based package
container with `manifest.json`, `content/`, `assets/`, `previews/`,
`metadata/`, `history/`, `recovery/`.

## Goals

- One container design for all eight extensions (`.loomdoc` … `.loomencode`)
  with per-app content schemas.
- Versioned schema with forward-compatibility strategy and migration rules.
- Corruption detection via per-entry SHA-256 checksums.
- Security limits against archive bombs and path traversal.
- Deterministic serialization for reproducible archives and golden tests.

## Non-goals

- A database or transactional filesystem as the primary store.
- Cloud storage of any kind.

## Proposed design

The design mirrors `FILE_FORMAT_FAMILY.md` and is implemented in
`loom-core/crates/loom-package` (`manifest.rs`, `zip.rs`):

- ZIP archive; `manifest.json` first; entries grouped under `content/`,
  `assets/`, `previews/`, `metadata/`, `history/`, `recovery/`.
- Manifest: `format_version` (integer, currently 1), stable `id` +
  `revision`, `created`/`modified` timestamps, `app` origin, per-entry
  SHA-256 checksums, entry table (path, size, kind, MIME).
- Reader enforcement: entry-count/size/compression-ratio limits, path
  traversal rejection, duplicate-entry rejection, checksum verification
  before use.
- Forward compatibility: readers ignore unknown optional entries and
  unknown manifest fields with a warning; future `format_version` opens
  read-only with an explanatory error.
- Migration: older versions migrate in memory and save current; original
  file replaced only after a successful save.
- Media policy: embedded vs external per application
  (`FILE_FORMAT_FAMILY.md` §7); external references are relative paths with
  checksums for relinking.
- Deterministic serialization: stable key order, no content timestamps,
  fixed float formatting.

## Alternatives

- **Plain directories**: simpler to inspect, but not a single file; harder
  to move/backup atomically. Rejected for the default; large media may use
  package directories per the root directive §9 trade-off note.
- **SQLite containers**: transactional, but not human-inspectable and more
  complex; rejected for documents.
- **Custom binary containers**: rejected — ZIP is inspectable and
  universally supported.

## Trade-offs

ZIP is not crash-atomic; mitigated by write-new-then-rename semantics and
the `history/`/`recovery/` journals (RFC-0018). Checksums add size/compute
overhead; acceptable for professional documents.

## Security

Limits above; ZIP-slip protection is mandatory; malformed input is fuzz
targeted (`IMPLEMENTATION_GUIDE.md` §5); extraction never follows symlinks.

## Performance

Readers stream entries and validate lazily; opening must not load the full
package into memory; previews load independently of content.

## Compatibility

`format_version` is the compatibility contract; bumping requires migration
tests, fixtures, a golden corpus, and `COMPATIBILITY.toml` entries
(`FILE_FORMAT_FAMILY.md` §3, `COMPATIBILITY_POLICY.md` §5).

## Migration

Implemented as specified in `FILE_FORMAT_FAMILY.md` §3; version-1 packages
are the current baseline.

## Testing

Round-trip property tests; corruption tests (truncation, bit flips, missing
entries, bad checksums, zip bombs, traversal); migration tests; fuzz
targets for reader and manifest parser; golden compatibility corpus
(`FILE_FORMAT_FAMILY.md` §8).

## Open questions

- Maximums above are initial defaults; tune via ADR if real packages
  exceed them.

## Final status

ACCEPTED. Implemented in `loom-package` (19 tests). `FILE_FORMAT_FAMILY.md`
is the normative reference.
