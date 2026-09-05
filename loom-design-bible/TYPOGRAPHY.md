# Loom Typography

Exact type roles and values live in [`tokens/loom.toml`](tokens/loom.toml) and [`contracts/desktop-ui.toml`](contracts/desktop-ui.toml). This file defines usage.

## Desktop UI hierarchy

Loom is a dense pointer/keyboard creative suite. Routine UI uses compact desktop typography rather than mobile-sized labels or oversized dashboard headings.

Roles:

- **UI label:** 13 logical px, medium 500. Buttons, toolbar labels, fields, tabs, inspector values.
- **Section label:** 13 logical px, semibold 600. Inspector/panel sections.
- **Caption/status:** 11 logical px. Status bar, timestamps, tertiary readouts.
- **Small metadata:** 12 logical px.
- **Body/help prose:** 14 logical px.
- **Subtitle:** 16 logical px.
- **Content heading:** 20 or 24 logical px only when the content hierarchy actually requires it.

Routine chrome does not use 20–40 px display typography. The work itself, not application branding, owns visual hierarchy.

Bold 700 is not the default chrome weight. Use regular/medium for routine controls and semibold for section/title emphasis. Excessive bold weight is treated as hierarchy noise.

## Font stack

Default UI family is `Noto Sans` with the fallback chain defined in `tokens/loom.toml`. Document applications may use `Noto Serif`, `Noto Sans Mono`, embedded/document fonts, and language-specific shaping for user content.

Rules:

- fonts are vector text, never bitmap UI labels;
- fallback is per glyph where shaping infrastructure supports it;
- missing UI fonts fall back rather than preventing launch;
- UI family choice is a token/system decision, not an application choice;
- document typography is domain content and remains independent of chrome typography.

## Numeric text

Coordinates, timecode, durations, spreadsheet values, percentages, progress, media timestamps, mixer values, and inspector numeric readouts use tabular figures when the renderer/font path supports them. Scrubbing a value must not cause surrounding UI to jitter horizontally.

## Truncation

Text falls into two categories.

**User content may ellipsize:** document/project title, file name, layer name, track name, media name, user-created style/name. Full value must remain available through tooltip/accessibility text.

**Interface language may not ellipsize:** action labels, button labels, inspector property labels, section headings, severity labels, command names, menu items.

If interface language does not fit, the layout must recompose: overflow toolbar commands, widen/stack inspector rows within bounds, or collapse optional panels. Do not hide a design failure behind `…`.

## Localization

All UI strings must survive pseudolocalization and bidirectional text. Logical leading/trailing layout is preferred over left/right assumptions.

- UI labels do not hyphenate.
- Long translated control labels trigger responsive composition, not clipping.
- Dates, numbers, currency, and units use locale-aware formatting where implemented.
- Text shaping and IME behavior are part of editor correctness, not visual polish only.

## Text scaling

Release tests require 1.0, 1.25, and 1.5 text-scale behavior; 2.0 is the accessibility stress target.

Scaling text does not blindly scale every panel and toolbar dimension. The responsive system keeps controls usable by moving lower-priority actions into overflow, stacking property rows where allowed, and collapsing optional panels before content is harmed.

No action/control label may clip at any required scale. User content may ellipsize only under the explicit truncation policy.

Content zoom is independent of UI text scale. A Writer page, Photo image, Present stage, or Video viewer can change zoom without making chrome text larger.

## Contrast

Routine UI/body text must meet the contrast floor defined in `tokens/loom.toml`. Focus and non-text control boundaries must meet their UI contrast floor. High-contrast mode is a semantic palette swap, not application-specific restyling.
