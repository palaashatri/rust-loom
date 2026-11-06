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
