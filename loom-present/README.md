# Loom Present

Loom Present is a local-first presentation editor for Windows, Linux and macOS. Decks are open, inspectable `.loomdeck` packages; nothing is uploaded and no account is needed.

Status: **functional, `ACCEPTANCE_BLOCKED`**. The working features below were verified in real windows on Windows 11 and (earlier builds) Linux under WSLg. Screen-reader output, a complete keyboard-only pass, the full viewport/theme/text-scale matrix, macOS, and PowerPoint opening an app-produced `.pptx` are not yet verified. See the current-truth section of the repository `AGENTS.md` for the evidence and open items.

## What works

Checked by hand in a real window on 2026-10-02/04: blank start, Insert Image through the native picker, image selection and handles, Save As, reopen with the source picture deleted, the saved-state status, the title following the file, and slideshow/presenter-view basics (earlier builds). Everything else listed here is covered by the automated suite (`cargo test --workspace`) but was not re-checked by hand this session.

- **Start**: a blank one-slide deck with a "Click to add title" prompt (the prompt is never saved, exported or shown in thumbnails). "New from Sample Deck" is in the File menu and command palette.
- **Slides**: add, duplicate, delete, reorder; the left strip shows real scaled thumbnails (text, shapes and images) that update as you edit.
- **Objects**: text, shapes and pictures. Click to select; drag to move; corner handles resize (pictures keep their aspect ratio); Delete removes the selection; double-click edits text in place. Everything is undoable and redoable, and it is one undo step per gesture.
- **Insert Image**: toolbar, Slide menu or command palette; native file picker for PNG/JPEG. The picture is centered, at most 60 % of the slide. Image bytes are stored inside the `.loomdeck` (deduplicated), so the deck still opens with its pictures after the original file is deleted.
- **Inspector**: Format (slide layout, selected-element geometry), Animate (transitions), Document.
- **Transitions**: None, Dissolve, Push; they play when the slideshow arrives at a slide. (Morph plays as Dissolve; it does not match objects.)
- **Slideshow**: F5 or Play; Right/Down/Space/Enter advance, Left/Up/Backspace go back, Esc exits; click advances.
- **Presenter view**: press P during a slideshow for a second window with the current slide, speaker notes, next-slide thumbnail, slide counter and a clock.
- **Notes**: a speaker-notes drawer per slide.
- **Files**: native Open / Save / Save As; the window title follows the saved file name; the status bar shows Saved / Edited. Closing with unsaved changes asks Save / Discard / Cancel. After a crash, the unsaved deck is offered back at the next launch; an intentional close (including Discard) clears it.
- **Export**: PDF (text, shapes, pictures; WinAnsi text) and PowerPoint `.pptx` (slides, text, pictures as `p:pic` with media parts; reopened in real PowerPoint).
- **Menus**: Windows and Linux draw one in-window File / Edit / View / Slide menu in the title row; macOS uses the native menu bar. Command palette on Ctrl+K.
- **Themes**: light (default), dark, high contrast.

## Known limitations

- PPTX export carries slide text and pictures, not transitions or animations. A deck with one picture exported by the app opened in Microsoft PowerPoint (COM) with the title placeholder and the picture rendered correctly (2026-10-04); other layouts were checked structurally only.
- No rich text runs inside a text box (one style per box), no tables, charts, audio or video, no master slides or slide-number fields.
- Present does not pick a second display for the presenter view; drag its window yourself.
- Typed numeric entry for geometry is not available yet (UI-18).
- On Linux a file-dialog helper is required: an `xdg-desktop-portal` or `zenity`.

## Development

```sh
cd loom-present
cargo test --workspace
cargo run -p loom-present-app
cargo run -p loom-present-app -- --screenshot out.png --size 1280x800 --theme light   # headless render
```

Downloadable builds are published by CI as the `nightly` release (Linux x86-64, Windows x86-64, macOS arm64/x86-64; unsigned).
