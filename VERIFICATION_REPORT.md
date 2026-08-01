# Loom Verification Report

Generated: 2026-08-01T08:55:03Z by scripts/generate-status-report.sh

This report is evidence-based: source presence and metadata parsing are not build, test, binary, smoke, or visual evidence.

Status classes: COMPLETE | FUNCTIONAL_WITH_LIMITATIONS | EXPERIMENTAL | SCAFFOLDED | NOT_STARTED | BLOCKED

## Cargo workspaces

| repo | cargo | metadata | build log | test log | test cases | app binary |
|------|-------|----------|-----------|----------|------------|-------------|
| loom-core | yes | PASS | PASS | PASS | 84 | — |
| loom-writer | yes | PASS | PASS | PASS | 6 | PASS |
| loom-sheets | yes | PASS | PASS | PASS | 12 | PASS |
| loom-present | yes | PASS | PASS | PASS | 5 | PASS |
| loom-photo | yes | PASS | PASS | PASS | 3 | PASS |
| loom-motion | yes | PASS | PASS | PASS | 3 | PASS |
| loom-video | yes | PASS | PASS | PASS | 3 | PASS |
| loom-studio | yes | PASS | PASS | PASS | 3 | PASS |
| loom-encode | yes | PASS | PASS | PASS | 2 | PASS |
| loom-vision | yes | PASS | PASS | PASS | 72 | — |
| loom-plugin-sdk | yes | PASS | PASS | PASS | 57 | — |

## Application smoke evidence

| app | binary | smoke log |
|-----|---------|------------|
| writer | PASS | PASS |
| sheets | PASS | PASS |
| present | PASS | PASS |
| photo | PASS | PASS |
| motion | PASS | PASS |
| video | PASS | PASS |
| studio | PASS | PASS |
| encode | PASS | PASS |

## Visual QA evidence

- report: INCOMPLETE/FAIL
- source: visual-qa-report.md and loom-bootstrap/.work/screenshots/
- missing baselines or failed comparisons are not passes

## Evidence sources

- build logs: loom-bootstrap/.work/build-<repo>.log
- test logs: loom-bootstrap/.work/test-<repo>.log
- smoke summary: loom-bootstrap/.work/smoke-summary.log
