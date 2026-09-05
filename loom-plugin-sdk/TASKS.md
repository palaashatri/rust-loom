# Tasks

Small, independently verifiable tasks. Format: ID — Title (status).

## Manifest

- M-01 Immutable `PluginManifest` schema with serde (COMPLETE).
- M-02 Hand-rolled plugin-id regex check (COMPLETE).
- M-03 `version_compatible` dotted-numeric compare + matrix tests (COMPLETE).
- M-04 Error taxonomy incl. UnknownCapability via custom Deserialize (COMPLETE).
- M-05 Size-limited `parse_manifest_with_limit` (COMPLETE).
- M-06 Serde round-trip + canonical kebab-case output tests (COMPLETE).

## Host

- H-01 `PluginStore::open` + index regeneration (COMPLETE).
- H-02 Install pre-scan: names, count, declared total (COMPLETE).
- H-03 Manifest read-bounded + validated before extraction (COMPLETE).
- H-04 Api-range overlap check (COMPLETE).
- H-05 Bounded streaming extraction with cleanup on failure (COMPLETE).
- H-06 Symlink-entry rejection via unix mode bits (COMPLETE).
- H-07 `check_permission` with component-aware prefix matching (COMPLETE).
- H-08 Hostile-archive integration tests with "nothing extracted" asserts (COMPLETE).

## CLI

- C-01 Hand-rolled subcommand parsing with usage exit code 2 (COMPLETE).
- C-02 validate/install/list/remove against the real binary (COMPLETE).
- C-03 Fixture generation test writing `target/fixtures/demo.loomplugin` (COMPLETE).

## Backlog (blocked or future)

- R-01 wasmtime pinning decision (BLOCKED: needs dependency review).
- R-02 WASI instantiation + `loom_host_*` import boundary (NOT_STARTED).
- R-03 Watchdog + memory-limit enforcement (NOT_STARTED).
- S-01 Ed25519 package signing + local keyring (NOT_STARTED).
- P-01 Process-per-plugin isolation (NOT_STARTED).
- P-02 CLI `bench` subcommand with enforced budgets (NOT_STARTED).
- P-03 Cross-version plugin fixture corpus (NOT_STARTED).

## Definition of done

`cargo fmt --check`, clippy `-D warnings`, `cargo test --workspace`,
`cargo build --release` all green; tests assert real behavior; status
reflects reality.
