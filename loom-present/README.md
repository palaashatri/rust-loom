# Loom Present

Loom Present is a local-first presentation editor. Decks are open, inspectable `.loomdeck` packages on your own computer; nothing is uploaded and no account is needed. It exports PDF and PowerPoint `.pptx`.

**Platforms:** Windows x86-64 and Linux x86-64. macOS is out of scope for v1 and is not claimed.

**Status: `ACCEPTANCE_BLOCKED`.** The supervisor has not recorded `ACCEPTED`. This is not a production-ready claim. The live ledger is the current-truth section of the repository `AGENTS.md`; this README lists what has evidence behind it and what does not.

Evidence tags used below:

- **[tests]** covered by an automated test that passes (`cargo test --workspace`; the ledger records 103 app tests and 62 core tests on 2026-10-07; not re-run while writing this README).
- **[live-W]** exercised by hand in a real Windows 11 window.
- **[live-L]** exercised by hand in a real Linux window (WSLg, Ubuntu).

## What works

### Start, files and recovery
- First launch is a blank one-slide deck with a faint "Click to add title" prompt. The prompt is never saved, exported or drawn in a thumbnail. "New from Sample Deck" is in the File menu and the command palette. [tests] [live-W] [live-L]
- Native Open, Save and Save As for `.loomdeck`; the window title follows the saved file name and the status bar shows Saved or Edited. A transition-only change also marks the deck Edited. [tests] [live-W]
- Closing, New or Open with unsaved changes asks Save / Discard / Cancel. [tests] [live-W]
- Crash recovery: an unsaved deck is offered back at the next launch and reads as unsaved (closing asks); an intentional close, including Discard, clears it. Drafts are marked stale by a 600 ms timer and serialised on a worker thread, so up to 600 ms of the latest changes can be lost in a crash. [tests] [live-W]
- Command line: `loom-present path/to/deck.loomdeck`. [tests]

