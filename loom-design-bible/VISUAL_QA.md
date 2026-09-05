# Visual QA

Visual regression is a release gate. This document fixes the process, the
capture contract, the comparison method, and the artifact rules.

## 1. Capture contract

* All baselines and actuals are captured **in-app** using the software
  renderer (deterministic CPU rasterization — the documented
  software-rendering path; never a screenshot of a GPU-presented window).
* Fixed capture size: **1280 × 800 logical pixels**, 1.0 text scale,
  fixed deterministic fonts, fixed locale (en-US default for baselines;
  pseudolocale/RTL sets are separate capture suites), fixed window chrome.
* Capture happens only in the **Docker visual-QA environment** (pinned
  Ubuntu base, software Vulkan, Xvfb — `loom-bootstrap` docker scripts).
  Baselines generated on contributor machines are rejected.
* Determinism requirements: fixed seed for any procedural content
  (sample documents, patterns), wall-clock animations disabled by
  advancing to final states (reduced-motion capture mode), no network
  anywhere in the pipeline.

## 2. Baseline storage

* Baselines live in this repository: `baselines/<app>/<name>.png`,
  where `<app>` is one of writer, sheets, present, photo, motion, video,
  studio, encode, vision, components (gallery).
* Each baseline ships with metadata (JSON sidecar, `<name>.meta.json`):
  app, capture date, commit id, renderer id, font configuration, theme,
  text scale, locale, reduced-motion flag, window layout, and the seed.
* The baseline set is versioned with the repository; a baseline changes
  only when the contract intentionally changes (token change, layout
  change), and the change requires the ADR/review path — never silent
  auto-updates.
* Component-gallery baselines (from the gallery milestone) cover the
  component state matrix; application baselines cover the application
  screens listed in §4.

## 3. Comparison method

Perceptual diff on RGBA pixels (software-renderer output is deterministic,
so the diff measures contract drift, not rasterizer noise):

* Metric 1 — **mean absolute error** across all pixels (0–255 scale):
  gate `mean < 1.0`.
* Metric 2 — **differing-pixel ratio**: pixels with per-channel abs diff
  > 8/255 (after 1 px erosion to ignore 1-px shifts) divided by total
  pixels: gate `ratio < 0.01`.
* Both gates must pass. A run that fails either produces artifacts and is
  reported; it does not update the baseline.
* Tolerances are fixed contract values; adjusting them requires an ADR.

## 4. Baseline coverage (minimum)

Per application:

* Empty state (no document), a representative sample project open, and a
  complex project open.
* Each component state that appears in the app's chrome (toolbar hover,
  button focus, dialog open, menu open, popover open, inspector sections).
* Selection states: single, multi, marquee in progress, text caret.
* Theme set: light, dark, high-contrast.
* Text scale 1.25 and 1.5 layouts (layout-stress set).
* Reduced-motion final states.
* RTL locale stress screens and pseudolocale screens (localization set).
* Error states: import failure toast, recovery browser, relink dialog.
* The full component-state matrix once the gallery milestone exists.

A new feature adds at minimum: its default state, its selected state, its
error state, and its reduced-motion final state.

## 5. Process

1. Application team adds/updates feature; runs the app's visual suite
   locally (actuals only — no baseline writes).
2. CI (Docker visual environment) renders actuals, diffs against committed
   baselines, and fails on gate violation.
3. On failure: artifacts — actual PNG, diff PNG (green = equal, red =
   differing, blue = missing region), metadata JSON — are collected into
   `artifacts/<run-id>/<app>/<name>.{png,meta.json}` and made available in
   the CI artifact store.
4. The design-system lead inspects severe diffs, classifies them:
   intentional contract change (→ baseline update through review) or
   defect (→ fix in the application, new baseline NOT created).
5. No auto-approval exists: no tool may accept a new baseline without
   human review in a PR that also updates the contract documents if the
   change is intentional.

## 6. Diff inspection rules

* Diffs beyond tolerance are classified: layout shift, color drift, missing
  element, extra element, text change, antialiasing noise.
* Diff magnitude: `mean` and `ratio` values are printed per capture with
  the region bounding box of the largest differing area — engineers fix by
  region, not by guessing.
* Screenshots must come from the real application binary in the container:
  no compositing screenshots from mockups, no hand-placed images.

## 7. Reporting

* `VISUAL_QA_REPORT.md` per run: pass/fail per capture, metrics table,
  artifact links, classifier notes, and the commit ids of baseline vs.
  actual. Shipped with the release artifacts (`loom-bootstrap`).
* A release ships only with a green visual gate for its supported theme
  and locale matrix.
