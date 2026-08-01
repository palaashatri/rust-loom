# RFC-0003 — Slint Integration Model

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-ui`, all applications

## Context

Loom's product directive selects Slint as the UI toolkit (Rust-native,
declarative, accessible, small-footprint). We need one shared way to build
and consume Slint UIs across applications, plus a deterministic screenshot
path for visual QA. Pinned version: **Slint 1.17.1** (license implications
recorded in `docs/adrs/ADR-0001-Slint-Licensing-and-Distribution.md`).

## Goals

- One `.slint` component library in `loom-core`'s UI crate (`loom-ui`,
  not yet implemented) shared by every application.
- Applications consume components via Slint's `library_paths`, never by
  copying `.slint` files between repositories.
- Deterministic rendering for tests: Slint **software renderer** in headless
  mode with a custom platform, capturing screenshots for golden baselines.
- GPU rendering (wgpu/Vulkan) remains the runtime path for canvases
  (future RFC-0004); the software path exists for QA and CPU-only tiers.

## Non-goals

- Multi-toolkit abstraction (RFC-0002).
- Browser-based UI.

## Proposed design

- `loom-ui` crate owns: the component gallery, design-token-driven Slint
  styles (`../loom-design-bible/` tokens), and shared components
  (inspector, palettes, dialogs, timeline widgets, command palette).
- Applications declare `slint::include_modules!()` against components
  resolved through `library_paths` pointing at `loom-ui`.
- Headless screenshots: a test harness creates the Slint window/component
  with the software renderer and a custom platform (no X11/Wayland
  required), renders to a pixel buffer, and writes PNGs
  (`docs/adrs/ADR-0003-Headless-Screenshots.md`). Baselines are committed
  in `loom-design-bible` and generated only in pinned Docker images.
- Visual QA runs the same screenshot path inside Docker
  (`../loom-bootstrap/docker/Dockerfile.visual`).
- Version pinning: Slint 1.17.1 in workspace dependencies with lockfiles;
  upgrades follow `COMPATIBILITY_POLICY.md` §2.

## Alternatives

- **Re-export Slint per app with copied components**: duplication and
  contract drift; rejected.
- **Live Qt-style toolkits**: outside the directive; rejected.
- **OS-level windowing tests (Xvfb)**: still needed for E2E, but slower and
  less deterministic than software-renderer screenshots for pixel tests;
  both are used (screenshots for regression, Xvfb/headless for E2E).

## Trade-offs

Software-renderer screenshots do not prove GPU path fidelity; mitigated by
per-surface GPU tests later (RFC-0004) and by keeping the canvas rendering
logic deterministic in shared crates. `library_paths` couples `loom-ui`
releases to apps; managed by `COMPATIBILITY.toml`.

## Security

Slint parses `.slint` at compile time; no runtime download. The custom
platform is test-only and never reached from release binaries' input path.

## Performance

Software renderer is test-only; runtime UI uses the accelerated renderer.
Screenshot tests must set explicit sizes to keep CI time bounded.

## Compatibility

Component identifiers in `loom-ui` are versioned with the crate
(`COMPATIBILITY_POLICY.md` §2); renaming a component requires a minor bump
and a bootstrap compatibility entry.

## Migration

No UI code exists yet; the model is adopted as `loom-ui` is created. Slint
1.17.1 is the pinned baseline from day one.

## Testing

- Component gallery screenshots + golden baselines (Docker-pinned).
- E2E flows per application through the headless/Xvfb path.
- A smoke test proving `library_paths` resolution in every app workspace.

## Open questions

- Exact set of first-wave shared components (decided at `loom-ui` kickoff).

## Final status

ACCEPTED. Slint 1.17.1; `loom-ui` with `library_paths`; software-renderer
screenshot harness; baselines only from pinned Docker images. `loom-ui` is
NOT_STARTED (`FEATURE_MATRICES.md` §1).
