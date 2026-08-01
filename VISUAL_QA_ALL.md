# Visual QA

## One command

```bash
cd loom-bootstrap
bash scripts/visual-qa-all.sh           # screenshots writer/sheets (light+dark), compares baselines
```

Latest result (2026-08-01): PASS — writer light/dark and sheets light/dark all
`metric=0.000000` vs baselines in `loom-design-bible/baselines/<app>/`.

## What it does

1. Finds each implemented app binary (`target/{release,debug}/loom-<app>`).
2. Runs `--screenshot <file> --size 1280x800` (light) and `--theme dark`.
3. Compares with `scripts/img-compare.sh` (perceptual metric, tolerance 0.02).
4. Writes `visual-qa-report.md` to the parent root; exits 1 on any diff.

## Baselines

- `loom-design-bible/baselines/writer/{writer-light,dark}.png`
- `loom-design-bible/baselines/sheets/{sheets-light,dark}.png`
- `loom-core/crates/loom-ui/baselines/light/smoke-window.png` (component gallery smoke)

Baselines are created deliberately (never auto-approved): run the app, inspect the
screenshot, then copy it into the baseline directory.

## Apps without binaries

present/photo/motion/video/studio/encode are reported as skipped until they exist.

## Headless details

Apps render through the loom-test-support software capture platform; no display
needed. Font stack is the host's — see KNOWN_LIMITATIONS.md for the container caveat.
