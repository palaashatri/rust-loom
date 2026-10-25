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

fn press_ctrl(app: &WriterApp, key: Key) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(Key::Control),
    });
    press(app, key);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(Key::Control),
    });
}

#[test]
fn ctrl_end_then_ctrl_home_returns_the_caret_and_the_page_to_the_top() {
    let text = (1..=120)
        .map(|n| format!("Paragraph {n} with enough words to wrap across a line of the page."))
        .collect::<Vec<_>>()
        .join("\n");
    let s = launched(&text);
    press_ctrl(&s.app, Key::End);
    let end = s.state.current.borrow().editor_text().len();
    assert_eq!(focus(&s.state), end, "Ctrl+End reaches the end of the text");
    assert!(
        s.state.viewport.borrow().scroll_y > 0.0,
        "and the page scrolls down to it"
    );

    press_ctrl(&s.app, Key::Home);
    assert_eq!(focus(&s.state), 0, "Ctrl+Home returns to the start");
    assert_eq!(
        s.state.viewport.borrow().scroll_y,
        0.0,
        "and the page returns to the top margin"
    );
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
