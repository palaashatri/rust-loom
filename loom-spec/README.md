# Loom Specification Repository

The authoritative product and engineering specification for **Loom**, a
professional, local-first, offline-first creative suite for desktop computers
(word processor, spreadsheet, presentations, photo editor, motion graphics,
video editor, DAW, transcoder), built with Rust and Slint on Linux/desktop.

This repository defines **what Loom is** — scope, capability, terminology, file
formats, compatibility, release criteria, and roadmap. It does **not** contain
implementation code.

## What this repository owns

| Concern | Where it lives |
|---|---|
| Product scope, mission, non-negotiables | `PRODUCT_SPEC.md` |
| Cross-application workflows | `PRODUCT_SPEC.md`, `CROSS_APP_WORKFLOWS.md` |
| System architecture and dependency direction | `ARCHITECTURE.md` |
| Shared terminology (glossary) | `TERMINOLOGY.md` |
| File format family (`.loomdoc`, `.loomtable`, …) | `FILE_FORMAT_FAMILY.md` |
| Release-blocking criteria | `RELEASE_CRITERIA.md` |
| Semver / MSRV / cross-repo pinning policy | `COMPATIBILITY_POLICY.md` |
| Phase plan and honest status | `ROADMAP.md` |
| Per-application capability status | `FEATURE_MATRICES.md` |
| How a capability gets specified and delivered | `IMPLEMENTATION_GUIDE.md` |
| Cross-cutting architecture decisions | `docs/rfcs/` |
| Smaller implementation decisions | `docs/adrs/` |

## What this repository does NOT own

Contracts that belong to other repositories. This spec references them and
never duplicates them:

| Contract | Authority |
|---|---|
| Visual, motion, interaction, accessibility design | `../loom-design-bible/` |
| Shared platform crates (package, document, color, jobs, command, history, text, storage) | `../loom-core/` |
| Vision provider traits, registry, model packs | `../loom-vision/` (esp. `ARCHITECTURE.md`) |
| Plugin manifest, host, sandbox | `../loom-plugin-sdk/` |
| Build orchestration, Docker visual QA, packaging, `COMPATIBILITY.toml` | `../loom-bootstrap/` |
| Per-application engine and CLI details | `../loom-writer/`, `../loom-sheets/`, … |

## How to read this repository

1. Read `PRODUCT_SPEC.md` first — the mission and non-negotiables shape every
   other document.
2. Read `ARCHITECTURE.md` — how the repositories fit together and where each
   contract lives.
3. Consult `TERMINOLOGY.md` whenever a term is ambiguous; terms used in any
   Loom document must match this glossary.
4. Use `FEATURE_MATRICES.md` + `ROADMAP.md` for current status; never trust a
   prose claim over a matrix entry.
5. Read RFCs/ADRs before changing any accepted contract. RFCs are numbered and
   never silently amended.

## Document conventions

- **Status vocabulary** (identical in every Loom document):
  `COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`, `EXPERIMENTAL`, `SCAFFOLDED`,
  `NOT_STARTED`, `BLOCKED`. See `AGENTS.md`.
- **RFCs**: `docs/rfcs/RFC-NNNN-*.md`. Accepted RFCs are normative.
- **ADRs**: `docs/adrs/ADR-NNNN-*.md`. Accepted ADRs are normative.
- Every Loom repository is MIT OR Apache-2.0 except where a distribution
  notice says otherwise (see `docs/adrs/ADR-0001-Slint-Licensing-and-Distribution.md`).

## RFC and ADR index

Accepted RFCs (normative):

- `RFC-0001-Repository-and-Versioning-Strategy.md`
- `RFC-0002-UI-and-Engine-Separation.md`
- `RFC-0003-Slint-Integration-Model.md`
- `RFC-0005-Text-Shaping-and-Layout.md`
- `RFC-0006-File-Package-Format.md`
- `RFC-0007-Undo-and-Transaction-System.md`
- `RFC-0008-Async-Job-Framework.md`
- `RFC-0010-Vision-Provider-Model.md`
- `RFC-0011-Model-Pack-Format.md`
- `RFC-0013-Color-Management.md`
- `RFC-0015-Visual-Regression-System.md`
- `RFC-0018-Autosave-and-Recovery.md`

Not yet drafted (per the roadmap): RFC-0004 (GPU renderer), RFC-0009 (plugin
ABI and sandboxing), RFC-0012 (media framework), RFC-0014 (accessibility),
RFC-0016 (cross-repo compatibility), RFC-0017 (application command system),
RFC-0019 (local search and indexing), RFC-0020 (localization).

Accepted ADRs (normative):

- `docs/adrs/ADR-0001-Slint-Licensing-and-Distribution.md`
- `docs/adrs/ADR-0002-Path-Based-Crate-Pinning.md`
- `docs/adrs/ADR-0003-Headless-Screenshots.md`
- `docs/adrs/ADR-0004-Deterministic-Mutation-Fuzzing.md`
- `docs/adrs/ADR-0005-Internal-PDF-Writer.md`
- `docs/adrs/ADR-0006-Image-Codec-Backend.md`
- `docs/adrs/ADR-0007-No-FFmpeg-in-Initial-Milestone.md`
