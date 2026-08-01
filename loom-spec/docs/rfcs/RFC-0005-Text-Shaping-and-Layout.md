# RFC-0005 — Text Shaping and Layout

- Status: **ACCEPTED (normative architecture; parts NOT_STARTED)**
- Date: 2026-08-01
- Author: Chief specification writer
- Scope: `loom-text`, `loom-ui`, Writer/Sheets/Present

## Context

Loom is a creative suite where typography is a defining capability
(`../loom-design-bible/TYPOGRAPHY.md`). Requirements: Unicode, bidi,
script shaping, font fallback, variable fonts, OpenType features, ligatures,
kerning, baseline alignment, hyphenation, justification, language-aware
behavior, style systems, baseline grids, vertical text where feasible,
high-DPI rendering, deterministic layout. `loom-text` already provides
paragraph/character style value objects and style runs
(`FEATURE_MATRICES.md` §1).

## Goals

- One text architecture shared by all applications.
- Deterministic line layout for tests and visual QA.
- A path to paginated document layout for Writer (master pages, columns,
  headers/footers, footnotes).
- Rich text editing surfaces in Writer, Present, and Photo (text layers).

## Non-goals

- Implementing a full shaping engine from scratch.
- Vertical text in the initial milestone (architecture must not preclude it).

## Proposed design

- **Rendering and shaping**: use Slint's text rendering
  (`Text`/`TextEdit` + font handling) for UI text, panels, dialogs, and
  simple labels — one stack, consistent with `RFC-0003`.
- **Typography engine**: evaluate Parley (shaping/layout, from the same
  ecosystem as Slint's text) plus Fontique (font discovery/fallback) as the
  engine-level shaping/layout layer behind a `loom-text` layout API. The
  choice is recorded as an architecture decision here; a dependency ADR must
  be added when the crates are actually adopted (license + maintenance
  verification per the root directive §5).
- **Deterministic layout**: `loom-text` exposes a layout pipeline (font
  resolution → shaping → line breaking → inline layout) that can run
  headless and produce deterministic metrics for tests and golden files.
- **Paginated mode (NOT_STARTED)**: pagination, master pages, columns,
  headers/footers, footnotes, TOC, and page styles are application-level
  layout on top of the engine API; no implementation exists yet.
- **Bidi**: text model stores logical order; bidi processing happens at
  layout/rendering time; cursor mapping APIs must exist for editing
  (UTF-16 cursor mapping tests are a planned task).

## Alternatives

- **Rustybuzz + fontdb direct**: mature and lightweight; still a viable
  fallback if Parley integration lags, but less aligned with Slint's stack.
- **Skrifa/read-fonts alone**: shaping/layout still needed on top.
- **System text stacks (Pango)**: non-Rust, GTK-adjacent; rejected per
  product directive.

## Trade-offs

Parley is evolving (API churn risk); mitigated by pinning and by keeping
`loom-text`'s layout API stable while the backend is swappable. Slint text
for UI vs Parley for documents creates two code paths; they converge on the
same style model in `loom-text`.

## Security

Font parsing is untrusted input; fonts from documents must go through the
chosen parser with fuzz coverage (untrusted font handling is a security
requirement, root directive §17). No network font loading ever.

## Performance

Layout must never block the UI thread for documents: async layout jobs
behind the engine API (`loom-jobs`). Deterministic layout enables cached
layout results keyed by style + width.

## Compatibility

Style objects (`loom-text`) are stable; the layout API is additive.
Document packages store logical text + styles, never laid-out metrics
(rendering stays deterministic per environment).

## Migration

None required yet: no layout code exists. When Parley is adopted it lands
behind the `loom-text` API.

## Testing

- Deterministic layout golden tests (fixed fonts in pinned Docker images).
- Bidi/Unicode cursor movement property tests; range operation tests.
- Shaping tests for ligatures/kerning/fallback with bundled test fonts.
- Hyphenation/justification tests for supported languages.
- Fuzz targets for rich-text parsing and font handling.

## Open questions

- Exact Parley adoption timing vs a Rustybuzz interim
  (resolved when `loom-text` layout work starts; recorded in an ADR).

## Final status

ACCEPTED as architecture. Style model implemented; layout/pagination
NOT_STARTED. Superseding decisions require a new RFC or ADR.
