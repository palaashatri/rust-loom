# Loom Mechanical Design Standard

This document is the normative implementation procedure for Loom UI. It exists so a coding agent with **no image understanding** can produce and validate the same interface geometry as a visual designer.

Machine-readable values live in [`contracts/desktop-ui.toml`](contracts/desktop-ui.toml). Primitive and semantic theme values live in [`tokens/loom.toml`](tokens/loom.toml). If prose and TOML disagree, TOML wins and CI must report the drift.

The target is not an imitation of proprietary assets or source. Loom uses the desktop interaction discipline associated with first-class macOS creative applications: useful document titles, restrained chrome, content-first hierarchy, compact pointer/keyboard controls, predictable panels, deliberate toolbar grouping, direct manipulation, and exhaustive state polish. Loom keeps its own iconography, accent, branding, and application identity.

## 1. Definition of polished

"Polished" is not a subjective acceptance criterion. A surface passes only when all of the following are true:

1. Every dimension belongs to the contract or is content-derived.
2. No control, action label, section heading, or persistent status indicator overlaps or clips at any required viewport/text-scale combination.
3. No toolbar wraps or scrolls. Lower-priority actions move to overflow before available width becomes insufficient.
4. User-authored names may ellipsize; action labels and control labels may not. Ellipsized user content exposes its full value by tooltip/accessibility description.
5. Primary work content receives the minimum share specified for the application in `desktop-ui.toml`.
6. Every interactive control has default, hover, pressed, keyboard-focus, and disabled behavior; selectable controls add selected/unselected; inputs add valid/invalid.
7. Every icon-only action has an accessible name and tooltip and is reachable through the same command registry as menus/keyboard shortcuts.
8. No visible control is a placebo. Disabled functionality is visibly disabled and explains why.
9. No application draws its own generic buttons, text fields, segmented controls, panel shells, title chrome, toolbar shells, inspector shells, status bars, menus, or popovers.
10. A human-approved baseline exists for realistic content, empty state, and error state. Baselines containing known defects are invalid evidence.

A screenshot matching an unreviewed baseline is **not** proof of quality.

## 2. Geometry authority

All measurements are logical desktop pixels.

### Window and chrome

- Title/document chrome: 40 px.
- Context toolbar: 40 px.
- Status bar: 28 px.
- Panel header: 32 px.
- Standard control: 28 px high.
- Compact control: 24 px high.
- Prominent control: 32 px high.
- Standard icon: 16 px.
- Default pointer target: 28 × 28 px.
- Hairline/separator: 1 px.

The app name is not useful document hierarchy and must not be repeated as a large title inside the window. The document/project name is the primary title. Neutral implementation facts such as "Local", "Editing locally", or package type do not occupy permanent title chrome. Surface them only when they change a decision or explain a state.

### Spacing

Allowed chrome/component spacing values are exactly:

`2, 4, 6, 8, 12, 16, 20, 24, 32, 40, 48, 64`.

No 5 px, 7 px, 9 px, 10 px, 11 px, 14 px, 15 px, 18 px, 26 px, 28 px padding/gap values may appear in application chrome merely because they "look right". If a missing value is genuinely necessary, change the contract and justify it once.

### Typography

- UI labels: 13 px / medium (500).
- Section headings: 13 px / semibold (600).
- Caption/status: 11 px.
- Small metadata: 12 px.
- Body/help prose: 14 px.
- Subtitles: 16 px.
- Content headings: 20 or 24 px only where content hierarchy requires them.
- Numeric readouts use tabular figures.
- Bold 700 is reserved for document content or exceptional hierarchy; routine chrome should not look shouted.

Text size changes must not alter button height, reorder controls, or create a second toolbar line. At larger text scales the responsive policy moves lower-priority controls into overflow.

## 3. Surface hierarchy

Use only these hierarchy levels:

1. **Canvas/work area** — user content and the background immediately around it.
2. **Chrome** — title/toolbar/status regions.
3. **Panel** — sidebar/inspector/timeline headers and bodies.
4. **Raised control** — buttons, fields, selected rows, menus.
5. **Overlay** — menu, popover, modal, tooltip.

