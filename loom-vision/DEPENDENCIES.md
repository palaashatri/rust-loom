# Dependencies

All versions are the resolved versions from the committed `Cargo.lock`
(verified by `cargo build`/`cargo test` on rustc 1.97.1, 2026-08-01).

## Direct dependencies

| Crate | Version | License | Purpose | Scope |
|---|---|---|---|---|
| serde | 1.0.229 | MIT OR Apache-2.0 | Serialization framework (derive) | core |
| serde_json | 1.0.151 | MIT OR Apache-2.0 | Model-pack manifest parsing | core |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | SHA-256 checksums for model files | core |
| rqrr | 0.10.1 | (MIT OR Apache-2.0) AND ISC | QR detection/decode (default features off; uses `prepare_from_bitmap` on raw buffers) | core |
| image | 0.25.10 | MIT OR Apache-2.0 | PNG/JPEG/etc. loading for CLI commands | cli |
| qrcode | 0.14.1 | MIT OR Apache-2.0 | QR *encoding* for tests and fixture generation | dev/example |
| tempfile | 3.27.0 | MIT OR Apache-2.0 | Temp directories in tests | dev |

## Notable transitive dependencies

`g2p` (1.2.2, MIT/Apache) and `lru` (0.16.4, MIT) — rqrr internals.
`image` pulls standard permissive decoders/encoders (png, jpeg/zune, gif,
webp, tiff, exr, rav1e/ravif for AVIF, qoi). Nothing in the transitive
tree is copyleft.

## Rules

- Add dependencies only via `[workspace.dependencies]` with a pinned range.
- Runtime deps of `loom-vision-core` must be pure Rust, `no_std`-friendly,
  network-free, and permissively licensed (audit via
  `cargo tree` + LICENSE_POLICY.md updates).
- `rqrr` is deliberately configured with `default-features = false` so the
  `image` crate stays out of `loom-vision-core`.
- Regenerate this table on any dependency change.
