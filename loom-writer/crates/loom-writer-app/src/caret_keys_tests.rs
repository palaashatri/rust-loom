//! Up, Down, Home and End follow the lines the page shows, not the hidden
//! text input's one-size layout. The first group sends real key events through
//! the window; the second calls the key handler directly for finer cases.

use super::keyboard_flow_tests::launched;
use super::*;
use loom_writer_core::FlowLine;
use slint::platform::{Key, WindowEvent};

const LONG: &str = "The quick brown fox jumps over the lazy dog while the sun sets slowly \
                    behind distant hills and the evening grows quiet";

fn dispatch(app: &WriterApp, text: impl Into<SharedString>) {
    let text = text.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn press(app: &WriterApp, key: Key) {
    dispatch(app, SharedString::from(key));
}

fn press_shifted(app: &WriterApp, key: Key) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(Key::Shift),
    });
    press(app, key);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(Key::Shift),
    });
}

fn lines(state: &GuiState) -> Vec<FlowLine> {
    let doc = state.current.borrow();
    let style = doc.page.page_style();
    doc.flow(
        &style,
        PageViewport {
            width: style.width_pt,
            height: style.height_pt,
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        },
    )
    .unwrap()
    .lines()
    .to_vec()
}

fn focus(state: &GuiState) -> usize {
    state.current.borrow().selection().focus
}

fn anchor(state: &GuiState) -> usize {
    state.current.borrow().selection().anchor
}

/// A centred, wrapped 24 pt heading followed by a body paragraph.
fn with_heading() -> super::keyboard_flow_tests::Session {
    let s = launched(&format!("{LONG}\nBody paragraph"));
    {
        let mut doc = s.state.current.borrow_mut();
        doc.blocks[0].kind = "heading1".into();
        doc.blocks[0].style.alignment = loom_text::Alignment::Center;
    }
    apply_state(&s.app, &s.state);
    s.app.invoke_focus_page();
    s
}

fn at(s: &super::keyboard_flow_tests::Session, offset: usize) {
    s.app.invoke_selection_changed(offset as i32, offset as i32);
}

#[test]
fn down_in_a_wrapped_heading_moves_to_its_second_visible_line() {
    let s = with_heading();
    let lines = lines(&s.state);
    assert!(lines.len() >= 3, "the heading wraps to several lines");
    at(&s, lines[0].global_start + 6);
    press(&s.app, Key::DownArrow);
    let second = &lines[1];
    let end = second.global_start + (second.end - second.start);
    assert!(
        (second.global_start..=end).contains(&focus(&s.state)),
        "focus {} should be on the second visible line {}..{}",
        focus(&s.state),
        second.global_start,
        end
    );
    press(&s.app, Key::DownArrow);
    let third = &lines[2];
    assert!(
        focus(&s.state) >= third.global_start || lines[2].block_index != 0,
        "and the next Down reaches the third line, not the paragraph"
    );
    assert!(focus(&s.state) > end, "moved past the second line");
}

#[test]
fn end_and_home_stay_on_the_visible_line_and_shift_extends() {
    let s = with_heading();
    let lines = lines(&s.state);
    let first = &lines[0];
    at(&s, first.global_start + 4);
    press(&s.app, Key::End);
    let visible = LONG[first.start..first.end].trim_end().len();
    assert_eq!(focus(&s.state), first.global_start + visible);
    assert!(focus(&s.state) < LONG.len(), "not the paragraph's end");
    assert_eq!(anchor(&s.state), focus(&s.state));

    press(&s.app, Key::Home);
    assert_eq!(focus(&s.state), first.global_start);

    at(&s, first.global_start + 4);
    press_shifted(&s.app, Key::End);
    assert_eq!(anchor(&s.state), first.global_start + 4);
    assert_eq!(focus(&s.state), first.global_start + visible);
    press_shifted(&s.app, Key::DownArrow);
    assert_eq!(anchor(&s.state), first.global_start + 4);
    assert!(focus(&s.state) > first.global_start + visible);
}

fn press_document_edge(app: &WriterApp, key: Key) {
    let (modifier, key) = match key {
        Key::End if cfg!(target_os = "macos") => (Key::Meta, Key::DownArrow),
        Key::Home if cfg!(target_os = "macos") => (Key::Meta, Key::UpArrow),
        key => (Key::Control, key),
    };
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(modifier),
    });
    press(app, key);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(modifier),
    });
}

