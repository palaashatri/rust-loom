# Loom Design Bible

The authoritative visual, motion, interaction, and accessibility specification for
the Loom creative suite (Writer, Sheets, Present, Photo, Motion, Video, Studio,
Encode, and Loom Vision).

Loom's design language in six words: **calm, precise, minimal, neutral, professional,
fast.** The chrome palette is deliberately neutral so that the user's content
carries the color (decision GOV-05). Everything in this repository exists to make that language unambiguous,
testable, and consistent across every application in the suite.

## Repository role

This repository contains the normative design specification and machine-readable
contracts. Runtime Slint components, the component gallery, and screenshot
baselines live in `loom-core/crates/loom-ui`; they are not stored here. What this
repository provides:

* The complete design contract applications are built against.
* Machine-readable design tokens in `tokens/loom.toml` (the canonical source).
* The visual QA process and baseline storage contract.
* Design review and acceptance procedures.

Applications never invent their own tokens, spacing, colors, or motion values.
They consume this contract.

## Document map

The prose documents below were consolidated into
[`docs/archive/AGENTS-ARCHIVE.md`](../docs/archive/AGENTS-ARCHIVE.md); each link
opens the archived copy. Some of that prose still repeats older values, such as
the former terracotta palette and some motion and sheet-tab metrics. Where it
disagrees with `tokens/loom.toml` or `contracts/desktop-ui.toml`, the TOML is
authoritative and the prose is a known defect tracked as GOV-02 in
[AGENTS.md](../AGENTS.md#gov-02--reconcile-design-bible-prose-with-canonical-contracts).

| Document | Contents |
|---|---|
| [DESIGN_BIBLE.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-design-bible-md) | Master document: principles, layout, typography, color, components, interaction, accessibility baseline, theming. Read this first. |
| [DESIGN_PRINCIPLES.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-design-principles-md) | The ten+ governing principles, each with rationale and anti-examples. |
| [DESIGN_TOKENS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-design-tokens-md) | Token architecture and the full token table. |
| [TYPOGRAPHY.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-typography-md) | Font stack, type scale, line lengths, heading hierarchy, tabular figures, i18n/RTL, scaling. |
| [COLOR.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-color-md) | Palettes, usage rules, contrast requirements, non-color indicators, data-viz palette. |
| [ICONOGRAPHY.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-iconography-md) | The original Loom icon family: grid, stroke, optical rules, icon inventory, labels. |
| [LAYOUT.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-layout-md) | Window anatomy, grid and spacing rules for application chrome. |
| [SPACING.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-spacing-md) | Spacing tokens and usage patterns. |
| [MOTION.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-motion-md) | Duration/easing tokens, motion grammar, reduced motion, interruption. |
| [COMPONENTS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-components-md) | Component inventory with full state matrices. |
| [WINDOWS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-windows-md) | Main window, dialogs, popovers, multi-window policy. |
| [TOOLBARS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-toolbars-md) | Single-row contextual toolbar model. |
| [SIDEBARS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-sidebars-md) | Collapsible sidebar behavior. |
| [INSPECTORS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-inspectors-md) | Contextual inspector model. |
| [MENUS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-menus-md) | Menu bar, context menus, mnemonics. |
| [COMMAND_PALETTE.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-command-palette-md) | Searchable command palette behavior. |
| [DIALOGS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-dialogs-md) | Modal policy and destructive confirmation. |
| [NOTIFICATIONS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-notifications-md) | Toasts, status bar reporting, error and recovery prompts. |
| [SELECTION.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-selection-md) | Selection visuals and behavior. |
| [CANVAS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-canvas-md) | Canvas chrome, zoom, pan, guides. |
| [TIMELINE.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-timeline-md) | Timeline anatomy and scrubbing. |
| [SPREADSHEET.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-spreadsheet-md) | Grid chrome, freeze panes, virtualization. |
| [DOCUMENT_EDITOR.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-document-editor-md) | Page canvas, cursor behavior, IME. |
| [DRAG_AND_DROP.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-drag-and-drop-md) | Drag feedback, drop targets, cross-app drag. |
| [KEYBOARD.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-keyboard-md) | Key assignments, configuration policy. |
| [POINTER_AND_PEN.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-pointer-and-pen-md) | Targeting, cursors, pen goals. |
| [ACCESSIBILITY.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-accessibility-md) | The complete, release-blocking accessibility contract. |
| [THEMING.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-theming-md) | Theme matrix and token mapping. |
| [VISUAL_QA.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-visual-qa-md) | Visual regression process, baselines, tolerances. |
| [PERFORMANCE.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-performance-md) | UI performance budgets. |
| [ANTI_PATTERNS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-anti-patterns-md) | Twenty+ named anti-patterns with examples. |
| [DESIGN_REVIEW.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-design-review-md) | Review checklist and process. |
| [UX_ACCEPTANCE_CHECKLIST.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-ux-acceptance-checklist-md) | Per-app acceptance checklist. |
| `tokens/loom.toml` | Canonical machine-readable tokens. |
| `contracts/desktop-ui.toml` | Normative desktop geometry, palette, and interaction contract. |
| `contracts/ui-foundation.toml` | Shared UI foundation status and mechanical acceptance requirements. |
| `contracts/geometry-manifest.toml` | Generated per-app geometry and source evidence; verify its generator and hashes before relying on it. |
| [ADR-0001-design-tokens.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-docs-adrs-adr-0001-design-tokens-md) | Decision record for the token architecture. |
| `docs/adrs/` | Further decision records (added over time). |
| `loom-core/crates/loom-ui/ui/foundation/` | Shared Slint foundation and interactive gallery. |
| `loom-core/crates/loom-ui/baselines/foundation/` | Current deterministic foundation captures. |

## Reading order for new agents

1. `README.md`, [AGENTS.md](../AGENTS.md)
2. [DESIGN_BIBLE.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-design-bible-md)
3. The three contract files: [DESIGN_TOKENS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-design-tokens-md), [THEMING.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-theming-md), [ACCESSIBILITY.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-accessibility-md)
4. The area documents relevant to your subsystem
5. [ANTI_PATTERNS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-anti-patterns-md) and [UX_ACCEPTANCE_CHECKLIST.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-ux-acceptance-checklist-md) before claiming anything done

## 2027 accessibility and identity notes

* Accessibility review uses WCAG 2.2 AA as applied to software by EN 301 549
  V4.1.1 ([AGENTS.md §18](../AGENTS.md#18-2027-readiness-baseline--platforms-accessibility-supply-chain-and-regulation)).
  `contracts/desktop-ui.toml` sets a 28 px minimum pointer target. Three
  handle targets are smaller than 24 px and need recorded exceptions or larger
  hit areas (GOV-06).
* The palette stays as defined in `tokens/loom.toml` (GOV-05, decided
  2026-10-25). Archived prose that describes a warm terracotta palette is
  outdated and will be corrected under GOV-02.

## Contract integrity rules

* `tokens/loom.toml` is the normative token source. [DESIGN_TOKENS.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-design-tokens-md),
  [THEMING.md](../docs/archive/AGENTS-ARCHIVE.md#source-loom-design-bible-theming-md), and any generated Slint constants derive from it.
* If a value appears in two documents and they disagree, the documents have a
  defect. Fix the defect; do not accept it.
* Nothing in this repository is a suggestion. All requirements are binding on
  application teams unless explicitly marked *proposal*, *goal*, or *future*.

## Related repositories

* `loom-core` — shared platform implementation, including the token generator.
* [SLOPOS](https://github.com/palaashatri/rust-slopos) — an independent Linux
  desktop. Its own design bible (`docs/CONTRACT.md` §5.2) uses this bible's
  *structure*: principles, single token source, theme matrix, anti-patterns and
  review. It does not use Loom's values. When Loom runs on SLOPOS, a versioned
  Slint theme/chrome adapter maps SLOPOS semantic roles onto the roles in
  `tokens/loom.toml` (future gate LOOM-SLOPOS-03, [AGENTS.md §15](../AGENTS.md#15-future-interoperability-goal--loom-as-sloposs-creative-suite)).
  The adapter never edits this token source, and document content is never re-themed.
* [PARAMA](https://github.com/palaashatri/parama) — an independent OS. A native
  PARAMA Loom target is far-future work. It would keep this bible unchanged
  and reach SLOPOS styling only through the same adapter.

The product-scope specification (`loom-spec`) and the plugin SDK are not part of
`main`; they remain on the `apps/later` branch.
