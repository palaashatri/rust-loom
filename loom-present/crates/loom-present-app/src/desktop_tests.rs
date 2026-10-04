use super::*;
use loom_desktop::{CommandSource, ScriptedFileDialogs};

fn test_state() -> GuiState {
    GuiState {
        session: RefCell::new(empty_session()),
        last_saved: RefCell::new(empty_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
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
        last_saved: RefCell::new(sample_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
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
    assert_eq!(app.get_active_element_label().as_str(), "Body text");
    assert_eq!(app.get_selection_count(), 1);
    assert_eq!(
        app.get_active_element_content().as_str(),
        "A private, native creative studio on your desktop."
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
        last_saved: RefCell::new(sample_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
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
    assert_eq!(app.get_active_element_label().as_str(), "Body text");

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
        last_saved: RefCell::new(sample_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
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
        last_saved: RefCell::new(empty_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
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
        last_saved: RefCell::new(empty_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
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
        last_saved: RefCell::new(empty_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
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
        last_saved: RefCell::new(empty_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
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

#[test]
fn closing_a_dirty_deck_asks_before_the_window_goes_away() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    state
        .session
        .borrow_mut()
        .document
        .add_slide("Changed", "content");
    wire_app_callbacks(&app, &state);
    wire_close_guard(&app, &state);

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);

    assert!(
        app.get_save_changes_open(),
        "the unsaved-changes dialog must open"
    );
    assert_eq!(
        state.pending_replacement.get(),
        Some(PendingReplacement::CloseWindow)
    );

    app.invoke_save_changes_cancel();
    assert!(!app.get_save_changes_open());
    assert_eq!(
        state.pending_replacement.get(),
        None,
        "cancel keeps the window open"
    );
}

#[test]
fn selecting_a_slide_thumbnail_does_not_panic_on_the_session_borrow() {
    // Regression: the select-slide callback kept the session mutably borrowed
    // while `refresh` read it, so clicking any thumbnail aborted the app.
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    state
        .session
        .borrow_mut()
        .document
        .add_slide("Second", "content");
    wire_app_callbacks(&app, &state);

    assert_eq!(state.session.borrow().document.active_index, 1);
    app.invoke_select_slide(0);

    assert_eq!(state.session.borrow().document.active_index, 0);
}

#[test]
fn undo_and_redo_shortcuts_reach_the_session() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    wire_app_callbacks(&app, &state);
    let before = state.session.borrow().document.len();
    app.invoke_add_slide();
    assert_eq!(state.session.borrow().document.len(), before + 1);

    app.invoke_undo();
    assert_eq!(state.session.borrow().document.len(), before);
    app.invoke_redo();
    assert_eq!(state.session.borrow().document.len(), before + 1);
}

#[test]
fn successful_save_clears_the_edited_status() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dir =
        std::env::temp_dir().join(format!("loom-present-saved-status-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join("deck.loomdeck");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(path.clone())],
    ));
    let state = GuiState {
        session: RefCell::new(empty_session()),
        last_saved: RefCell::new(empty_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs,
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    };
    state
        .session
        .borrow_mut()
        .document
        .add_slide("Changed", "content");
    refresh_without_recovery(&app, &state);
    assert_eq!(app.get_status_right(), "Edited");

    assert_eq!(save_current_deck(&app, &state, true), Ok(true));
    assert_eq!(app.get_status_right(), "Saved");

    state
        .session
        .borrow_mut()
        .document
        .add_slide("Again", "content");
    refresh_without_recovery(&app, &state);
    assert_eq!(app.get_status_right(), "Edited");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_transition_only_change_marks_the_deck_edited_until_saved() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dir = std::env::temp_dir().join(format!(
        "loom-present-transition-dirty-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(dir.join("deck.loomdeck"))],
    ));
    let state = GuiState {
        session: RefCell::new(empty_session()),
        last_saved: RefCell::new(empty_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs,
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    };
    assert!(!deck_is_dirty(&state));
    {
        let mut session = state.session.borrow_mut();
        let id = session.document.slides[0].id.clone();
        session.checkpoint();
        session.set_transition(&id, TransitionKind::Dissolve);
    }
    assert!(deck_is_dirty(&state));
    assert_eq!(save_current_deck(&app, &state, true), Ok(true));
    assert!(!deck_is_dirty(&state));
    state.session.borrow_mut().undo();
    assert!(
        deck_is_dirty(&state),
        "undoing a saved transition is an unsaved change"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn slideshow_keys_navigate_and_escape_exits() {
    use slint::platform::WindowEvent;
    use std::cell::Cell;

    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let next = Rc::new(Cell::new(0));
    let prev = Rc::new(Cell::new(0));
    let toggles = Rc::new(Cell::new(0));
    {
        let next = next.clone();
        app.on_next_slide(move || next.set(next.get() + 1));
        let prev = prev.clone();
        app.on_prev_slide(move || prev.set(prev.get() + 1));
        let toggles = toggles.clone();
        let weak = app.as_weak();
        app.on_toggle_preview_mode(move || {
            toggles.set(toggles.get() + 1);
            if let Some(app) = weak.upgrade() {
                app.set_is_preview_mode(!app.get_is_preview_mode());
            }
        });
    }
    app.show().expect("show");
    app.window()
        .set_size(slint::LogicalSize::new(1280.0, 800.0));
    let press = |text: &str| {
        let text: slint::SharedString = text.into();
        app.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        app.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
    };

    // F5 starts the slideshow from the editor.
    press(&char::from(slint::platform::Key::F5).to_string());
    assert!(app.get_is_preview_mode());
    for key in [
        slint::platform::Key::RightArrow,
        slint::platform::Key::PageDown,
    ] {
        press(&char::from(key).to_string());
    }
    press(" ");
    assert_eq!(next.get(), 3, "right arrow, page down and space advance");
    for key in [
        slint::platform::Key::LeftArrow,
        slint::platform::Key::Backspace,
    ] {
        press(&char::from(key).to_string());
    }
    assert_eq!(prev.get(), 2, "left arrow and backspace go back");

    // Mouse: click advances, right-click goes back.
    assert!(app.get_is_preview_mode());
    let click = |button: slint::platform::PointerEventButton| {
        let position = slint::LogicalPosition::new(300.0, 300.0);
        app.window()
            .dispatch_event(WindowEvent::PointerPressed { position, button });
        app.window()
            .dispatch_event(WindowEvent::PointerReleased { position, button });
    };
    click(slint::platform::PointerEventButton::Left);
    assert_eq!(next.get(), 4, "left click advances");
    click(slint::platform::PointerEventButton::Right);
    assert_eq!(prev.get(), 3, "right click goes back");

    // Escape leaves the slideshow and navigation keys no longer advance slides.
    press(&char::from(slint::platform::Key::Escape).to_string());
    assert!(!app.get_is_preview_mode());
    press(&char::from(slint::platform::Key::RightArrow).to_string());
    assert_eq!(next.get(), 4);
}

#[test]
fn slideshow_plays_the_arrival_slides_transition_and_the_editor_never_animates() {
    use std::time::Duration;

    fn settle(app: &PresentApp) -> f32 {
        // Rendering a frame is what runs pending `changed` handlers.
        snapshot_component(app, 1280.0, 800.0, 1.0).expect("render frame");
        slint::platform::update_timers_and_animations();
        app.get_transition_progress()
    }
    fn play_to_end(app: &PresentApp) {
        for _ in 0..200 {
            if settle(app) >= 1.0 {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    app.show().expect("show");
    app.window()
        .set_size(slint::LogicalSize::new(1280.0, 800.0));

    // Editing: moving to another slide with a transition set does not animate.
    app.set_transition_index(1);
    app.set_active_slide_index(1);
    assert!(
        (settle(&app) - 1.0).abs() < f32::EPSILON,
        "editor must not animate"
    );

    app.set_is_preview_mode(true);
    for (index, transition, name) in [(2, 1, "dissolve"), (3, 2, "push"), (4, 3, "morph")] {
        app.set_transition_index(transition);
        app.set_active_slide_index(index);
        let started = settle(&app);
        assert!(
            started < 0.5,
            "{name} starts near the beginning, got {started}"
        );
        std::thread::sleep(Duration::from_millis(120));
        let midway = settle(&app);
        assert!(
            midway > started && midway < 1.0,
            "{name} is mid-flight after 120 ms, got {midway}"
        );
        play_to_end(&app);
        assert!((settle(&app) - 1.0).abs() < f32::EPSILON, "{name} finishes");
    }

    // A slide with no transition appears at once.
    app.set_transition_index(0);
    app.set_active_slide_index(5);
    assert!(
        (settle(&app) - 1.0).abs() < f32::EPSILON,
        "None does not animate"
    );

    // Leaving the slideshow mid-transition snaps to the finished state.
    app.set_transition_index(1);
    app.set_active_slide_index(6);
    assert!(settle(&app) < 1.0);
    app.set_is_preview_mode(false);
    assert!(
        (settle(&app) - 1.0).abs() < f32::EPSILON,
        "exit cancels the animation"
    );
}

#[test]
fn mid_transition_frames_are_dimmed_for_dissolve_and_shifted_for_push() {
    fn lit_pixels(width: u32, raw: &[u8], x_range: std::ops::Range<u32>) -> usize {
        raw.chunks_exact(4)
            .enumerate()
            .filter(|(index, pixel)| {
                x_range.contains(&(*index as u32 % width))
                    && (pixel[0] as u32 + pixel[1] as u32 + pixel[2] as u32) > 120
            })
            .count()
    }
    fn brightness(raw: &[u8]) -> u64 {
        raw.chunks_exact(4)
            .map(|pixel| pixel[0] as u64 + pixel[1] as u64 + pixel[2] as u64)
            .sum()
    }

    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    app.show().expect("show");
    app.set_is_preview_mode(true);
    app.set_active_slide_index(0);
    let frame = |reveal: f32, transition: i32| {
        app.set_transition_index(transition);
        app.set_reveal(reveal);
        snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render frame")
    };

    let settled = frame(1.0, 1);
    let dissolve = frame(0.3, 1);
    assert!(
        brightness(dissolve.as_raw()) < brightness(settled.as_raw()) * 9 / 10,
        "a dissolve in progress is dimmer than the settled slide"
    );

    let settled = frame(1.0, 2);
    let push = frame(0.3, 2);
    assert!(
        lit_pixels(push.width(), push.as_raw(), 0..400)
            < lit_pixels(settled.width(), settled.as_raw(), 0..400) / 2,
        "a push in progress has not yet reached the left side of the screen"
    );
    assert!(
        lit_pixels(settled.width(), settled.as_raw(), 0..1280) > 10_000,
        "the settled slide is actually drawn"
    );
}

#[test]
fn presenter_view_follows_the_deck_and_drives_navigation() {
    use slint::platform::WindowEvent;

    set_platform();
    presenter::forget();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    {
        let mut session = state.session.borrow_mut();
        session.document.slides[0].title = "Opening".into();
        session.document.slides[0].speaker_notes =
            "  Welcome everyone.\nIntroduce the topic.  ".into();
        session.document.add_slide("Middle", "content");
        session.document.add_slide("Finale", "content");
        session.document.select_slide(0);
    }
    wire_app_callbacks(&app, &state);
    refresh(&app, &state);
    assert!(
        presenter::with_window(|_| ()).is_none(),
        "no presenter window exists until it is asked for"
    );

    // Starting a slideshow and pressing P opens the presenter view.
    app.show().expect("show");
    app.set_is_preview_mode(true);
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: "p".into() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: "p".into() });
    let read =
        |f: fn(&PresenterWindow) -> String| presenter::with_window(f).expect("presenter exists");
    assert_eq!(read(|w| w.get_position_text().to_string()), "Slide 1 of 3");
    assert_eq!(read(|w| w.get_slide_title().to_string()), "Opening");
    assert_eq!(
        read(|w| w.get_notes().to_string()),
        "Welcome everyone.\nIntroduce the topic.",
        "notes are shown trimmed"
    );
    assert_eq!(read(|w| w.get_next_title().to_string()), "Middle");
    assert_eq!(
        presenter::with_window(|w| (w.get_has_previous(), w.get_has_next())),
        Some((false, true))
    );

    // The presenter's own Next button moves the real deck and its own view.
    presenter::with_window(|w| w.invoke_next_slide());
    assert_eq!(state.session.borrow().document.active_index, 1);
    assert_eq!(app.get_active_slide_index(), 1);
    assert_eq!(read(|w| w.get_position_text().to_string()), "Slide 2 of 3");
    assert_eq!(
        read(|w| w.get_notes().to_string()),
        "",
        "a slide without notes shows none"
    );
    assert_eq!(read(|w| w.get_next_title().to_string()), "Finale");
    presenter::with_window(|w| w.invoke_next_slide());
    assert_eq!(
        presenter::with_window(|w| (w.get_has_previous(), w.get_has_next())),
        Some((true, false))
    );
    presenter::with_window(|w| w.invoke_next_slide());
    assert_eq!(
        state.session.borrow().document.active_index,
        2,
        "Next on the last slide stays put"
    );
    presenter::with_window(|w| w.invoke_previous_slide());
    assert_eq!(read(|w| w.get_slide_title().to_string()), "Middle");

    // Moving from the main window updates the presenter too.
    app.invoke_prev_slide();
    assert_eq!(read(|w| w.get_slide_title().to_string()), "Opening");

    // Edits to notes in the main window reach the presenter.
    app.invoke_notes_edited("Fresh notes".into());
    assert_eq!(read(|w| w.get_notes().to_string()), "Fresh notes");

    // The clock can be paused and reset from the presenter window.
    presenter::with_window(|w| w.invoke_toggle_timer());
    assert_eq!(
        presenter::with_window(|w| w.get_timer_running()),
        Some(false),
        "Pause stops the clock"
    );
    presenter::with_window(|w| w.invoke_reset_timer());
    assert_eq!(read(|w| w.get_elapsed().to_string()), "00:00");
    presenter::with_window(|w| w.invoke_toggle_timer());
    assert_eq!(
        presenter::with_window(|w| w.get_timer_running()),
        Some(true),
        "Resume restarts it"
    );

    presenter::with_window(|w| w.invoke_close_presenter());
    assert!(
        presenter::with_window(|w| w.window().is_visible()) == Some(false),
        "Close hides the presenter window"
    );
    presenter::forget();
}

#[test]
fn a_recovered_deck_reads_as_unsaved_but_a_fresh_start_does_not() {
    let (fresh, baseline) = startup_sessions(None, None).expect("fresh start");
    assert!(
        presentation_documents_match(&fresh.document, &baseline.document),
        "fresh start is clean"
    );

    let mut draft = sample_session();
    draft
        .document
        .add_slide("Added before the crash", "content");
    let bytes = save_presentation_session(&draft).expect("serialize draft");
    let (restored, baseline) = startup_sessions(Some(&bytes), None).expect("recovered start");
    assert!(
        presentation_documents_match(&restored.document, &draft.document),
        "the draft is what opens"
    );
    assert!(
        !presentation_documents_match(&restored.document, &baseline.document),
        "a recovered deck must read as unsaved, so closing asks first"
    );
    assert!(
        presentation_documents_match(&baseline.document, &empty_session().document),
        "the baseline is what a start without recovery would show"
    );

    let (_, baseline) = startup_sessions(Some(b"not a deck"), None).expect("corrupt recovery");
    assert!(
        presentation_documents_match(&baseline.document, &empty_session().document),
        "unreadable recovery data falls back to the blank deck"
    );
}

#[test]
fn the_save_prompt_says_closing_only_when_the_window_is_closing() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = test_state();
    state
        .session
        .borrow_mut()
        .document
        .add_slide("Unsaved", "content");
    assert!(deck_is_dirty(&state));

    assert!(request_deck_replacement(
        &app,
        &state,
        PendingReplacement::NewDeck
    ));
    assert!(!app.get_save_changes_closing(), "New says replacing");

    assert!(request_deck_replacement(
        &app,
        &state,
        PendingReplacement::CloseWindow
    ));
    assert!(app.get_save_changes_closing(), "closing says closing");
}

#[test]
fn presenter_thumbnails_follow_navigation_from_both_windows_and_live_edits() {
    use slint::Model;

    set_platform();
    presenter::forget();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(test_state());
    {
        let mut session = state.session.borrow_mut();
        session.document.slides[0].title = "Opening".into();
        session.document.add_slide("Middle", "content");
        session.document.add_slide("Finale", "content");
        for (index, slide) in session.document.slides.iter_mut().enumerate() {
            if slide.elements.is_empty() {
                slide.add_element(SlideElement {
                    id: format!("e{index}"),
                    element_type: ElementType::BodyText,
                    content: String::new(),
                    x: 100.0,
                    y: 200.0,
                    width: 600.0,
                    height: 120.0,
                    rotation_deg: 0.0,
                    action: None,
                });
            }
            for element in slide.elements.iter_mut() {
                element.content = format!("slide {} text", index + 1);
            }
        }
        session.document.select_slide(0);
    }
    wire_app_callbacks(&app, &state);
    refresh(&app, &state);
    presenter::open(&app, &state.session.borrow()).expect("open presenter");

    let first_content = |rows: fn(&PresenterWindow) -> ThumbRows| {
        presenter::with_window(|w| rows(w).contents.row_data(0).map(|s| s.to_string())).flatten()
    };
    let count = |rows: fn(&PresenterWindow) -> ThumbRows| {
        presenter::with_window(|w| rows(w).contents.row_count()).expect("presenter")
    };
    assert_eq!(
        first_content(|w| w.get_next_rows()),
        Some("slide 2 text".into())
    );
    assert_eq!(
        first_content(|w| w.get_current_rows()),
        Some("slide 1 text".into())
    );

    // The presenter's Next button moves both thumbnails.
    presenter::with_window(|w| w.invoke_next_slide());
    assert_eq!(
        first_content(|w| w.get_next_rows()),
        Some("slide 3 text".into())
    );
    assert_eq!(
        first_content(|w| w.get_current_rows()),
        Some("slide 2 text".into())
    );

    // The last slide has no next thumbnail.
    app.invoke_next_slide();
    assert_eq!(count(|w| w.get_next_rows()), 0);
    assert_eq!(presenter::with_window(|w| w.get_has_next()), Some(false));
    assert_eq!(
        first_content(|w| w.get_current_rows()),
        Some("slide 3 text".into())
    );

    // Going back from the main window brings the thumbnail back.
    app.invoke_prev_slide();
    assert_eq!(
        first_content(|w| w.get_next_rows()),
        Some("slide 3 text".into())
    );

    // A live edit of the active slide reaches the current thumbnail.
    app.invoke_update_element_content("edited live".into());
    refresh(&app, &state);
    let current = presenter::with_window(|w| {
        w.get_current_rows()
            .contents
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>()
    })
    .expect("presenter");
    assert!(
        current.iter().any(|text| text == "edited live"),
        "{current:?}"
    );

    // Render the presenter window and look at it: slide paper stays white.
    Theme::get(&app).set_active_theme(Theme::get(&app).get_active_theme());
    for (width, height, name) in [(960.0, 600.0, "960x600"), (640.0, 420.0, "640x420")] {
        let image = presenter::with_window(|w| {
            snapshot_component(w, width, height, 1.0).expect("render presenter")
        })
        .expect("presenter");
        let right = image.width() * 3 / 5;
        let white = image
            .pixels()
            .enumerate()
            .filter(|(i, p)| (*i as u32 % image.width()) >= right && p.0 == [255, 255, 255, 255])
            .count();
        assert!(
            white > 500,
            "{name}: the next-slide thumbnail shows paper ({white})"
        );
        if let Ok(dir) = std::env::var("LOOM_PRESENTER_PNG_DIR") {
            let path = Path::new(&dir).join(format!("presenter-{name}.png"));
            loom_test_support::png::save_png(&path, &image).expect("save png");
        }
    }
    presenter::forget();
}

#[test]
fn a_blank_deck_prompts_for_a_title_without_saving_the_prompt() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let state = Rc::new(GuiState {
        save_path: RefCell::new(None),
        ..test_state()
    });
    wire_app_callbacks(&app, &state);
    refresh(&app, &state);

    let placeholders: Vec<String> = app
        .get_element_placeholders()
        .iter()
        .map(|text| text.to_string())
        .collect();
    assert_eq!(
        placeholders.first().map(String::as_str),
        Some("Click to add title")
    );
    assert_eq!(app.get_element_types().row_data(0), Some(0));

    let session = state.session.borrow();
    let element = &session.document.slides[0].elements[0];
    assert!(
        element.content.is_empty(),
        "the prompt must not enter the deck"
    );
    assert!(!save_presentation_session(&session)
        .map(|bytes| String::from_utf8_lossy(&bytes).contains("Click to add"))
        .unwrap_or(true));
    drop(session);

    state.session.borrow_mut().document.slides[0].elements[0].content = "Quarterly plan".into();
    refresh(&app, &state);
    assert_eq!(
        app.get_element_placeholders().row_data(0).unwrap().as_str(),
        ""
    );
}