#[test]
fn ctrl_end_then_ctrl_home_returns_the_caret_and_the_page_to_the_top() {
    let text = (1..=120)
        .map(|n| format!("Paragraph {n} with enough words to wrap across a line of the page."))
        .collect::<Vec<_>>()
        .join("\n");
    let s = launched(&text);
    press_document_edge(&s.app, Key::End);
    let end = s.state.current.borrow().editor_text().len();
    assert_eq!(focus(&s.state), end, "Ctrl+End reaches the end of the text");
    assert!(
        s.state.viewport.borrow().scroll_y > 0.0,
        "and the page scrolls down to it"
    );

    press_document_edge(&s.app, Key::Home);
    assert_eq!(focus(&s.state), 0, "Ctrl+Home returns to the start");
    assert_eq!(
        s.state.viewport.borrow().scroll_y,
        0.0,
        "and the page returns to the top margin"
    );
}

#[test]
fn ctrl_home_returns_to_the_top_when_the_page_is_zoomed() {
    let text = (1..=120)
        .map(|n| format!("Paragraph {n} with enough words to wrap across a line of the page."))
        .collect::<Vec<_>>()
        .join("\n");
    let s = launched(&text);
    s.app.invoke_page_zoom_changed(1.5);
    press_document_edge(&s.app, Key::End);
    assert!(s.state.viewport.borrow().scroll_y > 0.0);
    press_document_edge(&s.app, Key::Home);
    assert_eq!(
        focus(&s.state),
        0,
        "Ctrl+Home returns to the start when zoomed"
    );
    assert_eq!(
        s.state.viewport.borrow().scroll_y,
        0.0,
        "and the zoomed page returns to the top margin"
    );
}

#[test]
fn ctrl_home_leaves_a_long_table_at_the_end_and_returns_to_the_top() {
    let s = launched("Intro paragraph");
    s.state
        .current
        .borrow_mut()
        .insert_table_block(usize::MAX, 60, 3)
        .expect("insert a long table");
    apply_state(&s.app, &s.state);
    s.app.invoke_focus_page();
    press_document_edge(&s.app, Key::End);
    assert!(
        s.state.viewport.borrow().scroll_y > 0.0,
        "Ctrl+End scrolls to the table"
    );
    press_document_edge(&s.app, Key::Home);
    assert_eq!(
        focus(&s.state),
        0,
        "Ctrl+Home leaves the table and returns to the start"
    );
    assert_eq!(s.state.viewport.borrow().scroll_y, 0.0);
}

fn session(blocks: Vec<RichBlock>) -> (WriterApp, Rc<GuiState>) {
    let mut document = WriterDocument::new("test", "Test");
    document.blocks = blocks;
    let (app, state) = super::actions_tests::test_state(
        document,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
    );
    wire_writer_shared_callbacks(&app, &state, None);
    (app, state)
}

fn key(app: &WriterApp, key: Key) -> bool {
    app.invoke_handle_table_key(SharedString::from(key), false, false, false)
}

#[test]
fn the_goal_column_survives_a_short_line_and_resets_after_other_movement() {
    let (app, state) = session(vec![
        RichBlock::new(1, "paragraph", "0123456789 0123456789 0123456789"),
        RichBlock::new(2, "paragraph", "ab"),
        RichBlock::new(3, "paragraph", "0123456789 0123456789 0123456789"),
    ]);
    app.invoke_selection_changed(20, 20);
    assert!(key(&app, Key::DownArrow));
    assert!(focus(&state) <= 36, "short line: {}", focus(&state));
    assert!(key(&app, Key::DownArrow));
    let third = 33 + 1 + 2 + 1;
    let col = focus(&state) - third;
    assert!(
        (19..=21).contains(&col),
        "back at about column 20, got {col}"
    );
    assert!(key(&app, Key::UpArrow));
    assert!(key(&app, Key::UpArrow));
    let col = focus(&state);
    assert!((19..=21).contains(&col), "and up again, got {col}");

    // Any other movement forgets the column.
    app.invoke_selection_changed(34, 34);
    assert!(key(&app, Key::DownArrow));
    assert!(focus(&state) - third <= 1, "fresh column at the start");
}

