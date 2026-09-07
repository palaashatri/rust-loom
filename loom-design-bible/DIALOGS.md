# Dialogs

Dialogs are rare by policy (`WINDOWS.md` §3). This document fixes when they
exist, how destructive actions behave, and the keyboard defaults.

## 1. When a dialog is justified

A modal dialog appears only when: the user must decide before the workflow
can continue, and the decision has consequences that cannot be inferred or
reversed cheaply. Accepted uses:

* Confirm destructive, irreversible actions (delete project, discard
  unsaved document, replace file).
* Resolve conflicts with no safe default (version incompatibility,
  media relink ambiguity).
* Enter values that require full attention and have no canvas analogue
  (print settings, export options are utility windows or panels where
  possible).

Not accepted: properties editing (inspector), formatting (toolbar), settings
(settings panel), confirmations for undoable actions (undo is the
confirmation), progress (status bar/notifications), error details
(notifications + diagnostics log).

## 2. Anatomy and behavior

* Anatomy per `COMPONENTS.md` §16: `radius-8`, `space-24` padding, min
  width 420 px, title, body, right-aligned action row.
* Backdrop dim: ink at 20% over the parent window; backdrop clicks do not
  dismiss (deliberate — a modal you must read, not swipe away).
* Only one dialog at a time; requests queue.
* Resize: dialogs may resize within 420–640 px width; body scrolls if the
  window is short; the action row never scrolls.
* Motion: fade + 4 px scale, out-quad 200 ms; exit 160 ms; reduced motion:
  instant in, fade-only out.

## 3. Destructive action confirmation

* Destructive button is **never the default**: the safe path (Cancel, or
  the constructive primary) is the default; Enter always activates the
  default (safe) button.
* Destructive confirmations state the consequence in the button itself
  ("Delete project") and repeat the consequence in the body ("This deletes
  8 media files and 120 hours of history. This cannot be undone.").
* Double-confirmation (type-to-confirm or second dialog) is used only for
  truly irreversible suite-level actions (delete project file, wipe
  recovery data); ordinary destructive edits rely on undo.
* Destructive buttons use `color-status-danger` fill with white text; the
  action row order is [Cancel] [Destructive], destructive rightmost when
  Cancel is default — and when the destructive is genuinely the only action
  (delete confirmation with "Cancel" present), Cancel stays left.
* Undoable destructive actions do not require a dialog at all (delete layer,
  delete clip: undoable, no modal) — dialog policy respects the undo system.

## 4. Keyboard defaults

| Key | Behavior |
|---|---|
| Enter | Activate default (safe) button |
| Esc | Cancel (always present unless no cancellation is possible, e.g. fatal error) |
| Tab | Move focus within dialog, wraps; focus traps while open |
| Shift+Tab | Reverse |
| Ctrl+Enter / Cmd+Enter | Same as Enter |
| Ctrl+W / Cmd+W | Cancel-and-close when the dialog can cancel |

* On open, focus goes to the first focusable control (usually the default
  button or the first field); never to the backdrop or title.
* On close, focus returns to the element that opened the dialog.
* Screen reader: dialog role announced with title; backdrop described as
  "inactive".

## 5. Recurring decision dialogs

* "Don't ask again" is allowed only when the decision is genuinely
  repeatable and reversible (checkbox in the dialog, saved to settings);
  the setting is discoverable in Preferences.
* A dialog that users dismiss via Esc more than twice in a session logs a
  diagnostics hint (privacy-preserving; no telemetry — the hint is local).

## 6. Error dialogs

* Fatal, unrecoverable errors (corrupt project at open, unrecoverable save
  failure) use a dialog because work must stop; recoverable errors use
  notifications (`NOTIFICATIONS.md`).
* Error dialogs: title states what failed in plain language, body explains
  what was lost and what the user can do (restore from recovery, relink
  media), actions offer the recovery path as default when it exists.
* Error details are available ("Show details" expands a monospace log
  block, redacted per `loom-core` diagnostics privacy rules), never a raw
  panic or stack dump by default.
