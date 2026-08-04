# Loom — Accessibility Report

## Evidence (2026-08-04)

- **Readiness audit dimension "accessibility semantics": 1.00/1.00** — shared
  semantic controls (`accessible-role`, `accessible-label`,
  `accessible-action-default`) in the shared component library plus native
  high-contrast capture evidence.
- **Keyboard journeys 8/8 PASS** — every application's command palette is
  fully keyboard-operable: open (Ctrl+K hook), type to narrow, arrow to
  move selection, Enter to invoke, Escape to dismiss — recorded through the
  real Slint input pipeline with per-step screenshots
  (`loom-bootstrap/.work/evidence/journeys/`).
- **Keyboard-first architecture** — the palette focus scope is an ancestor of
  the workspace layout; key events are dispatched and verified, not
  simulated at the model level.
- **Themes** — light, dark, and high-contrast captured natively for all eight
  apps and byte-distinct (theme smoke matrix PASS), covering high-contrast
  operation.
- **Design specification** — `loom-design-bible/ACCESSIBILITY.md`,
  `KEYBOARD.md`, `THEMING.md`, and `UX_ACCEPTANCE_CHECKLIST.md` define the
  full accessibility program (focus order, reduced motion, scalable UI,
  non-color status, screen-reader labels, configurable shortcuts).

## Honest gaps (not yet verified in this audit)

- Screen-reader (AT-SPI/accessibility-tree) integration has not been
  validated with a real screen reader.
- Reduced-motion behavior is tokenized in the theme system but not yet
  captured or automated.
- Text-scale and RTL layout stress captures are specified but not executed.
- Configurable-shortcut surfaces are designed but not fully shipped.

These are documented requirements in the design bible and remain open work;
nothing above claims screen-reader certification.