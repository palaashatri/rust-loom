# Notifications

Notifications report outcomes without stealing focus. Two surfaces share the
job: **toasts** for transient, attention-worthy outcomes; the **status bar**
for ongoing and background work. Both obey the non-color status rules in
`COLOR.md` §6.

## 1. Surface selection

| Condition | Surface |
|---|---|
| Task completed while user is idle in another area (export done, render finished) | Toast |
| Task failed; retry or recovery available | Toast (error style) + entry in Diagnostics |
| Ongoing background work with progress (import, proxy, encode queue) | Status bar progress + cancel |
| Transient status (autosave, relink, zoom reset) | Status bar message, auto-clears |
| Fatal, work-stopping error | Dialog (`DIALOGS.md` §6) |

Rules: no more than one toast visible per app at a time (queued); status bar
shows one primary progress job (others collapse to a "3 jobs" entry opening
the jobs panel); toasts never stack with dialogs.

## 2. Toast anatomy and behavior

* Top-right, width 320 px, `radius-8`, raised fill, `shadow-popover`;
  icon (16 px) + message (`type-size-13`, primary ink) + optional action
  button + close button.
* Severity: info (default), success, warning, error — icon + text per
  `COLOR.md` §6; never color-only.
* Auto-dismiss: info 6 s, success 4 s, warning 10 s, error persists until
  dismissed (errors also live in the Diagnostics log with full details).
* Action buttons in toasts: single action ("Open", "Retry", "View log");
  clicking dismisses the toast after running.
* Motion: entrance out-quad 200 ms (fade + 4 px slide from the edge);
  exit 160 ms; auto-dismiss fades 160 ms; reduced motion: fades only.
* Accessibility: toasts announce via live region (polite); focus does not
  move on toast arrival; toast action buttons are keyboard-reachable while
  visible; Esc dismisses the focused toast.

## 3. Status bar reporting

* Left region: primary status/progress line. Text `type-size-11`, secondary
  ink; progress uses the mini ProgressBar (80 × 6 px) + fraction in tabular
  figures + cancel button (always, for cancellable jobs).
* Right region: readouts (zoom, snapping, transport). Never more than 3
  readout groups.
* Status messages auto-clear after 4 s or when superseded; progress rows
  persist until the job ends.
* The status bar never shows errors in red alone; error text uses danger
  ink + alert icon and offers "Details" opening the Diagnostics log.

## 4. Error reporting

* Recoverable errors (import failure, file locked, model missing) go to a
  warning/error toast with a retry/recovery action; the full message lives
  in Diagnostics.
* Error messages follow a fixed shape: what failed, what was affected, what
  to do next. Example: "Import failed: 'clip.mov' is corrupt. Nothing was
  imported. Try re-exporting the file." Never: "Error 0x4F2 — internal
  error."
* Unexpected internal errors are logged to Diagnostics with a stable error
  ID and shown as a compact toast ("Unexpected error — see Diagnostics").
* No network-dependent behavior: error reporting is local and offline
  (`loom-spec` privacy contract).

## 5. Recovery prompts

* After a crash or interrupted session, the app opens the Recovery browser
  (per `loom-core` recovery contract). Presentation: a modal-style entry
  surface (recovery list) — designed as a dialog by default, listing
  recovered documents with timestamps and "Keep / Discard / Review"
  actions.
* Recovery prompts are explicit, never silent: the user chooses what to
  keep; nothing is deleted automatically at session start.
* If no recovery data exists, no prompt appears (no dead-end "welcome
  back" screens).

## 6. Autosave and background-job notifications

* Autosave is silent by default: the status bar shows a brief "Saved
  HH:MM" (4 s) after the first save of a session; failures are errors
  (toast + persist).
* Background jobs (proxy generation, thumbnails, indexing) never toast on
  success; they report progress in the status bar and toast only on
  failure with a retry action.
* Cancellation is immediate and announced in the status bar ("Import
  cancelled") — never a toast for user-initiated cancellations.
