## UI/UX repair cards — fresh 2026-09-14 evidence

All cards are OPEN. A screenshot proves only the visible state. Native observations are marked separately. Evidence paths below are relative to `.work/uiux-audit-2026-09-14/`. The full report embeds each accepted screenshot with its flow step. A cut-off control inside an otherwise complete app capture is a product finding, not an accidentally cropped evidence image.

### UI-01 — Give Linux users a visible route to basic file commands

**P1 · Sheets first; check each app when active · OPEN.** In the isolated Linux desktop with no global menu host, the window has no visible New/Open/Save menu or palette button. At 1280 px, overflow offers only Export CSV. A new user has to know Ctrl+K before discovering the commands.

**Evidence:** `sheets/09-native-start.png`, `14-native-after-new.png`, `16-native-palette.png`. Ctrl+K successfully opens a readable palette; retain that working shortcut. This observation is specific to the captured Linux environment, not a claim about every desktop menu host.

**Open:** `loom-sheets/crates/loom-sheets-app/ui/app.slint`, `SheetActionToolbar` in `ui/components.slint`, app command registry/palette, and menu-host detection in `loom-core/crates/loom-desktop/`.

1. Detect whether the desktop actually hosts the native/global menu. Publishing DBus menu data alone is not proof that users can see it.
2. When no host exists, render a shared local menu or clearly named menu button. Include New Workbook, Open, Save, Save As, Export, and Search Commands. Route every entry to the existing typed command and its enablement state.
3. Keep this entry visible at 1024, 1280, and 1440 px. Put secondary items inside the menu; do not squeeze more unlabeled icons into the toolbar.

**Done when:** A person using only the pointer can create, open, and save a test workbook without knowing a shortcut; a keyboard user can reach the same menu and return focus to the sheet. Check a Linux desktop without a global menu and one platform with native menu hosting. CODE-04 must protect the transitions.

### UI-02 — Stop opening a mostly empty Sheets inspector by default

**P2 · Sheets · OPEN.** The startup inspector consumes 320 px for a few properties and a large empty panel. At the reference 1280 px width, only 960 px remain for the central area (75%, before inner padding). The contract requires at least 78% and a closed inspector by default.

**Evidence:** `sheets/01-start-light-1440.png`, `02-compact-light-1024.png`, `09-native-start.png`. Compact mode already gives the sheet more room; keep that strength.

**Open:** Sheets `ui/app.slint` (`show-inspector`), `src/main.rs` (`INSPECTOR_WIDTH`, breakpoint handling); `loom-design-bible/contracts/desktop-ui.toml` `[app.sheets]`.

1. Start with the inspector closed, matching `right-inspector-default = 0`.
2. Keep a labeled Format toggle; opening it must preserve selection and show properties for that selection.
3. Respect the user's explicit open/closed choice across resizing. Use shared width tokens instead of an app-only 320 px constant.

**Done when:** A fresh 1280×800 window gives the central work surface at least 78% of available width. Toggle Format, resize below and above the compact breakpoint, and verify useful controls remain reachable without reopening a panel the user closed. Test 1024×720 and 1440×900 too.

### UI-03 — Reflow the template chooser and show truthful Recents

**P2 · Sheets · OPEN.** At 1024×720 the rightmost template previews and names are cut off. The chooser also repeats the selected Blank item under Recents and Basic; the Recents section is hard-coded rather than driven by real recent choices.

**Evidence:** `sheets/04-template-chooser.png`; source uses fixed 156 px cards in long horizontal rows. Cancel and Create remain visible — preserve that.

**Open:** `loom-sheets/crates/loom-sheets-app/ui/template_chooser.slint`, `SheetsTemplateCard`, category rows and Recents; template selection callbacks in `src/main.rs`.

1. Compute how many full cards fit in the content area after the category sidebar and padding. Move excess cards to another row; allow vertical scrolling of this content region.
2. Keep the entire template name and preview visible. Do not shrink the text or make the whole modal horizontally scroll.
3. Show Recents only from actual stored template choices. If there are none, omit that section or say `No recent templates`.
4. Keep keyboard selection visible as it moves; selection must refer to the intended template after reflow or category filtering.

**Done when:** At 1024×720 and 1.25/1.5 text scale, every template can be reached, its full name is readable, and Create produces the selected item. Escape/Cancel preserves the current workbook. No right-edge card is sliced in half.

