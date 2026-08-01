# ADR-0001: rqrr as the reference QR provider

Status: Accepted
Date: 2026-08-01
Scope: `loom-vision-core` reference provider for `CapabilityId::QrDetection`

## Context

Loom Vision needs a working, deterministic QR decoder that ships with the
framework: no model files, no GPU, no network. The candidates are pure-Rust
QR *decoders* (not encoders) usable from a raw grayscale buffer.

## Candidates considered

| Crate | Notes | Verdict |
|---|---|---|
| **rqrr 0.10.1** | Pure Rust decode; `prepare_from_bitmap(w, h, FnMut) -> bool` works directly on raw buffers; no `unsafe` in the path we use; actively maintained (0.10.1, June 2026); used as reference by `qrcode-decode` | **Selected** |
| `quircs` | Pure Rust; decoder-oriented; smaller ecosystem, slower detection | Not chosen: rqrr is more widely used and actively maintained |
| `zxing-cpp` | Bindings to C++; excellent quality | Not chosen: C++ FFI (`unsafe` boundary), heavier build |
| Home-grown | Decoder is a large, error-prone algorithm | Not chosen: would trade correctness for control |

## Decision

Use `rqrr = { version = "0.10", default-features = false }` in
`loom-vision-core`, with `PreparedImage::prepare_from_bitmap` fed from our
own BT.601 grayscale conversion. Keeping `default-features = false` keeps
the `image` crate out of `loom-vision-core` (the CLI adds `image` on its
own for file loading).

## License

rqrr is `(MIT OR Apache-2.0) AND ISC`. ISC is a permissive license fully
compatible with the project's `MIT OR Apache-2.0` dual licensing (both
components permit the use and relicensing we need). Recorded in
LICENSE_POLICY.md.

## Trade-offs

- rqrr's grid search is the dominant cost (≈17 ms on a 232×232 image in
  release builds); callers can downscale before decoding.
- rqrr decodes standard QR codes; Micro QR / other symbologies are out of
  scope (a future `Barcode` provider can add them).
- Deterministic: identical inputs produce identical outputs (no floating
  point in detection decisions at our call sites; verified by tests).

## Fallback plan

If rqrr becomes unmaintained or a license problem appears:
1. `quircs` is the drop-in pure-Rust alternative (same
   `prepare_from_bitmap`-style raw API).
2. The provider boundary isolates the swap: only
   `reference.rs::QrCodeProvider::run` changes; descriptors, CLI, and
   tests remain valid.
3. Vendor-on-crisis is documented as last resort (per Loom dependency
   policy, with an ADR update).

## Consequences

- `QrCodeProvider` can ship with zero model files and zero network use.
- Test strategy is fully self-contained: tests encode QR codes at runtime
  with `qrcode` (dev-dependency) and decode them back.
- The `(MIT OR Apache-2.0) AND ISC` license string must appear in
  NOTICE files at packaging time.
