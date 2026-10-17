//! Text size, weight, colour, fill and alignment of slide objects: chosen in the
//! inspector, drawn on the slide, undone as one step.
#![cfg(test)]

use std::cell::RefCell;
use std::rc::Rc;

use loom_desktop::{FileFilter, ScriptedFileDialogs};
use loom_present_core::{ElementType, SlideElement};
use slint::{ComponentHandle, Model};

use crate::{
    empty_session, refresh, text_element, DragState, ElementInk, GuiState, ObjectParts, PresentApp,
};

fn fixture(elements: Vec<SlideElement>) -> (PresentApp, Rc<GuiState>) {
    loom_test_support::capture::set_platform();
    let app = PresentApp::new().expect("create app");
    let mut session = empty_session();
    if let Some(slide) = session.document.active_slide_mut() {
        slide.elements = elements;
    }
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(session.document.clone()),
        last_saved_transitions: RefCell::default(),
        session: RefCell::new(session),
        pending_replacement: std::cell::Cell::new(None),
        selected_element: std::cell::Cell::new(0),
        inspector_available: std::cell::Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(ScriptedFileDialogs::new([], [])),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });
    crate::wire_app_callbacks(&app, &state);
    (app, state)
}

fn select(app: &PresentApp, state: &GuiState, id: &str) {
    state.session.borrow_mut().select_element(id, false);
    refresh(app, state);
}

fn style_of(state: &GuiState, index: usize) -> loom_present_core::ElementStyle {
    state
        .session
        .borrow()
        .document
        .active_slide()
        .unwrap()
        .elements[index]
        .style
        .clone()
}

fn parts_of(app: &PresentApp, index: usize) -> Vec<crate::ObjectPart> {
    let main = app.global::<ObjectParts>().get_main();
    main.row_data(index)
        .map(|row| row.iter().collect())
        .unwrap_or_default()
}

#[test]
fn a_larger_size_is_one_undoable_step_and_draws_the_text_at_that_size() {
    let (app, state) = fixture(vec![text_element(
        "t",
        ElementType::Title,
        "Quarterly plan",
        100.0,
        100.0,
        600.0,
        120.0,
    )]);
    select(&app, &state, "t");
    let ink = app.global::<ElementInk>();
    assert!(ink.get_available());
    assert_eq!(
        ink.get_size_label().as_str(),
        "27 pt",
        "a title starts at its type's size"
    );
    assert!(
        parts_of(&app, 0).is_empty(),
        "an unstyled title is drawn by the canvas itself"
    );

    ink.invoke_requested("size".into(), 1);
    assert_eq!(style_of(&state, 0).font_size_pt, Some(31.0));
    assert_eq!(ink.get_size_label().as_str(), "31 pt");
    let parts = parts_of(&app, 0);
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0].text.as_str(), "Quarterly plan");
    assert!(
        (parts[0].fs - 31.0 / 0.96).abs() < 0.01,
        "size in slide units: {}",
        parts[0].fs
    );
    assert!(
        parts[0].bold,
        "a title stays bold when only its size changed"
    );
    assert_eq!(parts[0].align, 1, "and stays centred");

    app.invoke_undo();
    assert!(style_of(&state, 0).is_default());
    assert!(
        parts_of(&app, 0).is_empty(),
        "one undo returns to the unstyled look"
    );
}

#[test]
fn bold_italic_alignment_and_colours_apply_to_every_selected_object() {
    let (app, state) = fixture(vec![
        text_element("a", ElementType::BodyText, "one", 50.0, 50.0, 300.0, 80.0),
        text_element("b", ElementType::Subtitle, "two", 50.0, 200.0, 300.0, 80.0),
    ]);
    state.session.borrow_mut().select_element("a", false);
    state.session.borrow_mut().select_element("b", true);
    refresh(&app, &state);
    let ink = app.global::<ElementInk>();

    ink.invoke_requested("bold".into(), 0);
    ink.invoke_requested("italic".into(), 0);
    ink.invoke_requested("align".into(), 2);
    ink.invoke_requested("color".into(), 3);
    ink.invoke_requested("fill".into(), 1);
    for index in 0..2 {
        let style = style_of(&state, index);
        assert_eq!(style.bold, Some(true), "object {index}");
        assert!(style.italic);
        assert_eq!(style.align, Some(loom_present_core::TextAlign::Right));
        assert_eq!(style.color.as_deref(), Some(loom_present_core::PALETTE[3]));
        assert_eq!(style.fill.as_deref(), Some(loom_present_core::PALETTE[1]));
        let parts = parts_of(&app, index);
        assert_eq!(parts.len(), 2, "a fill and the text");
        assert!(parts[0].has_bg && !parts[0].round);
        assert!(parts[1].bold && parts[1].italic && parts[1].align == 2);
        assert_eq!(
            parts[1].ink, 0,
            "an explicit colour replaces the type's ink"
        );
    }
    assert_eq!(ink.get_color_slot(), 3);
    assert_eq!(ink.get_fill_slot(), 1);

    ink.invoke_requested("color".into(), 7);
    ink.invoke_requested("fill".into(), 7);
    assert_eq!(style_of(&state, 0).color, None);
    assert_eq!(style_of(&state, 0).fill, None);
    assert_eq!(
        parts_of(&app, 1)[0].ink,
        2,
        "a subtitle goes back to its muted ink"
    );
}

