# Building

## Requirements

- Rust stable >= 1.80 (developed against 1.97.1). Verify: `rustc --version`.
- No system libraries, no network access needed to build, test, or run.
  `cargo` needs network only for the first dependency download.

## Build

```sh
cargo build                       # debug
cargo build --release             # release (LTO thin)
```

## Test fixtures

The demo plugin package is generated, never committed:

```sh
cargo test -p loom-plugin-cli      # writes target/fixtures/demo.loomplugin
```

This creates `crates/loom-plugin-cli/target/fixtures/demo.loomplugin` from
the committed sources under `fixtures/demo/`
(`manifest.json`, 8-byte `module.wasm`, `assets/notes.txt`).

## Using the CLI

```sh
cargo run -p loom-plugin-cli -- validate crates/loom-plugin-cli/fixtures/demo/manifest.json
cargo run -p loom-plugin-cli -- install crates/loom-plugin-cli/target/fixtures/demo.loomplugin --dir /tmp/loom-store
cargo run -p loom-plugin-cli -- list --dir /tmp/loom-store
cargo run -p loom-plugin-cli -- remove demo-actions --dir /tmp/loom-store
```

## Offline operation

The binaries make zero network calls. For a fully offline build, prime the
cargo cache once (`cargo fetch` online), then build with
`cargo build --offline`.
