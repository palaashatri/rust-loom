# License Policy

## Code

All original code in this repository is dual-licensed **MIT OR Apache-2.0**
(workspace-level `license = "MIT OR Apache-2.0"`, matching the Loom suite).

Each file carries no individual headers by convention; the workspace
manifest is authoritative.

## Dependencies (direct)

| Crate | Version | License | Notes |
| --- | --- | --- | --- |
| serde | 1.x | MIT OR Apache-2.0 | feature `derive` |
| serde_json | 1.x | MIT OR Apache-2.0 | |
| sha2 | 0.10 | MIT OR Apache-2.0 | |
| zip | 0.6.x | MIT | `default-features = false`, `deflate` only (drops bzip2/zstd/aes deps) |

All are permissive and compatible with MIT OR Apache-2.0. No dependency
forces a copyleft or viral license. Run `cargo deny` (when adopted by
loom-core) as a CI gate.

## Fixtures and assets

- The demo manifest, notes asset, and 8-byte wasm header are original,
  created within the project.
- No commercial, proprietary, or sample media is included.

## Constraints

- Adding a dependency with a non-permissive license requires an ADR and
  isolation behind a feature flag, per Loom's licensing policy.
- Model packs (future) must not bundle models whose licenses forbid
  redistribution (Loom Vision policy, RFC-0011).

## Verification

`cargo tree -d` + the pinned `Cargo.lock` are the dependency inventory.
Rebuilds are reproducible via the committed lockfile.
