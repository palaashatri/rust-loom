# Window frame choice and menu fixes — 2026-11-06

Scope: Loom Sheets, Writer and Present on Linux. Adds a View-menu choice between the
platform's own title bar (default) and Loom's own title bar, and fixes two View-menu
defects found while testing it. This is agent-run evidence on Linux only; it is not
macOS or Windows evidence and it changes no application's acceptance status.

## What changed

- `loom-core/crates/loom-desktop/src/window_frame.rs`: the two choices (`System`, `Loom`),
  their command ids and labels, the View-menu rows, the check projection, a per-app store,
  and the startup rule. On platforms that cannot draw Loom's title bar the answer is always
  `System`, and the `Loom` row is disabled. The `LOOM_CUSTOM_CHROME=1` environment override
  still forces Loom's title bar for one launch and is never written back.
- `loom-core/crates/loom-desktop/src/settings_file.rs`: the `settings.toml` reader and
  writer, shared by the appearance and the window-frame choices. A write keeps the other
  keys already in the file.
- `loom-core/crates/loom-desktop/src/ui_bindings.rs`: `window_chrome_bindings!` now takes the
  status-line setter and generates `start`, `install`, `apply`, `choose`, `dispatch`,
  `current` and `on_change`. `apply` sets Slint's `custom-chrome`, which Slint turns into
  `no-frame`; Slint re-applies decorations on each change, so the switch is live.
- Sheets, Writer and Present: the interactive startup calls `start`; headless renders and
  journeys keep the environment-only `install`. Each app adds the rows to its View menu,
  lists the command ids as supported, adds a check projection, and refreshes its menu on a
  frame change.
- Appearance store moved onto the shared settings module without behaviour change
  (`loom-desktop` tests: 487 passed across the loom-core workspace).

## Defects found and fixed during this pass

1. **Present: the Appearance menu rows failed.** Choosing `Appearance: Dark` showed
   `Menu action failed: invalid desktop request: unsupported Present menu command
   view.appearance.dark` and did not change the theme (before-fix screenshot:
   `11-present-appearance-dark-after-fix.png` is the after-fix capture; the failure is
   visible in `10-present-appearance-dark-before-fix.png`). Present's menu whitelist did not
   list the appearance command ids. Fixed by adding them.
2. **Writer: a new row was listed but reported "currently disabled".** Writer dispatches through a
   command registry, and a command missing from it is treated as disabled. Fixed by
   registering the two window-frame specs; the Loom spec is disabled where unsupported.
3. **Present's menu check mark did not follow a frame change** until the menu was rebuilt.
   Fixed with a frame-change listener that each app uses to refresh its menu.

## Verification (Linux, real X11 input, Xvfb + openbox window manager)

Isolated settings directory per run (`LOOM_CONFIG_DIR`), so no real preference was touched.

| Check | Sheets | Writer | Present |
|---|---|---|---|
| Default launch uses the platform frame (openbox title bar) | yes (`01-…`) | yes (`13-…`) | yes (`12-…`) |
| View menu lists `System title bar` (checked by default) and `Loom title bar` | yes (`02-…`) | yes | yes (`09-…`) |
| Choosing `Loom title bar` removes the platform frame, shows Loom's title bar and status `Window frame: Loom title bar.` | yes (`03-…`) | yes (`08-…`) | yes |
| Saved setting is `window_frame = "loom"` | yes | yes | yes |
| Restart keeps the Loom title bar | yes (`04-…`) | not run | not run |
| Choosing `System title bar` restores the platform frame and saves `system` | yes (`05-…`) | not run | yes (`12-…`) |
| `LOOM_CUSTOM_CHROME=1` forces Loom's bar for one launch, saved value unchanged | yes (`06-…`) | not run | not run |
| Loom's close button on a dirty document opens the unsaved-changes prompt (Cancel keeps the app open) | yes (`07-…`) | not run | not run |
| Loom's maximize button maximizes the window | yes (observed) | not run | not run |
| Appearance Dark applies and status reads `Appearance: Dark` | n/a | n/a | yes (`11-…`, after fix) |
| Both keys survive a save (`window_frame` and `appearance` in one file) | n/a | n/a | yes (`settings.toml` shows both) |