Do not create a rounded card for each section. Sidebars and inspectors are flush work surfaces separated by hierarchy, hairlines, spacing, and section headers. Rounded containers are reserved for controls, real card-like objects, menus/popovers, and modal surfaces.

Content owns contrast. Chrome is deliberately neutral. The Loom terracotta accent is restricted to focus, selection, active/checked state, primary action, and meaningful emphasis. It is not decoration.

## 4. Toolbar grammar

Toolbars are the highest-risk source of current Loom defects, so they follow an algorithm rather than free placement.

### 4.1 Maximum structure

A toolbar has at most three groups:

- leading: navigation/document structure;
- center: commands for current content/selection;
- trailing: view/search/export/overflow.

Groups are separated by 12 px. Items within a group use 4 px. A toolbar never wraps and never scrolls.

### 4.2 Priority algorithm

Every toolbar item has priority 0, 1, or 2.

- **P0:** always visible at every supported width.
- **P1:** visible with label at wide width; icon-only below 1180 px when the icon is unambiguous and a tooltip exists.
- **P2:** moves to the overflow menu below 1320 px.

If P0 still cannot fit, remove redundant toolbar actions and keep them in menus/command palette. Never solve width pressure by clipping text, shrinking controls below contract size, negative spacing, or drawing over adjacent controls.

### 4.3 Command placement

Every toolbar action exists in the command registry and therefore can also appear in the menu/command palette. Save As, infrequent import variants, advanced export settings, and similar secondary commands normally belong in menus rather than permanent toolbar slots.

Text-labeled actions are separated from unrelated icon-only groups. Actions never ellipsize.

## 5. Panels and inspectors

Sidebar default: 240 px, range 200–360 px.

Inspector default: 280 px, range 240–360 px.

Panel padding: 12 px. Section gap: 16 px. Property rows are at least 28 px high. Property labels reserve 84–112 px; values consume remaining width. If a localized property label cannot fit, the row becomes stacked rather than truncating the label.

Only the panel body scrolls. Nested section scrollbars are forbidden. Sections may collapse, but the user must not encounter scroll-within-scroll for normal property editing.

User-authored layer, track, file, and item names may ellipsize. Inspector commands, field labels, and section headings may not.

## 6. Canvas and direct manipulation

Persistent status UI must not cover a canvas. Play/pause state, mode name, zoom, timecode, snapping, and transport state live in chrome unless they are transient feedback tied to a direct manipulation gesture.

Canvas invariants:

- minimum useful viewport: 480 × 320 px;
- zoom-to-fit leaves 24 px minimum margin;
- selection outline: 1 px;
- visible transform handle: 8 px, hit target 20 px;
- snap distance: 6 px;
- guides: 1 px;
- pan/zoom never mutates document geometry;
- selection changes never create undo entries;
- a drag is one coalesced undoable operation;
- Escape restores pre-gesture geometry;
- keyboard nudge follows the same snapping/undo model.

No canvas is considered complete because it can draw content. It needs selection, hit testing, transform, keyboard operation, context commands, accessibility semantics, and persistence-aware undo.

## 7. Timeline grammar

Motion, Video, and Studio share timeline primitives rather than drawing three unrelated timelines.

- minimum timeline height: 220 px;
- track header: 180 px;
- ruler: 24 px;
- default track: 44 px; compact: 32 px;
- clip corner radius: 4 px;
- playhead: 1 px visual with a wider hit region;
- trim handle: 4 px visual / 12 px minimum hit region.

Track names may ellipsize only with tooltip/accessibility text. Tool names, transport commands, and ruler labels cannot collide with tracks. Timeline controls live outside the ruler row unless they are ruler interactions.

## 8. Application shell contracts

### Writer

Persistent structure: document title → context formatting toolbar → document canvas → status bar. No sidebar or inspector by default. The page occupies the visual focus. Formatting applies to text selection/caret state, never "all paragraphs" unless explicitly invoked as a document-wide command.

### Sheets

