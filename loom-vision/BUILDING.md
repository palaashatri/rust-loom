# Building

## Requirements

- Rust stable 1.80 or newer (developed and verified with 1.97).
  `rust-version` is set per crate to 1.80.
- No system libraries: all dependencies are pure Rust (the `image` crate
  in the CLI has no C dependencies for the formats we use).

## Build

```sh
cargo build --workspace            # debug
cargo build --release              # release (binary: target/release/loom-vision)
cargo build -p loom-vision-core    # library only
```

## Minimum supported Rust version (MSRV)

1.80 (declared in `rust-version` in the workspace `Cargo.toml`).
CI/tooling should verify with `cargo +1.80 check --workspace`.

## Dependency pinning

All direct dependencies are pinned in `[workspace.dependencies]`
(semver-compatible ranges). The committed `Cargo.lock` pins exact versions
for reproducible builds. Audit table: [DEPENDENCIES.md](DEPENDENCIES.md).
