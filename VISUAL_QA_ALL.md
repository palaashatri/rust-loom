# Visual QA

## One command

```bash
cd loom-bootstrap
bash scripts/visual-qa-all.sh           # screenshots every built app in light+dark, compares required baselines
```

Latest result (2026-08-01): **INCOMPLETE/FAIL** — 16 fresh captures, 4
comparisons at `metric=0.000000`, 0 diffs, and 12 missing baselines. Missing
baselines are failures; capture count alone is not a visual pass.

## What it does

1. Finds each implemented app binary (`target/{release,debug}/loom-<app>`).
2. Runs `--screenshot <file> --size 1280x800 --theme light` and the equivalent dark capture.
3. Compares with `scripts/img-compare.sh` (perceptual metric, tolerance 0.02).
4. Writes `visual-qa-report.md` to the parent root; exits 1 on any diff, missing baseline, missing binary, failed capture, or unavailable comparison tool.

## Baselines

- `loom-design-bible/baselines/writer/{writer-light,dark}.png`
- `loom-design-bible/baselines/sheets/{sheets-light,dark}.png`
- `loom-core/crates/loom-ui/baselines/light/smoke-window.png` (component gallery smoke; its tracked `.actual.json` remains historical evidence, not an approval)

Baselines are created deliberately in the Docker visual environment (never
auto-approved): run the app at 1280×800, inspect the screenshot, then copy it
into the baseline directory only after design review.

## Application baseline status

All eight application binaries currently build and capture. Writer and Sheets
have light/dark baselines; Present, Photo, Motion, Video, Studio, and Encode
currently have no committed app baselines and therefore keep the gate
incomplete.

## Headless details

Apps render through the loom-test-support software capture platform; no display
is needed. The host and container font stacks differ; see
KNOWN_LIMITATIONS.md. The container path is the required environment for
creating new committed baselines.
