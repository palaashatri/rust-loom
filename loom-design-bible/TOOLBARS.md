# Toolbars

The context toolbar is a single row that reflects the current tool and
selection. It is the second layer of progressive disclosure — after direct
manipulation, before the inspector.

## 1. Model

* One toolbar per main window: height 40 px, horizontally below the title
  bar (`LAYOUT.md`).
* The toolbar is **contextual**: its contents are driven by the active tool
  and the current selection. Select a text box → text tools; select a clip →
  trim/color tools; no selection → document-level tools (insert, mode
  switches).
* The toolbar never stacks to a second row and never scrolls. Items that do
  not fit move to an overflow popover at the row's end (`overflow-ellipsis`).
* Tool changes (context swaps) animate in-out 200 ms per group (fade +
  cross-slide 4 px); reduced motion: fade 120 ms.

## 2. Grouping

Order of groups, left to right (per app context):

1. **Primary tools** — the tool set for the current surface (Writer: text
   tools; Photo: tool row; Video: edit tools). Tool-type buttons use
   `ToolButton` (checked state = active tool).
2. **Selection actions** — actions that apply to the current selection
   (group, align, format-paint, trim). These are enabled/disabled with the
   selection; they disappear when nothing is selected (they are not just
   grayed out) — except primary actions that must remain discoverable.
3. **Surface controls** — view toggles that are always visible (zoom readout,
   ruler toggle, snapping, guides).
4. **Primary action** — the single most-likely action (Save, Export, Render),
   rendered as a primary Button on the right end, before the overflow.

Grouping rules:

* Groups are separated by `space-16`; never vertical rules.
* No group exceeds 6 controls; a group that would exceed 6 splits or moves
  to the inspector/overflow.
* Ordering is fixed per app and documented in `FEATURE_MATRIX.md`-level
  app docs; users never customize toolbar order in v1 (`[future]` for
  customization).
* Toolbar controls are `IconButton` (20 px icons) with tooltips; text labels
  only for primary actions and mode switches with non-obvious icons
  (e.g. "Present", "Render").

## 3. Overflow policy

* Overflow detection: at window width, items are dropped to overflow in
  reverse order of priority (surface controls first, then selection actions,
  then primary tools).
* The overflow popover (Menu component) shows the same controls with labels
  and shortcut hints; it does not reorder or hide them silently — a chevron
  badge on the overflow button communicates dropped items (non-color: the
  button shows a dot badge when items are hidden).
* Every control in the toolbar also exists in a menu or the command palette;
  overflow never makes a command unreachable.

## 4. States and feedback

* Tool buttons reflect the active tool with checked state (accent-tinted,
  `COMPONENTS.md` §3).
* Actions apply to selection show enable/disable truthfully; disabled buttons
  carry a reason tooltip where the cause is not obvious (e.g. "Select a clip
  to enable trimming").
* Pressed feedback is instant; hover is instant; no toolbar-wide motion on
  interaction beyond the 120 ms icon-state change.

## 5. Accessibility

* Every toolbar control: focusable, focus ring visible, tooltip with name +
  shortcut, `accessible-description`.
* Toolbar is a single logical focus group: Tab enters/exits the toolbar,
  arrow keys move within it (toolbar row pattern), matching the
  application's overall focus-order contract (`ACCESSIBILITY.md`).
* At 1.5× text scale the toolbar height may grow to 56 px and icons keep
  their 20 px size with labels becoming visible; controls must not clip.
