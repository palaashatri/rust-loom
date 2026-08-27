# Loom

Loom is an original, local-first creative software suite built with Rust and Slint.

The repository currently contains eight functional-alpha desktop applications:

- [**Loom Writer**](loom-writer/README.md) — Document composition with Apple Pages-class interface polish.
- [**Loom Sheets**](loom-sheets/README.md) — Analytical spreadsheet with Apple Numbers-class multi-sheet workbooks.
- [**Loom Present**](loom-present/README.md) — Presentation design with Apple Keynote-class theme chooser and presenter sessions.
- [**Loom Photo**](loom-photo/README.md) — Non-destructive layer and mask image editing.
- [**Loom Motion**](loom-motion/README.md) — Motion graphics and keyframe animation.
- [**Loom Video**](loom-video/README.md) — Non-linear video timeline editor.
- [**Loom Studio**](loom-studio/README.md) — Multitrack audio workstation.
- [**Loom Encode**](loom-encode/README.md) — Multi-destination batch media encoder.

Shared workspaces provide package formats, runtime services, native desktop adapters (file dialogs, macOS & Linux global menu bars), history, recovery, jobs, UI components, interoperability foundations, Loom Vision contracts, and the plugin SDK.

## Current status

Loom is in active productisation toward Apple Creator Studio-class depth and polish.
The current honest product-parity estimate and application boundaries are kept in [`TRUTH.md`](TRUTH.md).

The rigorous implementation programme, architecture contracts, application roadmaps, evidence gates, and final acceptance checklist are defined in [`AGENTS.MD`](AGENTS.MD).

The 23→100 implementation programme is underway:
- **Loom Writer**, **Loom Sheets**, and **Loom Present** have undergone Apple-class UI overhauls with structured inspectors, categorized template/theme choosers, and full desktop chrome.
- **Native Desktop Services**: Shared native file dialogs and platform global menu bar reflection (`MenuBarService`) supporting macOS AppKit `NSMenu` and Linux DBusMenu (`com.canonical.dbusmenu`).
- All current work integrates on:

```text
cline-implementation
```

## Build and test

The umbrella scripts live under `loom-bootstrap/scripts/`. Each application and shared subsystem is an independent Cargo workspace and can also be built from its own directory.

Typical validation for one workspace:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo build --workspace --release
```

The CI matrix additionally validates Linux x86-64, Windows x86-64, macOS arm64, and macOS x86-64 builds, native packages, functional journeys, keyboard palette journeys, UI productisation audits, and visual QA evidence.

## Principles

- Local-first and offline-capable core workflows.
- No mandatory account, telemetry, advertising, or hidden uploads.
- Versioned, inspectable Loom project formats.
- Original Loom visual identity and design system.
- Incomplete features remain visible rather than being represented as finished.
