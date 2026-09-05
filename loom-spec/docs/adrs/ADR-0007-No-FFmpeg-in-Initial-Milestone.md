# ADR-0007 — No FFmpeg in the Initial Milestone

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

Video, Motion, Studio, and Encode eventually need audio/video demux,
decode, encode, and transcode. FFmpeg is the common choice but brings
licensing complexity (codec patents, GPL components), packaging burden,
and a large native dependency. The initial milestones have no media apps
implemented (`FEATURE_MATRICES.md` §6–§11).

## Decision

- **Do not add FFmpeg (or any media framework) in the initial milestone.**
- Initial media scope is limited to formats implementable with small,
  license-clean dependencies: **PNG, JPEG, WebP (via `image` — ADR-0006),
  and WAV** for audio (plus future FLAC via a permissive crate). This is
  documented in `DEPENDENCIES.md` of each affected repository.
- When media apps begin (Phase 4/5), evaluate FFmpeg or GStreamer
  formally: licensing and patent analysis, packaging strategy, sandboxing
  (parser isolation), maintenance, and a replacement strategy — per the
  root directive §5 — and record the outcome in an ADR/RFC
  (RFC-0012-Media-Framework is the designated place).
- Any codec with patent implications is isolated behind feature flags with
  documented package variants (root directive §18).

## Consequences

- Clean licensing posture in early releases; no heavyweight native deps;
  media-heavy features stay honestly NOT_STARTED in `FEATURE_MATRICES.md`.
- Media workflows (video editing, transcoding) are impossible until the
  framework decision lands — an accepted trade-off for the milestone
  order.

## Verification

- Dependency audit lists exactly the permitted codec crates per repo;
  release gate fails if FFmpeg or GPL codecs appear in the initial
  milestone builds without an ADR (`RELEASE_CRITERIA.md` §1.6).
