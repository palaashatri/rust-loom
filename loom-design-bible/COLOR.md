# Color

The color contract for Loom: the three palettes, usage rules, contrast
requirements, non-color status indicators, and the data-visualization palette.

## 1. Role of color

Color has four jobs in Loom, in priority order:

1. **Grounding content** — canvas vs. surface vs. ink separation.
2. **Signal** — selection, focus, status, destructive intent.
3. **Meaning in data** — categorical series in charts.
4. **Warmth** — the warm-neutral material tone of the whole suite.

Color is never decorative, never the only channel for meaning, and never used
for hierarchy (hierarchy is size and weight).

## 2. Light palette (default)

| Token | Value | Usage |
|---|---|---|
| `color-surface-canvas` | `#FAF9F7` | Window backdrop, canvas surround, page-free areas |
| `color-surface-raised` | `#FFFFFF` | Panels, dialogs, controls, cards |
| `color-surface-sunken` | `#F1EFEA` | Input wells, media beds, disabled regions |
| `color-ink-primary` | `#26221C` | Primary text |
| `color-ink-secondary` | `#5C564C` | Secondary text, captions, placeholders |
| `color-accent-default` | `#B4552D` | Selection, focus, primary actions (terracotta) |
| `color-accent-ink` | `#FFFFFF` | Text/icon on accent fills |
| `color-accent-hover` | `#C9643A` | Hover/pressed accent states |
| `color-status-success` | `#3E6B4F` | Success signals |
| `color-status-warning` | `#A8681E` | Warning signals |
| `color-status-danger` | `#A43424` | Errors, destructive actions |
| `color-status-info` | `#3B5E7A` | Informational signals |

## 3. Dark palette

| Token | Value |
|---|---|
| `color-surface-canvas` | `#201D19` |
| `color-surface-raised` | `#2A2621` |
| `color-surface-sunken` | `#171512` |
| `color-ink-primary` | `#F2EFE9` |
| `color-ink-secondary` | `#B9B2A6` |
| `color-accent-default` | `#D97A4A` |
| `color-accent-ink` | `#1F1710` |
| `color-accent-hover` | `#E88B5E` |
| `color-status-success` | `#7FA98C` |
| `color-status-warning` | `#D5A14A` |
| `color-status-danger` | `#E0654F` |
| `color-status-info` | `#7FA3C4` |

## 4. High-contrast palette

True black/white with doubled-contrast accents (all values verified ≥ 7:1
against pure black; see §7 for the method):

| Token | Value |
|---|---|
| `color-surface-canvas` | `#000000` |
| `color-surface-raised` | `#000000` |
| `color-surface-sunken` | `#000000` |
| `color-ink-primary` | `#FFFFFF` |
| `color-ink-secondary` | `#E6E6E6` |
| `color-accent-default` | `#FF8A3C` |
| `color-accent-ink` | `#000000` |
| `color-accent-hover` | `#FFA266` |
| `color-status-success` | `#4ADE80` |
| `color-status-warning` | `#FFC24D` |
| `color-status-danger` | `#FF6B5E` |
| `color-status-info` | `#6CB2FF` |

In high contrast, surfaces lose all gray steps (everything is black); depth
is carried by white borders (`border-width-default` becomes white) and text
tiers (`ink-secondary` is only for tertiary labels).

## 5. Usage rules

* **Content first**: the canvas (`color-surface-canvas`) is the lightest
  (darkest in dark theme) ground. Content surfaces sit on it. Chrome sits on
  `color-surface-raised`. Wells sit in `color-surface-sunken`.
* **Ink**: primary text uses `color-ink-primary` on every surface; secondary
  uses `color-ink-secondary` — no third text tier on light/dark (captions are
  secondary, never a lighter gray).
* **Accent is a tool, not a theme**: accent is for selection, focus, and
  primary actions. It does not paint entire windows, toolbars, or panels.
  Accent fills (buttons) use `color-accent-ink` for text.
* **Danger**: destructive actions use `color-status-danger` for the action's
  label or icon and confirm buttons; never for the window chrome.
* **Borders**: `border-width-default` borders use a 12% opacity ink
  (`color-ink-primary` at 12% alpha) on light/dark; high contrast uses pure
  white.
* **Depth without shadows**: surface separation comes from canvas→raised→sunken
  steps only. `shadow-none` everywhere except `shadow-popover` on popovers
  (see `DESIGN_TOKENS.md` §10).

## 6. Status and non-color indicators

Color is never the sole channel for status. Every status has at least two
channels:

* Success: green + check icon + affirmative text ("Saved").
* Warning: ochre + triangle icon + explanation text.
* Error: red + octagon/alert icon + description text; the message also
  reaches a screen-reader live region.
* Info: blue + circle-info icon + text.

In high contrast, icons and text carry the meaning at full strength; the
color channels are secondary. Progress is a value (bar fill or percent), not
a color.

## 7. Contrast requirements

Floors (WCAG 2.x relative-luminance contrast, all four themes):

* **4.5:1** — body text, captions, placeholders, icon labels.
* **3:1** — large text (≥ 24 px or ≥ 18.66 px bold), UI component boundaries
  (focus rings, control outlines, toggle tracks), and iconographic indicators
  when the icon is the only label.
* **3:1** — text on accent fills at large sizes; body-size text on accent
  fills must be ≥ 4.5:1 (hence `color-accent-ink` on `color-accent-default`
  = 4.9:1 in light).

Verified values in the light theme (computed from the sRGB relative-luminance
formula): ink-on-canvas 15.0:1, ink-secondary-on-canvas 6.9:1,
accent-ink-on-accent 4.9:1, danger-on-white 6.8:1, success-on-white 6.1:1,
warning-on-white 4.5:1, info-on-white 6.9:1. Dark theme: ink-on-canvas 14.6:1,
ink-secondary-on-canvas 8.0:1, accent-ink-on-accent 5.7:1. High contrast:
accent-on-black 8.95:1, ink 21:1. CI verifies every committed palette value
against these floors; a value below its floor fails the build.

## 8. Data-visualization palette

Categorical series (charts, scopes, timelines), proposed set, values valid in
light and dark themes:

| Name | Value | Notes |
|---|---|---|
| terracotta | `#B4552D` | Suite accent — charts may use it as series 1 |
| teal | `#2E7D6E` | Series 2 |
| ochre | `#C98A2E` | Series 3 (≥ 3:1 as large marks; avoid small text) |
| slate | `#4A6B8A` | Series 4 |
| plum | `#7A5C8A` | Series 5 |
| moss | `#6B8A4A` | Series 6 |

Rules:

* Max 6 categorical series; beyond that, group and use a legend pattern.
* Series are always disambiguated by pattern, dash, or label in addition to
  color (colorblind-safe: teal/ochre and plum/slate pairs are distinguished by
  luminance as well as hue).
* Never pair `terracotta` with another series color that is visually
  identical to it; never use `color-status-*` colors as series colors in the
  same chart as status indicators.
* Chart backgrounds use `color-surface-sunken`; series must hold ≥ 3:1
  against it in both themes. Where a value fails (ochre on light), use larger
  marks with white or black outlines and verify in the theme's visual QA.

## 9. Color management

* All palette values are specified in sRGB; the render pipeline is
  color-managed (see `loom-core`'s `loom-color` contract and `loom-spec`).
* Screen colors are display-profile adjusted at runtime only; tokens are the
  canonical sRGB values for deterministic screenshots in the software renderer.
* Never sample a token from a screenshot; never tune a token "until it looks
  right" without re-running the contrast verification in §7.
