//! The icon-over-label toolbar, its View and Zoom menus, the inspector tab
//! buttons and the View-menu pane toggles all drive real window state.

use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_test_support::capture::{set_platform, snapshot_component};

fn launched(menu: Option<Rc<NativeMenuBar>>) -> (PresentApp, Rc<GuiState>) {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let session = sample_session();
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(session.document.clone()),
        last_saved_transitions: RefCell::default(),
        session: RefCell::new(session),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::default()),
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
        menu_service: menu,
        drag_state: RefCell::new(DragState::default()),
    });
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    configure_responsive_width(&app, 1280);
    wire_app_callbacks(&app, &state);
    wire_palette(&app);
    refresh(&app, &state);
    render(&app);
    (app, state)
}

fn render(app: &PresentApp) {
    render_at(app, (1280, 800));
}

fn render_at(app: &PresentApp, size: (u32, u32)) {
    app.window()
        .set_size(slint::PhysicalSize::new(size.0, size.1));
    let _ = snapshot_component(app, size.0 as f32, size.1 as f32, 1.0).expect("render");
}

fn root(app: &PresentApp) -> ElementHandle {
    [
        "Rectangle",
        "PresentApp",
        "Window",
        "VerticalLayout",
        "FocusScope",
    ]
    .iter()
    .flat_map(|name| ElementHandle::find_by_element_type_name(app, name).collect::<Vec<_>>())
    .max_by_key(|candidate| candidate.query_descendants().find_all().len())
    .expect("root element")
}

fn press(app: &PresentApp, role: AccessibleRole, label: &str) {
    render(app);
    let found = ElementHandle::find_by_accessible_label(app, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} named {label:?}"));
    found.invoke_accessible_default_action();
}

fn toolbar_button(app: &PresentApp, label: &str) -> ElementHandle {
    let min_height = if label == "More actions" { 20.0 } else { 40.0 };
    ElementHandle::find_by_accessible_label(app, label)
        .find(|e| {
            e.accessible_role() == Some(AccessibleRole::Button) && e.size().height >= min_height
        })
        .unwrap_or_else(|| panic!("toolbar button {label:?}"))
}

fn button(app: &PresentApp, label: &str) {
    render(app);
    toolbar_button(app, label).invoke_accessible_default_action();
}

fn focus_toolbar_button_by_keyboard(app: &PresentApp, size: (u32, u32), label: &str) {
    for _ in 0..24 {
        render_at(app, size);
        let focused = crate::keyboard_flow_tests::focus_name(app);
        if focused == label {
            return;
        }
        crate::keyboard_flow_tests::press(app, slint::platform::Key::Tab);
    }
    panic!(
        "keyboard did not reach toolbar button {label:?}; focused {:?}",
        crate::keyboard_flow_tests::focus_name(app)
    );
}

#[test]
fn every_toolbar_item_is_a_named_button_in_three_groups() {
    let (app, _state) = launched(None);
    let root = root(&app);
    let x_of = |label: &str| {
        let items = root
            .query_descendants()
            .match_accessible_role(AccessibleRole::Button)
            .match_predicate({
                let label = label.to_string();
                move |e| e.accessible_label().as_deref() == Some(label.as_str())
            })
            .find_all()
            .into_iter()
            .filter(|e| e.absolute_position().y < 100.0 && e.size().height >= 40.0)
            .collect::<Vec<_>>();
        assert!(!items.is_empty(), "toolbar button named {label:?}");
        let (p, s) = (items[0].absolute_position(), items[0].size());
        assert!(
            p.y < 100.0 && s.width > 0.0,
            "{label} sits in the toolbar band"
        );
        p.x
    };
    let left = ["View menu", "Zoom", "Add Slide"].map(x_of);
    let centre = ["Insert", "Text", "Shape"].map(x_of);
    let right = ["Play", "Format", "Animate", "Document"].map(x_of);
    assert!(left.windows(2).all(|w| w[0] < w[1]));
    assert!(centre.windows(2).all(|w| w[0] < w[1]));
    assert!(right.windows(2).all(|w| w[0] < w[1]));
    assert!(left[2] < centre[0] && centre[2] < right[0]);
}

