# Architecture

## Overview

Loom Vision decouples *applications* from *models*. An application asks for
a capability; a registered provider supplies the computation. Providers can
be pure algorithms (CPU reference implementations) or model-backed runtimes
(ONNX, Candle, GPU) — the application never sees the difference.

```
                 +---------------------------+
                 |      Application code      |
                 +-------------+--------------+
                               |  ProviderInput / RunContext
                               v
                 +-------------+--------------+
                 |      CapabilityRegistry     |   routes by CapabilityId
                 +-------------+--------------+
                               |
                 +-------------+--------------+
                 |      ProviderRegistry       |   ordered Vec<Arc<dyn CapabilityProvider>>
                 +-------------+--------------+
                               |
          +--------------------+---------------------+
          |                    |                     |
   +------v-------+   +--------v--------+   +--------v--------+
   | QrCodeProvider|   |ImageStatsProvider|  | (future) Onnx...|
   |  (rqrr, CPU)  |   |   (pure CPU)     |   | model provider  |
   +---------------+   +-----------------+   +-----------------+
```

## Provider model

- `CapabilityProvider: Send + Sync` with `descriptor()` and
  `run(&self, input, ctx)`.
- `ProviderDescriptor` declares everything a caller needs to decide
  *whether* to use a provider: capability id, input types, output schema,
  media formats, memory, latency, backends, license, provenance,
  determinism, batch/streaming/cancellation/progress support.
- `ProviderInput` is raw data only: image buffers (gray/rgb/rgba), audio
  samples, or text. Providers own their decoding of that data.
- `RunContext` carries cancellation (`cancel`/`check_cancelled`) and
  progress (`set_progress`, clamped to 0..1). Providers must poll
  `check_cancelled` every few rows/iterations and return
  `VisionError::Cancelled` promptly. The context is owned by the caller of
  `run` and is intentionally not `Sync`.
- `ProviderOutput` is a tagged result (OcrResult, DetectionResult,
  QrDecoded, ImageStats, Generic, ...).

## Registry

- `ProviderRegistry` keeps an ordered `Vec<Arc<dyn CapabilityProvider>>`
  behind an `RwLock`. Lookups return owned `Arc` handles, which is what
  makes the registry sound without `unsafe`: the lock is released before a
  handle escapes, and a handle keeps its provider alive.
- **First registered wins**: `best_for` returns the earliest provider for a
  capability. Applications can shadow built-ins by registering a preferred
  provider first. `unregister` removes all providers of a capability.
- `CapabilityRegistry` adds run routing: `run_all` (collect every result)
  and `run_first_success` (first `Ok`, else last error).

## Model-pack lifecycle

```
pack dir (manifest.json + model files)
   │  parse_manifest     — JSON schema, format_version, required fields
   v
validate_pack(_with_limit)
   │  path-traversal guard (no absolute / .. / . components, no symlinks)
   │  per-file: exists → regular file → size matches → SHA-256 matches
   │  cumulative size ≤ max (default 2 GiB, archive-bomb guard)
   v
ModelPackSummary
   │
install_pack / install_pack_force
   │  destination <dest_dir>/<id>-<version>/ (sanitized components)
   │  refuses symlinked destination; refuses overwrite with different
   │  checksum unless forced
   v
installed pack  (re-validatable like any pack)
```

- Manifest serialization uses `serde_json`; `ModelFile.sha256` is a `[u8; 32]`
  serialized as a 64-char lowercase hex string.
- `FORMAT_VERSION` is 1; manifests with other versions are rejected.
- Pack ids/versions are sanitized to `[A-Za-z0-9._-]` before being used in
  directory names, so a malicious manifest cannot escape `dest_dir`.

## Reference providers

- `QrCodeProvider` (capability `qr_detection`): converts input to grayscale
  (BT.601 weights), then runs `rqrr`'s `prepare_from_bitmap`/`detect_grids`/
  `decode`. Deterministic, CPU only, MIT (rqrr is `(MIT OR Apache-2.0) AND
  ISC`). No QR found → `VisionError::Internal(NO_QR_CODE_MESSAGE)`.
- `ImageStatsProvider` (capability `image_stats`): mean luma, population
  standard deviation, Michelson contrast `(max-min)/(max+min)`. Pure
  arithmetic, CPU only.

## Design decisions recorded

See `docs/adrs/ADR-0001-qr-reference-provider.md` for the QR backend choice
(rqrr vs alternatives, license, fallback plan).
