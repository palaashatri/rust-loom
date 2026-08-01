# Changelog

## 0.1.0 — 2026-08-01

Initial release: the Loom Vision framework with CPU reference providers.

### Added

- Provider model: `CapabilityId` (17 capabilities), `ProviderDescriptor`,
  `ProviderInput` (image/audio/text), `ProviderOutput`, `RunContext`
  (cancellation + clamped progress), `LumaImage`.
- Grayscale conversion (ITU-R BT.601) with a cancellable row-by-row
  variant (`image_to_luma` / `image_to_luma_checked`).
- `ProviderRegistry` (ordered, thread-safe, owned `Arc` handles,
  first-registered-wins `best_for`, `unregister`) and `CapabilityRegistry`
  (`run_all`, `run_first_success`).
- Model packs: `ModelPackManifest` (serde, hex SHA-256), `parse_manifest`,
  `validate_pack[_with_limit]` (path-traversal/symlink/size/checksum
  checks, 2 GiB archive-bomb guard), `install_pack`/`install_pack_force`
  (versioned dirs, sanitized components, no-overwrite-without-force).
- Reference providers: `QrCodeProvider` (rqrr 0.10, rgba/rgb/gray inputs,
  deterministic) and `ImageStatsProvider` (mean luma, population std,
  Michelson contrast).
- `loom-vision` CLI: `inspect-pack`, `qr`, `stats`, `bench`, `help`.
- Fixture generation example (`gen_fixture`) and committed
  `crates/loom-vision-cli/fixtures/hello.png`.
- 72 tests (63 unit, 8 integration, 1 doc), all gates green
  (fmt, clippy `-D warnings`, test, release build).
- Documentation set incl. ADR-0001 (QR backend choice).

### Known limitations

- Only two capabilities have providers (QR detection, image statistics);
  everything else returns `ProviderUnavailable`-style behavior.
- No ONNX/Candle/GPU backends yet.
- `RunContext` is not `Sync` (caller-owned; cancellation is same-thread).
- CLI reads images through the `image` crate; `loom-vision-core` itself
  remains buffer-only.