### Slides and objects
- Reorder slides: Slide > Move Slide Up/Down, Ctrl+Alt+Up/Down, the strip's up/down buttons, drag a thumbnail (a drop line shows the target, Escape cancels, the strip auto-scrolls at its edges) or Ctrl+Up/Down on a focused thumbnail; each move is one undo step, is announced, and save, export, recovery and undo keep the new order. [tests]
- Add slide (toolbar, Slide menu, Ctrl+Shift+N, palette), duplicate and delete. The left strip shows real scaled thumbnails (text, shapes, pictures) that update as you edit; one thumbnail is rewritten per edit. [tests] [live-W] [live-L]
- Slide layouts in the Format inspector: Title, Content, 2 Col, Image. Each supplies empty placeholders, never filler text. [tests]
- Insert Text, Shape (a rectangle) and Image from the toolbar. Insert Image is also in the Slide menu and the palette and uses a native picker for PNG and JPEG; the picture is centred and at most 60 % of the slide. Image bytes are stored inside the `.loomdeck` (deduplicated), so the deck reopens with its pictures after the source file is deleted. Missing, corrupt or cancelled files change nothing and say why. [tests] [live-W]
- Selection and direct manipulation: click to select, Shift-click and a marquee for multi-select, drag to move with alignment snapping guides, corner handles to resize (pictures keep their aspect ratio), a rotate handle, arrow keys nudge, Delete removes. A cancelled gesture restores geometry, selection and history; one gesture is one undo step. [tests] [live-W]
- Text edits in place: double-click an object (or Enter or F2 on a selected one); Enter adds a line, Escape or clicking away finishes. The edit goes through the same undoable command as the inspector. [tests] [live-W]
- Inspector: Format (slide layout, selected element's text content, position, size and rotation shown as read-only values), Animate (transition), Document (theme button, aspect ratio and slide count). [tests]
- Speaker notes: a notes drawer per slide (View menu, "Speaker Notes"), edited text is part of the undo state. [tests]
- Undo and redo (Ctrl+Z, Ctrl+Shift+Z or Ctrl+Y) for every edit above. [tests] [live-W]

### Slideshow and presenter view
- Transitions None, Dissolve, Push and Morph can be set per slide. A slide's transition plays when the slideshow arrives at it: Dissolve fades the slide in from black, Push slides it in from the right edge. The editor never animates. Morph has no object matching and plays as a Dissolve. [tests] [live-W for Push setup and arrival; smoothness on real hardware was not measured]
- Slideshow: F5 or Play makes the window full-screen with a black surround and the slide fitted; Right, Down, Page Down, Space and Enter advance, Left, Up, Page Up and Backspace go back, Escape exits; a left click advances and a right click goes back. Text scales with the slide. [tests] [live-W] [live-L]
- Presenter view: P during a slideshow (or "Open Presenter View" in the palette) opens a second window with the current slide's title and notes, a thumbnail and title of the next slide ("End of slideshow" at the end), "Slide n of N", a clock with Pause / Resume and Reset, and Previous / Next / Close. Its buttons and keys (arrows, Space, Page Up/Down, Escape) drive the main window, and it follows edits to the notes. [tests] [live-W] [live-L: the window opened and showed "Slide 2 of 3"; the clock and buttons were not exercised there]
- Loom does not choose or place a display for the presenter view; drag its window to a second display yourself.

### Export
- PDF: slide text, shapes and pictures, WinAnsi text (accents and typographic characters are single bytes; other characters become `?`). [tests] The checks read the file structure; a rendered check in an independent PDF viewer is not recorded.
- PowerPoint `.pptx`: 16:9 slides with elements at their exact position, size and rotation, the first title as the title placeholder, text boxes, rectangle and ellipse shapes, pictures as `p:pic` with media parts, speaker notes as notes slides, and transitions (Dissolve and Morph as fade, Push as push). Parts, relationships, content types and IDs are checked structurally. [tests] A deck with one picture exported by the app was opened in real Microsoft PowerPoint through COM and rendered its title placeholder and picture correctly (2026-10-04). Notes, transitions and other layouts have been checked structurally only. [live-W]
- Element click actions and per-element styling the model does not hold are not written.

### Keyboard and accessibility
- Menus, toolbar menus, the command palette, the template chooser, Save Changes, the notes drawer, the compact overflow menu and an add-slide / edit-text / navigate / save / present workflow are operable from the keyboard; 14 keyboard-flow tests send real key events and read the accessibility tree. Delete, Ctrl+Z and the arrow-key nudge work at launch without a click. Focus is trapped in dialogs and returned to the slide editor on close. [tests]
- Tab and Shift+Tab walk the objects on the slide and then leave the canvas, so the editor never traps the keyboard; thumbnails take Enter or Space. [tests]
- Alt+letter opens a menu even while typing in slide text or notes, without typing the letter. [tests]
- Accessibility tree: every interactive element has a name; the live Windows UI Automation dump showed 0 unnamed interactive controls. This is tree verification only; spoken screen-reader output has not been verified.

### Appearance and performance
- Appearance: View > Appearance: System, Light, Dark and High Contrast (also in the toolbar View menu and the command palette) switches the whole window live, including dialogs and popups, and is remembered per user in `settings.toml` under the platform config directory (`LOOM_CONFIG_DIR` overrides it). System follows the operating system's light/dark preference and falls back to light. `--theme system|light|dark|high-contrast` overrides the saved choice for that run without changing it. Document artwork (page, cells, slides) is not recoloured. [tests] The presenter window follows the chosen appearance.
- Text scale 1.0 to 2.0 (`--text-scale`), real device scale factor 1.0 to 4.0 (`--scale-factor`) and mirrored right-to-left chrome (`--rtl`, with the slide strip on the other side); tests render every reachable surface, including the presenter window, at text scale 1.0/1.5/2.0 and scale factors 1.25/1.5/2.0 and check controls stay inside the window. [tests] Renders at 1024x720, 1280x800, 1440x900 and 1920x1200 in all three themes were inspected without clipping (renderer evidence, not human sign-off). A `--scale-factor 2` render (2560x1600) was viewed on Windows and Linux. [live-W] [live-L]
- Compact windows hide the inspector and move low-priority commands into an overflow menu. [tests]
- Measured on an Intel i5-12400F, test profile at opt-level 3 with the software renderer (not `--release`, not a GPU frame), 2026-10-07, 20 elements per slide, p50: slide switch 0.05 / 0.14 / 0.34 ms and an edit 0.10 / 0.80 / 0.98 ms at 20 / 100 / 300 slides; the editor render at 300 slides took 6.0 ms. Earlier runs put drag, open, save, PDF and PPTX export and transition frames within budget at 20, 100 and 300 slides.

## Keyboard shortcuts

| Keys | Action |
|---|---|
| Alt+F / Alt+E / Alt+V / Alt+S, or F10 | Open the File / Edit / View / Slide menu (F10 opens the first); arrows walk, Enter runs, Escape closes |
| Ctrl+K | Command palette |
| Ctrl+N | New deck |
| Ctrl+Shift+N | New slide |
| Ctrl+Alt+Up / Ctrl+Alt+Down | Move slide up / down |
| Ctrl+O | Open |
| Ctrl+S / Ctrl+Shift+S | Save / Save As |
| Ctrl+E | Export PDF |
| Ctrl+Z / Ctrl+Shift+Z or Ctrl+Y | Undo / Redo |
| F5 | Start or exit the slideshow |
| Tab / Shift+Tab | Walk the objects on the slide, then leave the canvas |
| Enter or F2 | Edit the selected object's text (Escape finishes) |
| Arrow keys | Nudge the selected objects by 10 units |
| Delete or Backspace | Remove the selected objects |
| Page Up / Page Down | Previous / next slide in the editor |
| Slideshow: Right, Down, Page Down, Space, Enter / Left, Up, Page Up, Backspace | Next / previous slide |
| Slideshow: P, Escape | Open the presenter view, exit the slideshow |
| Presenter window: arrows, Space, Page Up/Down, Escape | Navigate, close the window |

Present has no F6 region key; Tab and the toolbar order cover the toolbar, inspector and canvas.

## Known limitations

- On keyboard layouts where AltGr counts as Ctrl+Alt, Ctrl+Alt+Up/Down may not fire; use the Slide menu, the strip buttons or Ctrl+Up/Down on a focused thumbnail. Repeating the same slide move twice in a row may not be re-announced by a screen reader.
- No tables, charts, audio or video, master slides or slide-number fields. Rich text runs are not supported: one style per text box. Shape inserts a rectangle only.
- Typed numeric entry for position, size and rotation is not available (UI-18); geometry is shown read-only.
- Present Zoom offers Fit, 75% and 50% only, no zoom in. The Document tab shows the aspect ratio read-only (16:9 Wide); there is no 4:3 option.
- "New from Template..." creates a new deck from a named layout set (Blank, Title and Content, Two Columns, Image and Text) after the usual Save / Discard / Cancel prompt. Templates are layouts only: there is no stored or applied visual deck theme (colours, background) and no stored per-deck appearance. [tests]
- Transitions: only None, Dissolve, Push and Morph; Morph plays as Dissolve. The tween is not a cross-fade (the previous slide is not drawn during the transition, so Dissolve is a fade through black) and runs on timer ticks, so smoothness on real hardware is unmeasured.
- PPTX export carries slide text, shapes, pictures, notes and transitions but not click actions or per-element styling; only a one-picture deck is known to open cleanly in PowerPoint. There is no PPTX import; Open reads `.loomdeck` only.
- No second-display placement, presenter-view memory of its position, slide-progress or pacing targets. The presenter view's next-slide area is a thumbnail only.
- Speaker-notes search exists in the core library but has no interface.
- Recovery drafts are debounced by 600 ms, so up to 600 ms of changes can be lost in a crash.
- Accessibility is tree-verified only; spoken screen-reader output, a fractional-scale live window, Wayland and a native (non-WSLg) Linux desktop are unverified. On Linux, Insert Image through the file dialog and a live check of the new slide strip were not re-run.
- GPU frame time and Linux and macOS performance numbers are not measured.
- Builds are unsigned (Windows SmartScreen warns on first run).

## Install and run

Download the portable archive from the GitHub `nightly` release (`https://github.com/palaashatri/rust-loom/releases/tag/nightly`): `loom-nightly-<commit>-windows-x86_64.zip` or `loom-nightly-<commit>-linux-x86_64.tar.gz`. It holds all Loom apps; run `loom-present.exe` (Windows) or `./loom-present` (Linux). Nothing needs installing.

- Windows: unsigned, so choose "More info" then "Run anyway" on the SmartScreen warning.
- Linux: needs a desktop session (X11 or Wayland), OpenGL and fontconfig; the Open, Save, Insert Image and export dialogs need `zenity` or an xdg-desktop-portal. `chmod +x loom-*` if the execute bit was lost.

## Development

```sh
cd loom-present
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p loom-present-app
# headless render (light, dark, high-contrast; text and device scale; RTL)
cargo run -p loom-present-app -- --screenshot out.png --size 1280x800 --theme light
cargo run -p loom-present-app -- --screenshot out.png --scale-factor 2 --rtl
```

Run cargo from inside `loom-present` so its `.cargo/config.toml` applies. `docs/` holds dated QA captures that are evidence for the states named in the ledger, not a current acceptance record.
