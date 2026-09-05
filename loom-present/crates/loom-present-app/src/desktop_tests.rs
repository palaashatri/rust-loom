use super::*;
use loom_desktop::{CommandSource, ScriptedFileDialogs};

fn test_state() -> GuiState {
    GuiState {
        session: RefCell::new(empty_session()),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(Some(PathBuf::from("projects/demo.loomdeck"))),
        dialogs: Rc::new(ScriptedFileDialogs::default()),
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    }
}

#[test]
fn new_presentation_is_blank_and_single_slide() {
    let session = empty_session();
    assert_eq!(session.document.len(), 1);
    assert_eq!(session.document.title, "Untitled Presentation");
    assert!(session.document.slides[0]
        .elements
        .iter()
        .all(|element| element.content.is_empty()));
}

#[test]
fn optional_inspector_and_notes_drawer_are_closed_by_default() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    assert!(!app.get_show_inspector());
    assert!(!app.get_show_notes_drawer());
}

#[test]
fn refresh_projects_selected_element_into_canvas_and_inspector() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = GuiState {
        session: RefCell::new(sample_session()),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(ScriptedFileDialogs::default()),
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    };

    state
        .session
        .borrow_mut()
        .select_element("cover-title", false);
    refresh(&app, &state);
    assert_eq!(app.get_active_element_label().as_str(), "Title");
    assert_eq!(
        app.get_active_element_content().as_str(),
        "Create without compromise"
    );
    assert_eq!(app.get_element_x_text().as_str(), "90 pt");
    assert_eq!(app.get_element_width_text().as_str(), "820 pt");
    assert_eq!(app.get_element_contents().row_count(), 2);
    assert_eq!(app.get_element_types().row_count(), 2);

    state
        .session
        .borrow_mut()
        .select_element("cover-body", false);
    refresh(&app, &state);
    assert_eq!(app.get_active_element_label().as_str(), "BodyText");
    assert_eq!(app.get_selection_count(), 1);
    assert_eq!(
        app.get_active_element_content().as_str(),
        "A private, native creative studio designed for Linux."
    );
    assert_eq!(app.get_element_y_text().as_str(), "230 pt");
    assert_eq!(app.get_element_height_text().as_str(), "120 pt");
}

#[test]
fn refresh_clears_inspector_when_domain_selection_is_empty() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = GuiState {
        session: RefCell::new(sample_session()),
        selected_element: Cell::new(1),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(ScriptedFileDialogs::default()),
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    };

    state
        .session
        .borrow_mut()
        .select_element("cover-body", false);
    refresh(&app, &state);
    assert_eq!(app.get_active_element_label().as_str(), "BodyText");

    state.session.borrow_mut().clear_selection();
    refresh(&app, &state);
    assert_eq!(
        app.get_active_element_label().as_str(),
        "No element selected"
    );
    assert_eq!(app.get_selection_count(), 0);
    assert_eq!(app.get_active_element_content().as_str(), "");
    assert_eq!(app.get_element_x_text().as_str(), "—");
}

#[test]
fn focused_canvas_arrow_key_nudges_selected_element() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    state.session.borrow_mut().select_element("elem-1", false);
    wire_app_callbacks(&app, &state);

    let before = state
        .session
        .borrow()
        .document
        .active_slide()
        .expect("slide")
        .elements[0]
        .x;
    app.invoke_canvas_key_pressed(slint::platform::Key::RightArrow.into(), false, false);
    let after = state
        .session
        .borrow()
        .document
        .active_slide()
        .expect("slide")
        .elements[0]
        .x;
    assert!((after - before - 10.0).abs() < f32::EPSILON);
}