Persistent structure: document title → formula bar → grid → sheet tabs → status bar. The grid fills available viewport width and height and virtualizes beyond visible cells. A fixed eight-column demo grid with unused canvas is a contract violation.

### Present

Persistent structure: title/toolbar → 220 px slide navigator → flexible stage → optional 280 px inspector → status. Slide thumbnails and inspector are collapsible before the stage falls below its minimum useful width.

### Photo

Persistent structure: title/toolbar → compact tool rail + flexible image canvas + layers/inspector → status. Tool/mode hints cannot permanently overlay the image. Direct manipulation is primary; inspector controls mirror selection state.

### Motion / Video

Persistent structure: title/toolbar → media/browser + viewer + inspector → shared timeline → status. Transport/timecode is chrome, not a canvas overlay. The project cannot claim media sources are empty while sample timeline clips exist.

### Studio

Persistent structure: title/transport → track headers + arrangement → optional mixer/inspector → status. Track names may ellipsize; all transport/action labels remain complete.

### Encode

Persistent structure: title/queue controls → queue + selected-job settings → status. Progress is subordinate to job identity and output settings; a giant isolated percentage is not the visual hierarchy.

## 9. Accessibility and platform behavior

Loom is a pointer/keyboard desktop suite. Standard controls use 28 × 28 px targets; specialized dense affordances may render as small as 20 px only when their actual hit target remains at least 28 px. Body/control text maintains at least 4.5:1 contrast; non-text controls and focus indication maintain at least 3:1.

Every operation available from a toolbar has a keyboard/menu path. Focus order follows visual reading order. Focus indication is 2 px and must remain visible in all themes. State is never communicated by color alone.

Text-scale tests are 1.0, 1.25, 1.5 for every release; 2.0 is the accessibility stress target. High contrast and reduced motion are first-class variants rather than afterthoughts.

## 10. Motion

- hover: 0 ms onset;
- press: 0 ms;
- micro state transition: 120 ms;
- ordinary transition: 180 ms;
- panel transition: 220 ms;
- routine interaction maximum: 250 ms;
- reduced motion: 0 ms for spatial/decorative motion.

Repeated professional controls must never make users wait for animation. No bounce is used for routine chrome.

## 11. Non-visual QA procedure

An agent without vision follows this exact sequence:

1. Read `contracts/desktop-ui.toml` and `tokens/loom.toml` before editing UI.
2. Use only shared `loom-ui` components for standard chrome and controls.
3. Run the design-system contract audit. Any literal palette or component metric drift fails.
4. Build the component gallery/smoke surface for light, dark, and high-contrast.
5. Capture required viewport/text-scale matrix.
6. Run deterministic image-diff against **approved** baselines. A changed image requires explicit baseline review; the agent may not bless its own change by simply replacing the baseline.
7. Run geometry/state assertions: no overlap, action clipping, toolbar wrap, or primary-work-surface violation.
8. Run keyboard/accessibility journeys for each changed component.
9. Run realistic-content, empty-state, and error-state fixtures.
10. Only then migrate an application to the new toolkit.

A no-vision agent is allowed to know that its screenshot differs; it is not allowed to infer that a difference is attractive. It must satisfy the contract and approved baseline or escalate the baseline for review.

## 12. Migration order

Application feature expansion pauses where it would create more legacy UI. The productization reset proceeds in this order:

1. Synchronize token sources and runtime theme.
2. Make `loom-ui` the sole owner of standard controls/chrome.
3. Build a complete component gallery/state matrix.
4. Add strict contract and baseline validation.
5. Replace title chrome and toolbar composition across all apps.
6. Migrate Writer and Sheets document/grid primitives.
7. Migrate Photo canvas/layer/inspector primitives.
8. Migrate Present/Motion/Video onto shared scene/timeline/direct-manipulation primitives.
9. Migrate Studio timeline/mixer primitives.
10. Migrate Encode queue/settings primitives.
11. Resume broad feature work only through toolkit components.

A migration step is complete only when old app-local implementations are deleted, not merely hidden behind a new component.