#[test]

fn inspector_defaults_closed_and_toolbar_buttons_switch_or_close_it() {
    let (app, _state) = launched(None);
    assert!(!app.get_show_inspector());
    assert!(!app.get_inspector_visible());
    assert_eq!(app.get_inspector_tab(), 0);

    button(&app, "Format");
    assert!(app.get_show_inspector());
    assert!(app.get_inspector_visible());

    button(&app, "Animate");
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 1);

    button(&app, "Document");
    assert_eq!(app.get_inspector_tab(), 2);

    button(&app, "Document");
    assert!(
        !app.get_show_inspector(),
        "second press closes the inspector"
    );

    button(&app, "Format");
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 0);
}

#[test]
fn inspector_requested_visibility_survives_responsive_resizes() {
    let (app, _state) = launched(None);
    let sizes = [(1024, 720), (1280, 800), (1440, 900)];

    assert!(
        !app.get_show_inspector(),
        "fresh Present state starts closed"
    );
    assert!(!app.get_inspector_visible());
    for size in sizes {
        let available = configure_responsive_width(&app, size.0);
        render_at(&app, size);
        assert_eq!(available, size.0 >= 1180);
        assert!(!app.get_show_inspector(), "closed request at {} px", size.0);
        assert!(
            !app.get_inspector_visible(),
            "closed effective state at {} px",
            size.0
        );
    }

    configure_responsive_width(&app, 1280);
    button(&app, "Format");
    assert!(
        app.get_show_inspector(),
        "toolbar request opens the inspector"
    );

    for size in sizes {
        let _available = configure_responsive_width(&app, size.0);
        render_at(&app, size);
        assert!(
            app.get_show_inspector(),
            "open request survives {} px",
            size.0
        );
        assert!(
            app.get_inspector_visible(),
            "the open inspector is effective at {} px, including compact widths",
            size.0
        );
    }

    configure_responsive_width(&app, 1440);
    render_at(&app, (1440, 900));
    button(&app, "Format");
    for size in sizes {
        app.set_view_menu_open(false);
        configure_responsive_width(&app, size.0);
        render_at(&app, size);
        assert!(
            !app.get_show_inspector(),
            "closed request survives {} px",
            size.0
        );
        assert!(!app.get_inspector_visible(), "closed at {} px", size.0);
    }
}

#[test]
fn inspector_mode_toolbar_buttons_remain_keyboard_reachable_at_responsive_widths() {
    let (app, _state) = launched(None);
    for size in [(1024, 720), (1280, 800), (1440, 900)] {
        app.set_show_inspector(false);
        app.set_inspector_tab(0);
        let available = configure_responsive_width(&app, size.0);
        render_at(&app, size);
        assert_eq!(available, size.0 >= 1180);

        app.invoke_focus_editor();
        render_at(&app, size);
        crate::keyboard_flow_tests::press(&app, slint::platform::Key::F6);
        render_at(&app, size);
        let toolbar_name = "View menu";
        assert_eq!(
            crate::keyboard_flow_tests::focus_name(&app),
            toolbar_name,
            "F6 reaches the toolbar at {} px",
            size.0
        );

        for (label, tab) in [("Format", 0), ("Animate", 1), ("Document", 2)] {
            focus_toolbar_button_by_keyboard(&app, size, label);
            crate::keyboard_flow_tests::press(&app, slint::platform::Key::Return);
            render_at(&app, size);
            assert!(
                app.get_show_inspector(),
                "keyboard activation of {label} requests the inspector at {} px",
                size.0
            );
            assert_eq!(app.get_inspector_tab(), tab);
            assert!(
                app.get_inspector_visible(),
                "effective inspector visibility at {} px",
                size.0
            );
        }
    }
}

