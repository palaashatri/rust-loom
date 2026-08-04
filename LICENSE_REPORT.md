# Loom — License Report

## Position

All original Loom code is prepared for permissive dual licensing under:

- MIT
- Apache-2.0

Declared per workspace via `workspace.package` (`license = "MIT OR Apache-2.0"`).
`loom-bootstrap/LICENSE_POLICY.md` is the governing policy document.

## Policy enforcement

- No dependency may force an incompatible license across the suite without
  explicit documentation and isolation.
- Optional system media frameworks and codecs are isolated behind features
  and documented package variants (see `loom-bootstrap/LICENSE_POLICY.md`).
- All original assets (icons, samples, sounds, instrument presets) are
  created in-project or under verified permissive licenses; no proprietary
  asset sets are bundled.
- Model packs are user-installed from files; models whose licenses do not
  permit redistribution are never bundled.
- `Loom-Complete.zip` ships with license notices per repository.

## Audit status (2026-08-04)

- `loom-bootstrap/LICENSE_POLICY.md` and per-repo `LICENSE_POLICY.md` exist.
- A full dependency-license inventory report is maintained as
  `loom-bootstrap/DEPENDENCIES.md` and regenerated with the dependency audit;
  the latest audit pass completed without license-incompatibility findings.
- Codec patent/redistribution notes are tracked with the media backend
  decision records (FFmpeg usage is runtime-discovery based, isolated behind
  the encode media engine).

## Caveat

This report states the licensing position and audit trail. It is not legal
advice; a formal legal review of every transitive dependency remains
recommended before distribution.