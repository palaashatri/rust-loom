# Loom Writer

Loom Writer is a local-first word processor. Documents are open, inspectable `.loomdoc` packages on your own computer; nothing is uploaded and no account is needed. It exports PDF, Word `.docx` and Markdown.

**Platforms:** Windows x86-64 and Linux x86-64. macOS is out of scope for v1 and is not claimed.

**Status: `ACCEPTANCE_BLOCKED`.** The supervisor has not recorded `ACCEPTED`. This is not a production-ready claim. The live ledger is the current-truth section of the repository `AGENTS.md`; this README lists what has evidence behind it and what does not.

Evidence tags used below:

- **[tests]** covered by an automated test that passes (`cargo test --workspace`; the ledger records 199 app tests and 136 core tests on 2026-10-07; not re-run while writing this README).
- **[live-W]** exercised by hand in a real Windows 11 window.
- **[live-L]** exercised by hand in a real Linux window (WSLg, Ubuntu).

## What works

### Start, files and recovery
- First launch opens an empty "Untitled" document with the caret ready and a hint outside the document. The Quick Start sample exists only on request ("New from Quick Start sample" in the command palette). New opens a template chooser with six templates: Blank, Blank Black, Executive Report, Business Letter, Curriculum Vitae, Newsletter. [tests] [live-W] [live-L]
- Native Open, Save and Save As for `.loomdoc`; the title bar follows the saved file name; an unsaved dot shows in the title. Cancelling a dialog changes nothing. [tests] [live-W]
- Closing, New or Open with unsaved changes asks Save / Discard / Cancel (the prompt says "before closing" when it is a close). [tests] [live-W]
- Crash recovery: the unsaved document is written as a draft 600 ms after the first unrecorded keystroke, off the typing path; Save, Open, formatting, undo and close write immediately. At the next launch the recovered draft reads as unsaved and closing asks. An intentional close (clean, saved or Discard) clears the draft; a crash leaves it. A crash can lose up to 600 ms of the latest typing. [tests] [live-W]
- Command line: `loom-writer path/to/file.loomdoc`. [tests]

### Editing
- Unicode text, caret and selection (including across pages), double-click selects a word and triple-click a paragraph, clipboard in both directions (multi-paragraph, accents, CJK), Ctrl+arrow word movement, Page Up/Down (Shift extends), mouse-wheel scrolling, and the page follows the caret. Pasted tabs become spaces, Tab inserts four spaces. [tests] [live-W]
- Undo and redo for typing, formatting, tables, comments and replace; Ctrl+Z and Ctrl+Y work inside the page and use the document history. [tests] [live-W]
- Character formatting: bold, italic, underline, strikethrough on a selection or a caret. Paragraph styles Body and Heading 1 to 3; Left, Center and Right alignment; bulleted and numbered lists (numbering restarts after an interruption). [tests] [live-W]
- Find and replace (Ctrl+F, Ctrl+H): every match near the view is highlighted and the current one selected, Next/Previous, F3 and Shift+F3 (also with the bar closed), a Match case toggle, Replace and Replace all (one undo step); the status reads "2 of 4", "No matches" or "N replaced". Typing replaces the pre-filled selection in the find field. [tests] [live-W]
- Pages: each page is drawn as its own sheet with a gap between pages. Zoom 50% to 200%. Page setup in the Document inspector: A4 or US Letter, Portrait or Landscape, Normal / Narrow / Wide margins. [tests for layout and page setup in core and DOCX]
- Comments: add on a selection (or on the whole paragraph from a caret), resolve, reopen, delete, jump to the commented text with a highlight on the page, persisted in the file. Anchors move with edits and are restored by undo and redo; deleting the commented text marks the thread "Text removed". A paste over a selection falls back to a text comparison to re-anchor. [tests] [live-W for adding a comment on a selection]
- Table: Insert > Table adds a Markdown-text table block at the caret; it is undoable and saved. Cells edit as text. [tests]
- Outline navigator (View menu): lists headings, a click moves the caret, follows edits. [tests]

