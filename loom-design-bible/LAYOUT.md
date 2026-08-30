# Loom Layout Contract

Exact values and responsive thresholds are machine-readable in [`contracts/desktop-ui.toml`](contracts/desktop-ui.toml). This document defines composition. Application layouts may not invent a second geometry system.

## Canonical shell objects

Every shell region has one named Slint owner in `loom-ui/ui/toolkit.slint`:

| Role | Canonical object | Compatibility names |
|---|---|---|
| title/document chrome | `TitleChrome` | `DocumentChrome` |
| 40 px context toolbar | `ContextToolbar` | `Toolbar { labeled-slot: false }` |
| 48–52 px labeled toolbar | `LabeledToolbar` | `Toolbar` (legacy default) |
| compact icon action | `IconOnlyToolbarItem` | `ToolbarIconButton` |
| icon-over-label action | `IconOverLabelToolbarItem` | `AppleToolbarItem` |
| Sheets tabs | `SheetTabStrip` | `TabStrip` |
| formula/name row | `FormulaBar` | — |
| inspector section | `InspectorSection` | — |
| property row / field | `PropertyRow` / `Field` | `TextField` |
| status / overflow | `StatusBar` / `Overflow` | `ToolkitStatusBar` / `ToolbarOverflowButton` |

Compatibility names are inheritance or re-export shims only. They do not own
geometry, colours, focus, or interaction state.

## Shared vertical chrome

A normal Loom document window is composed in this order when those regions apply:

```text
Document/title chrome
Context toolbar or app-specific persistent control row
Primary work area
Status bar
```

The title region and toolbar are separate semantic regions even when a platform later integrates them visually.

`ContextToolbar` is exactly 40 px high. A toolbar containing an
`IconOverLabelToolbarItem` must explicitly use `LabeledToolbar` (48–52 px);
hosts may not rely on child content to stretch a context row.

Rules:

- chrome is compact and neutral;
- the document/project name is the useful title;
- the application name is not repeated as a large banner inside the window;
- persistent "Local", package-format, prototype, or implementation-status badges are forbidden unless the state changes a user decision;
- the primary work area stretches before any decorative region;
- no persistent overlay covers a canvas, stage, viewer, grid, or document page;
- below a comfortable width, optional panels collapse before primary content violates its minimum size.

## Shared horizontal work-area grammar

When all regions exist:

```text
optional left sidebar | primary work surface | optional right inspector
```

The sidebar and inspector use contract widths and resize ranges. They are flush surfaces separated by hairlines. They are not floating rounded cards.

The primary surface must meet the per-app `primary-share-min` rule at the reference viewport. If it cannot, collapse optional chrome rather than squeeze controls or content.

## Application shells

### Writer

```text
Title 40
Formatting/context toolbar 40
Document canvas: flexible, dominant
Status 28
```

No sidebar or inspector is open by default. Pages are centered inside the flexible canvas with contract fit margins. Formatting chrome must not consume enough width to clip; lower-priority commands overflow.

### Sheets

```text
Title 40
Formula bar 32
Virtualized grid: fills width and height
Sheet tabs 30
Status 28
```

The visible grid expands to the viewport. Empty space after an arbitrary fixed column count is a defect, not intentional whitespace.

### Present

```text
Title 40
Context toolbar 40
Slide navigator 220 default | Stage flexible | Inspector 280 optional
Status 28
```

Navigator and inspector collapse before the stage becomes unusably narrow.

### Photo

```text
Title 40
Context toolbar 40
Tool rail 40 | Image canvas flexible | Layers/Inspector 280-ish
Status 28
```

Tool/mode/status hints are chrome or transient gesture feedback, never permanent image overlays.

### Motion / Video

```text
Title 40
Context toolbar 40
Browser 240 | Viewer flexible | Inspector 280 optional
Shared timeline >= 220 high
Status 28
```

Transport/timecode is placed in chrome. Timeline controls do not occupy the ruler's content lane. Viewer, media browser, and timeline must agree about loaded project/media state.

### Studio

```text
Title/transport chrome
Track headers 180 | Arrangement flexible
Optional mixer/inspector
Status 28
```

Track names may ellipsize with tooltip. Transport and command labels never truncate.

### Encode

```text
Title 40
Queue toolbar / batch controls
Queue 280 | Selected-job settings flexible/300
Status 28
```

Job identity and destination/settings have greater hierarchy than progress percentage. Progress is a property of the selected job or batch, not a hero element.

## Responsive algorithm

At every required viewport:

1. Reserve mandatory title/status regions.
2. Reserve the primary work surface minimum.
3. Allocate required fixed chrome.
4. Allocate optional panels at preferred width.
5. Shrink optional panels toward their minimums.
6. Collapse optional panels if the primary surface would otherwise violate its minimum.
7. Apply toolbar priority collapse/overflow.
8. Never reduce action controls below their contract dimensions.
9. Never wrap a toolbar.
10. Never overlap or clip a control to satisfy width.

The shared `ResponsivePolicy` owns the two transitions. Below 1180 px P1
actions use compact icon-only targets; below 1320 px P2 actions move to the
single overflow command. The required transition probes are 1179, 1180, 1279,
1280, 1319, and 1320 px. 1279/1280 are stability probes, not an additional
breakpoint.

The bootstrap UI audit builds a geometry manifest at every probe, required
viewport, direction, and 1.0/1.5 text scale. It asserts rectangle bounds,
positive-area overlap, toolbar line count, label-fit budget, and primary-surface
minimums independently of PNG hashes.

This ordering is mandatory. Arbitrary `compact-layout` branches that merely change paddings without following this ordering do not satisfy the contract.

## Scaling

Required release matrix: all contract viewports at text scale 1.0, plus reference/stress captures at 1.25 and 1.5. 2.0 is the accessibility stress target.

Larger text does **not** automatically multiply every chrome dimension. Controls remain coherent desktop controls; when labels no longer fit, responsive composition changes, rows stack where allowed, and lower-priority toolbar actions overflow. Content zoom is independent of UI text scale.

RTL is a first-class direction probe. Horizontal layouts mirror through the
platform layout direction while preserving command order, focus order, and the
same geometry budgets; no app-local padding or colour override is permitted.

## Alignment

- sibling controls align to the same baseline/center line;
- panel and toolbar edges align to shared boundaries;
- no arbitrary floating margins in chrome;
- text baselines, not box tops, govern mixed control/label alignment;
- hairlines are one logical pixel;
- every persistent 1 px separator must land on a deterministic logical boundary under the software renderer.
