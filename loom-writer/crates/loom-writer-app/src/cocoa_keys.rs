//! macOS Cocoa-style text navigation and editing bindings.
//!
//! Active when the window runs in macOS shortcut mode (`shortcut_modifier == "Cmd"`),
//! preserving Windows/Linux `Ctrl+A` (Select All), `Ctrl+K` (Palette), and
//! `Ctrl+Left/Right/Backspace` behavior when `shortcut_modifier == "Ctrl"`.

use loom_writer_core::{visual_caret_move, CaretAffinity, TextSelection, VisualMove};
use slint::platform::Key;
use slint::SharedString;

use crate::{GuiState, WriterApp};

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn clamp_char_boundary(text: &str, offset: usize) -> usize {
    let mut pos = offset.min(text.len());
    while pos > 0 && !text.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

pub(crate) fn word_left(text: &str, offset: usize) -> usize {
    let clamped = clamp_char_boundary(text, offset);
    let before = &text[..clamped];
    let mut iter = before.char_indices().rev();
    let mut pos = clamped;
    for (idx, ch) in iter.by_ref() {
        pos = idx;
        if is_word_char(ch) {
            break;
        }
    }
    for (idx, ch) in iter {
        if !is_word_char(ch) {
            return idx + ch.len_utf8();
        }
        pos = idx;
    }
    pos
}

pub(crate) fn word_right(text: &str, offset: usize) -> usize {
    let clamped = clamp_char_boundary(text, offset);
    let after = &text[clamped..];
    let mut iter = after.char_indices();
    let mut found_word = false;
    for (idx, ch) in iter.by_ref() {
        if is_word_char(ch) {
            found_word = true;
            break;
        }
        if idx > 0 && ch == '\n' {
            return clamped + idx;
        }
    }
    if !found_word {
        return text.len();
    }
    for (idx, ch) in iter {
        if !is_word_char(ch) {
            return clamped + idx;
        }
    }
    text.len()
}

fn move_to(
    app: &WriterApp,
    state: &GuiState,
    selection: TextSelection,
    target: usize,
    affinity: CaretAffinity,
    shift: bool,
) {
    crate::caret_keys::clear_goal_column();
    let anchor = if shift { selection.anchor } else { target };
    state.current.borrow_mut().set_selection(TextSelection {
        anchor: selection.anchor,
        focus: selection.focus,
        affinity,
    });
    app.invoke_selection_changed(
        anchor.min(i32::MAX as usize) as i32,
        target.min(i32::MAX as usize) as i32,
    );
    if (anchor, target) == (selection.anchor, selection.focus) {
        crate::refresh_render_projection_of(app, state);
    }
}

fn move_visual(
    app: &WriterApp,
    state: &GuiState,
    selection: TextSelection,
    kind: VisualMove,
    shift: bool,
) -> bool {
    let target = {
        let doc = state.current.borrow();
        visual_caret_move(&doc, selection.focus, selection.affinity, kind, None)
    };
    let Some(target) = target else {
        return false;
    };
    move_to(app, state, selection, target.offset, target.affinity, shift);
    true
}

fn delete_range(app: &WriterApp, state: &GuiState, start: usize, end: usize) {
    crate::caret_keys::clear_goal_column();
    if start >= end {
        return;
    }
    let text = state.current.borrow().editor_text();
    let start = clamp_char_boundary(&text, start);
    let end = clamp_char_boundary(&text, end);
    if start >= end {
        return;
    }
    let mut next = String::with_capacity(text.len() - (end - start));
    next.push_str(&text[..start]);
    next.push_str(&text[end..]);
    let caret = start.min(i32::MAX as usize) as i32;
    app.invoke_document_edited(next.into(), caret, caret);
}

pub(crate) fn handle(
    app: &WriterApp,
    state: &GuiState,
    key: &str,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
) -> bool {
    if !ctrl && !alt && !meta {
        return false;
    }
    if app.get_shortcut_modifier().as_str() != "Cmd" {
        return false;
    }
    let is = |code: Key| {
        let text: SharedString = code.into();
        key == text.as_str()
    };
    if crate::table_actions::get_table_cell_selection_range().is_some() {
        return false;
    }
    let (selection, text) = {
        let doc = state.current.borrow();
        let selection = doc.selection();
        if doc.table_cell_at_offset(selection.focus).is_some() {
            return false;
        }
        (selection, doc.editor_text())
    };

    // Option+Left / Option+Right (+ Shift): word backward / forward
    if alt && !ctrl && !meta && is(Key::LeftArrow) {
        let target = word_left(&text, selection.focus);
        move_to(
            app,
            state,
            selection,
            target,
            CaretAffinity::Downstream,
            shift,
        );
        return true;
    }
    if alt && !ctrl && !meta && is(Key::RightArrow) {
        let target = word_right(&text, selection.focus);
        move_to(
            app,
            state,
            selection,
            target,
            CaretAffinity::Downstream,
            shift,
        );
        return true;
    }

    // Cmd+Left / Cmd+Right (+ Shift): visual line start / end
    if meta && !ctrl && !alt && is(Key::LeftArrow) {
        return move_visual(app, state, selection, VisualMove::LineStart, shift);
    }
    if meta && !ctrl && !alt && is(Key::RightArrow) {
        return move_visual(app, state, selection, VisualMove::LineEnd, shift);
    }

    // Cmd+Up / Cmd+Down (+ Shift): document start / end
    if meta && !ctrl && !alt && is(Key::UpArrow) {
        move_to(app, state, selection, 0, CaretAffinity::Downstream, shift);
        return true;
    }
    if meta && !ctrl && !alt && is(Key::DownArrow) {
        move_to(
            app,
            state,
            selection,
            text.len(),
            CaretAffinity::Downstream,
            shift,
        );
        return true;
    }

    // Option+Backspace: delete word backward (or active selection)
    if alt && !ctrl && !meta && is(Key::Backspace) {
        let (start, end) = if selection.anchor != selection.focus {
            (
                selection.anchor.min(selection.focus),
                selection.anchor.max(selection.focus),
            )
        } else {
            (word_left(&text, selection.focus), selection.focus)
        };
        delete_range(app, state, start, end);
        return true;
    }

    // Cmd+Backspace: delete to visual line start (or active selection)
    if meta && !ctrl && !alt && is(Key::Backspace) {
        let (start, end) = if selection.anchor != selection.focus {
            (
                selection.anchor.min(selection.focus),
                selection.anchor.max(selection.focus),
            )
        } else {
            let line_start = {
                let doc = state.current.borrow();
                visual_caret_move(
                    &doc,
                    selection.focus,
                    selection.affinity,
                    VisualMove::LineStart,
                    None,
                )
                .map(|t| t.offset)
                .unwrap_or(0)
            };
            (line_start, selection.focus)
        };
        delete_range(app, state, start, end);
        return true;
    }

    // macOS Cocoa/Emacs control bindings: Ctrl+A (line start), Ctrl+E (line end), Ctrl+K (kill to line end)
    if ctrl && !meta && !alt && !shift {
        if key == "a" || key == "A" {
            return move_visual(app, state, selection, VisualMove::LineStart, false);
        }
        if key == "e" || key == "E" {
            return move_visual(app, state, selection, VisualMove::LineEnd, false);
        }
        if key == "k" || key == "K" {
            let line_end = {
                let doc = state.current.borrow();
                visual_caret_move(
                    &doc,
                    selection.focus,
                    selection.affinity,
                    VisualMove::LineEnd,
                    None,
                )
                .map(|t| t.offset)
                .unwrap_or(text.len())
            };
            let kill_end = if line_end > selection.focus {
                line_end
            } else if text.as_bytes().get(selection.focus) == Some(&b'\n') {
                selection.focus + 1
            } else {
                selection.focus
            };
            delete_range(app, state, selection.focus, kill_end);
            return true;
        }
    }

    false
}
