# Loom

Loom is an original, local-first creative software suite built with Rust and Slint.

The repository currently contains eight functional-alpha desktop applications:

- Loom Writer
- Loom Sheets
- Loom Present
- Loom Photo
- Loom Motion
- Loom Video
- Loom Studio
- Loom Encode

Shared workspaces provide package formats, runtime services, native desktop
adapters, history, recovery, jobs, UI components, interoperability foundations,
Loom Vision contracts, and the plugin SDK.

## Current status

Loom is not yet a production replacement for mature commercial creator suites.
The current honest product-parity estimate and application boundaries are kept
in [`TRUTH.md`](TRUTH.md).

The rigorous implementation programme, architecture contracts, application
roadmaps, evidence gates, and final acceptance checklist are defined in
[`AGENTS.MD`](AGENTS.MD).

The first 23→100 implementation milestone has started. Writer and Sheets now
use the shared native file-dialog service for their primary Open, Save/Save As,
and export destination workflows. Six applications still need migration, and
native menus, drag/drop, recent documents, asynchronous I/O, and atomic desktop
save orchestration remain incomplete.

All current implementation work is integrated on:

```text
cline-implementation
```

## Build and test

The umbrella scripts live under `loom-bootstrap/scripts/`. Each application and
shared subsystem is an independent Cargo workspace and can also be built from
its own directory.

Typical validation for one workspace:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

The CI matrix additionally validates Linux x86-64, Windows x86-64, macOS arm64,
and macOS x86-64 builds, native packages, functional journeys, keyboard palette
journeys, and visual evidence.

## Principles

- Local-first and offline-capable core workflows.
- No mandatory account, telemetry, advertising, or hidden uploads.
- Versioned, inspectable Loom project formats.
- Original Loom visual identity and assets.
- Incomplete features remain visible rather than being represented as finished.