#[test]
fn add_slide_text_and_shape_buttons_edit_the_deck() {
    let (app, state) = launched(None);
    let slides = state.session.borrow().document.len();
    button(&app, "Add Slide");
    assert_eq!(state.session.borrow().document.len(), slides + 1);

    let elements = |state: &GuiState| {
        let session = state.session.borrow();
        session.document.slides[session.document.active_index]
            .elements
            .len()
    };
    let before = elements(&state);
    button(&app, "Text");
    button(&app, "Shape");
    assert_eq!(elements(&state), before + 2);
}

#[test]
fn view_toolbar_menu_toggles_the_navigator_and_notes() {
    let (app, _state) = launched(None);
    assert!(app.get_show_navigator());
    assert!(!app.get_show_notes_drawer());

    button(&app, "View menu");
    assert!(app.get_view_menu_open());
    press(&app, AccessibleRole::ListItem, "Navigator");
    assert!(!app.get_show_navigator());
    assert!(
        !app.get_view_menu_open(),
        "choosing an entry closes the menu"
    );

    button(&app, "View menu");
    press(&app, AccessibleRole::ListItem, "Speaker Notes");
    assert!(app.get_show_notes_drawer());

    button(&app, "View menu");
    press(&app, AccessibleRole::ListItem, "Navigator");
    assert!(app.get_show_navigator());
}

#[test]
fn zoom_toolbar_menu_chooses_a_level() {
    let (app, _state) = launched(None);
    assert_eq!(app.get_zoom_index(), 0);
    button(&app, "Zoom");
    assert!(app.get_zoom_menu_open());
    assert!(!app.get_view_menu_open());
    press(&app, AccessibleRole::ListItem, "50%");
    assert_eq!(app.get_zoom_index(), 2);
    assert!(!app.get_zoom_menu_open());

    button(&app, "Zoom");
    press(&app, AccessibleRole::ListItem, "Fit");
    assert_eq!(app.get_zoom_index(), 0);
}

#[test]
fn view_menu_commands_toggle_panes_and_checkmarks_follow_every_control() {
    let menu = Rc::new(NativeMenuBar::new());
    let (app, state) = launched(Some(menu.clone()));
    menu.install_menu_bar(&build_present_menu_bar())
        .expect("install menu");
    sync_menu_state(&menu, &app, &state);
    let checked = |id: &str| match menu
        .installed_menu_bar()
        .and_then(|bar| bar.find_item(id).cloned())
    {
        Some(MenuItem::Check { checked, .. }) => checked,
        other => panic!("{id} is not a check item: {other:?}"),
    };
    assert!(checked("view.navigator"));
    assert!(!checked("view.notes"));

    assert!(dispatch_command(&app, "view.navigator"));
    render(&app);
    assert!(!app.get_show_navigator());
    assert!(!checked("view.navigator"), "menu check follows the command");

    assert!(dispatch_command(&app, "view.notes"));
    render(&app);
    assert!(app.get_show_notes_drawer());
    assert!(checked("view.notes"));

    // A toolbar control flips the same state and the menu follows it.
    button(&app, "View menu");
    press(&app, AccessibleRole::ListItem, "Speaker Notes");
    assert!(!app.get_show_notes_drawer());
    render(&app);
    assert!(!checked("view.notes"));

    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"view.navigator"));
    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"view.notes"));
}

#[test]
fn escape_closes_an_open_toolbar_menu() {
    use slint::platform::WindowEvent;
    let (app, _state) = launched(None);
    button(&app, "View menu");
    assert!(app.get_view_menu_open());
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(!app.get_view_menu_open());
}