### UI-04 — Use honest starter data and a clear empty-document path

**P2 · Sheets · OPEN.** Startup looks like a user's Budget but contains demo values. Rent/Transport say monthly, Food says weekly, yet Total simply adds all three. No common period or currency is stated.

**Evidence:** `sheets/01-start-light-1440.png`, `09-native-start.png`; `starter_workbook` assigns the mixed period labels.

**Open:** `loom-sheets/crates/loom-sheets-app/src/main.rs`, `starter_workbook`, startup selection, and template data.

1. Make a new document blank, with an obvious path to templates/examples. If opening an example is desired, name it `Example Budget` and make that choice explicit.
2. Give the example a single stated period and unit. Either all amounts are monthly or convert them with a visible formula and explanation before totaling.
3. Keep the sample's totals as real formulas. Never mark example data as a saved user document or silently overwrite user recovery with it.

**Done when:** First launch and New have clear, distinct blank/example behavior. Every Budget amount uses the same stated unit and time period, and changing an amount recalculates its total. Add a small test of example values/formulas, not a test that merely searches for the word Budget.

### UI-05 — Make Sheets header cells readable in high contrast

**P1 · Sheets accessibility · OPEN.** High-contrast mode hides the actual header text `Item`, `Amount`, and `Note`: the header background and text are both black. A1's formula field still says Item, confirming the text exists.

**Evidence:** `sheets/08-high-contrast.png` compared with `02-compact-light-1024.png`. Source uses `paper-line` for header fill and `paper-ink` for text; both are black in the high-contrast palette.

**Open:** Sheets `ui/components.slint`, cell background/text in `SheetGridSurface`; shared `loom-core/crates/loom-ui/ui/theme.slint`; corresponding design tokens/contracts.

1. Use a foreground/background pair for header cells. Do not use a line/separator color as a text background without its paired foreground.
2. Preserve the distinct active-cell border and readable user-specified fills. Check headers, selected ranges, and ordinary cells separately.
3. If a new semantic token is necessary, define it centrally and demonstrate it in the gallery; do not add an isolated color literal.

**Done when:** Item/Amount/Note are readable at 1024×720 and 1440×900 in all three themes, with and without selection/fills. Measure the contract's required contrast for each pair and inspect the actual screenshot. This repairs one visible accessibility defect; it is not full accessibility certification.

### UI-06 — Render the status and errors the controller produces

**P1 · Sheets trust/feedback · OPEN.** The window has no visible status bar or saved/unsaved indicator. Source declares and updates status strings, imports status components, but never instantiates those components. Users cannot rely on messages that only exist in memory.

**Evidence:** `sheets/09-native-start.png`, `11-native-commit.png`, `15-native-undo-after-new.png`; source check of `status-left`, `status-right`, and the complete `ui/app.slint` layout. Native title remains `Untitled` after the test edit. Save/error dialogs were not exhaustively exercised in this visual run.

**Open:** `loom-sheets/crates/loom-sheets-app/ui/app.slint`, status properties and layout; status-setting paths and document dirty state in `src/main.rs`; shared `LoomStatusBar`/`LoomStatusText`.

1. Render the existing status properties in the shared status bar using the contract height, outside the grid.
2. Show a clear unsaved state tied to actual document dirtiness. Clear it only after a successful save, not after merely opening a save chooser.
3. Show actionable error messages persistently enough to read; do not bury a failed save in an invisible property. Announce relevant messages through the existing accessibility mechanism without repeating every frame.

**Done when:** Edit, save, cancel save, force a write failure in a test folder, and trigger an invalid formula. Visible state must distinguish these outcomes. The grid must remain usable at 1024×720. A screen reader must receive the meaningful error once; record that check separately from screenshots.

### UI-07 — Clear edited fields when their document value changes

**P1 · Shared text input, reproduced in Sheets · OPEN.** After typing `audit123` and creating a new blank workbook, the formula field still paints that old text over its empty-field placeholder. The grid is blank. This makes the visible value disagree with the current document.

**Evidence:** `sheets/11-native-commit.png`, `15-native-undo-after-new.png`, `16-native-palette.png`. Reproduce with a freshly built native app before implementation; the first native captures used the existing local executable. Source points to the shared input's one-way `text: root.value` binding and its independently drawn placeholder.

