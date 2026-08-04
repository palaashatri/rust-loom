# Loom — Master Index

Entry point for the Loom suite workspace. All repositories, key documents,
reports, and evidence live under this parent directory.

## Repository map

| Repository | Role |
|---|---|
| `loom-bootstrap/` | Orchestration: builds, tests, visual QA, offline verification, packaging |
| `loom-spec/` | Authoritative product and engineering specification |
| `loom-design-bible/` | Visual, motion, interaction, and accessibility specification |
| `loom-core/` | Shared platform crates (runtime, UI, command, jobs, package, storage, text, color, media) |
| `loom-vision/` | Local-first computer-vision and perception provider framework |
| `loom-plugin-sdk/` | Sandboxed extension system (manifest, host, CLI) |
| `loom-writer/` | Word processor and page-layout application |
| `loom-sheets/` | Spreadsheet and data-analysis application |
| `loom-present/` | Presentation authoring application |
| `loom-photo/` | Nondestructive image-editing application |
| `loom-motion/` | Motion-graphics, animation, and compositing application |
| `loom-video/` | Nonlinear video editor |
| `loom-studio/` | Digital audio workstation |
| `loom-encode/` | Media transcoding and delivery application |
| `loom-samples/` | Original sample projects, conformance fixtures, and media |

See `REPOSITORY_MAP.md` for ownership and dependency direction.

## Entry documents

- `TRUTH.md` — human-maintained statement of what the workspace actually contains
- `VERIFICATION_REPORT.md` — evidence-based gate report (generated from logs)
- `FEATURE_STATUS.md` — per-application implementation status classification
- `KNOWN_LIMITATIONS.md` — honest gap list and audit blockers

## How-to guides

- `BUILD_ALL.md` — build every workspace
- `RUN_ALL.md` — launch the eight applications and CLIs
- `TEST_ALL.md` — test matrix, commands, and results
- `VISUAL_QA_ALL.md` — native visual QA evidence (UI matrix, theme smoke, journeys)
- `loom-bootstrap/BOOTSTRAP.md` — orchestration instructions and compatibility contract

## Reports

- `LICENSE_REPORT.md` — licensing position and dependency policy
- `DEPENDENCY_REPORT.md` — dependency inventory and audit status
- `SECURITY_REPORT.md` — security posture and evidence
- `ACCESSIBILITY_REPORT.md` — accessibility evidence and gaps
- `PERFORMANCE_REPORT.md` — performance budgets and measured evidence

## Audit evidence (generated, gitignored by convention)

- `loom-bootstrap/.work/evidence/native-functional-matrix.json` — CLI journey matrix (8/8)
- `loom-bootstrap/.work/evidence/ui/native-ui-matrix.json` — 8 apps x 3 sizes x 3 themes
- `loom-bootstrap/.work/evidence/ui/*.png` — deterministic native captures
- `loom-bootstrap/.work/evidence/journeys/<app>/<app>.json` + PNGs — recorded keyboard journeys
- `loom-bootstrap/.work/evidence/packages/` — native package evidence

## Artifacts

- `Loom-Complete.zip` — deterministic suite archive (see `LOOM_MASTER_INDEX.md` build note below)
- `Loom-Complete.zip.sha256` — checksum sidecar

## Current status (2026-08-04 audit)

- All gates PASS: fmt 11/11, clippy 11/11 (`loom_crate_issues=0`), test 11/11 (358 tests),
  build 11/11 (release), offline test 11/11 (`--network none`), package verification 11/11.
- Readiness audit: UI 9.75/10, functionality 8.35/10, keyboard journeys 8/8.
- 8 honest blockers listed in `KNOWN_LIMITATIONS.md`.