#[test]
fn a_circle_fills_as_an_ellipse_and_a_picture_offers_no_text_controls() {
    let (app, state) = fixture(vec![
        text_element(
            "c",
            ElementType::ShapeCircle,
            "",
            100.0,
            100.0,
            200.0,
            200.0,
        ),
        SlideElement {
            id: "p".into(),
            element_type: ElementType::Picture,
            content: "asset".into(),
            x: 400.0,
            y: 100.0,
            width: 200.0,
            height: 200.0,
            rotation_deg: 0.0,
            action: None,
            style: Default::default(),
        },
    ]);
    select(&app, &state, "c");
    app.global::<ElementInk>()
        .invoke_requested("fill".into(), 2);
    let parts = parts_of(&app, 0);
    assert!(
        parts[0].has_bg && parts[0].round,
        "the fill follows the circle's outline"
    );

    select(&app, &state, "p");
    assert!(!app.global::<ElementInk>().get_available());
    app.global::<ElementInk>()
        .invoke_requested("bold".into(), 0);
    assert!(
        style_of(&state, 1).is_default(),
        "a picture takes no text style"
    );
}

#[test]
fn a_style_survives_saving_and_reopening_the_deck() {
    let (app, state) = fixture(vec![text_element(
        "t",
        ElementType::Title,
        "Saved",
        100.0,
        100.0,
        600.0,
        120.0,
    )]);
    select(&app, &state, "t");
    let ink = app.global::<ElementInk>();
    ink.invoke_requested("size".into(), 1);
    ink.invoke_requested("color".into(), 0);
    let bytes = loom_present_core::save_presentation_session(&state.session.borrow()).unwrap();
    let back = loom_present_core::load_presentation_session(&bytes).unwrap();
    let style = &back.document.active_slide().unwrap().elements[0].style;
    assert_eq!(style.font_size_pt, Some(31.0));
    assert_eq!(style.color.as_deref(), Some(loom_present_core::PALETTE[0]));
}

#[test]
fn a_style_change_rewrites_the_slide_thumbnail() {
    let (app, state) = fixture(vec![text_element(
        "t",
        ElementType::Title,
        "Thumb",
        100.0,
        100.0,
        600.0,
        120.0,
    )]);
    select(&app, &state, "t");
    let key = |app: &PresentApp| {
        app.get_slide_thumbs()
            .row_data(0)
            .map(|row| row.key.to_string())
    };
    let before = key(&app);
    app.global::<ElementInk>()
        .invoke_requested("fill".into(), 2);
    let after = key(&app);
    assert_ne!(before, after, "the thumbnail still shows the old look");
    let row = app.get_slide_thumbs().row_data(0).unwrap();
    let parts: Vec<_> = row
        .parts
        .row_data(0)
        .map(|p| p.iter().collect::<Vec<_>>())
        .unwrap_or_default();
    assert!(
        parts.iter().any(|p| p.has_bg),
        "the thumbnail draws the fill"
    );
}

