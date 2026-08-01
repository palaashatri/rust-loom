# RFC-0010 — Loom Vision Provider Model

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-vision`, all applications

## Context

Loom Vision must expose perception capabilities without being hard-wired
to any model, vendor, runtime, or hardware backend
(`PRODUCT_SPEC.md` §4). Applications need stable capability traits, not
model-specific APIs; providers come from bundled reference
implementations and user-installed model packs.

## Goals

- Capability-based provider model: applications request a capability, the
  registry returns the best installed provider.
- Providers are backend-agnostic (CPU, Vulkan, ONNX, Candle, optional
  CUDA/ROCm/OpenVINO/…).
- Uniform cancellation and progress via `RunContext`.
- A CPU fallback exists for every capability, or the absence of a provider
  is clearly reported.

## Non-goals

- Bundling ML models in the platform.
- Remote inference of any kind.

## Proposed design

Mirrors `../loom-vision/ARCHITECTURE.md` and the implementation in
`loom-vision-core`:

- `CapabilityId` — stable capability names (`ocr`, `segmentation`,
  `object_detection`, `tracking`, `transcription`, `qr_decode`,
  `image_stats`, …).
- `ProviderDescriptor` — capability, input/output schema, media formats,
  languages, memory, latency estimates, backends, license, provenance,
  determinism, batch/streaming/cancellation/progress support.
- `CapabilityProvider: Send + Sync` — executes one capability; takes
  `ProviderInput` (`LumaImage` and friends), returns `ProviderOutput`
  (structured results, `BBox`), and receives a `RunContext` for
  cancellation and progress.
- `ProviderRegistry`/`CapabilityRegistry` — register/unregister, query by
  capability, best-provider selection. Implemented (12 tests).
- Model packs install providers at runtime after checksum-verified
  installation (`RFC-0011-Model-Pack-Format.md`); no network downloads.
- Reference providers prove the contract with pure-CPU implementations:
  `QrCodeProvider` and `ImageStatsProvider` are COMPLETE (15 tests).
- Backends are feature-gated; every provider either has a CPU fallback or
  registers an explicit "no compatible provider" state.

## Alternatives

- **Model-specific APIs per app**: fast to prototype, impossible to
  maintain across 8 apps; rejected.
- **One mandated runtime (ONNX-only)**: violates the vendor-neutral
  requirement; rejected.

## Trade-offs

The descriptor contract is richer than a minimal `run()` call — cost
accepted for scheduling decisions (memory/latency) and license/provenance
transparency. Provider output schema evolution is a versioning concern:
capability output versions are part of the descriptor.

## Security

Provider code is native and trusted (reference providers, or model packs
with verified checksums and provenance). Model packs are untrusted input
until validated (`RFC-0011`); providers never open network connections.

## Performance

CPU fallback for every capability is a release requirement; RunContext
progress enables progress UI; batch support declared per provider lets
apps amortize inference.

## Compatibility

Capability ids and output schemas are versioned (`COMPATIBILITY_POLICY.md`
§2); adding a capability is additive; changing an output schema requires a
major/minor bump per policy.

## Migration

None yet; no consumers besides the CLI. Applications begin consuming the
registry in Phase 4/6 (`ROADMAP.md`).

## Testing

- Trait contract tests (registry registration/selection, cancellation,
  progress) — 12 tests.
- Reference provider golden tests (QR decode of known fixtures; image
  statistics on deterministic images) — 15 tests.
- Model-pack install/validation tests — 24 tests.
- Future: per-capability conformance suites run against any provider.

## Open questions

- Whether capability output schemas get their own version suffix
  (deferred to first schema change).

## Final status

ACCEPTED. Provider model implemented; reference providers COMPLETE; all
ML-backed capabilities NOT_STARTED.
