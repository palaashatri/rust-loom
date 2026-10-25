# Loom

Loom is an original, local-first creative software suite built with Rust and Slint.

The v1 scope on `main` is three desktop applications. Their acceptance status is in the [current-truth section](AGENTS.md#current-truth).

- [**Loom Writer**](loom-writer/README.md) — Document composition with Apple Pages-class interface polish.
- [**Loom Sheets**](loom-sheets/README.md) — Analytical spreadsheet with Apple Numbers-class multi-sheet workbooks.
- [**Loom Present**](loom-present/README.md) — Presentation design with Apple Keynote-class theme chooser and presenter sessions.

Shared workspaces provide package formats, document and text engines, recovery, native desktop adapters (file dialogs, macOS & Linux global menu bars), UI components, and interoperability foundations for these applications. The other five applications (Photo, Motion, Video, Studio, Encode) are locked and kept on the `apps/later` branch.

## Current status

Loom is in active productisation toward Apple Creator Studio-class depth and polish.
The current honest product-parity estimate and application boundaries are kept in the [current-truth section](AGENTS.md#current-truth).

The engineering constitution, project references, architecture contracts, evidence gates, and acceptance rules are consolidated in [`AGENTS.md`](AGENTS.md).

## Future SLOPOS desktop integration (planned)

Loom is intended to be the creative suite for [SLOPOS](https://github.com/palaashatri/rust-slopos) while remaining a separate, cross-platform **Rust + Slint** project. A future versioned integration layer may provide SLOPOS launch/file associations, real global-menu commands, shared file dialogs/clipboard, accessibility, and versioned theme-token and icon-role mapping for **SLOPOS Classic (System 7/Platinum)**, **Sinty-inspired SLOPOS Modern** and safe user themes, while independently respecting Dock/taskbar layout profiles. Optional local SLOPOS Vision and AI desktop automation may interact through permissioned document actions.

The symbolic app-list icons in SLOPOS are pinned, licensed Lucide reference SVGs, not custom colorful Loom or Sinty app artwork. Loom keeps its original cross-platform Slint visual identity when SLOPOS is absent.

**Not implemented and not an active release gate.** Writer, Sheets and Present are the first integration targets; the other five Loom apps retain their existing separate status. No native PARAMA compatibility is implied. See [AGENTS.md §15](AGENTS.md#15-future-interoperability-goal--loom-as-sloposs-creative-suite) for the proposed acceptance criteria.

## Reusable interaction tooling

- [Linux desktop input skill](.agents/skills/linux-desktop-input/SKILL.md) — guarded mouse and keyboard interaction for Linux desktop QA when native computer-use controls are unavailable.

The 23→100 implementation programme is underway:
- **Loom Writer**, **Loom Sheets**, and **Loom Present** have undergone Apple-class UI overhauls with structured inspectors, categorized template/theme choosers, and full desktop chrome.
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

The CI matrix additionally validates Linux x86-64, Windows x86-64, macOS arm64, and macOS x86-64 builds, native packages, functional journeys, keyboard palette journeys, UI productisation audits, and visual QA evidence.

## Engineering discipline

Loom prefers **proven libraries with small adapters** over unnecessary engine rewrites. Compatibility is established with real Writer/Sheets/Present files across the complete native GUI workflow—import, edit, save, reopen, export and failure/recovery—not by startup or fixture-only tests. Repeatable performance measurements include input latency, p50/p95, peak memory, hardware/OS, actual file workload and failures. Imported source, libraries, models, fonts and assets need component-level provenance and license review.

AI-agent changes must preserve the current serial workflow gate and report independently tested evidence and unrun checks. SLOPOS and PARAMA integration remain future, separate milestones. See [AGENTS.md §16](AGENTS.md#16-engineering-discipline--reuse-interoperability-and-real-workflow-evidence).

## SLOPOS and future PARAMA platform goals

As an **originally authored suite**, Loom is a candidate for SLOPOS's optional first-party creative catalog after independently verified Linux GUI integration and licensing/packaging. Loom must remain **Rust + Slint** and cross-platform. On a future Pi Zero 2 W Linux session, small real Writer/Sheets/Present workflows should favor bounded memory, lazy work, cancellation and accurate file round-trips; **actual ARM64 device support has not been demonstrated**.

**PARAMA is a conditional future native source target**, requiring a real Rust/Slint-compatible app runtime, SURYA graphics/input, filesystem/fonts and actual performance/QA. No present PARAMA binary, SDK or support is claimed. Forked Tricycle and texture-auto-upscaler are not SLOPOS first-party bundle candidates. See [AGENTS.md integration policy](AGENTS.md#first-party-distribution-edge-profiles-and-future-parama-target).

## Principles

- Local-first and offline-capable core workflows.
- No mandatory account, telemetry, advertising, or hidden uploads.
- Versioned, inspectable Loom project formats.
- Original Loom visual identity and design system.
- Incomplete features remain visible rather than being represented as finished.