**Open:** `loom-core/crates/loom-ui/ui/foundation/controls.slint`, `LoomTextField` (also check `LoomSearchField`); Sheets `FormulaNameBar` and buffer reset paths.

1. Reproduce typing, then programmatically replacing the field's value with a different string and with empty text.
2. Make the editable text and public value stay synchronized, using Slint's appropriate two-way binding or explicit update path. The placeholder must depend on the actual displayed text being empty.
3. Ensure New/Open, selection changes, commit, and cancel reset the buffer consistently without writing stale text into the new document.

**Done when:** Type `audit123`, commit, start a blank document, then select another cell. No old text remains; exactly one placeholder appears in an empty field. Repeat using Undo and switching tabs. Verify the shared gallery case and native Sheets; a headless initial-state screenshot cannot expose this defect.

### UI-08 — Make chart ranges and comparisons understandable

**P2 · Sheets · OPEN.** The default Budget chart compares Rent, Food, Transport, Total, and Average as if they were five peer categories. It provides no visible range/unit explanation. This turns summary values into misleading bars.

**Evidence:** `sheets/05-chart.png`; `src/analysis.rs` `plan_chart` currently collects populated rows for the selected columns.

**Open:** `loom-sheets/crates/loom-sheets-app/src/analysis.rs`, chart command/controller, `ui/chart.slint`, persisted chart specification.

1. Build a chart from an explicit selected range, with clear category/value columns. Preserve that range in the model.
2. Show/edit the range and series label in chart properties. For the starter example, select only the actual expense rows; do not silently guess that any formula row must be excluded in arbitrary user data.
3. Include a clear unit/period when the data supplies one. Keep charts live when source cells change.

**Done when:** Select A1:B4 in the example and create a chart: exactly Rent/Food/Transport appear. Show that source range. Changing Food changes the chart, Undo reverses the change, and Save/Open preserves the range. A deliberately selected Total row must still be chartable as an explicit user choice.

### UI-09 — Distinguish a workbook from one sheet in its commands

**P1 · Sheets · OPEN.** The palette says `New Sheet`, but the command replaces the entire workbook and clears its tabs/history. This label suggests a much smaller action than it performs.

**Evidence:** `sheets/16-native-palette.png`, the New callback in CODE-04, and the native blank workbook after New.

**Open:** Sheets command labels in `src/main.rs`/`src/palette.rs`, menus, tab-add control, and user-facing status strings.

1. Use `New Workbook`, `Open Workbook`, `Save Workbook`, and `Save Workbook As` for whole-file actions.
2. Use `Add Sheet`, `Rename Sheet`, and `Delete Sheet` for operations on a tab inside that file.
3. Update labels, tooltips, accessible names, and palette search synonyms together; retain the established shortcuts.

**Done when:** A workbook with two tabs remains intact when Add Sheet is chosen and gains one tab. New Workbook invokes CODE-04's dirty-work decision and replaces the whole file only after consent. Every visible label accurately names its scope.

### UI-10 — Remove the contradictory permission to truncate action captions

**P2 · Shared design contract · OPEN · Source-confirmed design debt.** `AGENTS.MD` requires readable complete action labels, but `desktop-ui.toml` permits 10 px ellipsized toolbar captions. A small model can obey one rule and violate the other.

**Open:** `loom-design-bible/contracts/desktop-ui.toml`, `[component.labeled-toolbar-item]`, `[component.icon-over-label-toolbar-item]`, toolbar/ellipsis clauses; shared toolbar components and their contract audits.

1. Make the contract agree with the readable-action rule: full captions or an accessible icon-only action with a tooltip and a reachable fully labeled menu item.
2. Set one canonical minimum caption size from the shared typography scale. Do not solve a tight toolbar by shrinking captions to 10 px.
3. Update component measurement/overflow behavior and contract checks together. Preserve optional ellipsis for user content names where explicitly allowed.

**Done when:** Long translated action labels and 1.25/1.5 text scale remain operable at the boundary widths 1179/1180, 1279/1280, and 1319/1320. The contract and UI tests reject clipped action captions. This is a source-confirmed rule conflict, not a claim that every captured toolbar currently clips.
