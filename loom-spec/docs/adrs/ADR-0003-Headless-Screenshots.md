# ADR-0003 — Headless Screenshots via the Slint Software Renderer

- Status: **ACCEPTED**
- Date: 2026-08-01

## Context

Visual QA must be deterministic and runnable in CI/Docker without GPUs or
window managers (`RFC-0015-Visual-Regression-System.md`).

## Decision

- Screenshots are captured through the **Slint software renderer** with a
  **custom platform** (no X11/Wayland dependency), rendering components
  and windows to a pixel buffer and writing PNGs plus metadata (theme,
  locale, font config, renderer, commit).
- Baselines are generated **only inside the pinned Docker visual image**
  (`../loom-bootstrap/docker/Dockerfile.visual`) so fonts, locales, and
  renderer versions are identical for every run.
- Baselines are committed in `loom-design-bible`; diffs use documented
  perceptual tolerances; no automatic approval of changed baselines.
- Xvfb/headless E2E (input automation) remains a separate, complementary
  path.

## Consequences

- Deterministic pixel comparison on any host, offline; no GPU variance.
- Software rendering does not verify GPU paths; GPU-specific tests are
  deferred to RFC-0004.
- Screenshot capture must run in an environment matching the pinned image
  — regenerating baselines outside Docker is prohibited
  (`COMPATIBILITY_POLICY.md` §6).

## Verification

- Harness self-test: identical renders diff zero; an injected 1-pixel
  change exceeds tolerance (`RFC-0015` §Testing).
