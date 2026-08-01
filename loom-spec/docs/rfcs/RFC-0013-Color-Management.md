# RFC-0013 — Color Management

- Status: **ACCEPTED (normative design; sRGB first, ICC/BTO NOT_STARTED)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-color`, all applications

## Context

A professional creative suite must handle color correctly: ICC profiles,
display profiles, working spaces, conversion, linear-light processing,
HDR, wide color, rendering intents, soft proofing, image/video/print
differences, GPU precision, and deterministic reference tests
(`PRODUCT_SPEC.md` §2.5, root directive §10.6). `loom-color` exists with
basic types and conversions (8 tests).

## Goals

- One color foundation crate used by every application and the future
  renderer.
- Correct by construction: conversions are tested against reference
  vectors; no gamma confusion.
- Deterministic reference tests that run headless (no GPU dependency).
- An explicit path to ICC profile support and beyond-the-obvious
  (BTO) working spaces.

## Non-goals

- ICC/BTO support in the initial milestone.
- HDR tone-mapping pipelines in the initial milestone.

## Proposed design

- `loom-color` provides: color value types (RGB, RGBA, linear RGB, sRGB,
  HSV/HSL as UI helpers), working-space tagged values (never raw
  unlabeled floats), conversion functions between the tagged encodings,
  and a `ColorProfile` abstraction that starts as `sRGB` only.
- Pipeline principle: **linear-light processing** — compositing, blending,
  filtering, and rendering operate in linear light; sRGB is the exchange
  encoding. All UI surfaces and document content are sRGB-tagged in the
  initial milestone.
- Where a profile is unknown or untagged, default to sRGB with an
  explicit "assumed sRGB" marker in diagnostics (never silent guessing
  that changes output).
- ICC support (NOT_STARTED): `ColorProfile` extends to parsed ICC
  profiles, display-profile queries, working-space setup, rendering
  intents, and soft proofing. The architecture reserves the seams (tagged
  values, profile object, conversion API) so ICC lands without rewriting
  callers.
- BTO (beyond the obvious) and wide-gamut work spaces (NOT_STARTED) build
  on the same seams; HDR is a later milestone with its own RFC.

## Alternatives

- **Delegate all color to a C library (lcms2)**: mature and correct, but
  adds a native dependency; keep as a possible backend behind
  `ColorProfile` if ICC work proves large (decision at ICC milestone).
- **sRGB-only forever**: insufficient for professional Photo/Video;
  rejected as an endpoint, accepted as the initial milestone.

## Trade-offs

The initial sRGB-only pipeline is professionally limiting for Photo/Video
RAW and HDR workflows — documented as FUNCTIONAL_WITH_LIMITATIONS rather
than hidden. Deferring ICC keeps the first milestones small; the seams
above prevent rework.

## Security

Color data from untrusted files (ICC profiles embedded in images) is
parsed defensively; profile parsing is a fuzz target
(`IMPLEMENTATION_GUIDE.md` §5).

## Performance

Conversions must be SIMD-friendly and allocation-free in hot paths
(compositing); renderer work is linear-light with no per-pixel heap.

## Compatibility

Document content stores color as tagged values with the profile id;
packages written under sRGB remain valid when ICC lands (profile id
defaults to sRGB) (`FILE_FORMAT_FAMILY.md` §5).

## Migration

All existing content (none yet in apps) is sRGB; no migration required.

## Testing

- Reference-vector conversion tests (sRGB↔linear round trips, precision
  bounds) — 8 tests exist.
- Deterministic golden tests for compositing in linear light.
- Profile-parse fuzz tests (future, with ICC work).

## Open questions

- Whether ICC work uses lcms2 bindings or a Rust implementation
  (resolved at the ICC milestone; recorded in an ADR).

## Final status

ACCEPTED. sRGB/linear pipeline implemented in `loom-color`; ICC and BTO
NOT_STARTED.
