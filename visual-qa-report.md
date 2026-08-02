# Loom Visual QA Report

Generated: 2026-08-02T13:38:51Z by scripts/visual-qa-all.sh
Size: 1280x800 | Gates: mean absolute error < 1.0 (0..255), differing-pixel ratio < 0.01 after 1px erosion | Baselines: /Users/palaashatri/Code/loom/rust-loom/loom-design-bible/baselines

## Coverage

- executed: default application light/dark theme captures only
- full design-bible matrix: NOT RUN by this harness (high-contrast, text-scale, reduced-motion, locale, component/state, and error-state captures are outside this script)

| app | screenshot | dark theme | baseline | mean absolute error | differing-pixel ratio | result | artifacts |
|-----|------------|------------|----------|---------------------|------------------------|--------|-----------|
| writer | light | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/writer-light.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/writer-light.diff.png |
| writer | dark | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/writer-dark.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/writer-dark.diff.png |
| sheets | light | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/sheets-light.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/sheets-light.diff.png |
| sheets | dark | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/sheets-dark.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/sheets-dark.diff.png |
| present | light | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/present-light.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/present-light.diff.png |
| present | dark | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/present-dark.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/present-dark.diff.png |
| photo | light | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/photo-light.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/photo-light.diff.png |
| photo | dark | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/photo-dark.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/photo-dark.diff.png |
| motion | light | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/motion-light.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/motion-light.diff.png |
| motion | dark | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/motion-dark.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/motion-dark.diff.png |
| video | light | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/video-light.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/video-light.diff.png |
| video | dark | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/video-dark.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/video-dark.diff.png |
| studio | light | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/studio-light.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/studio-light.diff.png |
| studio | dark | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/studio-dark.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/studio-dark.diff.png |
| encode | light | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/encode-light.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/encode-light.diff.png |
| encode | dark | yes | ok | 0.000000 | 0.000000 | PASS | actual=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/screenshots/encode-dark.png; diff=/Users/palaashatri/Code/loom/rust-loom/loom-bootstrap/.work/diffs/encode-dark.diff.png |

## Summary

- screenshots captured: 16
- fixed comparison gates: mean absolute error < 1.0 (0..255) AND differing-pixel ratio < 0.01 after 1px erosion
- comparisons run: 16
- valid comparisons: 16
- diffs beyond gates: 0
- size mismatches: 0
- missing baselines: 0 (INCOMPLETE when non-zero)
- screenshot failures: 0
- apps missing binaries: 0
- comparison/input failures: 0
- coverage run: default application light/dark themes only
- full design-bible matrix: NOT RUN by this harness
- provenance caveat: this gate proves capture/baseline consistency; it is not an independent regression result when baselines come from the same reviewed capture set
- result: PASS
