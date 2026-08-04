# Loom — Feature Status

Status classes: COMPLETE | FUNCTIONAL_WITH_LIMITATIONS | EXPERIMENTAL |
SCAFFOLDED | NOT_STARTED | BLOCKED. Evidence: `VERIFICATION_REPORT.md`,
`TRUTH.md`, readiness audit (2026-08-04), test/binary/smoke/visual gates.

## Suite-wide gates (2026-08-04)

| Gate | Status |
|---|---|
| fmt / clippy (`-D warnings`, loom crates) | COMPLETE (11/11) |
| tests (358 across 11 workspaces) | COMPLETE |
| release builds (8 apps + CLIs) | COMPLETE |
| offline operation (`--network none`) | COMPLETE |
| package archive + extracted-tree verification | COMPLETE |
| native UI matrix (8x3x3 + palette + sample-open) | COMPLETE |
| theme smoke (light/dark/high-contrast distinct) | COMPLETE |
| recorded keyboard journeys (8/8) | COMPLETE |
| readiness audit | FUNCTIONAL_WITH_LIMITATIONS (UI 9.75, functionality 8.35) |

## Applications

| App | Status | Core capability evidence |
|---|---|---|
| Writer | FUNCTIONAL_WITH_LIMITATIONS | editable surface, undo/redo, recovery, `.loomdoc` packages, Markdown/PDF export |
| Sheets | FUNCTIONAL_WITH_LIMITATIONS | formula engine, CSV/TSV, package persistence, undo/redo, grid |
| Present | FUNCTIONAL_WITH_LIMITATIONS | deck model, notes, layouts, transitions, PDF export |
| Photo | FUNCTIONAL_WITH_LIMITATIONS | layers, blend modes, adjustments, PNG/JPEG export, undo/redo |
| Motion | FUNCTIONAL_WITH_LIMITATIONS | layers, keyframes, interpolation, timeline, SVG frame export |
| Video | FUNCTIONAL_WITH_LIMITATIONS | track/clip editing, timeline ops, FFmpeg export, cancellation |
| Studio | FUNCTIONAL_WITH_LIMITATIONS | tracks, regions, PCM/WAV, MIDI/oscillator synthesis, stereo mixing |
| Encode | FUNCTIONAL_WITH_LIMITATIONS | FFmpeg batch queue, presets, progress, cancel, retry, recovery |
| Vision | EXPERIMENTAL | provider/model-pack architecture; CPU reference QR/segmentation/embeddings/audio analysis; production models pending |
| Plugin SDK | EXPERIMENTAL | manifest/package/permissions/validation/trust/update/rollback; complete host ABI pending |

## Per-app keyboard journey evidence

writer `ex` (14 commands), sheets `format`, present `template`, studio
`workspace`, encode `queue`, photo `layer`, motion `frame`, video `clip` —
each 5-step journey (open, query, narrow, invoke, dismiss) passed and is
recorded under `loom-bootstrap/.work/evidence/journeys/`.

## Per-app CLI journey evidence (functional matrix)

create -> validate/inspect/eval/recalc/sort/scene -> export with output
signature checks: 8/8 PASS.

## Not yet shipped (see KNOWN_LIMITATIONS.md)

Professional typography/layout breadth, large-grid virtualization, pivots,
RAW workflows, GPU effects, synchronized playback, production codecs,
plugin hosting ABIs, production vision models, benchmarked performance,
screen-reader certification, fuzz campaigns.