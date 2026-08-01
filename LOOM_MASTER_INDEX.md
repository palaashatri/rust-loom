# Loom Master Index

Entry point for the Loom suite. All repositories live under this directory.

## Repositories

| Repo | Role | Status |
|------|------|--------|
| [loom-bootstrap/](loom-bootstrap/) | Orchestration: builds, tests, visual QA, offline tests, packaging | ACTIVE |
| [loom-spec/](loom-spec/) | Authoritative product + engineering specification | ACTIVE (docs) |
| [loom-design-bible/](loom-design-bible/) | Visual/motion/interaction/accessibility spec + baselines | ACTIVE (docs + baselines) |
| [loom-core/](loom-core/) | Shared platform crates (ui, package, storage, jobs, pdf, ...) | ACTIVE |
| [loom-vision/](loom-vision/) | Local-first CV/perception framework | SCAFFOLDED |
| [loom-plugin-sdk/](loom-plugin-sdk/) | Sandboxed extension SDK | SCAFFOLDED |
| [loom-writer/](loom-writer/) | Word processor app | FUNCTIONAL_WITH_LIMITATIONS |
| [loom-sheets/](loom-sheets/) | Spreadsheet app | FUNCTIONAL_WITH_LIMITATIONS |
| [loom-present/](loom-present/) | Presentation app | NOT_STARTED (docs) |
| [loom-photo/](loom-photo/) | Image editor | NOT_STARTED (docs) |
| [loom-motion/](loom-motion/) | Motion graphics / compositing | NOT_STARTED (docs) |
| [loom-video/](loom-video/) | Video editor | NOT_STARTED (docs) |
| [loom-studio/](loom-studio/) | DAW | NOT_STARTED (docs) |
| [loom-encode/](loom-encode/) | Transcoding app | NOT_STARTED (docs) |
| [loom-samples/](loom-samples/) | Sample projects | ACTIVE (samples for writer/sheets) |

## Key documents

- [REPOSITORY_MAP.md](REPOSITORY_MAP.md) — ownership and dependency direction
- [FEATURE_STATUS.md](FEATURE_STATUS.md) — per-area implementation status (honest)
- [KNOWN_LIMITATIONS.md](KNOWN_LIMITATIONS.md) — known gaps and defects
- [VERIFICATION_REPORT.md](VERIFICATION_REPORT.md) — repo-level verification matrix
- [visual-qa-report.md](visual-qa-report.md) — latest visual QA results
- [Loom-Manifest.json](Loom-Manifest.json) — packaging manifest with checksums
- [Loom-Verification-Report.md](Loom-Verification-Report.md) — package verification record

## How-to quick links

- [BUILD_ALL.md](BUILD_ALL.md) — build every repo
- [RUN_ALL.md](RUN_ALL.md) — run the applications
- [TEST_ALL.md](TEST_ALL.md) — run all tests
- [VISUAL_QA_ALL.md](VISUAL_QA_ALL.md) — capture screenshots and compare baselines
- Offline verification: `loom-bootstrap/scripts/docker-offline-test.sh` (network-none container)

## Reports

- [LICENSE_REPORT.md](LICENSE_REPORT.md) — licensing position and audit status
- [DEPENDENCY_REPORT.md](DEPENDENCY_REPORT.md) — dependency audit status
- [SECURITY_REPORT.md](SECURITY_REPORT.md) — security posture
- [ACCESSIBILITY_REPORT.md](ACCESSIBILITY_REPORT.md) — accessibility status
- [PERFORMANCE_REPORT.md](PERFORMANCE_REPORT.md) — performance status and budgets