#[test]
fn wide_windows_put_table_and_chart_in_the_toolbar_and_narrow_ones_keep_them_in_the_menu() {
    let (app, state) = launched(None);
    let count = |app: &PresentApp, label: &str| {
        render(app);
        ElementHandle::find_by_accessible_label(app, label)
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .count()
    };
    // 1280 wide is under the labelled-slot threshold: both live in the Slide menu.
    assert_eq!(count(&app, "Table"), 0);

    app.window().set_size(slint::PhysicalSize::new(1600, 900));
    configure_responsive_width(&app, 1600);
    assert_eq!(count(&app, "Table"), 1, "wide windows show Table");
    assert_eq!(
        count(&app, "Paste Chart from Sheets"),
        1,
        "wide windows show chart insertion"
    );

    let before = state
        .session
        .borrow()
        .document
        .active_slide()
        .map_or(0, |s| s.elements.len());
    press(&app, AccessibleRole::Button, "Table");
    app.invoke_insert_table_size(3, 3);
    let after = state
        .session
        .borrow()
        .document
        .active_slide()
        .map_or(0, |s| s.elements.len());
    assert!(
        after > before,
        "the toolbar Table button leads to an inserted table"
    );
}

#[test]
fn chart_command_labels_disclose_the_sheets_prerequisite() {
    let (app, _) = launched(None);

    assert_eq!(
        build_present_menu_bar()
            .find_item("slide.insert_chart")
            .expect("chart menu command")
            .label(),
        Some("Paste Chart from Sheets...")
    );
    assert_eq!(
        master_palette(&app)
            .into_iter()
            .find(|command| matches!(command.action, PaletteAction::AddChart))
            .expect("chart palette command")
            .label,
        "Paste Chart from Sheets"
    );

    app.window().set_size(slint::PhysicalSize::new(1600, 900));
    configure_responsive_width(&app, 1600);
    render(&app);
    assert_eq!(
        ElementHandle::find_by_accessible_label(&app, "Paste Chart from Sheets")
            .filter(|item| item.accessible_role() == Some(AccessibleRole::Button))
            .count(),
        1
    );
}

fn rectangles_overlap(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
    let overlap_x = (a.0 + a.2).min(b.0 + b.2) - a.0.max(b.0);
    let overlap_y = (a.1 + a.3).min(b.1 + b.3) - a.1.max(b.1);
    overlap_x > 0.0 && overlap_y > 0.0
}

#[test]
fn twelve_mixed_insertions_avoid_occupied_rectangles_and_skip_preoccupied_slots() {
    // The production text and shape boxes are too large to fit twelve disjoint
    // rectangles on one slide (6 * 520x100 + 6 * 300x140 alone exceeds the
    // 1000x562.5 slide area), so compact mixed boxes exercise the placement
    // contract itself: first rectangle-free slot, every insertion disjoint.
    let text_size = (200.0, 60.0);
    let shape_size = (120.0, 60.0);
    let mut session =
        PresentationSession::new(PresentationDocument::new("placement-test", "Placement"));
    {
        let slide = session.document.active_slide_mut().unwrap();
        slide.elements.clear();
        slide.add_element(text_element(
            "occupied-text-base",
            ElementType::BodyText,
            "Occupied text base",
            120.0,
            220.0,
            text_size.0,
            text_size.1,
        ));
        slide.add_element(text_element(
            "occupied-shape-base",
            ElementType::ShapeRectangle,
            "Occupied shape base",
            120.0,
            260.0,
            shape_size.0,
            shape_size.1,
        ));
        slide.add_element(text_element(
            "occupied-text-next",
            ElementType::BodyText,
            "Occupied text slot",
            168.0,
            268.0,
            text_size.0,
            text_size.1,
        ));
    }
    let expected = [
        (240.0, 340.0),
        (264.0, 404.0),
        (408.0, 462.5),
        (624.0, 462.5),
        (760.0, 462.5),
        (384.0, 260.0),
        (528.0, 220.0),
        (744.0, 260.0),
        (528.0, 292.0),
        (744.0, 332.0),
        (456.0, 364.0),
        (120.0, 404.0),
    ];

    for (index, expected_origin) in expected.iter().enumerate() {
        let (kind, base, size, content) = if index % 2 == 0 {
            (ElementType::BodyText, (120.0, 220.0), text_size, "New text")
        } else {
            (
                ElementType::ShapeRectangle,
                (120.0, 260.0),
                shape_size,
                "Shape",
            )
        };
        let origin = added_object_origin(&session, base, size);
        assert_eq!(
            origin, *expected_origin,
            "insertion {index} did not take the first rectangle-free slot"
        );
        let slide = session.document.active_slide().unwrap();
        let candidate = (origin.0, origin.1, size.0, size.1);
        for existing in &slide.elements {
            assert!(
                !rectangles_overlap(
                    candidate,
                    (existing.x, existing.y, existing.width, existing.height)
                ),
                "insertion {index} at ({}, {}) {}x{} overlaps {} at ({}, {}) {}x{}",
                origin.0,
                origin.1,
                size.0,
                size.1,
                existing.id,
                existing.x,
                existing.y,
                existing.width,
                existing.height
            );
        }
        session
            .document
            .active_slide_mut()
            .unwrap()
            .add_element(text_element(
                &format!("inserted-{index}"),
                kind,
                content,
                origin.0,
                origin.1,
                size.0,
                size.1,
            ));
    }

    let slide = session.document.active_slide().unwrap();
    for element in &slide.elements {
        assert!(
            element.x >= 0.0
                && element.y >= 0.0
                && element.x + element.width <= 1000.0
                && element.y + element.height <= 562.5,
            "a cascaded object left the slide: {} at ({}, {}) {}x{}",
            element.id,
            element.x,
            element.y,
            element.width,
            element.height
        );
    }
}