#[test]
fn shift_marquee_adds_to_existing_domain_selection() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    {
        let mut session = state.session.borrow_mut();
        session
            .document
            .active_slide_mut()
            .expect("slide")
            .elements
            .push(SlideElement {
                id: "elem-2".into(),
                element_type: ElementType::ShapeRectangle,
                content: "Second".into(),
                x: 300.0,
                y: 300.0,
                width: 50.0,
                height: 50.0,
                rotation_deg: 0.0,
                action: None,
            });
        session.select_element("elem-1", false);
    }
    wire_app_callbacks(&app, &state);

    app.invoke_canvas_pressed(280.0, 280.0, true);
    app.invoke_canvas_moved(380.0, 380.0);
    app.invoke_canvas_released(380.0, 380.0);

    assert_eq!(
        state.session.borrow().selected_elements,
        vec!["elem-1".to_string(), "elem-2".to_string()]
    );
    assert_eq!(app.get_selection_count(), 2);
}

#[test]
fn cancelled_pointer_gestures_restore_geometry_selection_and_history() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    state.session.borrow_mut().select_element("elem-1", false);
    state.selected_element.set(0);
    wire_app_callbacks(&app, &state);

    let before = state.session.borrow().document.clone();
    app.invoke_element_pressed(0, false);
    app.invoke_element_moved(0, 20.0, 30.0);
    assert_ne!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
    assert!(state.session.borrow().can_undo());
    app.invoke_element_cancelled(0);
    assert_eq!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
    assert_eq!(state.session.borrow().selected_elements, ["elem-1"]);
    assert!(!state.session.borrow().can_undo());

    app.invoke_canvas_pressed(0.0, 0.0, false);
    app.invoke_canvas_moved(80.0, 80.0);
    assert!(state.session.borrow().selected_elements.is_empty());
    app.invoke_canvas_cancelled();
    assert_eq!(state.session.borrow().selected_elements, ["elem-1"]);
    assert!(!state.session.borrow().can_undo());

    let before = state.session.borrow().document.clone();
    app.invoke_handle_pressed(0, "se".into(), false);
    app.invoke_handle_moved(0, "se".into(), 20.0, 15.0);
    assert_ne!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
    app.invoke_handle_cancelled(0, "se".into());
    assert_eq!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
    assert!(!state.session.borrow().can_undo());

    let before = state.session.borrow().document.clone();
    app.invoke_handle_pressed(0, "rotate".into(), false);
    app.invoke_handle_moved(0, "rotate".into(), 30.0, 0.0);
    assert_ne!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
    app.invoke_handle_cancelled(0, "rotate".into());
    assert_eq!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
    assert!(!state.session.borrow().can_undo());
}

#[test]
fn no_op_rotation_gesture_does_not_leave_history() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    state.session.borrow_mut().select_element("elem-1", false);
    wire_app_callbacks(&app, &state);

    let before = state.session.borrow().document.clone();
    app.invoke_handle_pressed(0, "rotate".into(), false);
    app.invoke_handle_moved(0, "rotate".into(), 0.0, 0.0);
    app.invoke_handle_released(0, "rotate".into());

    assert_eq!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
    assert!(!state.session.borrow().can_undo());
}

#[test]
fn preview_mode_rejects_pointer_edits_and_modified_canvas_arrows() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    state.session.borrow_mut().select_element("elem-1", false);
    wire_app_callbacks(&app, &state);

    let before = state.session.borrow().document.clone();
    let result =
        app.invoke_canvas_key_pressed(slint::platform::Key::RightArrow.into(), false, true);
    assert!(matches!(result, EventResult::Reject));
    assert_eq!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );

    app.set_is_preview_mode(true);
    app.invoke_element_pressed(0, false);
    app.invoke_element_moved(0, 20.0, 30.0);
    app.invoke_handle_pressed(0, "se".into(), false);
    app.invoke_handle_moved(0, "se".into(), 20.0, 15.0);
    app.invoke_canvas_pressed(0.0, 0.0, false);
    app.invoke_canvas_moved(100.0, 100.0);
    app.invoke_canvas_released(100.0, 100.0);
    assert_eq!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
    assert!(!state.session.borrow().can_undo());

    let result =
        app.invoke_canvas_key_pressed(slint::platform::Key::RightArrow.into(), false, false);
    assert!(matches!(result, EventResult::Reject));
    assert_eq!(
        state.session.borrow().document.integrity_digest(),
        before.integrity_digest()
    );
}

