# Loom Verification Report

Generated: 2026-08-04T08:12:01Z by scripts/generate-status-report.sh

This report is evidence-based: source presence and metadata parsing are not build, test, binary, smoke, or visual evidence.

Status classes: COMPLETE | FUNCTIONAL_WITH_LIMITATIONS | EXPERIMENTAL | SCAFFOLDED | NOT_STARTED | BLOCKED

## Cargo workspaces

| repo | cargo | metadata | build log | test log | test cases | app binary |
|------|-------|----------|-----------|----------|------------|-------------|
| loom-core | yes | PASS | PASS | PASS | 112 | — |
| loom-writer | yes | PASS | PASS | PASS | 21 | PASS |
| loom-sheets | yes | PASS | PASS | PASS | 22 | PASS |
| loom-present | yes | PASS | PASS | PASS | 9 | PASS |
| loom-photo | yes | PASS | PASS | PASS | 10 | PASS |
| loom-motion | yes | PASS | PASS | PASS | 10 | PASS |
| loom-video | yes | PASS | PASS | PASS | 9 | PASS |
| loom-studio | yes | PASS | PASS | PASS | 10 | PASS |
| loom-encode | yes | PASS | PASS | PASS | 8 | PASS |
| loom-vision | yes | PASS | PASS | PASS | 83 | — |
| loom-plugin-sdk | yes | PASS | PASS | PASS | 64 | — |

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

- report: PASS
- source: native-ui-matrix.json (deterministic native captures), visual-smoke-matrix report, and recorded keyboard journeys
- missing baselines or failed comparisons are not passes

## Evidence sources

- build logs: loom-bootstrap/.work/build-<repo>.log
- test logs: loom-bootstrap/.work/test-<repo>.log
- smoke summary: loom-bootstrap/.work/smoke-summary.log
- native UI matrix: loom-bootstrap/.work/evidence/ui/native-ui-matrix.json