#[test]
fn first_text_and_shape_keep_their_base_origin_on_an_empty_slide() {
    let (app, state) = launched(None);
    let origins = |state: &GuiState| -> Vec<(f32, f32)> {
        let session = state.session.borrow();
        session
            .document
            .active_slide()
            .unwrap()
            .elements
            .iter()
            .map(|element| (element.x, element.y))
            .collect()
    };
    let clear = |state: &GuiState| {
        state
            .session
            .borrow_mut()
            .document
            .active_slide_mut()
            .unwrap()
            .elements
            .clear();
    };

    clear(&state);
    button(&app, "Text");
    assert_eq!(origins(&state), vec![(120.0, 220.0)]);

    clear(&state);
    button(&app, "Shape");
    assert_eq!(origins(&state), vec![(120.0, 260.0)]);
}

#[test]
fn toolbar_insertions_skip_preoccupied_default_rectangles() {
    let (app, state) = launched(None);
    let clear = |state: &GuiState| {
        state
            .session
            .borrow_mut()
            .document
            .active_slide_mut()
            .unwrap()
            .elements
            .clear();
    };

    clear(&state);
    {
        let mut session = state.session.borrow_mut();
        session
            .document
            .active_slide_mut()
            .unwrap()
            .add_element(text_element(
                "occupied-text-base",
                ElementType::BodyText,
                "Occupied text base",
                120.0,
                220.0,
                520.0,
                100.0,
            ));
    }
    button(&app, "Text");
    {
        let session = state.session.borrow();
        let elements = &session.document.active_slide().unwrap().elements;
        let inserted = elements.last().unwrap();
        assert_eq!((inserted.x, inserted.y), (240.0, 340.0));
        assert!(
            !rectangles_overlap(
                (inserted.x, inserted.y, inserted.width, inserted.height),
                (
                    elements[0].x,
                    elements[0].y,
                    elements[0].width,
                    elements[0].height
                )
            ),
            "new text overlapped the pre-occupied rectangle"
        );
    }

    clear(&state);
    {
        let mut session = state.session.borrow_mut();
        session
            .document
            .active_slide_mut()
            .unwrap()
            .add_element(text_element(
                "occupied-shape-base",
                ElementType::ShapeRectangle,
                "Occupied shape base",
                120.0,
                260.0,
                300.0,
                140.0,
            ));
    }
    button(&app, "Shape");
    {
        let session = state.session.borrow();
        let elements = &session.document.active_slide().unwrap().elements;
        let inserted = elements.last().unwrap();
        assert_eq!((inserted.x, inserted.y), (432.0, 382.5));
        assert!(
            !rectangles_overlap(
                (inserted.x, inserted.y, inserted.width, inserted.height),
                (
                    elements[0].x,
                    elements[0].y,
                    elements[0].width,
                    elements[0].height
                )
            ),
            "new shape overlapped the pre-occupied rectangle"
        );
    }
}