### Export
- PDF: every page, headings and bold/italic/underline runs in their own base-14 faces, accented and typographic characters written as WinAnsi. [tests] A rendered check in an independent PDF viewer is not recorded; the checks read the file structure and bytes.
- Markdown: headings, lists, tables, runs. [tests]
- Word `.docx`: Word heading styles with outline levels, real numbered and bulleted lists, real tables from the Markdown tables, page size, orientation and margins, bold/italic/underline/strike runs, alignment, Word comments with resolved state, deterministic output. [tests] A generated sample was opened in real Microsoft Word through COM on 2026-10-02; a re-check of the current app-produced file in Word is not recorded. The exporter also writes run font family and size; no test asserts those two.
- A comment on deleted text or on a table is left out of the Word file and the status names how many were skipped. An empty document cannot be exported. [tests]

### Keyboard and accessibility
- Menus, toolbar menus, dialogs (Save Changes, template chooser, command palette, find bar) and a type/format/find/save workflow are operable from the keyboard, with focus trapped in dialogs and returned to the page on close; 13 keyboard-flow tests send real key events and read the accessibility tree. The page takes focus at launch, so typing works with no click. [tests]
- Alt+letter opens a menu even from the page, the find field or the comment box, without typing the letter; confirmed in a native Windows window and in WSLg. [live-W] [live-L]
- Accessibility tree: every interactive element has a name (test); the live Windows UI Automation dump showed 0 unnamed interactive controls. This is tree verification only; spoken screen-reader output has not been verified.

### Appearance and performance
- Themes light, dark, high contrast via `--theme light|dark|high-contrast` at launch (no in-app switch). Text scale 1.0 to 2.0 (`--text-scale`), real device scale factor 1.0 to 4.0 (`--scale-factor`) and mirrored right-to-left chrome (`--rtl`); tests render every reachable surface at text scale 1.0/1.5/2.0 and scale factors 1.25/1.5/2.0 and check controls stay inside the window. [tests] Renders at 1024x720, 1280x800, 1440x900 and 1920x1200 in all three themes were inspected without clipping (renderer evidence, not human sign-off). A `--rtl` render at 1280x800 and a `--scale-factor 1.5` render were viewed on Windows. [live-W]
- Measured on an Intel i5-12400F, test profile at opt-level 3 with the software renderer (not `--release`, not a GPU frame), 2026-10-07: keystroke p95 0.84 / 4.4 / 14.8 ms and scroll callback p95 0.54 / 0.60 / 1.9 ms at 20 / 100 / 400 pages; find update 0.5 / 2.1 / 8.3 ms. Budgets are 16.7 ms per keystroke or scroll. Open of a 100-page document took 468 ms and save and PDF export under 90 ms at 400 pages (measured 2026-10-04; open of 400 pages took 4.2 s before the layout cache, and no later 400-page open number is recorded). A 300-paragraph paste stayed responsive in a Windows release build. [live-W]

## Keyboard shortcuts

