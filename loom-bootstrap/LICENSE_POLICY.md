# Loom Bootstrap — license policy

## Original Loom code

All code authored for the Loom suite (including this repository) is released
under the permissive dual license:

- MIT
- Apache-2.0

Every Cargo package in the suite declares `license = "MIT OR Apache-2.0"`.
New code must not change this without an RFC and a suite-wide license review.

## Dependency policy

- Every direct and transitive dependency must carry a license compatible with
  MIT/Apache-2.0 distribution, or be isolated behind a feature flag with
  documented packaging consequences.
- A dependency whose license cannot be identified is release-blocking
  (`BLOCKED` in COMPATIBILITY.toml).
- Codecs, system media frameworks, and model runtimes are license-sensitive:
  they must be feature-gated, documented in `../LICENSE_POLICY.md` per repo,
  and covered by the dependency audit before any release.
- Pinned versions are recorded in each repository's `Cargo.lock`; lockfiles are
  mandatory and part of the deliverable.

## Model and asset policy

- No model whose license forbids redistribution may be bundled.
- Local model packs (see loom-vision) must carry a manifest with license and
  provenance; installing a pack is a user action.
- All icons, illustrations, sample media, fonts, and sounds in the suite must
  be original or under verified permissive licenses. See the design bible for
  asset provenance requirements.

## Process

- Before release, run the dependency audit (cargo-deny or equivalent) and
  produce the suite-wide `LICENSE_REPORT.md` at the parent level.
- The bootstrap `verify-package.sh` step includes a license check of the
  extracted archive.

## Disclaimer

This document states policy; it is not legal advice. A factual dependency and
license audit must accompany every release.