#[test]
fn empty_slide_keeps_inspector_truthful() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = test_state();
    state
        .session
        .borrow_mut()
        .document
        .active_slide_mut()
        .expect("empty session slide")
        .elements
        .clear();

    configure_responsive_width(&app, 1280);
    refresh(&app, &state);

    assert_eq!(
        app.get_active_element_label().as_str(),
        "No element selected"
    );
    assert_eq!(app.get_element_contents().row_count(), 0);
    assert_eq!(app.get_element_x_text().as_str(), "—");
    let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render empty slide");
    assert_eq!((image.width(), image.height()), (1280, 800));
}

#[test]
fn compact_stage_render_is_safe_for_short_windows() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = GuiState {
        session: RefCell::new(sample_session()),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(ScriptedFileDialogs::default()),
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    };
    configure_responsive_width(&app, 900);
    refresh(&app, &state);
    let image = snapshot_component(&app, 900.0, 480.0, 1.0).expect("render short window");
    assert_eq!(image.width(), 900);
    assert_eq!(image.height(), 480);
}

#[test]
fn expanding_past_overflow_breakpoint_closes_menu() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    configure_responsive_width(&app, 1024);
    assert!(app.get_overflow_toolbar());
    app.set_toolbar_overflow_open(true);

    configure_responsive_width(&app, 1320);

    assert!(!app.get_overflow_toolbar());
    assert!(!app.get_toolbar_overflow_open());
}

#[test]
fn responsive_policy_transition_probes_are_exact() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let expected = [
        (1179, true, true, false),
        (1180, false, true, false),
        (1279, false, true, false),
        (1280, false, true, false),
        (1319, false, true, false),
        (1320, false, false, true),
    ];
    for (width, icon_only, overflow, labeled) in expected {
        assert_eq!(
            responsive_toolbar_state(&app, width),
            ResponsiveToolbarState {
                icon_only,
                overflow,
                labeled,
            }
        );
        configure_responsive_width(&app, width);
        assert_eq!(app.get_icon_only_toolbar(), icon_only);
        assert_eq!(app.get_overflow_toolbar(), overflow);
        assert_eq!(app.get_labeled_toolbar(), labeled);
    }
}

#[test]
fn widening_window_preserves_palette_focus() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    configure_responsive_width(&app, 1024);
    wire_responsive_layout(&app);
    let _ = snapshot_component(&app, 1024.0, 800.0, 1.0).expect("render compact window");

    app.invoke_open_palette();
    let _ = snapshot_component(&app, 1024.0, 800.0, 1.0).expect("render open palette");
    let focused_before =
        slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
            .focus_item
            .borrow()
            .upgrade()
            .expect("palette should own focus");

    app.window().set_size(PhysicalSize::new(1280, 800));
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render widened window");
    let focused_after =
        slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
            .focus_item
            .borrow()
            .upgrade()
            .expect("palette focus should remain present");

    assert_eq!(focused_after, focused_before);
    assert!(app.get_palette_open());
}

#[test]
fn dialog_requests_use_current_directory_and_expected_extensions() {
    let state = test_state();
    let open = open_request(&state);
    let save = save_request(&state);
    let export = export_request(&state);

    assert_eq!(open.initial_directory, Some(PathBuf::from("projects")));
    assert_eq!(open.filters[0].extensions, vec!["loomdeck".to_string()]);
    assert_eq!(save.suggested_name.as_deref(), Some("demo.loomdeck"));
    assert_eq!(export.suggested_name.as_deref(), Some(EXPORT_FILENAME));
    assert_eq!(export.filters[0].extensions, vec!["pdf".to_string()]);
}

