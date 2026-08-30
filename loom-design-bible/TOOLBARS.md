# Loom Toolbar Contract

Toolbars are single-line, contextual command surfaces. Exact dimensions and breakpoints are defined in [`contracts/desktop-ui.toml`](contracts/desktop-ui.toml).

## Explicit toolbar slots

`ContextToolbar` owns the 40 px context row. `LabeledToolbar` owns the
48–52 px row required by `IconOverLabelToolbarItem`; `IconOnlyToolbarItem`
remains the 28 px compact target. `Toolbar` keeps a legacy labeled-slot
default for existing hosts, but new surfaces must choose an explicit slot.
Content cannot silently increase a context row's height.

## Structure

A toolbar contains at most three logical groups:

1. **Leading** — navigation/document structure or the primary tool family.
2. **Center** — commands for the active content, tool, or selection.
3. **Trailing** — view/search/export and the overflow menu.

More than three visible groups is a design failure. Do not solve command abundance with more separators or another toolbar row.

Items within a group use the contract item gap; groups use the contract group gap. Toolbars never scroll and never wrap.

## Priority algorithm

Every toolbar item declares one priority:

- `P0`: visible at all supported widths.
- `P1`: may collapse from labeled form to icon-only below the contract breakpoint, but only if the symbol is unambiguous and a tooltip/accessibility label exists.
- `P2`: moves to overflow below the contract breakpoint.

The implementation applies this order whenever width decreases:

1. move P2 actions to overflow;
2. convert eligible P1 actions to icon-only;
3. remove redundant toolbar exposure while retaining menu/palette access;
4. never clip, ellipsize, overlap, wrap, scroll, or shrink below control minimums.

The shared `ResponsivePolicy` evaluates these transitions at one canonical
set of boundaries: P1 icon-only below 1180 px, P2 overflow below 1320 px.
Validation exercises 1179, 1180, 1279, 1280, 1319, and 1320 px, plus 150%
text and both LTR/RTL directions. 1279/1280 must produce the same policy
state; a host-defined 1280 px breakpoint is a contract violation.

If P0 still does not fit, the toolbar contains too many P0 actions and must be redesigned.

## Command placement

A toolbar is not the command inventory. Every toolbar command exists in the shared command registry and therefore has a menu/command-palette/keyboard representation where applicable.

Permanent toolbar placement is reserved for frequently used or context-critical actions. Secondary commands such as Save As, uncommon import variants, detailed export settings, or one-off maintenance commands normally live in menus/palette.

A text-labeled action must not visually merge with an adjacent icon action. Keep labeled actions in their own group or give them the group spacing required by the contract.

## Visual treatment

Routine Mac-class desktop toolbar controls are visually quiet. An unchecked toolbar icon normally has no permanent heavy bezel. Hover, pressed, selected, disabled, and keyboard-focus states provide affordance.

Rules:

- action labels never truncate;
- toolbar controls use shared `loom-ui` components only;
- no app-local rounded rectangles pretending to be buttons;
- no decorative vertical rule after every small group;
- no application logo tile in the toolbar;
- no neutral "Local"/"Editing" badge occupying permanent toolbar width;
- export/render may be visually emphasized only when it is genuinely the primary current action;
- destructive commands are not promoted merely for symmetry.

## Context behavior

Toolbars reflect the active selection/tool while preserving stable command locations where possible. Context changes must not cause unrelated controls to jump between arbitrary positions.

Examples:

- Writer text selection → character/paragraph commands.
- Photo selected layer → transform/mask/adjustment commands.
- Present selected object → arrange/style commands.
- Video selected clip → trim/clip commands.

No selection exposes document/surface commands rather than a set of inexplicably disabled object controls.

## Accessibility

Every toolbar item has:

- accessible name;
- keyboard focus path;
- command registry identity;
- tooltip for icon-only representation;
- shortcut hint where one exists;
- disabled reason when the cause is not obvious.

Tab enters/leaves the toolbar as a logical region; arrow-key movement within a toolbar group is preferred when the shared component provides roving focus.

## Acceptance

A toolbar is accepted only when all required viewports and text scales satisfy:

- one line exactly;
- zero action-label clipping;
- zero overlap;
- no control below minimum size;
- at most three visible groups;
- deterministic overflow membership;
- every overflowed action remains reachable;
- keyboard focus order matches visual order;
- realistic localized/pseudolocalized labels do not break the layout.

The source audit records a geometry manifest for each state and rejects
positive-area rectangle overlap, label clipping, a second toolbar line, and a
primary-surface width below the shared minimum. Screenshot hashes remain useful
regression evidence but cannot substitute for these assertions.

Replacing a screenshot baseline to hide a toolbar collision is a release-process defect.
