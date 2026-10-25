//! Left and Right inside right-to-left text, through real window key events.
//! Left-to-right text keeps the input's own Left and Right.

use crate::keyboard_flow_tests::launched;
use crate::*;
use slint::platform::{Key, WindowEvent};

fn press(app: &WriterApp, key: Key) {
    let text: SharedString = key.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn focus(state: &GuiState) -> usize {
    state.current.borrow().selection().focus
}

#[test]
fn right_and_left_move_visually_through_hebrew_text() {
    // Alef then bet: bet is drawn on the left, so the line's end (offset 4) is
    // on the left and Right moves over bet to alef's boundary.
    let s = launched("\u{5D0}\u{5D1}");
    s.app.invoke_focus_page();
    s.app.invoke_selection_changed(4, 4);
    press(&s.app, Key::RightArrow);
    assert_eq!(focus(&s.state), 2, "Right moves over bet");
    press(&s.app, Key::RightArrow);
    assert_eq!(focus(&s.state), 0, "Right moves over alef");
    press(&s.app, Key::LeftArrow);
    assert_eq!(focus(&s.state), 2, "Left moves back over alef");
}

#[test]
fn left_to_right_text_keeps_the_logical_arrow_moves() {
    let s = launched("abc");
    s.app.invoke_focus_page();
    s.app.invoke_selection_changed(1, 1);
    press(&s.app, Key::RightArrow);
    assert_eq!(focus(&s.state), 2);
    press(&s.app, Key::LeftArrow);
    assert_eq!(focus(&s.state), 1);
}
