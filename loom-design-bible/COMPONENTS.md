# Components

The component inventory with full state matrices. Components are defined once
and shared across all applications through `loom-core`'s UI crate; applications
compose, never fork. All measurements are logical px.

## State model

Every interactive component has the states: **default, hover, active
(pressed), focus, disabled, checked/selected** (where applicable), plus
**focus-visible** (keyboard focus ring, see `ACCESSIBILITY.md`).

Focus ring: 2 px (`border-width-strong`) outline in `color-accent-default`,
offset 2 px outside the control bounds, visible on keyboard focus (and only
on keyboard focus in mouse-optimized surfaces; always visible where a
component can be reached by keyboard, which is everywhere).

## 1. Button

Sizes: height 32 px (default), 28 px (small), 40 px (large); horizontal
padding `space-12`–`space-16`; corner `radius-6`; label `type-size-13`,
weight 500.

* Default: raised fill, `color-surface-raised` on `color-surface-canvas`
  chrome; text `color-ink-primary`; 1 px border `border-width-default`
  (ink at 12% alpha).
* Primary variant: fill `color-accent-default`, text `color-accent-ink`,
  no border.
* Danger variant: text/label `color-status-danger` on raised; filled danger
  variant fill `color-status-danger`, text white.
* Hover: primary/raised lift via darker fill (accent-hover for primary);
  standard 0 ms hover onset (instant).
* Active: pressed-down shading, 1 px inward shift, instant.
* Focus: focus ring per state model.
* Disabled: 40% opacity of default rendering; cursor not-allowed; label
  remains readable (≥ 4.5:1 where possible) and a tooltip explains why when
  reason exists.

## 2. IconButton

32 × 32 px, icon 20 px, corner `radius-4`. States as Button. Tooltip required
(name + shortcut hint). Icon-only buttons must also exist as menu/palette
commands (`ICONOGRAPHY.md` §5).

## 3. ToolButton (toolbar)

32 × 32 px toggle: **checked** state renders with `color-accent-default`
fill at 15% opacity + accent icon; unchecked renders neutral. Hover: ink at
8% fill. Active: ink at 15%. Disabled: 40% opacity. Tooltip always.

## 4. SegmentedControl

Single row of segments, height 28 px, `radius-6` container on
`color-surface-sunken`; selected segment raised with `color-surface-raised`
fill and 1 px border; label `type-size-13`. States per segment: default,
hover (ink 8%), checked (raised + ink-primary text), focus (ring on checked
segment), disabled (40%). Keyboard: arrow keys move selection
(`KEYBOARD.md`). Never more than 6 segments; overflow becomes a ComboBox.

## 5. TextField

Height 32 px, `radius-4`, fill `color-surface-raised`, 1 px
`border-width-default` border; placeholder `color-ink-secondary`. States:
hover (border darkens), focus (accent border + ring), disabled (sunken fill,
40% text), error (danger border + inline message below, 120 ms). Single-line
and multiline variants; IME composing region styled with accent underline.

## 6. Slider

Track 4 px tall, fill `color-accent-default` to the value, remainder
`color-ink-primary` at 12%; handle 16 × 16 px circle, `color-surface-raised`,
1 px border, radius 8. States: hover (handle grows to 18 px, 120 ms),
active (handle accent-filled), focus (ring on handle), disabled (40%).
Keyboard: arrows/Home/End per `KEYBOARD.md`.

## 7. CheckBox

16 × 16 px box, `radius-4`, border 1 px; checked: accent fill + accent-ink
check glyph; indeterminate: accent dash. Focus ring per state model.
Keyboard: Space toggles. Label right, `space-8`, clickable with the box.

## 8. ComboBox

Height 32 px; control as TextField-with-arrow; popover list (see Menu);
each option row 28 px with `space-8` padding, selected option checkmarked
and accent-tinted; typeahead filters; keyboard: arrows, Enter, Esc.

## 9. ListItem

Row 28 px (dense) or 32 px (default), `radius-4`; text `type-size-13`;
selected: accent fill 15% + accent text; hover: ink 8%; focus: ring;
disabled: 40%. Multi-select rows show checkboxes on hover/focus or persistent
in selectable libraries. Row actions appear on hover AND on focus.