#[test]
fn presentation_path_round_trip_preserves_document() {
    let path = std::env::temp_dir().join(format!(
        "loom-present-roundtrip-{}.loomdeck",
        std::process::id()
    ));
    let session = empty_session();
    let bytes = save_presentation_session(&session).expect("serialize");
    std::fs::write(&path, bytes).expect("write");
    let loaded = load_session(&path).expect("load");
    let _ = std::fs::remove_file(&path);

    assert_eq!(loaded.document.title, session.document.title);
    assert_eq!(loaded.document.len(), session.document.len());
}

#[test]
fn present_menu_disables_unhandled_controller_commands() {
    set_platform();
    let menu = build_present_menu_bar();
    for id in [
        "edit.cut",
        "edit.copy",
        "edit.paste",
        "edit.select_all",
        "view.zoom_in",
        "view.zoom_out",
        "view.zoom_actual",
    ] {
        assert!(
            !menu.find_item(id).expect("menu command").is_enabled(),
            "unhandled Present command {id} must be disabled"
        );
    }
}

#[test]
fn present_menu_projection_derives_live_session_and_window_state() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let state = GuiState {
        session: RefCell::new(empty_session()),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs,
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    };
    let menu = NativeMenuBar::new();
    let bar = build_present_menu_bar();
    menu.install_menu_bar(&bar).expect("install menu");

    sync_menu_state(&menu, &app, &state);
    let installed = menu.installed_menu_bar().expect("installed menu");

    assert!(matches!(
        installed.find_item("edit.undo"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("edit.redo"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("slide.delete"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("slide.prev"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("slide.next"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("view.inspector"),
        Some(MenuItem::Check {
            checked: false,
            enabled: true,
            ..
        })
    ));

    state
        .session
        .borrow_mut()
        .document
        .add_slide("Slide 2", "content");
    sync_menu_state(&menu, &app, &state);
    let installed = menu.installed_menu_bar().expect("installed menu");

    assert!(matches!(
        installed.find_item("slide.delete"),
        Some(MenuItem::Action { enabled: true, .. })
    ));
    assert!(matches!(
        installed.find_item("slide.prev"),
        Some(MenuItem::Action { enabled: true, .. })
    ));
    assert!(matches!(
        installed.find_item("slide.next"),
        Some(MenuItem::Action { enabled: false, .. })
    ));

    state.session.borrow_mut().document.select_slide(0);
    sync_menu_state(&menu, &app, &state);
    let installed = menu.installed_menu_bar().expect("installed menu");

    assert!(matches!(
        installed.find_item("slide.prev"),
        Some(MenuItem::Action { enabled: false, .. })
    ));
    assert!(matches!(
        installed.find_item("slide.next"),
        Some(MenuItem::Action { enabled: true, .. })
    ));

    app.set_show_inspector(true);
    sync_menu_state(&menu, &app, &state);
    let installed = menu.installed_menu_bar().expect("installed menu");

    assert!(matches!(
        installed.find_item("view.inspector"),
        Some(MenuItem::Check {
            checked: true,
            enabled: true,
            ..
        })
    ));
}

#[test]
fn present_menu_disables_inspector_when_window_cannot_show_it() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let inspector_available = configure_responsive_width(&app, 900);
    let state = Rc::new(GuiState {
        session: RefCell::new(empty_session()),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(inspector_available),
        save_path: RefCell::new(None),
        dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });
    wire_app_callbacks(&app, &state);
    let menu = NativeMenuBar::new();
    menu.install_menu_bar(&build_present_menu_bar())
        .expect("install menu");

    sync_menu_state(&menu, &app, &state);

    assert!(matches!(
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item("view.inspector").cloned()),
        Some(MenuItem::Check {
            checked: false,
            enabled: false,
            ..
        })
    ));

    let before = app.get_show_inspector();
    let error = menu
        .dispatch_action_from("view.inspector", CommandSource::Menu)
        .expect_err("compact inspector action must be disabled");
    assert!(error
        .to_string()
        .contains("menu item view.inspector is disabled"));
    assert_eq!(app.get_show_inspector(), before);
}

#[test]
fn present_menu_action_sink_dispatches_to_controller_and_guards_disabled_boundary() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let menu_service = Rc::new(NativeMenuBar::new());
    let state = Rc::new(GuiState {
        session: RefCell::new(empty_session()),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs,
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: Some(menu_service.clone()),
        drag_state: RefCell::new(DragState::default()),
    });
    let bar = build_present_menu_bar();
    menu_service.install_menu_bar(&bar).expect("install menu");

    wire_app_callbacks(&app, &state);
    let app_ref = app.as_weak();
    menu_service
        .register_action_sink(std::sync::Arc::new(move |action: CommandAction| {
            assert_eq!(action.source, CommandSource::Menu);
            let app = app_ref
                .upgrade()
                .ok_or_else(|| DesktopError::InvalidRequest("Present app was dropped".into()))?;
            if dispatch_command(&app, &action.id) {
                Ok(())
            } else {
                Err(DesktopError::InvalidRequest(format!(
                    "unsupported Present menu command {}",
                    action.id
                )))
            }
        }))
        .expect("register sink");

    sync_menu_state(&menu_service, &app, &state);
    assert_eq!(state.session.borrow().document.len(), 1);

    let before_unsupported = state.session.borrow().document.len();
    let error = menu_service
        .dispatch_action_from("edit.cut", CommandSource::Menu)
        .expect_err("unsupported menu action must be disabled");
    assert!(error.to_string().contains("menu item edit.cut is disabled"));
    assert_eq!(state.session.borrow().document.len(), before_unsupported);

    menu_service
        .dispatch_action_from("slide.new", CommandSource::Menu)
        .expect("enabled menu action");
    assert_eq!(state.session.borrow().document.len(), 2);

    state.session.borrow_mut().document.select_slide(0);
    sync_menu_state(&menu_service, &app, &state);
    let err = menu_service
        .dispatch_action_from("slide.prev", CommandSource::Menu)
        .expect_err("disabled action");
    assert!(err.to_string().contains("menu item slide.prev is disabled"));
    assert_eq!(state.session.borrow().document.active_index, 0);

    state.session.borrow_mut().remove_slide(1);
    sync_menu_state(&menu_service, &app, &state);
    assert_eq!(state.session.borrow().document.len(), 1);
    let err = menu_service
        .dispatch_action_from("slide.delete", CommandSource::Menu)
        .expect_err("disabled slide.delete");
    assert!(err
        .to_string()
        .contains("menu item slide.delete is disabled"));
    assert_eq!(state.session.borrow().document.len(), 1);
}

#[test]
fn notes_edit_refreshes_undo_menu_state() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let menu_service = Rc::new(NativeMenuBar::new());
    let state = Rc::new(GuiState {
        session: RefCell::new(empty_session()),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: Some(menu_service.clone()),
        drag_state: RefCell::new(DragState::default()),
    });
    menu_service
        .install_menu_bar(&build_present_menu_bar())
        .expect("install menu");
    wire_app_callbacks(&app, &state);
    sync_menu_state(&menu_service, &app, &state);

    assert!(matches!(
        menu_service
            .installed_menu_bar()
            .and_then(|bar| bar.find_item("edit.undo").cloned()),
        Some(MenuItem::Action { enabled: false, .. })
    ));

    app.invoke_notes_edited(SharedString::from("Speaker notes"));

    assert_eq!(
        state
            .session
            .borrow()
            .document
            .active_slide()
            .expect("active slide")
            .speaker_notes,
        "Speaker notes"
    );
    assert!(matches!(
        menu_service
            .installed_menu_bar()
            .and_then(|bar| bar.find_item("edit.undo").cloned()),
        Some(MenuItem::Action { enabled: true, .. })
    ));
}