## Gates run (exact commands; results on the final tree)

- `cargo fmt --all --check` in loom-core, loom-sheets, loom-writer, loom-present: clean.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: loom-core, loom-sheets,
  loom-writer, loom-present all pass.
- `cargo test --workspace --locked --no-fail-fast`:
  - loom-core: 487 passed, 0 failed, 1 ignored (includes the 9 new window-frame and
    settings-file tests in `loom-desktop`).
  - loom-sheets: 1090 passed, 0 failed, 2 ignored.
  - loom-writer: 1304 passed, 0 failed, 5 ignored.
  - loom-present: 447 passed, 0 failed, 2 ignored (final run, after removing an unused helper).

## Gaps and not verified

- **macOS**: not tested. The `Loom title bar` row is disabled there by design, and the
  macOS code paths were compiled only through the shared cfg gates, not run.
- **Windows**: not tested (no Windows host).
- **Palette**: the two choices are in the View menu only. The command palette does not list
  them yet.
- **Writer and Present** were not checked through restart, the environment override or the
  close prompt; Sheets was.
- **Drag and resize of Loom's title bar** and its resize edges were not exercised on screen.
- The screenshots show the window at 1280x800 under openbox. No DPI scaling, high-contrast
  or RTL checks were run for the title bar.

---

# Writer tables, Markdown paste and hover — 2026-11-06 (later pass)

Scope: Loom Sheets, Writer and Present on Linux (Xvfb with openbox, real X11 pointer and keyboard input through xdotool). The owner waived human visual approval for this pass. Every screenshot below was reviewed by the agent, not by a person. No application acceptance status changed: all three remain `ACCEPTANCE_BLOCKED` in [workflow.toml](../../../loom-bootstrap/contracts/workflow.toml) and [current.md](../../workflow/current.md).

## Fixed in this pass

1. **Wrapped text overlapped inside Writer table cells.** Glyph tiles were positioned relative to each line's own frame, so every wrapped line was drawn at about y=0. Tiles are now positioned relative to the cell frame (`loom-writer-app/src/render_text.rs`). Evidence: [writer-table-cell-wrap-after.png](writer-table-cell-wrap-after.png) shows a long sentence stacked on four lines.
2. **Pasted line breaks in a table cell** become spaces, as typed line breaks do. Pasted Markdown outside a cell becomes headings, bullet and numbered lists, bold and italic runs and real tables, not raw markup (`loom-writer-app/src/markdown_paste.rs`).
3. **The caret after a Markdown paste that ends in a table landed in the table's last cell.** Typing `ZQ` produced `2ZQ` in the cell that held `2`. A paste that ends in a table now gets a paragraph after it, and the caret is placed from the same text that is inserted. Evidence: [before](writer-markdown-paste-table-last-before.png) and [after](writer-markdown-paste-table-last-after.png), where `ZQ` lands in a paragraph under the table and cell `2` is unchanged. A paste with a table in the middle was also checked: the caret lands after the last paragraph ("Tail line"). Unit test: `a_paste_ending_in_a_table_leaves_the_caret_below_it`.
4. **Hover feedback.** Icon buttons used `surface-raised` on hover, which is `#ffffff` in the light theme, the same as the surface they sit on, so the hover could not be seen. They now use `surface-sunken` (`#ededf0` light, `#18181a` dark), which shows in both themes. This covers the Sheets decimal and font-size steppers. The Writer font-family field had no hover at all and now shows the hover border and background. Non-primary buttons use `selection-soft`, and toggles, colour swatches and Sheets fill swatches show a hover border or track (earlier commit `9f40cf08`).

## Hover audit

Method: each interactive element in the AT-SPI tree of the running app had the pointer moved onto its centre. The pixel change was measured inside its box with ImageMagick `compare -metric AE`, and the pointer was then moved away to check for sticky hover. Script: `hover_audit.py`, run inside a private accessibility session.

