# Windows

Window types, anatomy, and multi-window policy for Loom applications.

## 1. Window types

| Type | Purpose | Modality | Count |
|---|---|---|---|
| Main window | The application surface | — | 1 per app instance |
| Dialog | Short decisions (save, confirm, preferences section) | Modal, rare | 0–1 at a time |
| Popover | Non-modal surface anchored to a control (menus, palettes, inspectors pop, color wells) | Non-modal, dismisses on outside click | 0–n, one per anchor |
| Utility window | Long-lived secondary surfaces: render queue, media browser, scopes, mixer | Non-modal | 0–3 |
| Floating panel | Detached inspector/canvas area | Non-modal | `[future]` |

## 2. Main window

* One main window per app; document tabs within it (Writer/Sheets/Present may
  open multiple documents in tabs; canvas apps open one document, project
  sessions in a library window).
* Anatomy per `LAYOUT.md`: title bar 40 px, context toolbar 40 px, sidebar
  240 px collapsible, inspector 280 px, status bar 28 px, canvas fills.
* The main window is resizable with minimums per `LAYOUT.md` §2; below the
  minimum, chrome collapses (sidebar auto-collapses) before content.
* Window state (size, position, sidebar/inspector state, theme) persists per
  app in user settings; document state persists per document (recovery per
  `loom-core` autosave contract).

## 3. Dialog windows

Policy: **dialogs are rare**. A modal appears only when the user must make a
decision before continuing and the decision has consequences (destructive
confirmation, file-overwrite confirmation, incompatible-version save). Do not
use dialogs for: properties (inspector), formatting (context toolbar),
navigation (sidebar/palette), or settings pages (utility window or panel).

Rules (`COMPONENTS.md` §16 for anatomy):

* Focus moves into the dialog on open (first focusable control); Tab order
  loops within the dialog; Esc = cancel; Enter = primary/default action.
* Default action button is always the safe one (primary "Save", or "Cancel"
  where the primary action is destructive); destructive buttons require
  explicit confirmation semantics (`DIALOGS.md`).
* A dialog opens over a dimmed, non-interactive backdrop (dim = ink at 20%);
  backdrop click does not dismiss (deliberate decision over dismissal).
* Only one dialog per app; a second dialog request is queued behind it.
* Dialog positions center on the parent window; sizes 420 px min width,
  640 px max width, height ≤ 80% of window; scroll inside body if needed.
* Motion: fade + 4 px scale-in, out-quad 200 ms; exit 160 ms; reduced motion:
  instant appearance, fade-only exit.

## 4. Popovers

* Anchored to their trigger control, offset 4 px; 320 px default width
  (menus/color wells may be smaller/larger per component spec).
* Non-modal: clicking outside dismisses; Esc dismisses; the trigger toggles
  state (arrow indicator shows open).
* Focus behavior: menus and combobox popovers move focus in (keyboard-first
  lists); color wells and preview popovers keep focus on the trigger.
* Popovers never open on hover alone (only after a deliberate click/keyboard
  action) — hover-reveal is prohibited (`ANTI_PATTERNS.md`).
* Motion: entrance out-quad 120 ms with 4 px slide from anchor direction;
  exit 120 ms; reduced motion: fade 120 ms only.
* Popovers are the only surfaces allowed `shadow-popover`
  (`DESIGN_TOKENS.md` §10).

## 5. Multi-window policy

* All windows are in the same process; a single app session owns them.
* Secondary windows (utility windows) are non-modal and move with the main
  window's state; they never steal focus on their own (no pop-to-front on
  background completion; status bar + notification covers that).
* Opening a document type handled by another Loom app (`.loomdoc` from
  Photo) opens that app — or, if the app is not installed, shows an install
  hint dialog; no in-app emulation of another app's surface.
* Cross-app drag and drop of assets/files uses the shared clipboard/drag
  contract (`DRAG_AND_DROP.md`).
* Presenter/secondary-display windows (Present, Video): follow
  `PRESENTER.md`-level rules — the presenter display is a dedicated full-
  screen window with no chrome, timing, and a rehearsal clock; it is
  considered a utility window variant.

## 6. Window chrome

* Title bar: app name on the right (or platform convention), document title
  centered-left, window controls per platform; on Linux, standard WM
  decorations are acceptable, but the in-app title bar must still exist for
  consistent theming and command access.
* Window icons: original Loom app icons per application (drawn to the icon
  family rules in `ICONOGRAPHY.md`), provided in PNG/ICO/SVG source
  (assets in `loom-core`, spec here).
