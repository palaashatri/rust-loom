# Menus

Menus are the fourth layer of progressive disclosure. They are complete —
every command exists in a menu — but calm: restrained grouping, no redundant
nests, mnemonics only where useful.

## 1. Menu bar model

* Menu bar row (24 px) below the title bar: File, Edit, View, (App-specific
  — e.g. Insert in Writer/Sheets/Present, Layers in Photo, Clip in Video,
  Track in Studio), Selection, and the suite's Help.
* On platforms with a global menu bar (macOS), the same commands live in the
  platform menu; the in-app menu bar is hidden. Linux/Windows: in-app menu
  bar visible, hideable per user setting (default visible; palette and
  `Alt` still reach everything).
* The menu bar is the last-chance discoverability surface: every command
  that has a shortcut shows it right-aligned in the menu item
  (`color-ink-secondary`, tabular figures).

## 2. Menu item anatomy

* Item rows 28 px: icon 16 px (optional, aligned left), label
  `type-size-13`, shortcut hint right, checkmark for toggles (checked =
  accent check), submenu chevron right.
* Separators: hairline, only between groups that genuinely differ (undo/
  redo vs clipboard vs find is over-separated; group by action class).
* Disabled items: 40% opacity, never removed (consistency), with reason
  tooltip where non-obvious.
* Destructive items (Delete Clip, Discard): label in danger color only when
  the command is destructive and irreversible without undo — most
  destructive commands have undo and stay neutral; see `DIALOGS.md`.

## 3. Menu interactions

* Open: click or `Alt+Underline` mnemonic; opens within 120 ms (out-quad,
  4 px slide; reduced motion: fade).
* Keyboard: arrows move; Enter/Space activate; Esc closes one level;
  Left/Right navigate submenus; mnemonics activate directly.
* Hover over another top-level item while a menu is open switches to it
  immediately (no hover delay, no animation between top-level switches).
* Mouse-up on the trigger closes the menu without activating; clicking an
  item activates on mouse-up.
* Menus never cascade more than one submenu level (two levels total).
  Deeper structure is a redesign signal.

## 4. Context menus

* Right-click context menus show the commands applicable to the hovered
  object/selection, in the object's command order, plus: undo/redo (top,
  when relevant), Copy/Cut/Paste, and a "Properties…" entry opening the
  inspector section.
* Context menus never contain disabled-only commands; irrelevant commands
  are absent.
* Keyboard access to context menus: the context-menu key (or Shift+F10)
  opens at the focused object; keyboard-opened context menus have focus and
  are navigable.
* Context menus open instantly (no delay); dismiss on outside click, Esc,
  or click-away, 120 ms fade.

## 5. Mnemonics policy

* Mnemonics (underlined letters) are provided on the Linux/Windows menu bar;
  macOS uses platform conventions.
* Mnemonic letters are assigned from a policy: first letter, then unique
  consonant, then unique letter — deterministic, never auto-assigned by
  "find first unused".
* UI labels never encode mnemonic letters in the visible string (no
  parentheses); the mnemonic is a separate localization string.

## 6. Accessibility

* Menu bar and all menus are fully keyboard-operable; focus is visible on
  menu items (accent row fill 15% + primary ink).
* Every menu item announces its state (checked/disabled) to screen readers.
* Menu labels are localized; keyboard equivalents follow the shortcut
  configuration policy (`KEYBOARD.md` §5) — configurable, never hard-coded
  in menus.
* No menu item is reachable only by mouse; no command exists only in a
  context menu (palette covers all commands; see `COMMAND_PALETTE.md`).

## 7. Per-app menu contract

File: New, Open, Open Recent, Save, Save As, Export, (Import), Close, Quit.
Edit: Undo, Redo, Cut, Copy, Paste, Paste Special, Delete, Select All,
Find, Preferences. View: toggle toolbar/sidebar/inspector/status bar,
zoom controls, full screen, theme. App menu: the application's insert/
create surface (Insert, Layers, Clip, Track, Slide, Song). Selection:
Select All, Deselect, Invert, (Expand/Contract). Help: Documentation,
Shortcuts, Diagnostics Log, About.

Every app's menu inventory is enumerated in its PRODUCT_SPEC with its
`FEATURE_MATRIX`; the Bible fixes the pattern above.