#[test]
fn compact_1024_keeps_each_inspector_mode_open_visible_and_keyboard_reachable() {
    let (app, state) = launched(None);
    state
        .session
        .borrow_mut()
        .select_element("cover-title", false);
    refresh(&app, &state);
    let available = configure_responsive_width(&app, 1024);
    assert!(!available, "1024 px is the compact icon-only toolbar width");
    assert!(
        !app.get_show_inspector(),
        "the compact inspector starts closed"
    );

    for (label, tab, focus_label, marker) in [
        ("Format", 0, "Layout Template", "Text colour Red"),
        ("Animate", 1, "Transition Type", "Transition Type"),
        ("Document", 2, "New from Template...", "Deck Information"),
    ] {
        app.set_show_inspector(false);
        render_at(&app, (1024, 720));
        button(&app, label);
        render_at(&app, (1024, 720));

        assert!(app.get_show_inspector(), "{label} requests the inspector");
        assert_eq!(app.get_inspector_tab(), tab, "{label} selects its mode");
        assert!(
            app.get_inspector_visible(),
            "{label} must make the compact inspector visible"
        );
        let panel = ElementHandle::find_by_element_type_name(&app, "PresentInspector")
            .next()
            .expect("inspector panel");
        assert!(
            panel.size().width > 0.0 && panel.size().height > 0.0,
            "{label} inspector has no visible box"
        );
        assert!(
            ElementHandle::find_by_accessible_label(&app, marker)
                .next()
                .is_some(),
            "{label} inspector content {marker:?} is missing"
        );

        // The compact panel stays a real keyboard region: F6 still reaches
        // the first control in the selected mode.
        app.invoke_focus_editor();
        render_at(&app, (1024, 720));
        crate::keyboard_flow_tests::press(&app, slint::platform::Key::F6);
        render_at(&app, (1024, 720));
        assert_eq!(
            crate::keyboard_flow_tests::focus_name(&app),
            "View menu",
            "F6 reaches the compact toolbar"
        );
        crate::keyboard_flow_tests::press(&app, slint::platform::Key::F6);
        render_at(&app, (1024, 720));
        assert_eq!(
            crate::keyboard_flow_tests::focus_name(&app),
            focus_label,
            "{label} content is keyboard reachable"
        );
    }
}

#[test]
fn compact_toolbar_and_local_menu_expose_distinct_view_names() {
    let (app, _state) = launched(None);
    for size in [(1024, 720), (1280, 800)] {
        app.set_view_menu_open(false);
        configure_responsive_width(&app, size.0);
        render_at(&app, size);

        let view_buttons: Vec<_> = ElementHandle::find_by_accessible_label(&app, "View")
            .filter(|e| e.accessible_role() == Some(AccessibleRole::Button))
            .collect();
        assert_eq!(
            view_buttons.len(),
            1,
            "only the local menu bar keeps an actionable View control at {} px",
            size.0
        );
        let toolbar_view = toolbar_button(&app, "View menu");
        assert!(
            toolbar_view.absolute_position().y > 25.0,
            "the View menu button is the toolbar item, not the menu bar"
        );

        toolbar_view.invoke_accessible_default_action();
        assert!(
            app.get_view_menu_open(),
            "the deduplicated toolbar name still operates the View menu at {} px",
            size.0
        );
    }
}
