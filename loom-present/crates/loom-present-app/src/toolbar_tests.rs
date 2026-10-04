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
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render");
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

fn button(app: &PresentApp, label: &str) {
    press(app, AccessibleRole::Button, label);
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
            .filter(|e| e.absolute_position().y < 100.0)
            .collect::<Vec<_>>();
        assert!(!items.is_empty(), "toolbar button named {label:?}");
        let (p, s) = (items[0].absolute_position(), items[0].size());
        assert!(
            p.y < 100.0 && s.width > 0.0,
            "{label} sits in the toolbar band"
        );
        p.x
    };
    let left = ["View", "Zoom", "Add Slide"].map(x_of);
    let centre = ["Insert", "Text", "Shape"].map(x_of);
    let right = ["Play", "Format", "Animate", "Document"].map(x_of);
    assert!(left.windows(2).all(|w| w[0] < w[1]));
    assert!(centre.windows(2).all(|w| w[0] < w[1]));
    assert!(right.windows(2).all(|w| w[0] < w[1]));
    assert!(left[2] < centre[0] && centre[2] < right[0]);
}

#[test]
fn inspector_is_open_by_default_and_tab_buttons_switch_or_close_it() {
    let (app, _state) = launched(None);
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 0);

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

    button(&app, "View");
    assert!(app.get_view_menu_open());
    press(&app, AccessibleRole::ListItem, "Navigator");
    assert!(!app.get_show_navigator());
    assert!(
        !app.get_view_menu_open(),
        "choosing an entry closes the menu"
    );

    button(&app, "View");
    press(&app, AccessibleRole::ListItem, "Speaker Notes");
    assert!(app.get_show_notes_drawer());

    button(&app, "View");
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
    button(&app, "View");
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
    button(&app, "View");
    assert!(app.get_view_menu_open());
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: slint::platform::Key::Escape.into(),
    });
    assert!(!app.get_view_menu_open());
}
