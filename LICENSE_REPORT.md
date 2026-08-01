# Loom License Report

Status: initial audit complete for declared dependencies; per-file headers pending bulk pass.

## Policy

- All original Loom code is intended for permissive dual licensing: MIT AND Apache-2.0.
- No dependency that forces an incompatible license (e.g. GPL/AGPL) is allowed without
  explicit documentation and isolation behind feature flags.
- No model files are bundled in this delivery (redistribution licensing not verified for
  any model); the model-pack format requires a license declaration per pack.
- Original assets in this repo (icons, samples, fixtures) are project-created.

## Direct dependency classes (from cargo lockfiles)

- Rust crates: overwhelmingly MIT / MIT OR Apache-2.0 / Apache-2.0 / BSD-3 / ISC / Zlib.
  Notable: Slint 1.17.1 (MIT OR Apache-2.0 with GPL-commercial exception; royalty-free
  for open-source use — see DEPENDENCY_REPORT.md).
- No GPL/AGPL crates in the build graph at the pinned lockfile versions (verified via
  `cargo deny`-style manual audit of the lockfile; automated SBOM tooling is a TODO).
- FFmpeg/media libraries: NOT yet introduced; ADR pending before any media backend is added.

## Deliverables

- `DEPENDENCY_REPORT.md` — crate inventory status and audit method.
- Per-crate `LICENSE`/`NOTICE` files: present in loom-core crates (MIT OR Apache-2.0 headers
  added during crate creation); full per-file header pass is a TODO.
- Codec patent/redistribution notes: none applicable yet (no codecs in tree).

## Honest status

The legal position is documented policy, not a legal guarantee. An automated license
audit (`cargo-deny` or equivalent) must run in CI before the first external release; the
lockfile has been manually reviewed for GPL/AGPL/Copyleft crates and none are present.
