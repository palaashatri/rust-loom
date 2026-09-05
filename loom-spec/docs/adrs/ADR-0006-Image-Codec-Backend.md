# ADR-0006 — `image` Crate as the Image Codec Backend

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

Loom apps need PNG/JPEG/WebP (and later TIFF/AVIF/EXR/OpenRaster) codecs.
Writing codecs from scratch is out of scope; the backend must be
maintained, Linux-supporting, and license-compatible.

## Decision

- Use the Rust **`image`** crate as the image codec backend for the
  initial milestone (PNG, JPEG, WebP, GIF basics; decode and encode),
  wrapped behind a future `loom-core` media API (`loom-media`,
  NOT_STARTED) so the backend stays swappable.
- Verify current official docs, maintenance status, Linux support,
  license (MIT/Apache-2.0), and thread-safety before pinning; record the
  pinned version and the verification in `DEPENDENCIES.md` per the
  dependency policy (root directive §5).
- TIFF, AVIF, EXR, and RAW support are future milestones via feature
  flags; PSD/OpenRaster require dedicated work (NOT_STARTED).

## Consequences

- Fast start with a well-maintained backend; `image` pulls codec crates
  that may vary in maintenance — feature-gate codecs so a weak optional
  codec cannot block the core build, and document each codec in the
  dependency audit.
- Encoder/decoder behavior differences are isolated behind `loom-media`'s
  API; swapping later is contained.

## Verification

- Round-trip tests (decode→encode→decode) per format; golden-file image
  tests; the wrapped API has fuzz targets for decoders
  (`IMPLEMENTATION_GUIDE.md` §5).
