# Linux native journey evidence — 2026-11-01

Scope: Loom Sheets on Linux, driven with real X11 pointer and keyboard input. This is
evidence for the gate table in `docs/workflow/current.md`. It is agent-run, not human
sign-off, and it does not change any application status by itself.

## Environment

- Linux container, Ubuntu 24.04 userland, Xvfb `:99` at 1920×1200, 24-bit. No window manager.
- `xdotool` for input. Keystrokes are delivered only after `xdotool windowfocus --sync <id>`,
  because there is no window manager to move focus. Missing focus is the cause of an earlier
  set of lost keystrokes, not a product defect.
- Save and Open use the real GTK file chooser through `xdg-desktop-portal` and
  `xdg-desktop-portal-gtk` in a private `dbus-run-session`. Without a portal, `rfd` returns no
  file and the app reports "Save cancelled" (see gaps).
- Binary: `loom-sheets/target/debug/loom-sheets` built from `b95060ab`.
- LibreOffice 24.2.7.2 (Calc) for the interop readback, run headless with its own profile.

## Sheets journeys

| # | Journey (native input) | Observed result | Evidence |
|---|---|---|---|
| 1 | Click B3/B4, type `Revenue`, `250`, Enter | Values committed; Enter moves the selection down; status "1 cells" then "2 cells" | `01-typed-cell-and-enter.png`, `08-…` |
| 2 | Click B5, type `=B4*2`, Enter | `500` shown; status "1 formulas" | `02-formula-evaluates.png` |
| 3 | Ctrl+Z | Formula removed, B5 empty | `03-undo-reverts-formula.png` |
| 4 | Ctrl+S to a new path via the GTK dialog | File written, 756 bytes; title loses the dirty marker | `04-save-dialog-portal.png` |
| 5 | Ctrl+O, choose the saved file | "Opened … (2 sheets)"; `Revenue` and `250` intact | `05-reopened-workbook.png` |
| 6 | Kill the process with a timeout, relaunch with the file | "Restored unsaved changes to sheet-journey.loomtable"; contents restored from recovery | `06-recovery-after-kill-and-export-menu.png` |
| 7 | Keyboard only: Tab, Enter, Left, Escape, F2, Enter | Escape cancelled a typed `9`; F2 then committed `99` in B7; 6 cells counted | `07-keyboard-only-entry.png` |
| 8 | Enter `=B7*2` (C7) and `=SUM(B4,B7)` (C8); Export Excel | C7 = 198, C8 = 349 in the app; XLSX keeps `<f>` and cached `<v>` | `08-formulas-and-cached-values.png` |
| 9 | LibreOffice 24.2 reads the Loom XLSX (CSV export of sheet 2) | `Revenue`, `250`, `Units`, `Price`, `widgets`, `99`, `198`, `349` read back | command below |

LibreOffice readback command (sheet 2 must be selected explicitly; the default exports only sheet 1):

    soffice -env:UserInstallation=file:///tmp/lo-profile --headless \
      --convert-to 'csv:Text - txt - csv (StarCalc):44,34,76,1,,0,false,true,false,false,false,2' \
      --outdir OUT sheet-formulas.xlsx

## Gaps and findings from this pass

- **Save without a file chooser reports "Save cancelled".** With no portal, `rfd` returns no
  path and the app cannot tell that from a user cancel. This is a truthfulness problem for
  gate 7 on Linux installs without a desktop portal. Open.
- **Inspector opens by default** on a new workbook, showing the cell format panel at about a
  quarter of the 1280-pixel window. Recorded for design review (UI-02 is marked FIXED in the
  repair index). Not changed here.
- **New untitled window name is "Untitled Window"** under X11 (`xdotool getwindowname`).
  Not investigated further.
- Excel is not available, so the XLSX interop claim is checked against LibreOffice only.
- No native GPU frame or input latency was measured. The CI software-renderer budget is not a
  substitute (PERF-01 stays `NEEDS_REVIEW`).
- Writer and Present were not driven in this pass.
