# Loom Design Tokens

Loom has one token authority and one behavioral geometry authority:

- [`tokens/loom.toml`](tokens/loom.toml) — primitive and semantic visual values.
- [`contracts/desktop-ui.toml`](contracts/desktop-ui.toml) — component metrics, layout rules, responsive behavior, forbidden patterns, and acceptance thresholds.

[`MECHANICAL_DESIGN_STANDARD.md`](MECHANICAL_DESIGN_STANDARD.md) explains how to apply both. If a prose document duplicates a value and disagrees with TOML, the TOML value is authoritative and the prose must be corrected in the same change.

## Rules

1. Application UI consumes semantic roles or shared `loom-ui` components; it does not define local palette values.
2. Standard control sizes, radii, toolbar geometry, panel geometry, status geometry, focus treatment, and typography are shared-system values, not application choices.
3. A new token is added only when an existing semantic role cannot express the requirement. Do not add a token merely to preserve arbitrary legacy geometry.
4. Theme variants change semantic values, never token names.
5. Content-specific roles such as paper, media stage, grid, waveform, or chart series are valid only for actual content surfaces; they must not be used to decorate chrome.
6. The Loom accent is reserved for focus, selection, active/checked state, primary actions, and meaningful emphasis. It is not a background-decoration color.
7. Shadows are absent from routine chrome. The only general elevation token is for menus/popovers/tooltips. Document/media content may define content-specific elevation only through a reviewed contract extension.
8. Interactive state changes never alter layout geometry.
9. Runtime `theme.slint`, this TOML source, and the desktop contract are checked together by `loom-bootstrap/scripts/audit-product-ui.py`.

## Core numeric system

The exact values are intentionally not repeated as tables here; duplication previously allowed the documentation and runtime implementation to diverge. Agents and contributors must read the TOML sources directly.

The current system is built around:

- compact pointer/keyboard desktop controls;
- 13 logical-pixel UI labels;
- a finite spacing scale;
- 1 px separators and 2 px keyboard focus treatment;
- neutral light/dark chrome;
- high-contrast semantic variants;
- deterministic motion durations with reduced-motion replacement;
- a single original Loom icon family.

## Token-generation direction

`tokens/loom.toml` is the input contract. A generator may emit Slint constants, Rust constants, documentation tables, or design-tool exports, but generated outputs are never edited manually. Until generation fully replaces `theme.slint`, CI verifies that the runtime Slint values match the TOML contract.

A generator is considered complete only when:

- every palette role is emitted for light, dark, and high contrast;
- type, spacing, radius, border, icon, motion, and component metrics are emitted;
- invalid or missing semantic references are compile/test failures;
- applications no longer need raw design literals for standard UI;
- generated output is deterministic and checked for repository cleanliness.

## Governance

A design-token change is a product change. The commit must state:

- which semantic role changes;
- which component/application workflows are affected;
- accessibility/contrast impact;
- required reference-baseline updates;
- whether the desktop UI contract changes too.

Do not change a token solely to make one screenshot pass. Fix the component or layout unless the shared product rule itself is wrong.