#[test]
fn the_keyboard_and_the_palette_reach_the_same_style_controls() {
    let (app, state) = fixture(vec![text_element(
        "t",
        ElementType::BodyText,
        "Keys",
        100.0,
        100.0,
        600.0,
        120.0,
    )]);
    // With nothing selected, Ctrl+B is not taken, so it can still reach other handlers.
    let nothing = app.invoke_canvas_key_pressed("b".into(), false, true);
    assert!(matches!(
        nothing,
        slint::private_unstable_api::re_exports::EventResult::Reject
    ));

    select(&app, &state, "t");
    for (key, check) in [
        (
            "b",
            Box::new(|s: &loom_present_core::ElementStyle| s.bold == Some(true))
                as Box<dyn Fn(&loom_present_core::ElementStyle) -> bool>,
        ),
        ("i", Box::new(|s| s.italic)),
        ("]", Box::new(|s| s.font_size_pt == Some(18.0))),
        ("[", Box::new(|s| s.font_size_pt == Some(16.0))),
    ] {
        let taken = app.invoke_canvas_key_pressed(key.into(), false, true);
        assert!(
            matches!(
                taken,
                slint::private_unstable_api::re_exports::EventResult::Accept
            ),
            "Ctrl+{key} was not taken"
        );
        assert!(
            check(&style_of(&state, 0)),
            "Ctrl+{key} did not change the style"
        );
    }

    let ids: Vec<&str> = crate::master_palette(&app).iter().map(|c| c.id).collect();
    for id in [
        "present.text-bold",
        "present.text-italic",
        "present.text-larger",
        "present.text-smaller",
    ] {
        assert!(
            ids.contains(&id),
            "{id} is missing from the command palette"
        );
    }
}

fn geometry_of(state: &GuiState, index: usize) -> (f32, f32, f32, f32, f32) {
    let session = state.session.borrow();
    let e = &session.document.active_slide().unwrap().elements[index];
    (e.x, e.y, e.width, e.height, e.rotation_deg)
}

#[test]
fn typed_position_size_and_rotation_move_the_selected_object_as_undoable_steps() {
    let (app, state) = fixture(vec![text_element(
        "t",
        ElementType::BodyText,
        "Typed",
        100.0,
        120.0,
        400.0,
        80.0,
    )]);
    select(&app, &state, "t");
    let ink = app.global::<ElementInk>();
    assert_eq!(
        (
            ink.get_x_text().as_str(),
            ink.get_y_text().as_str(),
            ink.get_w_text().as_str(),
            ink.get_h_text().as_str()
        ),
        ("100", "120", "400", "80"),
        "the fields start with the object's real values"
    );

    ink.invoke_geometry_requested("x".into(), "250".into());
    ink.invoke_geometry_requested("h".into(), " 150 pt ".into());
    ink.invoke_geometry_requested("rot".into(), "30°".into());
    let (x, y, w, h, rot) = geometry_of(&state, 0);
    assert_eq!(
        (x, y, w, h),
        (250.0, 120.0, 400.0, 150.0),
        "units and spaces are accepted"
    );
    assert!((rot - 30.0).abs() < 0.01);
    assert_eq!(
        ink.get_x_text().as_str(),
        "250",
        "the fields follow the object"
    );
    assert_eq!(ink.get_rot_text().as_str(), "30");

    app.invoke_undo();
    assert!(
        (geometry_of(&state, 0).4).abs() < 0.01,
        "one undo takes back the rotation only"
    );
    app.invoke_undo();
    assert_eq!(geometry_of(&state, 0).3, 80.0, "and one more the height");
}

#[test]
fn an_impossible_or_unreadable_entry_changes_nothing_and_the_field_shows_the_real_value() {
    let (app, state) = fixture(vec![text_element(
        "t",
        ElementType::BodyText,
        "Typed",
        100.0,
        120.0,
        400.0,
        80.0,
    )]);
    select(&app, &state, "t");
    let ink = app.global::<ElementInk>();

    ink.invoke_geometry_requested("x".into(), "abc".into());
    assert_eq!(
        geometry_of(&state, 0).0,
        100.0,
        "text that is not a number does nothing"
    );
    assert!(
        app.get_status_left().contains("not a number"),
        "{}",
        app.get_status_left()
    );

    // A position past the right edge is held on the slide, and the field says where it landed.
    ink.invoke_geometry_requested("x".into(), "5000".into());
    assert_eq!(
        geometry_of(&state, 0).0,
        600.0,
        "1000 wide slide minus the 400 wide object"
    );
    assert_eq!(ink.get_x_text().as_str(), "600");

    // Escape (an empty entry) only restores the field.
    ink.set_y_text("999".into());
    ink.invoke_geometry_requested("y".into(), "".into());
    assert_eq!(ink.get_y_text().as_str(), "120");
    assert_eq!(geometry_of(&state, 0).1, 120.0);
}
