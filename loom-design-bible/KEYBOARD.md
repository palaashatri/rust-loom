# Keyboard

Keyboard operation is a release requirement. This document fixes the shared
key map, per-app additions, and the shortcut configuration policy.

## 1. Convention

* Primary modifier is `Cmd` on macOS, `Ctrl` on Linux/Windows (written
  `Cmd`/`Ctrl` below as `Mod`).
* `Mod+Shift+P` opens the command palette everywhere; `F1` opens help;
  `F10`/`Alt` reveals menu mnemonics on Linux/Windows.
* Shortcut hints render in menus, tooltips, and the palette footer
  (`MENUS.md`, `COMMAND_PALETTE.md`).
* All shortcuts are **configurable** (§5); defaults below are the contract.

## 2. Standard editing keys (all apps)

| Action | Key |
|---|---|
| Undo / Redo | `Mod+Z` / `Mod+Shift+Z` (Linux also Ctrl+Y) |
| Cut / Copy / Paste | `Mod+X` / `Mod+C` / `Mod+V` |
| Paste without formatting | `Mod+Shift+V` |
| Delete selection | `Backspace` / `Delete` |
| Select all | `Mod+A` |
| Find | `Mod+F` (surface-specific: document find, palette find, inspector search) |
| Preferences | `Mod+,` |
| Command palette | `Mod+Shift+P` |
| Zoom in/out/fit/100% | `Mod+Plus` / `Mod+Minus` / `Mod+0` / `Mod+1` |
| Toggle sidebar / inspector | `Mod+\` / `Mod+Option+I` (Win: `Ctrl+Alt+I`) |
| Full screen | `F11` (Linux/Win), `Ctrl+Cmd+F` (macOS) |
| Focus search | `Mod+Shift+F` |
| Close window/doc | `Mod+W` |
| Save | `Mod+S` (Save As: `Mod+Shift+S`) |
| New / Open | `Mod+N` / `Mod+O` |
| Show shortcuts help | `Mod+?` (also Help menu) |

## 3. Per-app keys

**Writer**: arrows/Home/End/PageUp/PageDown per `DOCUMENT_EDITOR.md` §2;
`Mod+B/I/U` bold/italic/underline; `Mod+K` insert link; `Mod+Shift+S`
save-as; `Mod+E` center-aligned text? — no: align via `Mod+Shift+L/C/R`
(par. align left/center/right); headings: `Mod+Alt+1..6`; lists
`Mod+Shift+7`/`Mod+Shift+8` (bulleted/numbered); footnote `Mod+Alt+F`;
comment `Mod+Option+M` (macOS `Mod+Alt+M`); track changes toggle
`Mod+Shift+E`; find next/prev `Enter`/`Shift+Enter` in find bar.

**Sheets** (`SPREADSHEET.md` §3 table is the core): cell editing `F2`,
go-to `Mod+G`; insert row/col `Mod+Shift+K` / `Mod+Shift+K` (col variant
`Mod+Option+K`); delete row/col `Mod+Delete`; formula entry `=`;
autofill: select + fill-handle drag (pointer); keyboard fill `[goal]`;
filter toggle `Mod+Shift+L`; freeze toggle `Mod+Alt+F`; sheet cycle
`Ctrl+PageUp/PageDown`; recalc manual `F9` (auto by default).

**Present**: next/prev slide `→`/`←` (or Space/Shift+Space in edit mode:
`→` steps slides, `Shift+→` selects next object); present mode
`Mod+Shift+Enter`; add slide `Mod+Enter` (or `Mod+M`); duplicate slide
`Mod+Shift+D`; notes panel `Mod+Option+N`; rehearse `Mod+Shift+R`.

**Photo**: tool shortcuts (single letters, non-modifier): V select, M
marquee/lasso, C crop, B brush, E eraser, H hand, R rotate, G gradient,
I eyedropper, T text, Z zoom (with Mod = zoom out), A magic-wand,
L levels, W white-balance; bracket keys `[`/`]` brush-size; `X` swap
foreground/background colors; `Mod+Shift+N` new layer; `Mod+J` duplicate
layer; `Mod+G` group; `Mod+E` merge layers; `Mod+Shift+E` flatten; `/`
toggle mask overlay; `Mod+Shift+M` show/hide mask.

**Motion/Video**: Space play/pause; J/K/L scrub (`TIMELINE.md` §4);
`Mod+Right/Left` next/prev edit point (Shift adds 1-frame step); `Mod+B`
blade at playhead; `Mod+Shift+B` ripple delete at playhead; `Mod+D`
duplicate clip; `Mod+[`/`]` trim to playhead; `Mod+Shift+E` open export
queue; `Mod+1..9` jump to marker 1–9 (Option+1..9 set marker); `N`
toggle snapping; `Mod+Option+B` set loop range; `G` toggle graph editor
(Motion); `P` pick parent (Motion); `K` pause.

**Studio**: Space play/pause; `Mod+T` metronome; `Mod+R` record-arm
toggled; `Mod+K` create automation point; `Mod+Shift+A` show automation;
`Mod+1..8` mute track 1–8 (add Option for solo); `[`/`]` zoom time;
`Mod+Plus/Minus` zoom; `F` follow playhead; `Shift+Space` half-speed;
`Option+Arrow` nudge note 1 semitone/1 grid step (in piano roll).

**Encode**: Space start/pause selected job; `Mod+Return` start job;
`Mod+Shift+Return` start all; `Esc` stop selected job (confirm only if
mid-write); `Tab` moves focus between queue columns; `Mod+O` add sources;
`Mod+P` open presets panel.

**Common canvas keys**: `V` selection tool in canvas apps; `H` hand;
`Space` temporary hand (hold); `Z` zoom tool (Mod inverts); `Mod+Alt` + drag
duplicate (Photo/Motion); arrows nudge (Shift ×10).

## 4. Focus and scope

* Shortcuts apply to the focused surface: a text field consumes typing keys;
  JKL transport only applies when the timeline or canvas has focus (typing
  in a text field never scrubs); the rule is: surface-local keys are active
  when their surface has focus, global keys (`Mod+S`, palette) always.
* The focused surface is visually indicated (focus ring); the status bar
  shows the active surface ("Timeline").
* Tab navigation order is per `ACCESSIBILITY.md` §focus-order: chrome →
  toolbar → canvas/editor surface → panels in the order they were opened;
  Tab groups per surface (toolbar as one group, sidebar as one group).
* No key is reserved twice for different actions in the same surface
  (conflict detection is a config-time validation, §5).

## 5. Shortcut configuration policy

* All shortcuts are user-configurable via a shortcuts panel (searchable
  list, per-app and global sections, import/export as JSON — the exported
  file is user-owned data).
* Configuration applies on the fly; conflicts are reported with the
  conflicting action and the user resolves or cancels; configurable
  shortcuts never disable accessibility-critical keys (Tab, Space in text
  fields, Enter, Esc, arrows) — these are not overridable.
* Defaults are reset-able per app and globally; the config file lives in
  user settings with the same redaction/privacy rules as all Loom data.
* Layout variants: a keyboard layout (e.g. Dvorak) maps by key position;
  AZERTY users get mnemonic-safe defaults (`[goal]` — first release ships
  US/QWERTY defaults + configuration, layout-aware remapping is a goal).
* Screen-reader note: shortcut hints are read to screen-reader users; the
  shortcuts panel is fully keyboard-operable and announced.