## 10. Toolbar

Single row 40 px (`LAYOUT.md`), padding `space-16`; groups separated by
`space-16`; controls 32 px; overflow into overflow popover. Toolbar is
contextual: it reflects the current selection/tool (`TOOLBARS.md`).

## 11. StatusBar

28 px strip (`LAYOUT.md`); left progress/cancellation region, right readouts.
Text `type-size-11`, secondary ink; progress uses ProgressBar-mini (80 × 6 px)
with cancel button. Never steals focus; never uses motion beyond 120 ms.

## 12. SidePanel

Column surface `color-surface-raised` with header 40 px (title + collapse +
pin). Body scrolls as one surface; sections collapse per `INSPECTORS.md`.
Resize drag edge 4 px (`SIDEBARS.md`).

## 13. InspectorSection

Header 32 px: title `type-size-13` weight 600, disclosure chevron (rotates
120 ms), optional pin. Body `space-16` padding, two-column property rows.
Collapsed sections show a summary line of the most relevant current value
(keeps search and context alive).

## 14. EmptyState

Centered illustration (original loom-thread art) + title `type-size-16`
weight 600 + body `type-size-14` secondary + primary action Button + optional
secondary action. Padding `space-48`. Every empty state has a clear action
path — never a dead end. See `UX_ACCEPTANCE_CHECKLIST.md` empty-state gate.

## 15. Menu

Popover list, `radius-8`, raised fill, `shadow-popover`, 1 px border; item
rows 28 px with icon 16 px, label, shortcut hint right-aligned
`color-ink-secondary`, checkmark for checked items; separators hairlines;
disabled items 40%. Opens within 120 ms (out-quad, 4 px from anchor), closes
instantly. Keyboard: arrows, Enter, Esc, mnemonics where configured.

## 16. Dialog

Window-level surface, `radius-8`, padding `space-24`, min width 420 px;
title `type-size-16` weight 600; body `type-size-14`; action row right-aligned
`space-8` gaps with primary/destructive button rightmost (see `DIALOGS.md`).
Focus moves to the dialog on open (first focusable); Tab traps within;
Esc = cancel; Enter = primary.

## 17. Notification (toast)

Entry from top-right, width 320 px, `radius-8`, raised fill,
`shadow-popover`; icon + message + optional action + close; auto-dismiss
after 6 s for info, 10 s for warning/error (errors also persist in the
diagnostics log); severity never color-only (`COLOR.md` §6). Motion:
entrance out-quad 200 ms (fade + 4 px slide), exit 160 ms. Reduced motion:
fade only.

## 18. ProgressBar

Track 4 px, fill `color-accent-default`; determinate: fill snaps to value
within 120 ms; indeterminate (rare): 2 px accent segment marching 24 px,
out-quad — only when the task genuinely cannot report progress, and always
paired with text describing the phase. Never used where a determinate value
exists.

## 19. TabBar

Height 40 px, tabs 32 px high, text `type-size-13` weight 500; selected tab
accent underline 2 px (animated in-out 200 ms) + primary ink; unselected
secondary ink; hover ink 8%. Keyboard: arrows move selection, Ctrl+Tab /
Ctrl+Shift+Tab move with wrap. Tab bars are for peer views, not for
progressive disclosure of a single surface (menus/command palette do that).

## 20. Tooltip

Raised fill, `shadow-popover`, `radius-4`, text `type-size-12`; appears after
400 ms hover delay, offset 8 px from control; dismisses on move-away or
within 3 s; keyboard focus shows the same text in the status bar
(`ICONOGRAPHY.md` §5). Never contains interaction.

## 21. SpinBox (numeric stepper)

TextField + up/down segment; value `type-size-13` tabular figures; arrows
step by unit, Shift steps by 10×; focus ring; validation: out-of-range
values clamp on commit and show inline error.

## Cross-cutting rules

* All components honor `theming` (token swaps only), text scaling, reduced
  motion, and RTL mirroring.
* Hover states never change layout; active states never move the control.
* Focus rings are always painted by the theme tokens, never an image.
* Component states are all visual-QA baseline candidates
  (`VISUAL_QA.md`).
