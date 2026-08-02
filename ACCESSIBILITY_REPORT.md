# Loom Accessibility Report

Status: foundation present, verification partially automated. Accessibility is a
release requirement, not a post-launch add-on.

## In place

- Keyboard: every action maps to commands; buttons are keyboard-reachable (Slint focus);
  shortcut configuration architecture documented (loom-core loom-shortcuts crate exists).
- Visible focus: design tokens define focus-visible styles; smoke fixture exercises them.
- Screen-reader labels: accessibility strings on controls in loom-ui components
  (`accessible-label` on buttons/fields); audit annotations exist in the design bible.
- Non-color status indicators: status surfaces carry icons + text, not color alone.
- Reduced motion: theme token (`Theme.active-motion`) defined; motion spec documented in
  loom-design-bible (MOTION.md); reduced-motion final states specified for visual QA.
- Themes: light and dark are verified in the current application screenshot
  gate. High-contrast contrast targets are specified in the design tokens but
  that visual matrix is not run by the current harness.
- Scalable UI: token-based sizing; Slint supports UI scaling (scale parameter in the
  capture platform and window scaling at runtime).

## Verified this session

- writer + sheets render in the default light/dark themes (visual QA PASS for
  the current capture/baseline gate).
- Focus/keyboard E2E automation: NOT yet implemented (no input-injection test harness).

## Not yet done

- Screen-reader E2E verification (no Orca-based CI).
- Keyboard navigation tests for the sheets grid and writer canvas.
- RTL layout test screens and pseudolocale (localization architecture documented only).
- High-contrast visual baselines in the QA pipeline (light/dark only today).

## Honest status

The UI is built on accessible primitives with documented behavior, but automated
accessibility regression testing is incomplete; treat current state as
FUNCTIONAL_WITH_LIMITATIONS until E2E a11y tests exist.
