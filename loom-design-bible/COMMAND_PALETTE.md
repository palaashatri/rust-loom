# Command Palette

The command palette is the fastest path to every command, in every app, with
the keyboard only. It is the fourth layer's power surface.

## 1. Invocation and anatomy

* Open: `Cmd+Shift+P` (Windows: `Ctrl+Shift+P`); also the title bar's
  `commands` icon button and the Help menu. Focus goes into the palette on
  open; typing starts searching immediately (no "click to search" step).
* Popover surface centered top-third of the window, width 480 px, max height
  480 px, `shadow-popover`, `radius-8`; entrance: out-back 320 ms (the one
  sanctioned overshoot), reduced motion: fade 120 ms.
* Anatomy: search field (magnifier icon, placeholder "Search commands,
  files, help…"), result list, footer row with hints ("↑↓ navigate · ↵
  run · ⌫ history").

## 2. Scope

The palette searches, in order:

1. **Commands** — every command in the app (the command registry:
   `loom-core` command system), filtered by current context: commands that
   apply now rank above commands that cannot run; disabled commands show at
   40% with a reason and remain selectable-and-blocked (never silently
   removed).
2. **Recently opened documents** — the app's recent files, when the query
   matches their name.
3. **Help topics** — bundled help entries, when the query matches (help
   search per `loom-core` search contract).
4. **Settings** — named settings reachable via the palette ("theme",
   "shortcuts", "autosave").

A single result list, grouped by these scopes with tiny group labels
(`type-size-11` caps, `space-8` spacing).

## 3. Fuzzy search

* Fuzzy substring matching with per-character skip, case-insensitive;
  matches are highlighted in the result label with accent.
* Ranking: exact-prefix > word-prefix > contiguous substring > scattered
  matches; tie-break by most-recently-used.
* Minimum query: empty query shows **most-recent-first command history** +
  recently opened documents (see §4); one character filters.
* All results render in under one frame of typing (search is over an
  in-memory index; no disk I/O in the palette path).

## 4. Ordering

* History ordering: commands the user has run most recently rank above
  alphabetically-equal peers; a persistent MRU list (per app, bounded at
  20 entries, reset-able in settings).
* When the query matches nothing in a scope, that scope hides; when nothing
  matches at all, show the empty state: "No command found — press Esc to
  dismiss" plus a "Search in help" action.

## 5. Keyboard model

| Key | Action |
|---|---|
| Type | Filter |
| ↑ / ↓ | Move selection (wraps) |
| Enter | Run selected command |
| Cmd+Enter | Run without closing (sticky) |
| Esc | Close (cancels) |
| Backspace at empty | Return to history view |
| Tab | Cycle scope groups (commands → files → help → settings) |

* The palette is keyboard-first but mouse-usable (click item runs it; click
  outside closes).
* Running a command closes the palette (except `Cmd+Enter`); the command's
  undo description feeds the undo menu as usual.
* Screen reader: opening announces "Command palette"; result selection is
  announced; Enter confirms.

## 6. Behavior rules

* The palette is non-modal in the popover sense (it does not lock the
  window), but while open, focus stays inside it; Esc always returns focus
  to where it was.
* Palette state (last query) is not persisted across sessions; MRU is.
* Commands that require context run with the existing selection — the
  palette never silently changes the selection.
* Every command in every app must be in the palette; the acceptance gate is:
  for each command in the app's command registry, assert its palette entry
  exists with the documented label (`UX_ACCEPTANCE_CHECKLIST.md`).
