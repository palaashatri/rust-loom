# Loom Dependency Report

Status: initial audit; automation pending.

## Method

- All five cargo workspaces use pinned lockfiles (`Cargo.lock` committed), enabling
  reproducible and offline (`--offline`) builds.
- Registry source cache is required for cold offline builds (container test mounts the
  host registry cache); this is documented in KNOWN_LIMITATIONS.md.
- Manual scan of the merged dependency graph for GPL/AGPL/copyleft entries: none found.

## Key dependencies

| Crate | Version | License | Role | Notes |
|-------|---------|---------|------|-------|
| slint | 1.17.1 | MIT OR Apache-2.0 (commercial exception applies) | UI toolkit | pinned in COMPATIBILITY.toml; royalty-free for open source |
| slint-build | 1.17.1 | MIT OR Apache-2.0 | .slint compiler | |
| image | 0.25.x | MIT OR Apache-2.0 | PNG raster for capture/baselines | |
| zip | 2.x | MIT | .loomdoc/.loomtable container | archive limits enforced |
| serde/serde_json | 1.x | MIT OR Apache-2.0 | manifest/content JSON | |
| sha2 | 0.10 | MIT OR Apache-2.0 (or Zlib) | checksums | |
| wgpu (planned) | — | MIT OR Apache-2.0 | GPU rendering/compute | not yet in tree |
| FFmpeg (planned) | — | LGPL/GPL variants | media backend | ADR + isolation required before adoption; not in tree |

## Policy obligations

- Every new dependency: verify docs, maintenance, Linux support, license, thread-safety,
  performance, record an ADR, pin a version, define a replacement strategy.
- Optional backends (CUDA, ONNX Runtime, etc.) must be feature-gated with license notes.
- `cargo audit`/`cargo deny` in CI: TODO (tooling not installed in the current environment;
  manual lockfile review stands in).

## Honest status

Lockfiles are complete and buildable offline with a warm cache. An automated
vulnerability/license audit pipeline is not yet wired into CI.
