use super::*;
use crate::command_dispatch::dispatch_command;

#[test]
fn rich_text_and_arrange_commands_mutate_elements_and_undo_redo() {
    let el1 = text_element(
        "e1",
        ElementType::BodyText,
        "First",
        40.0,
        40.0,
        200.0,
        40.0,
    );
    let el2 = text_element(
        "e2",
        ElementType::BodyText,
        "Second",
        40.0,
        100.0,
        200.0,
        40.0,
    );
    let (app, state) = crate::element_style_tests::fixture(vec![el1, el2]);
    state.session.borrow_mut().select_element("e1", false);
    crate::refresh(&app, &state);

    assert!(dispatch_command(&app, "format.bold"));
    assert!(dispatch_command(&app, "format.italic"));
    assert!(dispatch_command(&app, "format.underline"));
    assert!(dispatch_command(&app, "format.font_increase"));
    {
        let s = state.session.borrow();
        let el = &s.document.slides[0].elements[0];
        assert!(el.style.bold == Some(true) || el.runs.first().and_then(|r| r.bold) == Some(true));
        assert!(el.style.italic || el.runs.first().and_then(|r| r.italic) == Some(true));
        assert!(el.style.underline || el.runs.first().and_then(|r| r.underline) == Some(true));
    }

    // Bring e1 to front (moves from index 0 to index 1)
    assert!(dispatch_command(&app, "arrange.bring_front"));
    assert_eq!(
        state.session.borrow().document.slides[0].elements[1].id,
        "e1"
    );

    // Undo z-order, then redo
    assert!(state.session.borrow_mut().undo());
    assert_eq!(
        state.session.borrow().document.slides[0].elements[0].id,
        "e1"
    );
    assert!(state.session.borrow_mut().redo());
    assert_eq!(
        state.session.borrow().document.slides[0].elements[1].id,
        "e1"
    );
}