#[test]
fn keys_inside_a_table_are_left_to_cell_navigation() {
    let (app, state) = session(vec![
        RichBlock::new(1, "paragraph", "above"),
        loom_writer_core::markdown_table_block(2, "| a | b |\n| - | - |\n| c | d |"),
    ]);
    app.invoke_selection_changed(8, 8);
    assert!(state.current.borrow().table_cell_at_offset(8).is_some());
    for k in [Key::UpArrow, Key::DownArrow, Key::Home, Key::End] {
        assert!(!key(&app, k), "{k:?} is not consumed inside a table");
    }
    assert_eq!(focus(&state), 8);
}

fn press_shifted_document_edge(app: &WriterApp, key: Key) {
    let (modifier, key) = match key {
        Key::End if cfg!(target_os = "macos") => (Key::Meta, Key::DownArrow),
        Key::Home if cfg!(target_os = "macos") => (Key::Meta, Key::UpArrow),
        key => (Key::Control, key),
    };
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(Key::Shift),
    });
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(modifier),
    });
    press(app, key);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(modifier),
    });
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(Key::Shift),
    });
}

#[test]
fn shift_document_edge_extends_selection_to_end_and_back_to_start_with_scroll() {
    let text = (1..=120)
        .map(|n| format!("Paragraph {n} with enough words to wrap across a line of the page."))
        .collect::<Vec<_>>()
        .join("\n");
    let s = launched(&text);
    let mid = text.len() / 2;
    at(&s, mid);

    press_shifted_document_edge(&s.app, Key::End);
    assert_eq!(anchor(&s.state), mid, "Shift+Ctrl/Cmd+End keeps anchor");
    assert_eq!(focus(&s.state), text.len(), "and moves focus to end");
    assert!(
        s.state.viewport.borrow().scroll_y > 0.0,
        "and scrolls down to reveal the document end"
    );

    press_shifted_document_edge(&s.app, Key::Home);
    assert_eq!(anchor(&s.state), mid, "Shift+Ctrl/Cmd+Home keeps anchor");
    assert_eq!(focus(&s.state), 0, "and moves focus to document start");
    assert_eq!(
        s.state.viewport.borrow().scroll_y,
        0.0,
        "and returns scroll to the top margin"
    );
}

#[test]
fn home_and_end_at_top_and_bottom_document_edges_keep_margin_clearance() {
    let text = (1..=120)
        .map(|n| format!("Paragraph {n} with enough words to wrap across a line of the page."))
        .collect::<Vec<_>>()
        .join("\n");
    let s = launched(&text);

    // Jump to the bottom edge, then press Home and End on the last line.
    press_document_edge(&s.app, Key::End);
    let bottom_scroll = s.state.viewport.borrow().scroll_y;
    assert!(bottom_scroll > 0.0);

    let last_line_start = text.rfind('\n').unwrap() + 1;
    press(&s.app, Key::Home);
    assert_eq!(focus(&s.state), last_line_start);
    assert!(
        s.state.viewport.borrow().scroll_y > 0.0,
        "Home on the last line stays scrolled near the bottom"
    );

    press(&s.app, Key::End);
    assert_eq!(focus(&s.state), text.len());
    assert!(
        (s.state.viewport.borrow().scroll_y - bottom_scroll).abs() < 1.0,
        "End returns to the bottom scroll position"
    );

    // Up on the very first line moves to 0 and keeps top margin; Down on the
    // very last line moves to the end.
    press_document_edge(&s.app, Key::Home);
    at(&s, 5);
    press(&s.app, Key::UpArrow);
    assert_eq!(focus(&s.state), 0, "Up on line 0 lands at document start");
    assert_eq!(s.state.viewport.borrow().scroll_y, 0.0);

    at(&s, last_line_start + 3);
    press(&s.app, Key::DownArrow);
    assert_eq!(
        focus(&s.state),
        text.len(),
        "Down on the last line lands at document end"
    );
}

fn press_chord(app: &WriterApp, modifiers: &[Key], key: impl Into<SharedString>) {
    for m in modifiers {
        app.window().dispatch_event(WindowEvent::KeyPressed {
            text: SharedString::from(*m),
        });
    }
    dispatch(app, key);
    for m in modifiers.iter().rev() {
        app.window().dispatch_event(WindowEvent::KeyReleased {
            text: SharedString::from(*m),
        });
    }
}