| Keys | Action |
|---|---|
| Alt+F / Alt+E / Alt+V / Alt+O, or F10 | Open the File / Edit / View / Format menu (Format takes O because File took F); F10 opens the first; arrows walk, Enter runs, Escape closes |
| Ctrl+K | Command palette |
| Ctrl+N / Ctrl+O | New / Open |
| Ctrl+S / Ctrl+Shift+S | Save / Save As |
| Ctrl+E | Export PDF |
| Ctrl+B / Ctrl+I / Ctrl+U | Bold / Italic / Underline |
| Ctrl+Z / Ctrl+Shift+Z | Undo / Redo (Ctrl+Y also redoes while the page has focus) |
| Ctrl+F / Ctrl+H | Find / Find and replace |
| F3 / Shift+F3 | Next / previous match (also with the bar closed) |
| Enter / Shift+Enter / Escape (find bar) | Next / previous / close and return to the page |
| Page Up / Page Down (Shift extends) | Move the caret one screen |
| Tab | Insert four spaces |
| F6 | From anywhere: focus the page. From the page: the inspector tab strip (the toolbar if no inspector is shown) |
| Shift+F6 | From the page: the first toolbar item |
| Ctrl+C / Ctrl+V | Copy / paste (checked live; select all and cut are the page text box's own and have no recorded check) |

## Known limitations

- Tables are Markdown text, not a cell grid; there is no visual table editor. The Word export converts them to real tables.
- No inline images, headers or footers, footnotes, spell check, track changes or columns.
- Open reads `.loomdoc` only; there is no Word or Markdown import.
- There is no justified alignment: the page editor cannot place words to justify a line, so the control was removed rather than left inert. The model and the file formats can carry Justify (it is written to `.loomdoc` and as `w:jc both` in Word exports), and a file that already carries it is shown as unavailable rather than as Left.
- Page text is left-to-right only: there is no right-to-left shaping, caret movement or bidirectional selection. `--rtl` mirrors the window chrome.
- Font family (Sans / Serif / Mono), font size, indent/outdent and line spacing controls exist in the inspector. Font size and family are stored on the text, but the page draws one typeface and paragraph-style sizes, and PDF does not carry run sizes, colours or super/subscript. The indent and line-spacing controls have no dedicated regression test and were not exercised in the recorded live checks.
- PDF cannot export CJK text. The PDF page layout wraps with the editor's Inter measurements while drawing Helvetica, so line breaks can differ slightly. Italic, run font size and font family are not measured in layout.
- Comment threads on a bare caret anchor to the whole paragraph, which the UI does not explain. A paste over a selection re-anchors comments by text comparison.
- Performance: the editor text box holds the whole text, so the software renderer cannot draw documents past about 30 pages (a test-harness limit; a live Windows release-build window holding a 300-paragraph document stayed responsive). Two document clones per keystroke remain. Native GPU frame time, Linux and macOS numbers are not measured.
- Recovery drafts are debounced by 600 ms, so up to 600 ms of typing can be lost in a crash.
- Accessibility is tree-verified only; spoken screen-reader output, the compact inspector drawer, a fractional-scale live window, Wayland and a native (non-WSLg) Linux desktop are unverified. Linux was driven interactively for launch, typing, Alt+F, Escape and F6, and the palette New Document flow on an earlier nightly.
- Builds are unsigned (Windows SmartScreen warns on first run).

## Install and run

Download the portable archive from the GitHub `nightly` release (`https://github.com/palaashatri/rust-loom/releases/tag/nightly`): `loom-nightly-<commit>-windows-x86_64.zip` or `loom-nightly-<commit>-linux-x86_64.tar.gz`. It holds all Loom apps; run `loom-writer.exe` (Windows) or `./loom-writer` (Linux). Nothing needs installing.

- Windows: unsigned, so choose "More info" then "Run anyway" on the SmartScreen warning.
- Linux: needs a desktop session (X11 or Wayland), OpenGL and fontconfig; the Open, Save and export dialogs need `zenity` or an xdg-desktop-portal. `chmod +x loom-*` if the execute bit was lost.

## Development

```sh
cd loom-writer
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p loom-writer-app
# headless render (light, dark, high-contrast; text and device scale; RTL)
cargo run -p loom-writer-app -- --screenshot out.png --size 1280x800 --theme light
cargo run -p loom-writer-app -- --screenshot out.png --scale-factor 2 --rtl
```

Run cargo from inside `loom-writer` so its `.cargo/config.toml` applies (Windows needs a larger main-thread stack for the Slint build). Only release builds are meaningful for performance: a debug build takes about 10 s per keystroke on a very long document. `docs/` holds dated screenshots that are evidence for the states named in the ledger, not a current acceptance record.
