# Document Editor

The document surface of Writer (and the text surfaces of Present and Photo).
This document fixes the page canvas chrome, cursor behavior, and IME rules.

## 1. Page canvas chrome

* Pages render on `color-surface-canvas`; the page itself is
  `color-surface-raised` with a 1 px hairline border (dark theme) — depth by
  color, never a drop shadow (`DESIGN_TOKENS.md` §10).
* **Page gaps**: consecutive pages separated by `space-16` vertical gaps
  (continuous mode: no gap, one flowing column); page mode shows a page
  break glyph (2 px line + "Page 2" label, `type-size-11` secondary ink)
  only when gaps are hidden.
* **Margins and text bounds**: editable text shows a subtle bounds inset
  (1 px accent at 25% when the caret or selection is inside the paragraph);
  margins visible via rulers (see §2).
* **Rulers**: visible by default in page mode; 20 px top + left ruler
  strips (`CANVAS.md` §1): page edge, margins, indents, tab stops, column
  guides; tab-stop drag handles on the top ruler; margin markers draggable
  (Option+drag moves both margins).
* **Text cursor**: 2 px wide (`border-width-strong` / 2 px), accent color,
  height = line height; blinks 530 ms on / 270 ms off while idle, **stops
  blinking while the user is typing or moving the cursor** (focus
  stability); caret color switches to a contrasting color when the caret
  rests on accent-colored text (visibility rule).
* **Selection in text**: accent fill at 25% behind glyphs (never obscures
  the glyph); selection across pages stays continuous in the logical text
  flow; screen-reader selection announcements per `ACCESSIBILITY.md`.

## 2. Cursor movement rules

* Movement is by **logical character** (grapheme cluster), honoring
  bidirectional text: arrow keys move visually in RTL paragraphs (visual
  movement with logical storage — the standard modern editor model); Home/
  End = start/end of visual line; Ctrl/Cmd+Left/Right = word; Ctrl/Cmd+Up/
  Down = paragraph; PageUp/PageDown = viewport with caret following.
* Double-click selects word; triple-click selects paragraph; Shift+arrows
  extends selection; the selection anchor never moves on its own.
* Cursor mapping between logical (UTF-16/UTF-32) offsets and visual
  positions is a tested contract (`loom-core` `loom-text`); property tests
  cover RTL and combining characters (`loom-spec` testing section).
* The caret never scrolls out of view: typing near the edge auto-scrolls
  with a 24 px margin; scroll is instant (no animation while typing).

## 3. Input and editing

* Input is composition-aware (see §5); everything typed goes through the
  document model as text operations — undoable, journaled, replayable
  (undo/recovery contract in `loom-core`).
* Typing latency budget: keystroke → glyph on screen within one frame
  (16.7 ms); layout of the visible paragraph only (incremental layout —
  never re-layout the whole document per keystroke); heavy documents
  (10,000+ paragraphs) must keep the caret line fluid while background
  layout catches up.
* Autocorrect/auto-capitalization: off by default in Loom documents
  (professional control); the features, when enabled, show a transient
  underline (ink at 40%) and a non-modal suggestion popover (Option+click
  or palette to accept) — never silent replacement.
* Smart quotes/dashes: on by default in writing apps, per locale, with an
  "undo auto-replacement" affordance (the replaced text is undoable as one
  step); straight quotes preserved in code-like contexts (formulas, HTML).

## 4. Page, layout, and long documents

* Pagination and layout run off-thread; the visible page renders from the
  committed layout; repagination is observable via a status-bar chip
  ("Paginating…") only when it exceeds 250 ms.
* Footnotes, endnotes, headers/footers, and TOC markers are layout objects:
  they render inline in the page flow; selection of a marker selects its
  content ("footnote 3").
* Find & replace: `Cmd+F` opens a non-modal find bar (in-toolbar,
  32 px): matches highlighted with accent outline (current match: accent
  fill 25% + 2 px outline); Replace panel extends the same surface; find
  never creates a dialog (`DIALOGS.md`).
* Word count and document stats live in the status bar (word count,
  page/line position of the caret, tabular figures), updated on idle.

## 5. IME composition

* Composition is a first-class state: the composing region shows a 2 px
  accent underline under the current segment; candidate windows appear
  near the caret, sized to candidate text, styled per Menu/popover rules.
* Composition state is preserved across cursor moves within the segment;
  committing a composition (Enter/Space per IME) inserts the text as one
  undoable unit; cancelling (Esc) restores the pre-composition text.
* The editor never reorders or re-layouts a composing segment mid-
  composition except for caret-driven scrolling; composition with
  bidirectional text and RTL locales is tested per `loom-spec`'s IME test
  matrix.
* On window blur with active composition: composition commits (per platform
  convention); on app crash, composition state is journaled with the
  document (recovery contract).

## 6. Accessibility

* Caret and selection are fully reported to assistive technologies: caret
  position (character + line), selection ranges, composing text.
* Typeahead/autocomplete surfaces announce options as a listbox with
  selected-state announcements.
* Text scaling: at 1.25×/1.5× the page zoom stays at document zoom — text
  scale applies to UI chrome and the editing surface's minimum text size,
  never silently to document formatting (document text scale is a document
  property, user-controlled).
* All cursor movements are keyboard-native by construction (this surface IS
  the keyboard surface); pointer users get the same model via click/drag.
* Spell-check and grammar suggestions: never modal, never color-only
  (underline + context-menu/palette action + screen-reader announcement of
  the suggestion count).
