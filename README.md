# Loom

Loom is an original, local-first creative software suite built with Rust and Slint.

The v1 scope on `main` is three desktop applications. Their acceptance status is in the [current-truth section](docs/workflow/current.md#current-truth).

- [**Loom Writer**](loom-writer/README.md) — Document composition with structured inspectors, rich typography, tables, and DOCX/PDF/Markdown export.
- [**Loom Sheets**](loom-sheets/README.md) — Analytical spreadsheet with multi-sheet workbooks, 2,000+ Excel-compatible formulas, charts, and XLSX/CSV/PDF export.
- [**Loom Present**](loom-present/README.md) — Presentation design with categorized theme chooser, slide navigator, speaker notes, presenter view, and PPTX/PDF export.

## Visual showcase

| Application | Light Theme | Dark Theme |
|---|---|---|
| [**Loom Sheets**](loom-sheets/README.md) | ![Loom Sheets Light](docs/screenshots/loom-sheets-light.png) | ![Loom Sheets Dark](docs/screenshots/loom-sheets-dark.png) |
| [**Loom Writer**](loom-writer/README.md) | ![Loom Writer Light](docs/screenshots/loom-writer-light.png) | ![Loom Writer Dark](docs/screenshots/loom-writer-dark.png) |
| [**Loom Present**](loom-present/README.md) | ![Loom Present Light](docs/screenshots/loom-present-light.png) | ![Loom Present Dark](docs/screenshots/loom-present-dark.png) |

Shared workspaces provide package formats, document and text engines, recovery, native desktop adapters (file dialogs, macOS & Linux global menu bars), UI components, and interoperability foundations for these applications. The other five applications (Photo, Motion, Video, Studio, Encode) are locked and kept on the `apps/later` branch.

## Current status

Loom is in active productisation toward full-featured v1 desktop depth and polish.
The current honest product-parity estimate and application boundaries are kept in the [current-truth section](docs/workflow/current.md#current-truth).

The engineering constitution, project references, architecture contracts, evidence gates, and acceptance rules are routed by [`AGENTS.md`](AGENTS.md) to focused files in [docs](docs/README.md).


## Reusable interaction tooling

- [Linux desktop input skill](.agents/skills/linux-desktop-input/SKILL.md) — guarded mouse and keyboard interaction for Linux desktop QA when native computer-use controls are unavailable.

The 23→100 implementation programme is underway:
- **Loom Writer**, **Loom Sheets**, and **Loom Present** have undergone full UI overhauls with structured inspectors, categorized template/theme choosers, and native desktop chrome.
- **Native Desktop Services**: Shared native file dialogs and platform global menu bar reflection (`MenuBarService`) supporting macOS AppKit `NSMenu` and Linux DBusMenu (`com.canonical.dbusmenu`).
- All current work integrates on:

```text
main
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

The CI matrix additionally validates Linux x86-64, Windows x86-64, macOS arm64, and macOS x86-64 builds (macOS x86-64 is being retired: macOS 27 dropped Intel Macs and GitHub retires its Intel runner in fall 2027; see [AGENTS.md §18](docs/rules/platforms.md#18-2027-readiness-baseline--platforms-accessibility-supply-chain-and-regulation)), native packages, functional journeys, keyboard palette journeys, UI productisation audits, and visual QA evidence.

## Engineering discipline

Loom prefers **proven libraries with small adapters** over unnecessary engine rewrites. Compatibility is established with real Writer/Sheets/Present files across the complete native GUI workflow—import, edit, save, reopen, export and failure/recovery—not by startup or fixture-only tests. Repeatable performance measurements include input latency, p50/p95, peak memory, hardware/OS, actual file workload and failures. Imported source, libraries, models, fonts and assets need component-level provenance and license review.

AI-agent changes must preserve the current serial workflow gate and report independently tested evidence and unrun checks. SLOPOS and PARAMA integration remain future, separate milestones. See [AGENTS.md §16](docs/rules/engineering.md#16-engineering-discipline--reuse-interoperability-and-real-workflow-evidence).

## Edge efficiency goal

Loom aims for **responsive small-document workflows on a Raspberry Pi Zero 2 W** (AArch64 Cortex-A53, **512 MB RAM**) as a future low-end reference, while preserving fully featured workloads on capable systems. The GUI must stay interactive and allow progress/cancellation during slow disk/network/CPU work; large documents and demanding media/ML may require more memory and must fail safely with truthful explanations, never silently drop data. Memory, input p95, caches, startup and swapping must be instrumented on **actual hardware**.

**AArch64 Pi Zero 2 W support is not implemented or verified.** Existing Rust/Slint, supported host-platform gates and sequential app development remain unchanged. See [AGENTS.md §17](docs/rules/platforms.md#17-edge-first-efficiency-raspberry-pi-zero-2-w-reference).

## Principles

- Local-first and offline-capable core workflows.
- No mandatory account, telemetry, advertising, or hidden uploads.
- Versioned, inspectable Loom project formats.
- Original Loom visual identity and design system.
- Incomplete features remain visible rather than being represented as finished.
