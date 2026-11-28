//! Behavioral tests for PR Wave 9 Creator Studio features:
//! 1. 8-handle object selection (`nw/n/ne/e/se/s/sw/w` + stem rotate, 24x24 px targets,
//!    single-axis edge resizing with min-size clamping & undo),
//! 2. Keynote-style Format inspector `Style | Text | Arrange` sub-tabs,
//! 3. True object-level Magic Move (`TransitionKind::Morph`) slideshow playback + reduced-motion,
//! 4. Thumbnail navigator active badge + dynamic background & shape rendering.

use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_desktop::ScriptedFileDialogs;
use loom_present_core::{DynamicBackground, DynamicBgPreset, Slide};
use loom_test_support::capture::snapshot_component;
use slint::Model;

fn launched(session: PresentationSession) -> (PresentApp, Rc<GuiState>) {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(GuiState {
        session: RefCell::new(session.clone()),
        last_saved: RefCell::new(session.document),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(ScriptedFileDialogs::default()),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    configure_responsive_width(&app, 1280);
    wire_app_callbacks(&app, &state);
    wire_palette(&app);
    refresh_without_recovery(&app, &state);
    (app, state)
}

#[test]
fn eight_handle_edge_resizing_clamps_minimum_size_and_undoes_cleanly() {
    let mut doc = PresentationDocument::new("edge-test", "Edge Test");
    doc.slides[0].elements = vec![text_element(
        "box-1",
        ElementType::ShapeRectangle,
        "Card",
        200.0,
        150.0,
        240.0,
        120.0,
    )];
    let mut session = PresentationSession::new(doc);
    session.selected_elements = vec!["box-1".to_string()];
    let (app, state) = launched(session);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");

    // All 9 handles (8 resize + 1 rotate) meet the 24x24 px GOV-06 touch-target floor.
    let handles: Vec<_> = ElementHandle::find_by_element_type_name(&app, "ResizeHandle").collect();
    assert_eq!(handles.len(), 9, "expected 8 resize + 1 rotate handles");
    for h in &handles {
        let size = h.size();
        assert!(
            size.width >= 24.0 && size.height >= 24.0,
            "handle touch target must be >= 24x24 px, got {size:?}"
        );
    }

    let rect = |st: &GuiState| {
        let s = st.session.borrow();
        let e = &s.document.slides[0].elements[0];
        (e.x, e.y, e.width, e.height)
    };

    // East edge grows width ONLY (x, y, height unchanged).
    app.invoke_handle_pressed(0, "e".into(), false);
    app.invoke_handle_moved(0, "e".into(), 50.0, 40.0);
    app.invoke_handle_released(0, "e".into());
    assert_eq!(rect(&state), (200.0, 150.0, 290.0, 120.0));
    app.invoke_undo();
    assert_eq!(rect(&state), (200.0, 150.0, 240.0, 120.0));

    // South edge grows height ONLY (x, y, width unchanged).
    app.invoke_handle_pressed(0, "s".into(), false);
    app.invoke_handle_moved(0, "s".into(), 35.0, 30.0);
    app.invoke_handle_released(0, "s".into());
    assert_eq!(rect(&state), (200.0, 150.0, 240.0, 150.0));
    app.invoke_undo();
    assert_eq!(rect(&state), (200.0, 150.0, 240.0, 120.0));

    // West edge shrinks with min-size clamp (20.0) keeping right edge pinned at 440.0.
    app.invoke_handle_pressed(0, "w".into(), false);
    app.invoke_handle_moved(0, "w".into(), 500.0, 0.0);
    app.invoke_handle_released(0, "w".into());
    let (x, y, w, h) = rect(&state);
    assert_eq!((y, h), (150.0, 120.0));
    assert_eq!(w, 20.0);
    assert_eq!(x, 420.0);
    app.invoke_undo();
    assert_eq!(rect(&state), (200.0, 150.0, 240.0, 120.0));

    // North edge shrinks with min-size clamp (20.0) keeping bottom edge pinned at 270.0.
    app.invoke_handle_pressed(0, "n".into(), false);
    app.invoke_handle_moved(0, "n".into(), 0.0, 400.0);
    app.invoke_handle_released(0, "n".into());
    let (x, y, w, h) = rect(&state);
    assert_eq!((x, w), (200.0, 240.0));
    assert_eq!(h, 20.0);
    assert_eq!(y, 250.0);
    app.invoke_undo();
    assert_eq!(rect(&state), (200.0, 150.0, 240.0, 120.0));
}

#[test]
fn format_inspector_style_text_arrange_sub_tabs_switch_cleanly_and_keep_mutations_working() {
    let (app, state) = launched(sample_session());
    app.invoke_add_text();
    app.set_show_inspector(true);
    app.set_inspector_tab(0);
    let ink = app.global::<ElementInk>();

    // Sub-tab bar exposes Style | Text | Arrange.
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    for label in ["Style", "Text", "Arrange"] {
        assert!(
            ElementHandle::find_by_accessible_label(&app, label)
                .next()
                .is_some(),
            "Format sub-tab {label:?} must be present"
        );
    }

    // Text (1) shows Typography & B/I/U/S, hides Shape Fill and Geometry.
    ink.set_format_sub_tab(1);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render text");
    assert!(ElementHandle::find_by_accessible_label(&app, "Font family")
        .next()
        .is_some());
    assert!(ElementHandle::find_by_accessible_label(&app, "X position")
        .next()
        .is_none());

    // Style (0) shows Shape Fill, hides Font family and Geometry; programmatic geometry still works!
    ink.set_format_sub_tab(0);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render style");
    assert!(ElementHandle::find_by_accessible_label(&app, "Shape Fill")
        .next()
        .is_some());
    assert!(ElementHandle::find_by_accessible_label(&app, "Font family")
        .next()
        .is_none());
    ink.invoke_geometry_requested("x".into(), "333".into());
    {
        let s = state.session.borrow();
        let active_id = s.selected_elements[0].clone();
        let el = s
            .document
            .active_slide()
            .unwrap()
            .elements
            .iter()
            .find(|e| e.id == active_id)
            .unwrap();
        assert_eq!(
            el.x, 333.0,
            "programmatic mutations work regardless of active sub-tab"
        );
    }

    // Arrange (2) shows Geometry inputs (X/Y/W/H/Rot) and hides Font family.
    ink.set_format_sub_tab(2);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render arrange");
    for label in ["X position", "Y position", "Width", "Height", "Rotation"] {
        assert!(
            ElementHandle::find_by_accessible_label(&app, label)
                .next()
                .is_some(),
            "{label} must be visible in Format -> Arrange"
        );
    }
    assert!(ElementHandle::find_by_accessible_label(&app, "Font family")
        .next()
        .is_none());
}

#[test]
fn magic_move_interpolates_matching_objects_in_slideshow_and_respects_reduced_motion() {
    let mut doc = PresentationDocument::new("morph-show", "Morph Show");
    doc.slides[0].elements = vec![
        text_element(
            "hero",
            ElementType::ShapeRectangle,
            "Hero",
            100.0,
            100.0,
            160.0,
            80.0,
        ),
        text_element(
            "only1",
            ElementType::BodyText,
            "Exiting",
            100.0,
            300.0,
            200.0,
            60.0,
        ),
    ];
    let mut slide2 = Slide::new("slide-2", "Slide 2", "content");
    slide2.elements = vec![
        text_element(
            "hero",
            ElementType::ShapeRectangle,
            "Hero",
            500.0,
            260.0,
            320.0,
            160.0,
        ),
        text_element(
            "only2",
            ElementType::BodyText,
            "Entering",
            600.0,
            100.0,
            200.0,
            60.0,
        ),
    ];
    doc.slides.push(slide2);
    let mut session = PresentationSession::new(doc);
    session.set_transition("slide-2", TransitionKind::Morph);

    let (app, state) = launched(session);
    app.set_is_preview_mode(true);
    app.invoke_next_slide();

    assert!(
        morph_playback::is_running(),
        "Magic Move timer must start on slideshow advance to Morph slide"
    );

    // At t = 0.5, matched "hero" sits halfway between (100,100,160,80) and (500,260,320,160).
    assert!(morph_playback::step_to_for_test(&app, &state, 0.5));
    let xs = app.get_element_xs();
    let ys = app.get_element_ys();
    let ws = app.get_element_widths();
    let hs = app.get_element_heights();
    let contents = app.get_element_contents();
    let metas = app.global::<ObjectParts>().get_element_meta();
    let hero_idx = (0..contents.row_count())
        .find(|&i| contents.row_data(i).as_deref() == Some("Hero"))
        .expect("Hero present in morph frame");
    assert!((xs.row_data(hero_idx).unwrap() - 300.0).abs() < 1.0);
    assert!((ys.row_data(hero_idx).unwrap() - 180.0).abs() < 1.0);
    assert!((ws.row_data(hero_idx).unwrap() - 240.0).abs() < 1.0);
    assert!((hs.row_data(hero_idx).unwrap() - 120.0).abs() < 1.0);

    let exit_idx = (0..contents.row_count())
        .find(|&i| contents.row_data(i).as_deref() == Some("Exiting"))
        .expect("Exiting element fades out");
    let enter_idx = (0..contents.row_count())
        .find(|&i| contents.row_data(i).as_deref() == Some("Entering"))
        .expect("Entering element fades in");
    assert!((metas.row_data(exit_idx).unwrap().opacity - 0.5).abs() < 0.05);
    assert!((metas.row_data(enter_idx).unwrap().opacity - 0.5).abs() < 0.05);

    // With reduced-motion enabled, Magic Move snaps cleanly without starting a timer.
    morph_playback::cancel();
    app.invoke_prev_slide();
    let theme = Theme::get(&app);
    let mut tokens = theme.get_tokens();
    tokens.reduced_motion = true;
    theme.set_tokens(tokens);
    app.invoke_next_slide();
    assert!(
        !morph_playback::is_running(),
        "reduced-motion must disable Magic Move playback timer"
    );
    assert!((app.get_transition_progress() - 1.0).abs() < f32::EPSILON);
}

#[test]
fn thumbnails_carry_dynamic_backgrounds_shapes_and_active_badge() {
    let mut doc = PresentationDocument::new("thumb-polish", "Thumb Polish");
    doc.slides[0].dynamic_bg = Some(DynamicBackground::new(DynamicBgPreset::AuroraVeil));
    doc.slides[0].elements.push(text_element(
        "pill",
        ElementType::ShapeRoundedRect,
        "Badge",
        100.0,
        120.0,
        240.0,
        80.0,
    ));
    let slide_title = doc.slides[0].title.clone();
    let (app, _state) = launched(PresentationSession::new(doc));
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let thumb = app
        .get_slide_thumbs()
        .row_data(0)
        .expect("first slide thumbnail");
    assert!(thumb.has_dynamic_bg);
    assert!(thumb.dynamic_bg_image.size().width > 0);
    assert!(thumb.types.row_count() >= 1);
    let expected_label = format!("Slide 1 of 1, {slide_title}");
    assert!(
        ElementHandle::find_by_accessible_label(&app, &expected_label)
            .any(|e| e.accessible_role() == Some(AccessibleRole::Button)),
        "active slide thumbnail remains a button ({expected_label:?})"
    );
    dynamic_bg_view::stop_timer();
}
