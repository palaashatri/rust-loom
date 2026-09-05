# RFC-0011 — Model-Pack Format

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-vision`

## Context

Loom must not require online model downloads. Users install models from
files. The root directive defines the required contents of a model pack:
manifest, model files, checksums, license information, provenance,
capability declarations, runtime requirements, preprocessing and
postprocessing definitions, test vectors, optional sample inputs, version
and compatibility range.

## Goals

- A local model-pack format installable from a file, verified locally.
- Validation strong enough that an invalid or tampered pack is rejected
  before any model file is used.
- No network access anywhere in the install/validate path.
- License and provenance transparency for every model.

## Non-goals

- A remote marketplace or account system.
- Auto-download of missing models.

## Proposed design

Mirrors `loom-vision-core/model_pack.rs` (implemented, 24 tests):

- A model pack is a directory containing `manifest.json` and the model
  files it references (ZIP distribution optional, unpacked to a store
  directory).
- Manifest fields: pack id/version, capability declarations
  (`CapabilityId` + input/output schema versions), model files with
  SHA-256 checksums, license text/identifier, provenance (source, training
  data summary, publication), runtime requirements (backend, memory,
  format), preprocessing/postprocessing definitions, test vectors with
  expected outputs, optional sample inputs, format version and
  compatibility range.
- Installation validation: parse manifest; verify every referenced file
  exists; verify checksums; reject path traversal (paths must stay inside
  the pack root); verify license field non-empty; record pack metadata in
  the provider registry.
- A pack's test vectors run at install time when practical, so a broken
  pack is rejected before use.
- Re-installation with a different checksum set is rejected or requires
  explicit user confirmation (no silent overwrite).

## Alternatives

- **Model downloads in-app**: violates offline-first and user-control
  principles; rejected.
- **Manifest-less model folders**: undetectable corruption and no license
  hygiene; rejected.

## Trade-offs

Checksum verification costs IO at install; acceptable (install is a
background job, `RFC-0008`). Running test vectors at install costs time but
catches incompatible formats early — worth it.

## Security

Checksums prevent tampering; path traversal rejection prevents pack-driven
file writes outside the store; license field enforcement prevents
unknowingly installing non-redistributable models (root directive §4.3);
the reader is a fuzz target (`IMPLEMENTATION_GUIDE.md` §5).

## Performance

Validation is streaming and memory-bounded; packs can be large, so
validation runs as a cancellable job.

## Compatibility

Pack format version and capability compatibility range are declared in the
manifest; a pack declaring an unsupported range is rejected with a clear
message (`COMPATIBILITY_POLICY.md` §2).

## Migration

Packs are forward-declared: readers ignore unknown optional manifest
fields with a warning; unknown required fields reject.

## Testing

- Validation tests: missing files, bad checksums, traversal paths,
  malformed manifests (24 tests exist).
- Install/remove round trips; duplicate/corrupt pack handling; version
  range rejection.
- Fuzz target for the manifest parser.

## Open questions

- ZIP vs directory distribution for the first shipped packs (directory
  store + optional ZIP wrapper; resolved at packaging phase).

## Final status

ACCEPTED. Format and validator implemented in `loom-vision-core`
(`model_pack.rs`, 24 tests).
