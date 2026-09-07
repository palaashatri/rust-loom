# RFC-0018 — Autosave and Recovery

- Status: **ACCEPTED (normative design; recovery browser NOT_STARTED)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-storage`, all applications

## Context

User work must survive crashes. The root directive requires transactional
writes, temporary-file safety, periodic autosave, an operation-journal
architecture, recovery after crashes, a recovery browser, explicit
recovered-document handling, storage limits, and privacy-preserving
diagnostics. No such subsystem exists yet in `loom-core`.

## Goals

- Transactional save semantics: a package on disk is always either the old
  or the new complete version, never partial.
- Periodic autosave with no visible interruption to editing.
- Crash recovery from autosave + operation journal, reconciled
  deterministically.
- A recovery browser (NOT_STARTED) for choosing among recovered states.

## Non-goals

- Version snapshots (future; tracked separately).
- Cloud backup of any kind.

## Proposed design

- **Transactional writes** (in `loom-storage`, partially implemented: path
  and transactional temp-file primitives, 7 tests): write to a temp file
  in the target directory, fsync, atomic rename over the destination, then
  fsync the directory. Never write in place.
- **Periodic autosave**: a `loom-jobs` background job serializes the
  document to a recovery snapshot at a configurable interval (default per
  app, e.g. 30 s) and after significant edits, throttled; the job yields
  to input and never blocks the UI thread.
- **Operation journal**: between autosaves, committed history transactions
  (`RFC-0007`) are appended to a journal in the package `recovery/`
  directory; journals are checksummed (`RFC-0006`) and bounded
  (size/rotation).
- **Recovery**: on open, the app detects autosave/journal presence; the
  newest valid revision wins; a diverging branch is preserved and offered
  in the recovery browser. Recovery opens read-only until the user
  explicitly saves.
- **Storage limits**: recovery snapshots are bounded (count + total size
  per project); eviction is documented and user-visible.
- **Diagnostics**: crash logs are local, readable, redactable, and contain
  no content payloads beyond what the user opts into
  (`PRODUCT_SPEC.md` §2.2).
- **Recovery browser (NOT_STARTED)**: a shared `loom-ui` surface listing
  recovered documents with timestamps and branch states.

## Alternatives

- **Save-in-place**: fast but crash-corrupting; rejected.
- **Full journal replay only**: unbounded journals; rejected in favor of
  periodic snapshots + bounded journal.

## Trade-offs

Atomic rename is not durable on all filesystems without directory fsync;
documented per-platform behavior and covered by tests on the CI
filesystem. Journaling duplicates content between snapshots; bounded by
rotation. Autosave IO is background by construction
(`RFC-0008-Async-Job-Framework.md`).

## Security

Recovery files are packages (checksums, path rules); recovery must never
overwrite the user's existing file without explicit save; temp files use
safe names and permissions.

## Performance

Autosave must never visibly interrupt editing (target: sub-frame UI
impact); snapshot serialization is a low-priority cancellable job;
journaling is batched.

## Compatibility

Recovery entries are versioned with the package format; a recovery entry
of an unknown version opens read-only with an explanatory error.

## Migration

No recovery files exist yet; format is new in this milestone.

## Testing

- Crash simulations: kill mid-save, mid-edit; assert recovery reconciles.
- Transactional write tests: failures at every step leave the old file
  intact.
- Journal rotation and storage-limit tests.
- Integration tests for the full create→edit→crash→recover→save cycle
  (`IMPLEMENTATION_GUIDE.md` §5).

## Open questions

- Autosave interval defaults per application (decided at each app's
  integration; recorded in app docs).

## Final status

ACCEPTED. Transactional primitives implemented in `loom-storage`;
autosave, journal, recovery browser NOT_STARTED.
