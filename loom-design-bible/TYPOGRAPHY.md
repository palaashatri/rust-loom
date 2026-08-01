# Typography

Typography is a defining capability of Loom, not a decoration. This document
defines the font stack, type scale, line lengths, heading hierarchy, tabular
figures, language and RTL behavior, and text scaling.

## 1. Font stack

Default sans-serif: **Noto Sans**, with the following fallback chain for UI
and body text:

1. `"Noto Sans"` — preferred, shipped or system-installed, all languages.
2. `"Noto Sans CJK SC"`, `"Noto Sans CJK JP"`, `"Noto Sans KR"`, `"Noto Sans
   Arabic"`, `"Noto Sans Devanagari"` — Noto family coverage for complex
   scripts when the primary face lacks them.
3. Generic: `"DejaVu Sans"`, then `sans-serif`.

Serif/monospace families are not part of the UI; document applications may
offer serif (Noto Serif) and monospace (Noto Sans Mono) for content text, with
the same fallback policy.

Rules:

* Fonts are a build-time and runtime resource; missing fonts are handled by
  fallback, never by failure. Missing-font warnings are non-blocking and
  logged in the diagnostics channel.
* All UI text is vector-rendered text in Slint; no bitmap fonts, ever.
* The stack is tokenized (`font-family-ui`, `font-family-serif`,
  `font-family-mono` in `loom.toml`'s type section) so a later variable-font
  migration (`[future]`) is a token change, not a code change.

## 2. Type scale

| Token | Size px | Leading ratio | Weight range | Role |
|---|---|---|---|---|
| `type-size-11` | 11 | 1.5 (`relaxed`) | 400/500/600 | Captions, timestamps, status text |
| `type-size-12` | 12 | 1.5 | 400/500/600 | Small labels, dense inspector values |
| `type-size-13` | 13 | 1.45 (`body`) | 400/500/600 | UI labels, toolbar labels |
| `type-size-14` | 14 | 1.45 | 400/500/600/700 | **Default body** |
| `type-size-16` | 16 | 1.45 | 400/500/600/700 | Body-large, dialogs |
| `type-size-20` | 20 | 1.4 (`compact`) | 500/600/700 | Subtitles, section titles |
| `type-size-24` | 24 | 1.4 | 500/600/700 | Headings, panel titles |
| `type-size-32` | 32 | 1.4 | 500/600/700 | Display-1: feature headings |
| `type-size-40` | 40 | 1.4 | 500/600/700 | Display-2: rare, hero surfaces |

Weight tokens: `type-weight-regular` 400, `type-weight-medium` 500,
`type-weight-semibold` 600, `type-weight-bold` 700. Defaults: body 400,
headings 600, labels 500. Bold (700) is reserved for emphasis inside prose
and for numbers that must stand out; it is not the default for headings.

## 3. Line length

* Body text target: **45–75 characters per line** (about 55–90 mm at 14 px in
  most fonts). Prefer 60–72 in document applications.
* UI labels, inspector values, and captions may use shorter lines; they must
  never exceed 75 ch.
* Text containers must reflow rather than truncate unless the field is
  explicitly single-line (labels, table cells) — in which case truncation uses
  ellipsis, and the full text is available via tooltip or accessible name.

## 4. Heading hierarchy

Heading levels are visual roles, not tag semantics; every heading still maps
to a semantic role for screen readers:

1. Window/document title — `type-size-20`, weight 600.
2. Section heading — `type-size-24`, weight 600 (documents), or `type-size-20`
   (panels).
3. Subsection — `type-size-16`, weight 600.
4. Sub-subsection — `type-size-14`, weight 600.

Hierarchy is conveyed by size and weight only (calm rule); color is not used
for hierarchy. Headings must scale with the text scale factor (§8).

## 5. Tabular figures

* Use tabular (monospaced-digit) figures for all data: spreadsheet cells,
  numeric inspector values, timestamps, durations, coordinates, money.
* Times, durations, and coordinates in UI are always in tabular figures so
  columns align and scrubbing values do not jitter horizontally.
* Use proportional figures for running prose and display text.

## 6. Language, RTL, and internationalization

* All text layout goes through the shaping engine; no pre-shaped text.
* Bidirectional text is supported wherever text is editable or shown
  (see `loom-spec`'s localization contract; the Bible adds the visual rules).
* RTL UI mirroring is required for the four built-in themes; logical layout
  (not visual mirroring) for progression — see `THEMING.md` and the layout
  stress screens in the gallery milestone.
* Language-aware line breaking and hyphenation: UI labels never hyphenate;
  document text uses the locale's hyphenation dictionary when available.
* Date/time/number formatting uses the locale; all layouts must survive
  long-formatted strings (pseudolocale test requirement).

## 7. Font fallback behavior

* Fallback is per-glyph, not per-word: a missing glyph resolves to the next
  face that has it.
* Fallback chains are tested with an installed-font audit in CI (missing
  glyph detection for the supported language matrix).
* Documents embedding fonts do so through the package format's asset rules;
  the UI itself never embeds fonts in documents.

## 8. Text scaling

The UI supports a text scale factor applied globally: **1.0 (default),
1.25, 1.5**.

* Scaling multiplies all `type-size-*` tokens; leading ratios stay constant.
* Chrome (toolbar 40 px, sidebar 240 px, inspector 280 px) may grow up to
  1.5× at scale 1.5 to fit text; content surfaces flex.
* No control may clip its label at 1.5×; no layout may become unusable at
  1.5×. This is a visual-QA gate (`VISUAL_QA.md`).
* Scaling is independent of window zoom (which scales the whole canvas
  including pixels).
