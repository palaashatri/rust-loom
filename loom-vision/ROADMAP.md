# Roadmap

Status vocabulary: `COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`,
`EXPERIMENTAL`, `SCAFFOLDED`, `NOT_STARTED`, `BLOCKED`.

## 0.1.0 (this release) — framework + CPU reference providers

| Item | Status |
|---|---|
| Provider traits, descriptor, input/output model, RunContext | COMPLETE |
| ProviderRegistry / CapabilityRegistry (thread-safe, Arc handles) | COMPLETE |
| Model-pack manifest parse + validate + install (checksums, traversal, archive guards) | COMPLETE |
| QrCodeProvider (rqrr, CPU) | COMPLETE |
| ImageStatsProvider (mean/std/Michelson contrast) | COMPLETE |
| CLI: inspect-pack, qr, stats, bench | COMPLETE |
| Offline-first (no network anywhere) | COMPLETE |

## 0.2.0 — broader capability coverage

| Item | Status |
|---|---|
| OcrProvider (reference: OCR on raw buffers, pure-Rust engine TBD) | NOT_STARTED |
| BarcodeProvider (1D barcodes, pure-Rust decoder) | NOT_STARTED |
| FaceDetectionProvider (CPU reference) | NOT_STARTED |
| Model-pack test-vector execution (run `test_vectors` against providers) | NOT_STARTED |
| `loom-vision` subcommand: `providers` (list registered/available) | NOT_STARTED |

## Backends (all NOT_STARTED)

| Backend | Status |
|---|---|
| ONNX Runtime integration | NOT_STARTED |
| Candle integration | NOT_STARTED |
| Vulkan compute | NOT_STARTED |
| CUDA / TensorRT / ROCm / OpenVINO / DirectML / CoreML | NOT_STARTED (optional external) |

## Capability areas with no implementation yet

Document analysis, object detection, segmentation, matting, pose, embeddings,
tracking, optical flow, speech recognition, audio analysis, image generation,
inpainting, super-resolution — all `NOT_STARTED`; capability ids exist so the
trait surface is stable, but no provider implements them. Applications must
handle `ProviderUnavailable` gracefully (per the Loom requirement that AI
capabilities are optional).

## Guiding constraints (never traded away)

Local-first, no telemetry, deterministic CPU fallback for every capability,
documented model packs with checksums, honest descriptors.
