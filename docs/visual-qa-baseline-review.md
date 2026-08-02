# Visual baseline review

Date: 2026-08-02

The current visual harness covers the default light and dark application
screens only. It does not cover the full design-bible matrix (high contrast,
text scale, reduced motion, locales, component states, or error states).

## Reviewed capture evidence

- The canonical reviewed Docker capture set in
  `loom-bootstrap/.work/docker-postwriter/screenshots/` contains 16 valid
  1280x800 screenshots: eight applications in light and dark themes. Every
  image was inspected for the correct window, nonblank content, stable layout,
  and visible default selection state.
- The existing Writer baselines describe the old read-only document surface;
  the current editable surface differs at RMSE 0.091097 (Docker light) and
  0.094037 (Docker dark), so those baselines are stale.
- The existing Sheets baselines also drift from the Docker renderer at RMSE
  0.047112 (light) and 0.040684 (dark), primarily from the renderer/font
  contract. They are replaced with the same Docker capture set so the
  committed baselines match the documented capture environment.
- Twelve application/theme baselines were absent. They were added from the
  inspected Docker captures; no baseline was accepted from a host-only render.

The fresh current-worktree Docker run in
`loom-bootstrap/.work/screenshots/` captured and compared all 16 states:
zero diffs, zero missing baselines, zero size mismatches, and zero capture or
comparison failures. The canonical gate report is
`loom-bootstrap/.work/docker-baseline-gate/visual-qa-report.md`.

## Update decision

The baseline update was an intentional capture-contract correction, not an
automatic acceptance of an arbitrary diff. The default light/dark slice is
verified; the full design-bible matrix remains an open QA gap (high contrast,
text scales, reduced motion, locales, component states, and error states).
