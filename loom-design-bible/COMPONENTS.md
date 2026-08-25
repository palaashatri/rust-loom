# Loom Shared Components

This file defines component **semantics and ownership**. Exact measurements live in [`contracts/desktop-ui.toml`](contracts/desktop-ui.toml); token values live in [`tokens/loom.toml`](tokens/loom.toml). Do not duplicate or override those values in an application.

## Ownership rule

`loom-core/crates/loom-ui` owns every generic desktop control and shell primitive. Applications compose them and supply domain state/commands. They do not fork visual implementations.

Shared ownership includes at minimum:

- document/title chrome;
- context toolbar and toolbar groups/overflow;
- buttons, icon buttons, toggles, checkboxes, radio/segmented controls;
- text/search/numeric fields;
- sliders and steppers;
- menus, popovers, tooltips, dialogs;
- sidebars, inspectors, section headers, status bars;
- tabs and list/tree rows;
- empty/loading/error states;
- command palette;
- canvas backdrop, selection overlay, transform handles, rulers and guides;
- layer/object lists;
- timeline ruler, track header, clips, playhead and trim handles.

If two Loom applications need the same interaction pattern, it belongs in `loom-ui` unless the shared-platform owner documents why their semantics differ.

## Universal state model

Every interactive component implements these states where applicable:

`default → hover → pressed → keyboard-focus → disabled`

Selectable components additionally implement `unselected` and `selected`. Inputs additionally implement `valid` and `invalid`. Async actions additionally implement `idle`, `running`, `success`, `failure`, and `cancelled` when those states are observable.

State rules:

- hover/press/focus never change component geometry;
- keyboard focus uses the shared focus token and remains visible in every theme;
- disabled controls are not clickable, keyboard-activatable, or accessibility-activatable;
- selected/checked state is not represented by color alone;
- error state includes text or icon semantics, not only a red border;
- every icon-only control has an accessible name and tooltip;
- every action resolves to the same typed command used by menus and shortcuts.

## Standard controls

### Button / ToolButton

Use the contract's standard control height, horizontal padding, radius, typography, and state durations. A button label never ellipsizes. If there is not enough width, change toolbar composition or move the action to overflow.

Toolbar controls are visually quiet. Routine toolbar items do not need a permanent raised bezel; hover, checked, pressed, and focus states provide affordance.

### IconButton

Uses the standard square target and standard icon size. Tooltip is mandatory. An unambiguous symbol can replace a text label only where the corresponding command remains discoverable in menus/palette.

### Text/Search/Numeric fields

Use shared field geometry. Placeholder text is secondary, not disabled. Validation does not resize the control; error/help text occupies a separate row below or an inspector message region.

### Segmented control

Use only for a small mutually-exclusive set. Six segments is the absolute maximum; larger sets become a menu/combobox/list. Segment labels never truncate.

### Slider

The visual handle may be smaller than the desktop target only when its hit region remains at least the contract target. Arrow keys adjust values and the accessible value is exposed.

### List/tree row

User-authored names may ellipsize with full tooltip/accessibility value. Command labels, badges with semantic meaning, and row actions may not collide with the name.

## Chrome components

### DocumentChrome / AppHeader compatibility surface

The long-term shared component is document-oriented chrome, not a branded app banner. Existing `AppHeader` call sites are a migration compatibility surface.

Rules:

- document/project title is primary;
- app name is not repeated as a large in-window title;
- neutral facts such as "Local" or package type are not permanent chrome;
- no decorative app-logo tile;
- title may ellipsize because it is user content;
- important modified/sync/error state may appear only when it changes a user decision.

### WorkspaceToolbar

Implements the algorithm in `TOOLBARS.md` and the machine contract: one line, maximum three groups, deterministic priority collapse, no scrolling, no clipping.

### Sidebar / Inspector

Flush work surfaces, not card stacks. One body scroll surface. Sections use spacing and headers rather than decorative containers. Property labels do not truncate; rows stack when localization/text scale requires it.

### StatusBar

Contains transient status, progress/cancel affordance, and compact readouts. It is not a second toolbar and never competes with content hierarchy.

## Editor primitives

Shared editor primitives are as important as shared buttons. A professional suite cannot obtain coherent behavior if Photo, Present, Motion, Video, and Studio each invent selection, drag, snapping, zoom, and timeline semantics.

### Canvas

The shared canvas family owns viewport pan/zoom, hit testing, selection overlay, transform handles, snapping, guides, keyboard nudging, gesture cancellation, and direct-manipulation state. Domain engines provide objects and operations; the UI layer does not own document truth.

### Selection overlay

Selection handles use exact visual/hit sizes from the contract. Selection itself is not an undoable edit. Drag operations coalesce into one operation; Escape restores the pre-gesture state.

### Timeline

Motion, Video, and Studio share ruler/playhead/track/clip primitives. Application-specific track content can differ, but pointer capture, selection, trim hit regions, scrolling/zoom, snapping, and keyboard semantics are shared.

### Grid/document viewport

Writer and Sheets use specialized editor surfaces built from shared scrolling, selection, keyboard-routing, ruler/header, and inspector primitives. A generic `TextEdit` or fixed demo grid is not a professional editor architecture.

## Component completion gate

A shared component is complete only when:

1. its geometry comes from the contract/tokens;
2. all required states render and behave;
3. keyboard and accessibility activation work;
4. it appears in the deterministic component reference surface;
5. realistic, empty, disabled, and error states have fixtures as applicable;
6. required viewport/theme/text-scale captures pass approved-baseline comparison;
7. no known overlap/clipping defect remains;
8. at least one application uses it in a real workflow;
9. the previous app-local implementation is deleted after migration.

Passing a source-string audit or producing a stable screenshot alone is insufficient.
