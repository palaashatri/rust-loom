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
| Product scope, mission, non-negotiables | [PRODUCT_SPEC.md](../AGENTS.md#source-loom-spec-product-spec-md) |
| Cross-application workflows | [PRODUCT_SPEC.md](../AGENTS.md#source-loom-spec-product-spec-md), [CROSS_APP_WORKFLOWS.md](../AGENTS.md#source-loom-spec-cross-app-workflows-md) |
| System architecture and dependency direction | [ARCHITECTURE.md](../AGENTS.md#source-loom-spec-architecture-md) |
| Shared terminology (glossary) | [TERMINOLOGY.md](../AGENTS.md#source-loom-spec-terminology-md) |
| File format family (`.loomdoc`, `.loomtable`, …) | [FILE_FORMAT_FAMILY.md](../AGENTS.md#source-loom-spec-file-format-family-md) |
| Release-blocking criteria | [RELEASE_CRITERIA.md](../AGENTS.md#source-loom-spec-release-criteria-md) |
| Semver / MSRV / cross-repo pinning policy | [COMPATIBILITY_POLICY.md](../AGENTS.md#source-loom-spec-compatibility-policy-md) |
| Phase plan and honest status | [ROADMAP.md](../AGENTS.md#source-loom-spec-roadmap-md) |
| Per-application capability status | [FEATURE_MATRICES.md](../AGENTS.md#source-loom-spec-feature-matrices-md) |
| How a capability gets specified and delivered | [IMPLEMENTATION_GUIDE.md](../AGENTS.md#source-loom-spec-implementation-guide-md) |
| Cross-cutting architecture decisions | `docs/rfcs/` |
| Smaller implementation decisions | `docs/adrs/` |

## What this repository does NOT own

Contracts that belong to other repositories. This spec references them and
never duplicates them:

| Contract | Authority |
|---|---|
| Visual, motion, interaction, accessibility design | `../loom-design-bible/` |
| Shared platform crates (package, document, color, jobs, command, history, text, storage) | `../loom-core/` |
| Vision provider traits, registry, model packs | `../loom-vision/` (esp. [ARCHITECTURE.md](../AGENTS.md#source-loom-spec-architecture-md)) |
| Plugin manifest, host, sandbox | `../loom-plugin-sdk/` |
| Build orchestration, Docker visual QA, packaging, `COMPATIBILITY.toml` | `../loom-bootstrap/` |
| Per-application engine and CLI details | `../loom-writer/`, `../loom-sheets/`, … |

## How to read this repository

1. Read [PRODUCT_SPEC.md](../AGENTS.md#source-loom-spec-product-spec-md) first — the mission and non-negotiables shape every
   other document.
2. Read [ARCHITECTURE.md](../AGENTS.md#source-loom-spec-architecture-md) — how the repositories fit together and where each
   contract lives.
3. Consult [TERMINOLOGY.md](../AGENTS.md#source-loom-spec-terminology-md) whenever a term is ambiguous; terms used in any
   Loom document must match this glossary.
4. Use [FEATURE_MATRICES.md](../AGENTS.md#source-loom-spec-feature-matrices-md) + [ROADMAP.md](../AGENTS.md#source-loom-spec-roadmap-md) for current status; never trust a
   prose claim over a matrix entry.
5. Read RFCs/ADRs before changing any accepted contract. RFCs are numbered and
   never silently amended.

## Document conventions

- **Status vocabulary** (identical in every Loom document):
  `COMPLETE`, `FUNCTIONAL_WITH_LIMITATIONS`, `EXPERIMENTAL`, `SCAFFOLDED`,
  `NOT_STARTED`, `BLOCKED`. See [AGENTS.md](../AGENTS.md).
- **RFCs**: read the RFC sections in the [consolidated reference index](../AGENTS.md#index). Accepted RFCs are normative.
- **ADRs**: read the ADR sections in the [consolidated reference index](../AGENTS.md#index). Accepted ADRs are normative.
- Every Loom repository is MIT OR Apache-2.0 except where a distribution
  notice says otherwise (see [ADR-0001-Slint-Licensing-and-Distribution.md](../AGENTS.md#source-loom-spec-docs-adrs-adr-0001-slint-licensing-and-distribution-md)).

## RFC and ADR index

Accepted RFCs (normative):

- [RFC-0001-Repository-and-Versioning-Strategy.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0001-repository-and-versioning-strategy-md)
- [RFC-0002-UI-and-Engine-Separation.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0002-ui-and-engine-separation-md)
- [RFC-0003-Slint-Integration-Model.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0003-slint-integration-model-md)
- [RFC-0005-Text-Shaping-and-Layout.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0005-text-shaping-and-layout-md)
- [RFC-0006-File-Package-Format.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0006-file-package-format-md)
- [RFC-0007-Undo-and-Transaction-System.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0007-undo-and-transaction-system-md)
- [RFC-0008-Async-Job-Framework.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0008-async-job-framework-md)
- [RFC-0010-Vision-Provider-Model.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0010-vision-provider-model-md)
- [RFC-0011-Model-Pack-Format.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0011-model-pack-format-md)
- [RFC-0013-Color-Management.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0013-color-management-md)
- [RFC-0015-Visual-Regression-System.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0015-visual-regression-system-md)
- [RFC-0018-Autosave-and-Recovery.md](../AGENTS.md#source-loom-spec-docs-rfcs-rfc-0018-autosave-and-recovery-md)

Not yet drafted (per the roadmap): RFC-0004 (GPU renderer), RFC-0009 (plugin
ABI and sandboxing), RFC-0012 (media framework), RFC-0014 (accessibility),
RFC-0016 (cross-repo compatibility), RFC-0017 (application command system),
RFC-0019 (local search and indexing), RFC-0020 (localization).

Accepted ADRs (normative):

- [ADR-0001-Slint-Licensing-and-Distribution.md](../AGENTS.md#source-loom-spec-docs-adrs-adr-0001-slint-licensing-and-distribution-md)
- [ADR-0002-Path-Based-Crate-Pinning.md](../AGENTS.md#source-loom-spec-docs-adrs-adr-0002-path-based-crate-pinning-md)
- [ADR-0003-Headless-Screenshots.md](../AGENTS.md#source-loom-spec-docs-adrs-adr-0003-headless-screenshots-md)
- [ADR-0004-Deterministic-Mutation-Fuzzing.md](../AGENTS.md#source-loom-spec-docs-adrs-adr-0004-deterministic-mutation-fuzzing-md)
- [ADR-0005-Internal-PDF-Writer.md](../AGENTS.md#source-loom-spec-docs-adrs-adr-0005-internal-pdf-writer-md)
- [ADR-0006-Image-Codec-Backend.md](../AGENTS.md#source-loom-spec-docs-adrs-adr-0006-image-codec-backend-md)
- [ADR-0007-No-FFmpeg-in-Initial-Milestone.md](../AGENTS.md#source-loom-spec-docs-adrs-adr-0007-no-ffmpeg-in-initial-milestone-md)
