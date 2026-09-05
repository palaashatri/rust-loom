# Inspectors

The contextual inspector is the third layer of progressive disclosure: it
shows properties of the current selection in a structured, searchable panel.
One model across the suite; applications do not fork it.

## 1. Placement and anatomy

* Right dock by default; width 280 px, resizable 240–360 px; persists per
  app. Never coexists with the sidebar on the same side.
* Toggle: toolbar `inspector` IconButton; shortcut `Cmd+Option+I`
  (Windows: `Ctrl+Alt+I`); collapses/expands the panel (out-quad 200 ms,
  reduced motion: instant).
* Anatomy: header 40 px (title = current selection summary, e.g. "Text box
  — 2 selected", plus collapse); body is a single scrolling column of
  **sections**.

## 2. Sections

Order per selection type, fixed within an app and documented in the app's
PRODUCT_SPEC:

1. **Object** — geometry, position, name, transform; the manipulation
   primitives of the selection.
2. **Style** — visual properties: fill, stroke, type style, effects.
3. **Document** — properties that affect the document/page/project context
   (page size, margins, master, layers settings).
4. **Metadata** — name, description, tags, dates, source, sync/relink state.
5. **Advanced** — everything else: scripting hooks, format-specific options,
   hidden format controls. Collapsed by default; expanded via its header.

Rules:

* Empty selections show **no** sections (the inspector shows an empty state
  with a hint: "Select an object to edit its properties").
* Sections are shown only if applicable: a text layer shows Object + Style
  + Metadata; a project-wide selection shows Document + Metadata.
* Each section has a summary line of the current key value when collapsed
  (e.g. Style: "Fill #B4552D · Weight 1.5").
* Sections never reorder on selection change; the scroll position and
  expanded sections persist across selection changes (stability rule).
* No tab bar inside the inspector; one scrolling column, search above it.

## 3. Property rows

* Two-column rows: label (left, `type-size-13` secondary ink) and control
  (right, aligned to a shared control column edge).
* Row group headers (`type-size-11` caps) only where a section is large
  (transform → position/size/rotation).
* Rows update live from the selection: changing an object's position in the
  canvas updates the row within 120 ms — the inspector is a view of the
  document, not a buffer (no "apply" buttons).
* Numeric rows: SpinBox with tabular figures; scrubby labels (drag label to
  adjust, Shift = ×10, Option = ×0.1) — direct manipulation in the
  inspector.
* Color rows: swatch + hex; swatch opens a color-well popover
  (`COMPONENTS.md`, popover rules) with eyedropper.

## 4. Search

* A search field pinned above the sections. Typing filters property rows by
  label and value across all sections (fuzzy substring, case-insensitive);
  matching sections open, non-matching collapse; matches are highlighted
  with accent.
* Search never hides sections permanently: clearing the query restores the
  prior expanded state.
* Keyboard: search is reachable via `Cmd+F` while the inspector has focus
  or via the palette; Esc clears search, then closes.
* Search is discoverable: the field shows "Search properties…" placeholder
  and a magnifier icon; it is one of the inspector's three persistent
  affordances (search, pin, collapse).

## 5. Pinning

* `pin` IconButton in the section header pins a section: the pinned section
  stays expanded and scrolls to remain visible (or the inspector splits into
  two columns: pinned section above, scrolling remainder below — the
  simpler v1 behavior: pinned sections rise to the top and never collapse
  until unpinned).
* Pinning is per-app-persistent and shows in the section header as an accent
  pin glyph; pinned sections are announced to screen readers ("Section
  pinned").

## 6. Keyboard operation

* Inspector is a focus group: Tab enters, arrows move rows, Space/Enter
  activate the focused control, Esc leaves.
* Arrow keys move focus between rows in a section; collapsed sections are
  skipped; Home/End jump to top/bottom of the panel.
* All rows are reachable: sliders with arrows, SpinBox with typed input,
  ComboBox with arrows+Enter; nothing requires a mouse.
* Focus ring per the state model; row focus shows the row's label read by
  the screen reader ("Position, X, 120").

## 7. Updating and performance

* The inspector updates on selection change within 200 ms (standard token);
  updates animate as 120 ms fade for value text (reduced motion: same, it's
  opacity).
* Never block input while refreshing; refresh is incremental, not a panel
  rebuild.
* At 1.5× text scale the inspector floor width rises to 320 px so
  label+control rows remain usable.
