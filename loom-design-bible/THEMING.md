# Theming

Loom ships four built-in configurations: three themes (light, dark,
high-contrast) and one orthogonal mode (reduced motion). Themes are pure
token swaps; components are theme-agnostic.

## 1. Theme matrix

| Configuration | Purpose | Selection |
|---|---|---|
| Light (default) | Standard professional work | Default; manual |
| Dark | Low-light work, media review | Manual; follows OS dark preference by default (can be fixed) |
| High-contrast | Accessibility requirement | Manual; follows OS high-contrast preference by default |
| Reduced motion | Motion accessibility | OS preference; manual toggle; orthogonal to theme |

Rules:

* A theme change applies instantly (token swap, no restart), animates as a
  200 ms in-out cross-fade at most, and is reduced-motion-safe (instant).
* Theme choice persists per app in settings; document content (page
  backgrounds, document colors) is **not** re-themed — only UI chrome
  re-themes. The page always renders in its document color space.
* Third-party/custom themes are `[future]`: the theme API will be a token
  document (the same TOML schema) loaded from user settings with
  validation against the contrast floors; not a plugin API surface yet.

## 2. Token mapping (identical in all three themes)

Each theme supplies values for the same semantic tokens; names never vary.
Full values in `tokens/loom.toml` and `DESIGN_TOKENS.md` §4–§11. Summary
map (light shown; dark/high-contrast replace values only):

| Semantic token | Light value | Role |
|---|---|---|
| `color-surface-canvas` | `#FAF9F7` | Window/canvas ground |
| `color-surface-raised` | `#FFFFFF` | Panels, dialogs, controls |
| `color-surface-sunken` | `#F1EFEA` | Wells, inputs, beds |
| `color-ink-primary` | `#26221C` | Primary text |
| `color-ink-secondary` | `#5C564C` | Secondary text |
| `color-accent-default` | `#B4552D` | Selection, focus, primary actions |
| `color-accent-ink` | `#FFFFFF` | Text on accent |
| `color-accent-hover` | `#C9643A` | Accent hover/pressed |
| `color-status-success` | `#3E6B4F` | Success signals |
| `color-status-warning` | `#A8681E` | Warning signals |
| `color-status-danger` | `#A43424` | Error/destructive signals |
| `color-status-info` | `#3B5E7A` | Info signals |
| `space-2 … space-64` | 2…64 px | Spacing scale |
| `radius-2 … radius-12` | 2…12 px | Corner radii |
| `border-width-hairline/default/strong` | 1 / 1 / 2 px | Borders |
| `type-size-11 … 40` | 11…40 px | Type scale |
| `type-leading-compact/body/relaxed` | 1.4 / 1.45 / 1.5 | Leading |
| `type-weight-regular/medium/semibold/bold` | 400/500/600/700 | Weights |
| `motion-duration-instant/fast/standard/deliberate/slow` | 0/120/200/320/500 ms | Durations |
| `motion-easing-out-quad/in-out/out-back` | (0.33,1,0.68,1) / (0.65,0,0.35,1) / (0.34,1.56,0.64,1) | Easings |
| `shadow-none` / `shadow-popover` | none / 0 1px 3px rgba(38,34,28,0.12) | Elevation |
| `icon-viewbox-20` / `icon-stroke-width-15` / `icon-corner-radius-2` | 20 / 1.5 / 2 | Icons |

Theme-independent tokens (identical in all themes): spacing, radii, border
widths, type scale/leading/weights, durations, easings, shadows, icon grid.
Only palette tokens vary by theme.

## 3. Dark-theme specifics

* Same contrast floors as light (`COLOR.md` §7); surfaces darken
  canvas→sunken instead of lightening.
* Ink hierarchy inverts; accent lightens (`#D97A4A`) to keep ≥ 4.5:1 with
  `accent-ink` (`#1F1710`); media wells are darker than the canvas
  (`#171512`) so previews pop.
* Borders: ink at 18% alpha (brighter than light's 12% — needed on dark);
  page edges gain a 1 px hairline to separate page from canvas.

## 4. High-contrast specifics

* Surfaces collapse to pure black; separation via white borders and text
  tiers only (`COLOR.md` §4).
* Selection: 2 px white outline + white handles on black; focus ring
  black-on-white (or white-on-black) by surface; hover = inverted.
* Accents carry doubled contrast (≥ 7:1 vs black) so they remain
  distinguishable from pure ink.
* Icons: full ink, no opacity tiers below 80% except disabled (40%).

## 5. Reduced motion as a mode

* Applies over any theme; disabled animations: translation, scale,
  overshoot, spring — replaced per `MOTION.md` §4 (opacity ≤ 120 ms only).
* Not a theme: there is no separate "reduced motion theme"; the setting
  switches animation behavior globally.

## 6. Theme integrity

* Components consume tokens only — a hard-coded literal in a component is
  a defect that fails CI (token lint).
* Every theme must pass: the full visual-QA baseline set, the contrast
  gate (§`COLOR.md` 7), the 1.5× text-scale gate, and the reduced-motion
  assertion suite. A theme that fails a gate does not ship.
* `tokens/loom.toml` carries all three palettes; adding a theme = adding a
  palette table + its QA passes, never new components.

## 7. Theme application

* The runtime theme is selected from settings at startup and applied at
  the UI root; theme is user settings, not document data.
* Future custom themes will load through the token document schema with
  validation (contrast floors enforced) — noted here so the current
  architecture (token swap at the root) is not contradicted later.