| App | Interactive elements | No visible hover change | Sticky after leaving | Unnamed |
|---|---:|---:|---:|---:|
| Writer | 47 | 8 | 0 | 0 |
| Sheets | 66 | 13 | 0 | 0 |
| Present | 21 | 4 | 0 | 0 |

The remaining items were checked one by one:

- **Selected state already shows:** Writer's Format tab, Format inspector toggle, `Body`, `1.15` and `None` are visibly selected on screen. Sheets' fill swatches `Automatic text colour` and `No fill`, its Format tab, and Present's `Slide 1 of 1` thumbnail are inferred to be selected, not checked on screen. In each group the other items of the same control do respond to hover.
- **Disabled:** Writer's `Text Color` is enabled only while a text selection exists (`color-formatting-available`). Present's `Move Slide Up` and `Move Slide Down` are disabled with one slide.
- **Text entries (documented exception):** Writer's `Document body` and `Font size`; Sheets' `Name box`, `Formula bar`, `Search`, `Font size in pixels` and `Decimals count`.
- **Not confirmed as hover targets:** Sheets' `Selected cell fill handle` (5 px) and `Table select handle` (8 px), `Fill down`, and Present's `Title, empty` canvas placeholder. Their intended hover design was not checked.
- **Clipped, not measured:** Sheets' decimal stepper buttons sit at the bottom edge of the 1280×800 window, and the audit's pointer position falls under the status bar. They use the same `LoomIconButton` as the font-size steppers, which change on hover (AE 764). See the layout finding below.

## Gates on the final tree

- Writer: `cargo fmt --all --check` clean; `cargo clippy --workspace --all-targets --locked -- -D warnings` clean; `cargo test --workspace --locked --no-fail-fast` 1315 passed, 0 failed, 5 ignored.
- Sheets: `cargo clippy --workspace --all-targets --locked -- -D warnings` clean; tests 1090 passed, 0 failed, 2 ignored. Format check clean on this tree.
- Present: `cargo clippy --workspace --all-targets --locked -- -D warnings` clean; tests 447 passed, 0 failed, 2 ignored. Format check clean on this tree.
- `audit-governance.py`, `audit-ui-foundation.py`, `audit-code-structure.py`, `audit-assets.py` and `documentation.py`: PASS.
- Build settings: the sandbox disk allowance was exhausted by full debuginfo builds, so these runs used `CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0`. These settings change only build output, not test code.

## Findings left open

- **Inspector clipping at 1280×800.** The Sheets Decimals row sits at the bottom edge, so its stepper buttons are only partly visible until the inspector is scrolled. Writer's table-style chips also clip at the right edge (`1st Col Off`). Neither is fixed.
- **`surface-raised` hover elsewhere.** About 20 shared components use `surface-raised` as their hover background (for example `toolkit.slint`, `components.slint`, `overlays.slint`, `chrome.slint`, `data.slint`, `canvas.slint`, `toolbar_item.slint`, and the Writer `outline_pane.slint` and `template_chooser.slint`). They show only on a non-white surface. Only the icon button and the screens above were audited.
- **Caret after paste.** Checked for two Markdown shapes only: a table in the middle and a table at the end.

## Still not done (every gate stays open)

- Orca walkthroughs for Writer and Present. Sheets has a 2026-11-01 record; Sheets' Tab-into-grid announcement burst is still to review.
- Software-renderer frame benchmark (release build) for each app.
- Visual matrix (4 viewports × 3 themes × 3 text scales × LTR/RTL). Only the Writer tables set was captured earlier.
- Keyboard-only, undo/redo and persistence journeys, and DOCX/PDF, PPTX/PDF and XLSX export readbacks: not re-run in this pass.
- Independent review, and macOS and Windows testing (not available in this sandbox).
- The PR #21 branch `owner/office-completion-2026-10-31` was not updated by this pass. Commits went to `ccr-a3bd7f3a-803fm5` (`9f40cf08`, `1bfb0b84`).
