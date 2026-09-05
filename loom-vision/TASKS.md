# Task ledger

Statuses: `DONE`, `IN_PROGRESS`, `NOT_STARTED`, `BLOCKED`.

## COMPLETE (0.1.0)

| ID | Task | Evidence |
|---|---|---|
| LV-001 | Workspace (resolver 2, edition 2021, rust-version 1.80, MIT OR Apache-2.0, pinned `[workspace.dependencies]`) | Cargo.toml |
| LV-002 | `VisionError` with Display/Error/From<io::Error> | `error.rs` + tests |
| LV-003 | `CapabilityId` (17 variants), `InputType`, `Backend`, `ProviderDescriptor` | `provider.rs` + tests |
| LV-004 | `ProviderInput`/`ProviderOutput`/`BBox`/`LumaImage` | `provider.rs` + tests |
| LV-005 | `RunContext` (AtomicBool + Cell<f32>, cancel/check/progress clamp) | `provider.rs` + tests |
| LV-006 | Grayscale conversion (BT.601) + cancellable row variant | `provider.rs` + tests |
| LV-007 | `ProviderRegistry` (Arc handles, first-wins `best_for`, `unregister`) | `registry.rs` + tests |
| LV-008 | `CapabilityRegistry` (`run_all`, `run_first_success`) | `registry.rs` + tests |
| LV-009 | `ModelPackManifest` serde model, hex sha256 (de)serialization | `model_pack.rs` + tests |
| LV-010 | `parse_manifest` (format_version, required fields) | `model_pack.rs` + tests |
| LV-011 | `validate_pack[_with_limit]` (traversal, symlink, size, SHA-256, 2 GiB guard) | `model_pack.rs` + tests |
| LV-012 | `install_pack` / `install_pack_force` (versioned dir, sanitize, no-overwrite-without-force) | `model_pack.rs` + tests |
| LV-013 | `QrCodeProvider` (rqrr, rgba/rgb/gray, cancellation, progress) | `reference.rs` + tests |
| LV-014 | `ImageStatsProvider` (mean/std/contrast) | `reference.rs` + tests |
| LV-015 | CLI `inspect-pack`, `qr`, `stats`, `bench` + help, exit codes | `main.rs`, verified runs |
| LV-016 | QR fixture generation example + committed `fixtures/hello.png` | `examples/gen_fixture.rs` |
| LV-017 | Integration tests (registry flows, pack lifecycle, tamper) | `tests/integration.rs` |
| LV-018 | Docs (README, AGENTS, ARCHITECTURE, IMPLEMENTATION_GUIDE, BUILDING, TESTING, VISUAL_QA, PERFORMANCE, SECURITY, ACCESSIBILITY, ROADMAP, TASKS, CONTRIBUTING, LICENSE_POLICY, DEPENDENCIES, CHANGELOG, ADR-0001) | docs/ |
| LV-019 | Quality gates green: fmt, clippy -D warnings, 72 tests, release build | verification run |

## NEXT (0.2.0)

| ID | Task | Status |
|---|---|---|
| LV-020 | Reference OcrProvider (pure-Rust engine evaluation first) | NOT_STARTED |
| LV-021 | Reference BarcodeProvider | NOT_STARTED |
| LV-022 | Execute model-pack `test_vectors` | NOT_STARTED |
| LV-023 | `providers` subcommand listing available providers | NOT_STARTED |
| LV-024 | ONNX Runtime backend behind a feature flag | NOT_STARTED |
| LV-025 | Candle backend behind a feature flag | NOT_STARTED |
| LV-026 | Fuzz targets for manifest parser and image-buffer validation | NOT_STARTED |

## Rules

- A task is `DONE` only with the listed evidence (real test output, not
  prose).
- Unimplemented work stays visible here and in ROADMAP.md.
