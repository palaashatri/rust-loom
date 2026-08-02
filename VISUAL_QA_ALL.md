# Visual QA

## One command

```bash
cd loom-bootstrap
bash scripts/visual-qa-all.sh           # screenshots every built app in light+dark, compares required baselines
```

Latest result (2026-08-02): **PASS for the default light/dark slice** — 16
fresh captures, 16 comparisons at `metric=0.000000`, 0 diffs, 0 size
mismatches, 0 missing baselines, and 0 capture/comparison failures. The full
design-bible matrix remains unrun; capture count alone is not a visual pass.

## What it does

1. Finds each implemented app binary (`target/{release,debug}/loom-<app>`).
2. Runs `--screenshot <file> --size 1280x800 --theme light` and the equivalent dark capture.
3. Compares with `scripts/img-compare.sh` (mean absolute error < 1.0 and
   differing-pixel ratio < 0.01 after one-pixel erosion).
4. Writes `visual-qa-report.md` to the parent root; exits 1 on any diff, missing baseline, missing binary, failed capture, or unavailable comparison tool.

## Baselines

- `loom-design-bible/baselines/{writer,sheets,present,photo,motion,video,studio,encode}/*-{light,dark}.png`
- `loom-core/crates/loom-ui/baselines/light/smoke-window.png` (component gallery smoke; its tracked `.actual.json` remains historical evidence, not an approval)

Baselines are created deliberately in the Docker visual environment (never
auto-approved): run the app at 1280×800, inspect the screenshot, then copy it
into the baseline directory only after design review. The current application
baselines came from the reviewed Docker capture set used for this gate, so the
16/16 result proves capture/baseline consistency rather than independent
historical regression protection.

## Application baseline status

All eight application binaries currently build and capture. All eight have
committed light/dark baselines and pass the default visual gate. High-contrast,
text-scale, reduced-motion, locale, component/state, and error-state coverage
remain outside this harness.

## Headless details

Apps render through the loom-test-support software capture platform; no display
is needed. The host and container font stacks differ; see
KNOWN_LIMITATIONS.md. The container path is the required environment for
creating new committed baselines.
