//! Deck replacement, slide selection, Enter/F2 on a selected object, the blank
//! deck's layout and the Save changes wording. Every replacement entry point is
//! driven the way a user reaches it (key chords, the File menu, the palette's
//! callbacks, the template chooser and the window close button), so a path that
//! skips the Save / Discard / Cancel question fails here.

use super::keyboard_flow_tests::{chord, launched, launched_with, press, render, tap, Session};
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_present_core::ElementType;
use slint::platform::{Key, WindowEvent};

/// One way a user can replace the deck: its name for failures, and the action.
type Replacement = (&'static str, fn(&Session));

fn slide_count(s: &Session) -> usize {
    s.state.session.borrow().document.slides.len()
}

/// A deck the user has changed: exactly two slides and no save yet.
fn two_slide_deck() -> Session {
    let s = launched();
    while slide_count(&s) > 2 {
        s.app.invoke_delete_slide();
    }
    while slide_count(&s) < 2 {
        s.app.invoke_add_slide();
    }
    render(&s.app);
    assert!(
        s.app.get_deck_dirty(),
        "the test deck starts with unsaved work"
    );
    s
}

fn editing(s: &Session) -> bool {
    ElementHandle::find_by_accessible_label(&s.app, "Edit text")
        .next()
        .is_some()
}

#[test]
fn every_way_of_replacing_a_dirty_deck_asks_first() {
    let entries: [Replacement; 8] = [
        ("Ctrl+N", |s| chord(&s.app, &[Key::Control], "n")),
        ("Alt+F then Enter on New", |s| {
            // The menu hands the highlighted command to the app; the app's own
            // handler then runs it through the same routing as the menu bar.
            chord(&s.app, &[Key::Alt], "f");
            press(&s.app, Key::Return);
            let command = s.actions.borrow().last().cloned().unwrap_or_default();
            assert_eq!(command, "file.new", "Enter on the File menu runs New");
            dispatch_command(&s.app, &command);
        }),
        ("File > New from Sample Deck", |s| {
            dispatch_command(&s.app, "file.new_sample");
        }),
        ("Ctrl+O", |s| chord(&s.app, &[Key::Control], "o")),
        ("File > Open", |s| {
            dispatch_command(&s.app, "file.open");
        }),
        ("New from template", |s| s.app.invoke_create_theme(1)),
        ("Palette New Presentation", |s| s.app.invoke_new_deck()),
        ("Closing the window", |s| {
            s.app.window().dispatch_event(WindowEvent::CloseRequested);
        }),
    ];
    for (name, act) in entries {
        let s = two_slide_deck();
        act(&s);
        render(&s.app);
        assert!(
            s.app.get_save_changes_open(),
            "{name} must ask Save, Discard or Cancel before it replaces a dirty deck"
        );
        assert_eq!(
            slide_count(&s),
            2,
            "{name} changed the slides before the answer"
        );
        assert!(s.app.get_deck_dirty(), "{name} cleared the unsaved marker");
        assert!(
            s.app.get_can_undo(),
            "{name} cleared undo before the answer"
        );
        press(&s.app, Key::Escape);
        assert!(
            !s.app.get_save_changes_open(),
            "{name}: Escape cancels the question"
        );
        assert_eq!(slide_count(&s), 2, "{name} lost slides after Cancel");
        assert!(
            s.app.get_deck_dirty(),
            "{name} lost the unsaved marker after Cancel"
        );
    }
}

#[test]
fn closing_while_a_replacement_question_is_open_asks_the_close_question() {
    let s = two_slide_deck();
    s.app.invoke_new_deck();
    render(&s.app);
    assert!(
        !s.app.get_save_changes_closing(),
        "New asks about replacing"
    );
    // The window is closing now, so the question must say so, and Discard would close it.
    s.app.window().dispatch_event(WindowEvent::CloseRequested);
    render(&s.app);
    assert!(
        s.app.get_save_changes_closing(),
        "the later close request replaces the question"
    );
    assert_eq!(
        s.app.get_status_left().as_str(),
        "Unsaved changes — choose Save and close, Discard, or Cancel"
    );
    assert_eq!(
        slide_count(&s),
        2,
        "nothing was replaced while the question is open"
    );
}

#[test]
fn the_status_names_the_same_button_the_dialog_shows() {
    let s = two_slide_deck();
    s.app.invoke_new_deck();
    render(&s.app);
    assert_eq!(
        s.app.get_status_left().as_str(),
        "Unsaved changes — choose Save, Discard, or Cancel",
        "replacing the deck offers a plain Save"
    );
    press(&s.app, Key::Escape);
    s.app.window().dispatch_event(WindowEvent::CloseRequested);
    render(&s.app);
    assert_eq!(
        s.app.get_status_left().as_str(),
        "Unsaved changes — choose Save and close, Discard, or Cancel",
        "closing the window offers Save and close, as the button says"
    );
}

#[test]
fn moving_between_slides_does_not_mark_a_saved_deck_edited() {
    let dir = std::env::temp_dir().join(format!("loom-present-select-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("selection.loomdeck");
    let s = launched_with(Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(path.clone())],
    )));
    while slide_count(&s) < 2 {
        s.app.invoke_add_slide();
    }
    s.app.invoke_save_deck();
    render(&s.app);
    assert!(!s.app.get_deck_dirty(), "a saved deck starts clean");

    s.app.invoke_select_slide(1);
    render(&s.app);
    assert!(!s.app.get_deck_dirty(), "selecting a slide is not an edit");
    press(&s.app, Key::PageUp);
    render(&s.app);
    assert!(
        !s.app.get_deck_dirty(),
        "moving back with the keyboard is not an edit"
    );

    s.app.invoke_add_slide();
    render(&s.app);
    assert!(s.app.get_deck_dirty(), "adding a slide is an edit");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn enter_and_f2_edit_the_selected_object_again_after_escape() {
    let s = launched();
    press(&s.app, Key::Return);
    assert!(editing(&s), "Enter starts editing the selected title");
    tap(&s.app, "H");
    tap(&s.app, "i");
    press(&s.app, Key::Escape);
    render(&s.app);
    assert!(!editing(&s), "Escape finishes editing");
    assert!(
        focus_name_of(&s).starts_with("Selected Title: Hi"),
        "the selected title keeps keyboard focus after Escape, got {:?}",
        focus_name_of(&s)
    );

    press(&s.app, Key::Return);
    assert!(editing(&s), "Enter after Escape starts editing again");
    press(&s.app, Key::Escape);
    press(&s.app, Key::F2);
    assert!(editing(&s), "F2 starts editing as well");
    press(&s.app, Key::Escape);
}

/// The accessible name of whatever has keyboard focus.
fn focus_name_of(s: &Session) -> String {
    super::keyboard_flow_tests::focus_name(&s.app)
}

#[test]
fn a_typed_letter_with_an_object_selected_says_how_to_edit_it() {
    let s = launched();
    let before = s.state.session.borrow().document.slides[0].elements.clone();
    tap(&s.app, "x");
    render(&s.app);
    assert_eq!(
        s.app.get_status_left().as_str(),
        "Press Enter or F2 to edit the selected text",
        "a typed letter is explained, not dropped silently"
    );
    assert!(!editing(&s), "typing does not start editing by itself");
    assert_eq!(
        s.state.session.borrow().document.slides[0].elements,
        before,
        "the typed letter changes nothing"
    );
    press(&s.app, Key::Return);
    assert!(editing(&s), "the hint is true: Enter starts editing");
}

#[test]
fn a_blank_deck_has_the_placeholders_of_the_layout_its_control_names() {
    let s = launched();
    s.app.invoke_new_deck();
    render(&s.app);
    let (layout, kinds) = {
        let session = s.state.session.borrow();
        let slide = &session.document.slides[0];
        let kinds: Vec<ElementType> = slide
            .elements
            .iter()
            .map(|e| e.element_type.clone())
            .collect();
        (slide.layout.clone(), kinds)
    };
    assert_eq!(
        s.app.get_active_template_index(),
        slide_layouts::choice_for_layout(&layout),
        "the layout control names the blank slide's layout ({layout})"
    );
    assert!(
        kinds.contains(&ElementType::Title),
        "the blank slide has the title placeholder its control names: {kinds:?}"
    );

    let expected = [
        (0, ElementType::Subtitle),
        (1, ElementType::BodyText),
        (2, ElementType::BodyText),
        (3, ElementType::BodyText),
    ];
    for (choice, placeholder) in expected {
        s.app.invoke_apply_template(choice);
        render(&s.app);
        assert_eq!(
            s.app.get_active_template_index(),
            choice,
            "the control shows the chosen layout"
        );
        let session = s.state.session.borrow();
        let slide = &session.document.slides[0];
        assert!(
            slide.elements.iter().any(|e| e.element_type == placeholder),
            "layout {choice} has its {placeholder:?} placeholder: {:?}",
            slide
                .elements
                .iter()
                .map(|e| e.element_type.clone())
                .collect::<Vec<_>>()
        );
    }
}
