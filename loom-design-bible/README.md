# Loom Design Bible

The authoritative visual, motion, interaction, and accessibility specification for
the Loom creative suite (Writer, Sheets, Present, Photo, Motion, Video, Studio,
Encode, and Loom Vision).

Loom's design language in six words: **calm, precise, minimal, warm, professional,
fast.** Everything in this repository exists to make that language unambiguous,
testable, and consistent across eight applications.

## Repository role

This repository is **specification only**. It contains no application code and no
Slint components. The component gallery, Slint example components, and screenshot
baselines arrive in a later milestone. What it must contain, from day one:

* The complete design contract applications are built against.
* Machine-readable design tokens in `tokens/loom.toml` (the canonical source).
* The visual QA process and baseline storage contract (`baselines/`, added with
  the gallery milestone).
* Design review and acceptance procedures.

Applications never invent their own tokens, spacing, colors, or motion values.
They consume this contract.

## Document map

| Document | Contents |
|---|---|
| `DESIGN_BIBLE.md` | Master document: principles, layout, typography, color, components, interaction, accessibility baseline, theming. Read this first. |
| `DESIGN_PRINCIPLES.md` | The ten+ governing principles, each with rationale and anti-examples. |
| `DESIGN_TOKENS.md` | Token architecture and the full token table. |
| `TYPOGRAPHY.md` | Font stack, type scale, line lengths, heading hierarchy, tabular figures, i18n/RTL, scaling. |
| `COLOR.md` | Palettes, usage rules, contrast requirements, non-color indicators, data-viz palette. |
| `ICONOGRAPHY.md` | The original Loom icon family: grid, stroke, optical rules, icon inventory, labels. |
| `LAYOUT.md` | Window anatomy, grid and spacing rules for application chrome. |
| `SPACING.md` | Spacing tokens and usage patterns. |
| `MOTION.md` | Duration/easing tokens, motion grammar, reduced motion, interruption. |
| `COMPONENTS.md` | Component inventory with full state matrices. |
| `WINDOWS.md` | Main window, dialogs, popovers, multi-window policy. |
| `TOOLBARS.md` | Single-row contextual toolbar model. |
| `SIDEBARS.md` | Collapsible sidebar behavior. |
| `INSPECTORS.md` | Contextual inspector model. |
| `MENUS.md` | Menu bar, context menus, mnemonics. |
| `COMMAND_PALETTE.md` | Searchable command palette behavior. |
| `DIALOGS.md` | Modal policy and destructive confirmation. |
| `NOTIFICATIONS.md` | Toasts, status bar reporting, error and recovery prompts. |
| `SELECTION.md` | Selection visuals and behavior. |
| `CANVAS.md` | Canvas chrome, zoom, pan, guides. |
| `TIMELINE.md` | Timeline anatomy and scrubbing. |
| `SPREADSHEET.md` | Grid chrome, freeze panes, virtualization. |
| `DOCUMENT_EDITOR.md` | Page canvas, cursor behavior, IME. |
| `DRAG_AND_DROP.md` | Drag feedback, drop targets, cross-app drag. |
| `KEYBOARD.md` | Key assignments, configuration policy. |
| `POINTER_AND_PEN.md` | Targeting, cursors, pen goals. |
| `ACCESSIBILITY.md` | The complete, release-blocking accessibility contract. |
| `THEMING.md` | Theme matrix and token mapping. |
| `VISUAL_QA.md` | Visual regression process, baselines, tolerances. |
| `PERFORMANCE.md` | UI performance budgets. |
| `ANTI_PATTERNS.md` | Twenty+ named anti-patterns with examples. |
| `DESIGN_REVIEW.md` | Review checklist and process. |
| `UX_ACCEPTANCE_CHECKLIST.md` | Per-app acceptance checklist. |
| `tokens/loom.toml` | Canonical machine-readable tokens. |
| `docs/adrs/ADR-0001-design-tokens.md` | Decision record for the token architecture. |
| `docs/adrs/` | Further decision records (added over time). |
| `baselines/` | Visual regression baselines, `<app>/<name>.png` (added with gallery milestone). |
| `test/` | Reserved for the gallery milestone's visual test harness fixtures. |

## Reading order for new agents

1. `README.md`, `AGENTS.md`
2. `DESIGN_BIBLE.md`
3. The three contract files: `DESIGN_TOKENS.md`, `THEMING.md`, `ACCESSIBILITY.md`
4. The area documents relevant to your subsystem
5. `ANTI_PATTERNS.md` and `UX_ACCEPTANCE_CHECKLIST.md` before claiming anything done

## Contract integrity rules

* `tokens/loom.toml` is the canonical token source. `DESIGN_TOKENS.md`,
  `THEMING.md`, and any generated Slint constants derive from it.
* If a value appears in two documents and they disagree, the documents have a
  defect. Fix the defect; do not accept it.
* Nothing in this repository is a suggestion. All requirements are binding on
  application teams unless explicitly marked *proposal*, *goal*, or *future*.

## Related repositories

* `loom-spec` — product scope and capability contracts (references this Bible).
* `loom-core` — shared platform implementation, including the token generator.
* `loom-plugin-sdk` — extension contract; visual and interaction rules apply to
  plugins as they do to first-party UI.
