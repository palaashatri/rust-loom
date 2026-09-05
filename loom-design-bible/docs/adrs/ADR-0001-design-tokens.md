# ADR-0001: Design Token Architecture

Status: **Accepted**
Date: 2026-08-01
Owner: Design-system lead

## Context

Eight applications and a shared platform must render a single visual
language. Early sketches showed three failure modes: (1) values copy-pasted
per app and drifting, (2) a single monolithic "theme" struct mixing raw
colors with layout decisions, and (3) prose-only contracts that agents and
teams interpret differently. The suite also requires four theme
configurations (light, dark, high-contrast, reduced-motion mode) that must
not multiply component code.

## Decision

Adopt a two-layer token architecture with one canonical machine-readable
file.

1. **Primitive layer**: raw values (hex colors, px sizes, ms durations,
   cubic-bezier points). Versioned; changed only via ADR.
2. **Semantic layer**: purpose-named tokens (`color-surface-canvas`,
   `space-8`, `motion-duration-fast`) consumed by components. Semantic
   tokens are the only thing components may reference.

Rules:

* Canonical source: `tokens/loom.toml` in `loom-design-bible`.
* Naming: `category-role-scale`, lowercase with hyphens. Categories:
  `color`, `space`, `radius`, `border`, `type`, `motion`, `shadow`, `icon`.
* Mapping to Slint: a generator (owned by `loom-core`) emits
  `Tokens.slint` `export const` declarations with native types (`color`,
  `length`, `duration`, `easing`) so wrong-typed usage fails at compile
  time. Theme variants are structs of the same token names per theme.
* Documentation (`DESIGN_TOKENS.md`, `THEMING.md`) must match the TOML
  exactly; the TOML wins on conflict.
* Themes are token-value swaps only; components are theme-agnostic.
* No component may hard-code a literal; a token lint fails CI.
* Adding/changing/deleting a token is an ADR-gated change
  (`DESIGN_TOKENS.md` §13).

## Alternatives considered

* **Slint-only tokens**: define everything in a `Tokens.slint`. Rejected:
  no machine-readable form for CI checks, no single source for docs and
  generator, poor diff-ability.
* **One giant theme struct**: single struct per theme containing colors,
  sizes, and motion. Rejected: conflates categories, no compile-time
  guidance, hard to diff, encourages per-app forks.
* **YAML/JSON source**: rejected for determinism of ordering and
  commentability; TOML chosen (sorted, commented, diff-friendly).
* **CSS-variable-style runtime strings**: rejected — no type safety, no
  generator, and the toolkit consumes typed values.

## Consequences

* Pro: one source of truth; compile-time enforcement; themes become data;
  CI can verify contrast floors by reading the TOML.
* Pro: agents and teams consume identical names everywhere; doc drift is a
  mechanical defect, not an interpretation issue.
* Con: token changes are heavier (ADR + TOML + docs + generator emit);
  accepted as intentional friction.
* Con: the generator is a new `loom-core` dependency; mitigated by
  specifying the emit contract now (see `DESIGN_TOKENS.md` §3) so the
  gallery milestone consumes a stable format.

## Migration

No prior tokens exist; this is the initial contract. Future migrations
(adding themes, custom user themes) are value additions to the same file
schema (`THEMING.md` §6–7).

## Verification

* Consistency check: script compares token names/values across TOML and
  both documents (CI).
* Contrast check: CI computes WCAG contrast for palette pairs
  (`COLOR.md` §7).
* Emit check: generated `Tokens.slint` compiles in the `loom-core` UI
  crate fixture (gallery milestone).
