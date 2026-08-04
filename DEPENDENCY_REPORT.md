# Loom — Dependency Report

## Inventory

Eleven Cargo workspaces with locked, reproducible dependency graphs:

- `loom-core` (shared platform crates)
- `loom-writer`, `loom-sheets`, `loom-present`, `loom-photo`, `loom-motion`,
  `loom-video`, `loom-studio`, `loom-encode` (applications)
- `loom-vision`, `loom-plugin-sdk`

Every workspace commits `Cargo.lock`; all builds in this audit used
`--locked` where the verification path allows it and `cargo --offline`
successfully for the network-disabled gate.

## Key dependency categories

| Category | Examples | Policy |
|---|---|---|
| UI toolkit | Slint 1.17 | Pinned in `COMPATIBILITY.toml`; MSRV contract |
| Async runtime | Tokio | Background orchestration only, structured cancellation |
| Image/QR | `image`, `rqrr` | Pure-Rust paths, local only |
| Media | FFmpeg via runtime discovery | Isolated behind encode/media engine, feature-gated |
| Plugin runtime | Wasmtime (optional), WASM validation | Sandboxed, capability-checked, optional execution |
| WebAssembly/plugins | `wasmparser`-style validation | Defensive, no remote code |
| System bindings | `libasound2` (dev), xkbcommon, wayland/x11 | Container CI image; Linux target |

## Audit status (2026-08-04)

- `cargo test --locked --offline --workspace` passes for all 11 workspaces
  (locked dependency graphs resolve from cache with no network).
- `loom-bootstrap/DEPENDENCIES.md` is the maintained inventory with
  version/license/active-maintenance notes and replacement strategies for
  critical dependencies.
- Supply-chain posture: lockfiles committed, reproducible build documented in
  `loom-bootstrap/BOOTSTRAP.md`, no mandatory update checks, no network calls
  from core workflows (offline gate PASS).

## Fresh transitive additions (2026-08-04)

`loom-plugin-sdk` gained 275 lockfile lines during the clean rebuild
(signing/zeroization deps such as `ed25519`, `curve25519-dalek`, `subtle`,
`zeroize`); committed as `ffc69b1` after the clean build verified them.