#[test]
fn cocoa_word_and_line_navigation_and_deletion() {
    let s = launched("alpha beta gamma\nsecond line");
    s.app.set_shortcut_modifier("Cmd".into());

    // Option+Right / Option+Left word navigation and Shift extension via real WindowEvent dispatch
    at(&s, 0);
    press_chord(&s.app, &[Key::Alt], SharedString::from(Key::RightArrow));
    assert_eq!(focus(&s.state), 5, "Option+Right jumps to end of 'alpha'");
    press_chord(
        &s.app,
        &[Key::Alt, Key::Shift],
        SharedString::from(Key::RightArrow),
    );
    assert_eq!(s.state.current.borrow().selection().anchor, 5);
    assert_eq!(
        focus(&s.state),
        10,
        "Option+Shift+Right extends over ' beta'"
    );
    press_chord(&s.app, &[Key::Alt], SharedString::from(Key::LeftArrow));
    assert_eq!(focus(&s.state), 6, "Option+Left jumps to start of 'beta'");

    // Cmd+Right / Cmd+Left line end / start and Shift extension
    press_chord(&s.app, &[Key::Meta], SharedString::from(Key::RightArrow));
    assert_eq!(focus(&s.state), 16, "Cmd+Right jumps to line 1 end");
    press_chord(
        &s.app,
        &[Key::Meta, Key::Shift],
        SharedString::from(Key::LeftArrow),
    );
    assert_eq!(s.state.current.borrow().selection().anchor, 16);
    assert_eq!(focus(&s.state), 0, "Cmd+Shift+Left selects to line 1 start");

    // Cmd+Down / Cmd+Up doc end / start and Shift extension
    press_chord(&s.app, &[Key::Meta], SharedString::from(Key::DownArrow));
    assert_eq!(focus(&s.state), 28, "Cmd+Down jumps to document end");
    press_chord(
        &s.app,
        &[Key::Meta, Key::Shift],
        SharedString::from(Key::UpArrow),
    );
    assert_eq!(s.state.current.borrow().selection().anchor, 28);
    assert_eq!(focus(&s.state), 0, "Cmd+Shift+Up selects to document start");

    // Option+Backspace deletes word backward
    at(&s, 10);
    press_chord(&s.app, &[Key::Alt], SharedString::from(Key::Backspace));
    assert_eq!(
        s.state.current.borrow().editor_text(),
        "alpha  gamma\nsecond line"
    );

    // Cmd+Backspace deletes to visual line start
    press_chord(&s.app, &[Key::Meta], SharedString::from(Key::Backspace));
    assert_eq!(
        s.state.current.borrow().editor_text(),
        " gamma\nsecond line"
    );
}

#[test]
fn cocoa_ctrl_a_e_k_work_on_macos_while_preserving_cmd_a_and_ctrl_a_on_windows_linux() {
    // macOS mode ("Cmd"): Ctrl+A (line start), Ctrl+E (line end), Ctrl+K (kill to line end)
    let s = launched("first line\nsecond line");
    s.app.set_shortcut_modifier("Cmd".into());

    at(&s, 17); // inside "second line"
    press_chord(&s.app, &[Key::Control], "a");
    assert_eq!(focus(&s.state), 11, "Ctrl+A moves to line start on macOS");
    assert_eq!(
        s.state.current.borrow().selection().anchor,
        11,
        "Ctrl+A does not select all on macOS"
    );

    press_chord(&s.app, &[Key::Control], "e");
    assert_eq!(focus(&s.state), 22, "Ctrl+E moves to line end on macOS");

    // Ctrl+K kills from caret to line end, without opening Command Palette
    at(&s, 14); // "sec|ond line"
    press_chord(&s.app, &[Key::Control], "k");
    assert!(
        !s.app.get_palette_open(),
        "Ctrl+K must not open palette on macOS"
    );
    assert_eq!(s.state.current.borrow().editor_text(), "first line\nsec");

    // Cmd+A still selects all on macOS
    press_chord(&s.app, &[Key::Meta], "a");
    assert_eq!(s.state.current.borrow().selection().anchor, 0);
    assert_eq!(focus(&s.state), 14, "Cmd+A still selects all on macOS");

    // Windows/Linux mode ("Ctrl"): Ctrl+A still selects all
    let win = launched("windows line");
    win.app.set_shortcut_modifier("Ctrl".into());
    at(&win, 5);
    press_chord(&win.app, &[Key::Control], "a");
    assert_eq!(win.state.current.borrow().selection().anchor, 0);
    assert_eq!(focus(&win.state), 12, "Ctrl+A selects all on Windows/Linux");
}
