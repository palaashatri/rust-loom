# RFC-0015 — Visual Regression System

- Status: **ACCEPTED (normative)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-design-bible`, `loom-bootstrap`, all applications

## Context

Visual QA must not consist of "the window launched"
(root directive §15). Loom needs deterministic pixel comparison for
components and full application windows, in CI and in Docker, without GPUs.

## Goals

- Deterministic screenshots: software-rendered via Slint with a custom
  platform (no window manager), captured in pinned Docker images.
- Golden baselines committed in `loom-design-bible`; perceptual diffing
  with documented tolerances; no automatic baseline approval.
- Coverage: component states, windows, empty states, menus, dialogs,
  errors, selection, hover, focus, drag/drop, zoom, themes (light/dark/
  high contrast), large fonts, RTL, reduced-motion final states.

## Non-goals

- Pixel fidelity verification of the GPU renderer (deferred to RFC-0004).
- Approving baseline changes without review.

## Proposed design

- **Capture**: a test harness renders Slint components/windows with the
  software renderer and a custom platform (`docs/adrs/ADR-0003-Headless-Screenshots.md`),
  writing PNG + metadata (component id, theme, locale, font config,
  renderer info, commit id) into an artifacts tree.
- **Baselines**: committed in `loom-design-bible` (screenshot directory),
  generated only in the pinned Docker visual image
  (`../loom-bootstrap/docker/Dockerfile.visual`) so fonts, locales, and
  renderer versions are identical everywhere.
- **Diff**: perceptual diff with documented tolerance per metric (per-pixel
  color distance thresholds, allowed diff ratios); output shows baseline,
  actual, diff, and metadata.
- **Review**: a changed baseline requires explicit human approval (or an
  ADR); the pipeline fails on unapproved diffs beyond tolerance.
- **Reduced motion**: motion-sensitive tests capture final states only and
  assert deterministic end states.

## Alternatives

- **Xvfb + screenshot tools (import/scrot)**: works for E2E but is
  slower and less deterministic (rendering timing); kept only for E2E
  input automation, not pixel regression.
- **GPU-rendered screenshots**: environment-dependent (driver, GPU);
  rejected for baselines.

## Trade-offs

Software-renderer screenshots don't prove GPU output; accepted — GPU
paths get targeted tests later. Determinism costs: fonts, locales, and
renderer must be pinned; that is what the Docker images provide.

## Security

Screenshots may contain user data; baselines and artifacts must come from
fixture content only, never real user documents. Artifacts are local;
nothing is uploaded.

## Performance

Screenshot runs are bounded (explicit sizes, batched); CI time budget is
tracked; capturing full windows runs in the visual image only.

## Compatibility

Baselines are tied to pinned Slint, fonts, and Docker image versions; any
change to those requires regenerating baselines with review
(`COMPATIBILITY_POLICY.md` §6).

## Migration

No baselines exist yet; the system is established with the first `loom-ui`
components.

## Testing

- Harness self-tests: two identical renders diff zero; injected one-pixel
  change diff above tolerance; metadata completeness.
- Per-component screenshot tests run in the visual image.

## Open questions

- Tolerance defaults per surface type (resolved at first baseline
  review; recorded in the design bible's `VISUAL_QA.md`).

## Final status

ACCEPTED. Harness and baselines NOT_STARTED (`FEATURE_MATRICES.md` §1,
`ROADMAP.md` Phase 2 tail).
