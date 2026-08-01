# Loom Design Bible

Master document for the Loom design language. This file is the index of the
contract; each section names the owning document and summarizes the binding
rules. Where this file and a section document disagree, the section document
holds — except for token values, where `tokens/loom.toml` wins.

## 1. Design language

Loom is **calm, precise, minimal, warm, professional, fast, predictable,
capable, timeless.**

* Calm — nothing shouts. Restrained chrome, low noise, generous silence.
* Precise — snapping, exact alignment, truthful progress, deterministic layout.
* Minimal — content is the focus; chrome is supporting furniture.
* Warm — a warm-neutral palette (canvas `#FAF9F7`, terracotta accent
  `#B4552D`) rather than cold grays and saturated blues.
* Professional — depth through progressive disclosure, not clutter.
* Fast — input feedback within one frame; no UI-thread blocking, ever.
* Predictable — same action, same place, same result in every application.
* Capable — professional features exist; they are simply revealed on demand.
* Timeless — no decorative trends, no visual noise that will age.

## 2. Principles

The ten+ principles in `DESIGN_PRINCIPLES.md` govern all decisions:

1. Content first. 2. Calm over busy. 3. Direct manipulation first. 4. Progressive
disclosure. 5. Predictability across the suite. 6. Truthful feedback. 7. Motion
with meaning. 8. Accessibility is release-blocking. 9. Professional depth,
never featurelessness. 10. Performance is a feature. 11. Warmth without
decoration. 12. Every default is a deliberate choice.

## 3. Layout system

* Window anatomy (see `LAYOUT.md`): title bar, single-row contextual toolbar
  (**40 px**), collapsible sidebar (**240 px**), contextual inspector
  (**280 px**), status bar (**28 px**), content canvas fills the remainder.
* Spacing is drawn from the scale 2, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48, 64 px.
  Only these values, with one exception: components may use
  `0.5 × space-2` = 1 px only for internal hairline offsets, documented locally.
* Chrome heights are fixed and DPI-scaled; content surfaces flex.

## 4. Typography

* Default sans: **Noto Sans** (per `TYPOGRAPHY.md`), with a documented fallback
  chain and a future variable-font path.
* Type scale: 11, 12, 13, 14 (body), 16, 20, 24, 32, 40 px; leading ratios
  1.4–1.5; weights 400/500/600/700.
* Body line length 45–75 characters per line (prefer 60–72 in documents).
* Tabular figures for all data columns, timestamps, and numeric inspectors.
* UI scales with a text scale factor of 1.0 / 1.25 / 1.5.

## 5. Color

* Full palettes (light, dark, high-contrast) in `COLOR.md` and
  `tokens/loom.toml`. Accent is warm terracotta `#B4552D` (light).
* Contrast floors: 4.5:1 body text, 3:1 large text, 3:1 UI components and
  iconographic indicators (WCAG 2.x relative luminance).
* Status is never conveyed by color alone; every status has a text, icon,
  shape, or position component.
* Depth is conveyed by color steps, not shadows (`shadow-none`); popovers are
  the only allowed exception (`shadow-popover`).

## 6. Components

* Component inventory and full state matrices (default/hover/active/focus/
  disabled/checked) in `COMPONENTS.md`.
* All components use the same tokens; there are no per-application forks of a
  component. Applications may combine components; they may not redefine them.
* Every interactive component has an `accessible-description`, a focus ring,
  a disabled visual, and a keyboard path.

## 7. Interaction rules

* Direct manipulation first, then context toolbar, then inspector, then menus
  and command palette, then advanced workspace, then scripting — six layers of
  progressive disclosure (`DESIGN_PRINCIPLES.md` §4).
* Selection visuals: accent 2 px outline plus overlay (`SELECTION.md`).
* Every long operation is a job: cancellable, observable, recoverable. Nothing
  blocks the UI thread.
* Dragging, reordering, zooming, scrubbing all have specified feedback and
  motion (`MOTION.md`, `DRAG_AND_DROP.md`).

## 8. Motion

* Durations: instant 0, fast 120 ms, standard 200 ms, deliberate 320 ms,
  slow 500 ms.
* Easings: out-quad `(0.33, 1, 0.68, 1)`, in-out `(0.65, 0, 0.35, 1)`,
  out-back `(0.34, 1.56, 0.64, 1)`.
* Every animation answers a usability question. All animations are
  interruptible. Reduced motion disables translation/scale and keeps opacity
  changes at 120 ms (`MOTION.md`).

## 9. Accessibility baseline

Release-blocking (`ACCESSIBILITY.md`):

* Complete keyboard navigation; every control reachable and operable by keyboard.
* Visible focus at all times (accent 2 px ring, min 2 px offset).
* `accessible-description` on every control; logical focus order.
* High-contrast theme (true black/white, doubled-contrast accents).
* Text scaling 1.0/1.25/1.5 without layout breakage.
* Reduced-motion mode; non-color status indicators.
* Accessible canvas/timeline/grid navigation strategies per application.
* Configurable shortcuts; errors announced via live regions.

## 10. Theming

* Four built-in configurations: **light**, **dark**, **high-contrast**, and the
  orthogonal **reduced-motion** mode (`THEMING.md`).
* Themes are pure token swaps. Components are theme-agnostic; they consume
  tokens only.
* Custom/third-party themes are `[future]` and must go through the same token
  interface.

## 11. Visual QA

* In-app screenshots with the software renderer at fixed **1280×800**, stored
  as `baselines/<app>/<name>.png`.
* Perceptual diff with two gates: mean absolute error `< 1.0` and
  differing-pixel ratio `< 0.01`. No auto-approval. Baselines are generated
  only in the Docker visual environment (`VISUAL_QA.md`).

## 12. Performance

* Input feedback within one frame; animations at 60 fps; no allocations per
  frame in scroll paths; warm launch `< 1 s` for lightweight apps; bounded
  memory via documented cache policies; cancellation feedback immediate
  (`PERFORMANCE.md`).

## 13. Grammar for writing requirements

Requirements must be written so a less-capable agent can implement them
without inference. Prefer: "The toolbar is a single row, 40 px tall, left
aligned, with the primary action first." over "The toolbar should feel
efficient." See `AGENTS.md` §4.

## 14. Governance

* Token changes: ADR + TOML + prose in one change.
* New component: component spec in `COMPONENTS.md` + states matrix +
  `DESIGN_REVIEW.md` checklist entry.
* New motion: duration/easing from the token set; no bespoke values.
* Any exception to this Bible must be an ADR. Exceptions without an ADR are
  defects in the application, not precedents.
