# AGENTS.md — working in loom-vision

## Ground rules

1. **Everything is local.** No network calls at runtime, ever. No telemetry,
   no remote model APIs. A change that requires the network to work is a
   release-blocking defect.
2. **No `unsafe`.** `#![forbid(unsafe_code)]` is set in both crates. If you
   believe you need unsafe, redesign instead (the registry uses owned `Arc`
   handles for exactly this reason).
3. **Real algorithms only.** Reference providers must be real
   implementations with tests. No fakes, no hard-coded demo outputs, no
   `assert!(true)` tests.
4. **No hardcoded absolute paths.** Everything is relative to the workspace
   or passed as arguments.
5. **Every public item documented.** `#![warn(missing_docs)]` is set in
   `loom-vision-core`.

## Quality gates (mandatory before claiming completion)

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo build --release
```

Run them from the workspace root. Claims of completion require the actual
output of these commands.

## Conventions

- Workspace: `resolver = "2"`, edition 2021, rust-version 1.80, MIT OR
  Apache-2.0. Shared dependency versions live in `[workspace.dependencies]`.
- `loom-vision-core` must stay free of the `image` crate; it works on raw
  buffers (`rqrr` is used with `default-features = false`). Only the CLI
  depends on `image` for file I/O.
- New provider capabilities: add a `CapabilityId` variant (and `as_str`
  arm), then implement `CapabilityProvider`. Update
  `provider::tests` if you change serialized identifiers.
- Model-pack format changes bump `FORMAT_VERSION` in `lib.rs`; old
  versions must be rejected explicitly, not silently reinterpreted.
- Tests: unit tests live next to the code (`#[cfg(test)]`); cross-module
  flows live in `crates/loom-vision-core/tests/integration.rs`.
- Cargo.lock is committed; dependencies are pinned in
  `[workspace.dependencies]` and audited in DEPENDENCIES.md.
- No runtime dependencies may be added without updating DEPENDENCIES.md
  and LICENSE_POLICY.md.

## Ownership

- `crates/loom-vision-core`: provider model, registry, model packs,
  reference providers.
- `crates/loom-vision-cli`: command surface only — no business logic beyond
  argument parsing, image loading, and printing.
- `docs/`: architecture decisions (ADR-0001+); update the doc index in
  README.md when adding files.
