# Spacing

Spacing is the cheapest way to communicate structure. This document defines
the spacing scale, its roles, and the patterns that use it.

## 1. The scale

| Token | Value | Primary use |
|---|---|---|
| `space-2` | 2 px | Internal hairlines, focus-ring offsets (≥ 2 px from control edge) |
| `space-4` | 4 px | Dense control insets, dot markers, icon-to-icon gaps in compact rows |
| `space-6` | 6 px | Compact gaps between inline controls in one control group |
| `space-8` | 8 px | **Standard gap**: control-to-control, icon-to-label, list rows |
| `space-12` | 12 px | Control-to-field-group, list padding, table cell padding |
| `space-16` | 16 px | **Panel padding**, dialog padding, card padding |
| `space-20` | 20 px | Section spacing inside panels, between field groups |
| `space-24` | 24 px | Window/chrome padding, sidebar panel padding |
| `space-32` | 32 px | Page-level grouping, between major regions |
| `space-40` | 40 px | Generous whitespace: hero states, onboarding |
| `space-48` | 48 px | Between primary regions (toolbar → content) in large windows |
| `space-64` | 64 px | Maximum required gap; window-scale separation |

Rules: sizes below `space-8` never separate independent controls; sizes above
`space-24` never appear inside a single control. If a layout "needs" 10 px,
use `space-12` — the scale is a discipline, not a suggestion.

## 2. Patterns

* **Inset pattern**: control content inset = control padding; control padding
  = 4 px within a 32 px-tall control (toolbar buttons are 32 px with 4 px
  inset, yielding a 24 px icon/action area).
* **Label pattern**: label to control `space-8`; label column to control
  column `space-12`; field-group to field-group `space-20`.
* **List pattern**: row content padding `space-8` vertical, `space-12`
  horizontal; icon to text `space-8`; section label to list `space-8`.
* **Panel pattern**: panel padding `space-16`; header to body `space-8`;
  header icon to title `space-8`; panel to panel `space-0` (stacked flush)
  separated by a hairline; panel-group to panel-group `space-20`.
* **Dialog pattern**: dialog padding `space-24`; content to actions
  `space-24`; action button to action button `space-8` (`DIALOGS.md`).
* **Toolbar pattern**: control-to-control `space-8`; group-to-group
  `space-16`; toolbar left inset `space-16`; right inset `space-16`.

## 3. Anti-patterns

* Padding values "slightly more than 16": the scale is fixed; a 20 px
  requirement inside a panel means the layout is wrong, not the token.
* Centering arbitrary content to "fill space" (use `space-32`/`space-48`
  groupings instead of random margins).
* Negative margins and absolute-positioned layout gymnastics to defeat the
  scale: components are authored to the scale; if a component cannot be
  authored to the scale, the component spec is wrong — fix `COMPONENTS.md`,
  not the layout.
* Different apps using different paddings for the same element: panel padding
  is `space-16` everywhere. Per-app drift in spacing is a review-blocking
  defect.

## 4. Spacing and text

* Line-height ratios in `TYPOGRAPHY.md` are per text token, not derived from
  the spacing scale; UI rows stack with `space-4`–`space-8` gaps above
  line-height breathing room.
* Vertical rhythm in document content (Writer) is a document-style property
  (paragraph spacing), governed by `loom-spec`, not by UI spacing tokens.
* At 1.25×/1.5× text scale, spacing tokens are unchanged (text grows inside
  its box); only chrome heights may grow (`TYPOGRAPHY.md` §8).
