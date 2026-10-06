//! Keyboard-only operation of Present: menus, palette, toolbar, theme chooser,
//! Save Changes, the slide editor and the slideshow. Every step sends real key
//! events through `Window::dispatch_event`, so Slint's own focus handling and
//! key bubbling decide the result. Focus is read from the window, then matched
//! to the named accessibility element under it.

use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use loom_test_support::capture::{set_platform, snapshot_component};
use slint::platform::{Key, WindowEvent};
use slint::private_unstable_api::re_exports::{ItemWeak, WindowInner};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;

/// Where keyboard focus is: its accessible name, role and window rectangle.
#[derive(Clone, Debug)]
struct Stop {
    name: String,
    role: Option<AccessibleRole>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    visible: bool,
}

impl Stop {
    fn on_screen(&self) -> bool {
        self.visible
            && self.w > 0.0
            && self.h > 0.0
            && self.x >= -0.5
            && self.y >= -0.5
            && self.x + self.w <= WIDTH + 0.5
            && self.y + self.h <= HEIGHT + 0.5
    }
}

pub(super) struct Session {
    pub(super) app: PresentApp,
    pub(super) state: Rc<GuiState>,
    actions: Rc<RefCell<Vec<String>>>,
}

fn launched_with(dialogs: Rc<dyn FileDialogService>) -> Session {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    window_chrome::install(&app);
    let session = sample_session();
    let menu = Rc::new(NativeMenuBar::new());
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(session.document.clone()),
        last_saved_transitions: RefCell::default(),
        session: RefCell::new(session),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs,
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
        menu_service: Some(menu.clone()),
        drag_state: RefCell::new(DragState::default()),
    });
    app.window().set_size(slint::PhysicalSize::new(1280, 800));
    configure_responsive_width(&app, 1280);
    wire_app_callbacks(&app, &state);
    wire_close_guard(&app, &state);
    menu.install_menu_bar(&build_present_menu_bar())
        .expect("install menu bar");
    local_menu::wire_keyboard(&app);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let seen = actions.clone();
    app.on_local_menu_action(move |id| seen.borrow_mut().push(id.to_string()));
    wire_palette(&app);
    state
        .session
        .borrow_mut()
        .select_element("cover-title", false);
    refresh(&app, &state);
    local_menu::sync(&app, &menu).expect("project menu");
    render(&app);
    focus_editor_at_launch(&app);
    render(&app);
    Session {
        app,
        state,
        actions,
    }
}

pub(super) fn launched() -> Session {
    launched_with(Rc::new(loom_desktop::ScriptedFileDialogs::default()))
}

fn render(app: &PresentApp) {
    let _ = snapshot_component(app, WIDTH, HEIGHT, 1.0).expect("render");
}

fn key_down(app: &PresentApp, text: impl Into<SharedString>) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
}

fn key_up(app: &PresentApp, text: impl Into<SharedString>) {
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
}

fn tap(app: &PresentApp, text: impl Into<SharedString>) {
    let text = text.into();
    key_down(app, text.clone());
    key_up(app, text);
    render(app);
}

fn press(app: &PresentApp, key: Key) {
    tap(app, SharedString::from(key));
}

fn chord(app: &PresentApp, modifiers: &[Key], text: &str) {
    for m in modifiers {
        key_down(app, SharedString::from(*m));
    }
    tap(app, text);
    for m in modifiers.iter().rev() {
        key_up(app, SharedString::from(*m));
    }
}

fn ctrl(app: &PresentApp, text: &str) {
    chord(app, &[Key::Control], text);
}

fn alt(app: &PresentApp, letter: &str) {
    chord(app, &[Key::Alt], letter);
}

fn shift_key(app: &PresentApp, key: Key) {
    chord(app, &[Key::Shift], &String::from(char::from(key)));
}

fn type_text(app: &PresentApp, text: &str) {
    for c in text.chars() {
        tap(app, c.to_string());
    }
}

fn tab(app: &PresentApp) {
    press(app, Key::Tab);
}

fn shift_tab(app: &PresentApp) {
    shift_key(app, Key::Tab);
}

fn focus_weak(app: &PresentApp) -> ItemWeak {
    WindowInner::from_pub(app.window())
        .focus_item
        .borrow()
        .clone()
}

fn same_focus(a: &ItemWeak, b: &ItemWeak) -> bool {
    match (a.upgrade(), b.upgrade()) {
        (Some(a), Some(b)) => a == b,
        (None, None) => true,
        _ => false,
    }
}

/// The focused item, described by the named accessible element whose
/// rectangle is closest to it (a FocusScope usually fills the control it
/// serves). An empty name means nothing under the focus is named.
fn stop(app: &PresentApp) -> Option<Stop> {
    let item = focus_weak(app).upgrade()?;
    let geometry = item.geometry();
    let origin = item.map_to_window(geometry.origin);
    let (cx, cy) = (
        origin.x + geometry.size.width / 2.0,
        origin.y + geometry.size.height / 2.0,
    );
    let mut best: Option<((f32, f32), ElementHandle)> = None;
    for element in app
        .root_element()
        .query_descendants()
        .match_predicate(|_| true)
        .find_all()
    {
        if element.accessible_label().is_none_or(|l| l.is_empty()) {
            continue;
        }
        let (p, s) = (element.absolute_position(), element.size());
        if !(cx >= p.x && cx <= p.x + s.width && cy >= p.y && cy <= p.y + s.height) {
            continue;
        }
        let distance = (p.x - origin.x).abs()
            + (p.y - origin.y).abs()
            + (s.width - geometry.size.width).abs()
            + (s.height - geometry.size.height).abs()
            // Prefer a control over a caption that happens to share its rectangle.
            + if element.accessible_role() == Some(AccessibleRole::Text) { 0.5 } else { 0.0 };
        let score = (distance, s.width * s.height);
        if best.as_ref().is_none_or(|(b, _)| score < *b) {
            best = Some((score, element));
        }
    }
    let (name, role) = best
        .map(|(_, e)| {
            (
                e.accessible_label().unwrap().to_string(),
                e.accessible_role(),
            )
        })
        .unwrap_or_default();
    Some(Stop {
        name,
        role,
        x: origin.x,
        y: origin.y,
        w: geometry.size.width,
        h: geometry.size.height,
        visible: item.is_visible(),
    })
}

fn focus_name(app: &PresentApp) -> String {
    stop(app).map(|s| s.name).unwrap_or_default()
}

fn labels(app: &PresentApp) -> Vec<String> {
    app.get_local_menu_labels()
        .iter()
        .map(|l| l.to_string())
        .collect()
}

fn popup_labels(app: &PresentApp) -> Vec<String> {
    app.get_local_menu_popup_items()
        .iter()
        .map(|i| i.label.to_string())
        .collect()
}

fn selected_row(app: &PresentApp) -> String {
    let index = app.get_local_menu_popup_selected_index();
    app.get_local_menu_popup_items()
        .row_data(index as usize)
        .map(|i| i.label.to_string())
        .unwrap_or_default()
}

fn slide_count(s: &Session) -> usize {
    s.state.session.borrow().document.slides.len()
}

fn active_slide(s: &Session) -> usize {
    s.state.session.borrow().document.active_index
}

fn tab_to(app: &PresentApp, name: &str) {
    for _ in 0..40 {
        if focus_name(app) == name {
            return;
        }
        tab(app);
    }
    panic!("Tab never reached {name:?}");
}

#[test]
fn alt_letters_open_every_menu_and_arrows_walk_them() {
    let s = launched();
    let page = focus_weak(&s.app);
    let menus = labels(&s.app);
    assert_eq!(menus, ["File", "Edit", "View", "Slide"]);

    for (index, letter) in ["f", "e", "v", "s"].iter().enumerate() {
        alt(&s.app, letter);
        assert_eq!(
            s.app.get_local_menu_open_index(),
            index as i32,
            "Alt+{letter}"
        );
        assert!(
            !popup_labels(&s.app).is_empty(),
            "{} has rows",
            menus[index]
        );
        press(&s.app, Key::Escape);
        assert_eq!(s.app.get_local_menu_open_index(), -1);
        assert!(
            same_focus(&focus_weak(&s.app), &page),
            "Escape returns to the slide editor"
        );
    }

    alt(&s.app, "f");
    let first = selected_row(&s.app);
    assert!(!first.is_empty());
    press(&s.app, Key::DownArrow);
    assert_ne!(selected_row(&s.app), first);
    press(&s.app, Key::UpArrow);
    assert_eq!(selected_row(&s.app), first);
    press(&s.app, Key::RightArrow);
    assert_eq!(s.app.get_local_menu_open_index(), 1);
    press(&s.app, Key::LeftArrow);
    press(&s.app, Key::LeftArrow);
    assert_eq!(
        s.app.get_local_menu_open_index(),
        3,
        "Left wraps to the last menu"
    );
    press(&s.app, Key::Escape);
    assert_eq!(slide_count(&s), 3, "walking menus changed nothing");
}

#[test]
fn f10_opens_and_closes_the_first_menu_and_enter_runs_a_row() {
    let s = launched();
    let page = focus_weak(&s.app);
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), 0);
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), -1, "F10 again closes");
    assert!(same_focus(&focus_weak(&s.app), &page));

    // Slide menu, New Slide: Alt+S, Enter runs the first enabled row.
    alt(&s.app, "s");
    assert_eq!(selected_row(&s.app), "New Slide");
    press(&s.app, Key::Return);
    assert_eq!(*s.actions.borrow(), ["slide.new"]);
    assert_eq!(s.app.get_local_menu_open_index(), -1);
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus is back on the slide editor"
    );
}

#[test]
fn menu_keys_do_nothing_while_a_modal_owns_the_keyboard() {
    let s = launched();
    s.app.invoke_open_palette();
    render(&s.app);
    alt(&s.app, "f");
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), -1);
    assert!(s.app.get_palette_open());
}

#[test]
fn ctrl_k_palette_runs_a_command_and_escape_closes_it_returning_focus() {
    let s = launched();
    let page = focus_weak(&s.app);
    ctrl(&s.app, "k");
    assert!(s.app.get_palette_open(), "Ctrl+K opens the palette");
    assert!(focus_name(&s.app).starts_with("Command palette"));
    type_text(&s.app, "add slide");
    assert!(s.app.get_palette_commands().row_count() > 0);
    press(&s.app, Key::Return);
    assert!(
        !s.app.get_palette_open(),
        "Enter runs the command and closes"
    );
    assert_eq!(slide_count(&s), 4, "the palette added a slide");
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus is back on the editor"
    );

    ctrl(&s.app, "k");
    press(&s.app, Key::DownArrow);
    press(&s.app, Key::Escape);
    assert!(!s.app.get_palette_open(), "Escape closes");
    assert!(same_focus(&focus_weak(&s.app), &page));
}

/// Put focus on the first toolbar item: the menu bar's last stop is followed by
/// the toolbar, so walk Tab from the menu bar.
fn focus_toolbar_item(app: &PresentApp, name: &str) {
    app.invoke_focus_editor();
    render(app);
    // Shift+Tab from the editor goes back through the slide strip and toolbar.
    for _ in 0..40 {
        if focus_name(app) == name && stop(app).is_some_and(|s| s.y < 100.0 && s.y > 36.0) {
            return;
        }
        shift_tab(app);
    }
    panic!("Shift+Tab never reached toolbar item {name:?}");
}

#[test]
fn toolbar_items_are_reachable_in_order_and_view_zoom_menus_work_by_keyboard() {
    let s = launched();
    focus_toolbar_item(&s.app, "View");
    let mut seen = vec![focus_name(&s.app)];
    for _ in 0..9 {
        tab(&s.app);
        seen.push(focus_name(&s.app));
    }
    assert_eq!(
        seen,
        [
            "View",
            "Zoom",
            "Add Slide",
            "Insert",
            "Text",
            "Shape",
            "Play",
            "Format",
            "Animate",
            "Document"
        ]
    );

    // View menu: Enter opens, Down moves to Speaker Notes, Enter toggles it.
    s.app.invoke_focus_editor();
    render(&s.app);
    let page = focus_weak(&s.app);
    focus_toolbar_item(&s.app, "View");
    press(&s.app, Key::Return);
    assert!(s.app.get_view_menu_open(), "Enter opens the View menu");
    press(&s.app, Key::DownArrow);
    press(&s.app, Key::Return);
    assert!(!s.app.get_view_menu_open(), "a row closes the menu");
    assert!(
        s.app.get_show_notes_drawer(),
        "Speaker Notes was toggled on"
    );
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus is back on the editor"
    );

    // Zoom menu: Down, Down, Enter picks 50%.
    focus_toolbar_item(&s.app, "Zoom");
    press(&s.app, Key::Return);
    assert!(s.app.get_zoom_menu_open());
    press(&s.app, Key::DownArrow);
    press(&s.app, Key::DownArrow);
    press(&s.app, Key::Return);
    assert_eq!(s.app.get_zoom_index(), 2, "50% chosen");

    // Escape closes either menu and returns focus to the item that opened it.
    focus_toolbar_item(&s.app, "View");
    press(&s.app, Key::Return);
    press(&s.app, Key::Escape);
    assert!(!s.app.get_view_menu_open());
    assert_eq!(focus_name(&s.app), "View", "focus returns to the trigger");
    press(&s.app, Key::Tab);
    press(&s.app, Key::Return);
    assert!(s.app.get_zoom_menu_open());
    press(&s.app, Key::Escape);
    assert_eq!(focus_name(&s.app), "Zoom");
}

#[test]
fn inspector_tab_buttons_and_toolbar_commands_run_from_the_keyboard() {
    let s = launched();
    focus_toolbar_item(&s.app, "Add Slide");
    press(&s.app, Key::Return);
    assert_eq!(
        slide_count(&s),
        4,
        "Enter on the Add Slide item adds a slide"
    );

    focus_toolbar_item(&s.app, "Document");
    press(&s.app, Key::Return);
    assert_eq!(s.app.get_inspector_tab(), 2, "Document tab is showing");
    focus_toolbar_item(&s.app, "Animate");
    press(&s.app, Key::Return);
    assert_eq!(s.app.get_inspector_tab(), 1);
}

fn request_close(app: &PresentApp) {
    app.window().dispatch_event(WindowEvent::CloseRequested);
    render(app);
}

#[test]
fn save_changes_traps_tab_defaults_to_save_and_escape_cancels() {
    let s = launched();
    let page = focus_weak(&s.app);
    press(&s.app, Key::Delete);
    assert_eq!(
        s.state.session.borrow().document.slides[0].elements.len(),
        1
    );
    request_close(&s.app);
    assert!(
        s.app.get_save_changes_open(),
        "closing a dirty deck asks first"
    );
    assert_eq!(focus_name(&s.app), "Save", "the primary button has focus");

    let mut order = Vec::new();
    for _ in 0..6 {
        tab(&s.app);
        order.push(focus_name(&s.app));
    }
    assert_eq!(
        order,
        ["Cancel", "Discard", "Save", "Cancel", "Discard", "Save"]
    );
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Discard");
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Cancel");
    assert!(s.app.get_save_changes_open());

    press(&s.app, Key::Escape);
    assert!(
        !s.app.get_save_changes_open(),
        "Escape cancels from a button"
    );
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus returns to the editor"
    );

    // Enter on Cancel after one Shift+Tab from Save... Save is the default.
    request_close(&s.app);
    shift_tab(&s.app);
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Cancel");
    press(&s.app, Key::Return);
    assert!(!s.app.get_save_changes_open());
    assert_eq!(
        s.state.session.borrow().document.slides[0].elements.len(),
        1,
        "nothing was discarded"
    );
}

#[test]
fn theme_chooser_is_keyboard_complete_and_traps_tab() {
    let s = launched();
    let page = focus_weak(&s.app);
    // Reach it from the keyboard: Document tab, then Choose Theme.
    focus_toolbar_item(&s.app, "Document");
    press(&s.app, Key::Return);
    tab_to(&s.app, "Choose Theme...");
    press(&s.app, Key::Return);
    assert!(
        s.app.get_theme_chooser_open(),
        "Enter opens the theme chooser"
    );
    assert_eq!(s.app.get_theme_selected(), 0);

    press(&s.app, Key::RightArrow);
    press(&s.app, Key::RightArrow);
    assert_eq!(s.app.get_theme_selected(), 2);
    press(&s.app, Key::LeftArrow);
    assert_eq!(s.app.get_theme_selected(), 1);
    press(&s.app, Key::DownArrow);
    assert_eq!(s.app.get_theme_category(), 1);
    press(&s.app, Key::UpArrow);
    assert_eq!(s.app.get_theme_category(), 0);

    // Tab walks the aspect control, Cancel and Create, then wraps to the chooser.
    let chooser = focus_weak(&s.app);
    let mut order = Vec::new();
    for _ in 0..4 {
        tab(&s.app);
        order.push(focus_name(&s.app));
    }
    assert_eq!(
        order[..3],
        ["Theme Aspect Ratio", "Cancel", "Create Presentation"]
    );
    assert!(
        same_focus(&focus_weak(&s.app), &chooser),
        "Tab wrapped from Create back to the chooser"
    );
    shift_tab(&s.app);
    assert_eq!(
        focus_name(&s.app),
        "Create Presentation",
        "Shift+Tab wraps backwards"
    );
    assert!(s.app.get_theme_chooser_open());

    press(&s.app, Key::Escape);
    assert!(!s.app.get_theme_chooser_open(), "Escape cancels");
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus returns to the editor"
    );
}

#[test]
fn speaker_notes_drawer_is_reachable_editable_and_closable_without_a_pointer() {
    let s = launched();
    s.app.set_show_notes_drawer(true);
    render(&s.app);
    tab_to(&s.app, "Speaker notes");
    type_text(&s.app, "Remember the demo");
    assert_eq!(
        s.state.session.borrow().document.slides[0].speaker_notes,
        "Remember the demo"
    );
    // Tab leaves the notes instead of typing a tab character.
    tab(&s.app);
    assert_ne!(focus_name(&s.app), "Speaker notes");
    assert_eq!(
        s.state.session.borrow().document.slides[0].speaker_notes,
        "Remember the demo",
        "Tab did not type into the notes"
    );
    tab_to(&s.app, "Close speaker notes");
    press(&s.app, Key::Return);
    assert!(!s.app.get_show_notes_drawer(), "Enter closes the drawer");
}

#[test]
fn add_slide_edit_text_navigate_save_and_present_without_a_pointer() {
    let dir = std::env::temp_dir().join(format!("loom-present-keyboard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("keyboard.loomdeck");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(target.clone())],
    ));
    let s = launched_with(dialogs);

    // Ctrl+Shift+N is the New Slide shortcut the Slide menu shows.
    chord(&s.app, &[Key::Control, Key::Shift], "N");
    assert_eq!(
        slide_count(&s),
        4,
        "Ctrl+Shift+N adds a slide, not a new deck"
    );
    assert_eq!(active_slide(&s), 3);
    press(&s.app, Key::PageUp);
    assert_eq!(active_slide(&s), 2, "Page Up goes to the previous slide");
    press(&s.app, Key::PageDown);
    assert_eq!(active_slide(&s), 3, "Page Down goes to the next slide");

    // Tab selects the first object, Enter edits its text in place.
    s.app.invoke_focus_editor();
    render(&s.app);
    press(&s.app, Key::Tab);
    assert_eq!(s.app.get_selection_count(), 1, "Tab selected an object");
    assert_eq!(s.app.get_active_element_index(), 0);
    press(&s.app, Key::Return);
    type_text(&s.app, "Typed by keyboard");
    press(&s.app, Key::Escape);
    let content = s.state.session.borrow().document.slides[3].elements[0]
        .content
        .clone();
    assert!(
        content.contains("Typed by keyboard"),
        "Enter then typing edited the title: {content:?}"
    );

    // Slide strip: Tab reaches each slide and Enter selects it.
    s.app.invoke_focus_editor();
    shift_key(&s.app, Key::Tab);
    tab_to(&s.app, "Slide 1: Create without compromise");
    press(&s.app, Key::Return);
    assert_eq!(active_slide(&s), 0, "Enter on a slide thumbnail selects it");

    // Save with Ctrl+S through the scripted dialog.
    s.app.invoke_focus_editor();
    ctrl(&s.app, "s");
    assert!(target.exists(), "Ctrl+S saved {}", target.display());

    // F5 starts the slideshow, Right advances, Escape leaves it.
    press(&s.app, Key::F5);
    assert!(s.app.get_is_preview_mode(), "F5 starts the slideshow");
    press(&s.app, Key::RightArrow);
    assert_eq!(active_slide(&s), 1, "Right advances the slideshow");
    press(&s.app, Key::LeftArrow);
    assert_eq!(active_slide(&s), 0);
    press(&s.app, Key::Escape);
    assert!(!s.app.get_is_preview_mode(), "Escape ends the slideshow");
    let _ = std::fs::remove_dir_all(&dir);
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Region {
    Notes,
    Inspector,
    Menu,
    Toolbar,
    Strip,
}

fn region(stop: &Stop) -> Region {
    if stop.y < 36.0 {
        Region::Menu
    } else if stop.y < 90.0 {
        Region::Toolbar
    } else if stop.x >= 1000.0 {
        Region::Inspector
    } else if stop.x < 220.0 {
        Region::Strip
    } else {
        Region::Notes
    }
}

/// Within one row left to right, rows top to bottom.
fn reads_in_order(stops: &[&Stop]) -> bool {
    stops.windows(2).all(|w| {
        if (w[1].y - w[0].y).abs() < 6.0 {
            w[1].x > w[0].x
        } else {
            w[1].y > w[0].y
        }
    })
}

#[test]
fn tab_order_is_logical_named_visible_and_free_of_traps() {
    for (notes, inspector_tab) in [(false, 0), (true, 1), (false, 2)] {
        let s = launched();
        s.app.set_show_notes_drawer(notes);
        s.app.set_inspector_tab(inspector_tab);
        render(&s.app);
        let editor = focus_weak(&s.app);

        // Tab walks the slide's objects first, then leaves the editor.
        let mut presses = 0;
        loop {
            tab(&s.app);
            presses += 1;
            assert!(presses <= 6, "Tab never left the slide editor");
            if !same_focus(&focus_weak(&s.app), &editor) {
                break;
            }
        }
        let mut stops = vec![(focus_weak(&s.app), stop(&s.app).unwrap())];
        loop {
            tab(&s.app);
            if same_focus(&focus_weak(&s.app), &editor) {
                break;
            }
            assert!(stops.len() < 80, "Tab did not come back to the editor");
            stops.push((focus_weak(&s.app), stop(&s.app).unwrap()));
        }

        for (i, (item, stop)) in stops.iter().enumerate() {
            assert!(
                !stop.name.is_empty(),
                "stop {i} has no accessible name: {stop:?}"
            );
            assert!(stop.role.is_some(), "stop {i} has no role: {stop:?}");
            assert!(
                stop.on_screen(),
                "stop {i} {} is not on screen: {stop:?}",
                stop.name
            );
            for (j, (other, _)) in stops.iter().enumerate().skip(i + 1) {
                assert!(
                    !same_focus(item, other),
                    "stops {i} and {j} are the same control"
                );
            }
        }

        // Regions come in one block each, in a fixed order: notes, inspector,
        // menu bar, toolbar, slide strip, and each reads left to right, top down.
        let mut blocks: Vec<(Region, Vec<&Stop>)> = Vec::new();
        for (_, stop) in &stops {
            let r = region(stop);
            match blocks.last_mut() {
                Some((last, group)) if *last == r => group.push(stop),
                _ => blocks.push((r, vec![stop])),
            }
        }
        let order: Vec<Region> = blocks.iter().map(|(r, _)| *r).collect();
        let expected: Vec<Region> = if notes {
            vec![
                Region::Notes,
                Region::Inspector,
                Region::Menu,
                Region::Toolbar,
                Region::Strip,
            ]
        } else {
            vec![
                Region::Inspector,
                Region::Menu,
                Region::Toolbar,
                Region::Strip,
            ]
        };
        assert_eq!(order, expected, "regions in tab order: {blocks:#?}");
        for (r, group) in &blocks {
            if *r != Region::Strip {
                assert!(reads_in_order(group), "{r:?} order: {group:#?}");
            }
        }
        let names = |r: Region| -> Vec<String> {
            blocks
                .iter()
                .find(|(x, _)| *x == r)
                .map(|(_, g)| g.iter().map(|s| s.name.clone()).collect())
                .unwrap_or_default()
        };
        assert_eq!(names(Region::Menu), ["File", "Edit", "View", "Slide"]);
        assert_eq!(
            names(Region::Toolbar),
            [
                "View",
                "Zoom",
                "Add Slide",
                "Insert",
                "Text",
                "Shape",
                "Play",
                "Format",
                "Animate",
                "Document",
                "More actions"
            ]
        );
        let strip = names(Region::Strip);
        assert_eq!(strip.len(), 5, "three slides, Add and Delete: {strip:?}");
        assert!(strip[0].starts_with("Slide 1"));
    }
}

#[test]
fn compact_overflow_menu_opens_runs_a_command_and_escape_returns_to_the_editor() {
    let s = launched();
    s.app.set_overflow_toolbar(true);
    render(&s.app);
    let page = focus_weak(&s.app);
    focus_toolbar_item(&s.app, "More actions");
    press(&s.app, Key::Return);
    assert!(
        s.app.get_toolbar_overflow_open(),
        "Enter opens the overflow menu"
    );
    assert_eq!(
        focus_name(&s.app),
        "Choose theme",
        "focus starts on the first enabled command"
    );
    press(&s.app, Key::Escape);
    assert!(!s.app.get_toolbar_overflow_open(), "Escape closes it");
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus returns to the editor"
    );

    // A command in the menu runs from the keyboard and closes the menu.
    focus_toolbar_item(&s.app, "More actions");
    press(&s.app, Key::Return);
    press(&s.app, Key::Return);
    assert!(!s.app.get_toolbar_overflow_open());
    assert!(
        s.app.get_theme_chooser_open(),
        "Enter on Choose theme opened the chooser"
    );
    press(&s.app, Key::Escape);
    assert!(!s.app.get_theme_chooser_open());
}

#[test]
fn alt_letters_open_menus_while_editing_slide_text_without_typing_into_it() {
    let s = launched();
    s.app.invoke_focus_editor();
    render(&s.app);
    press(&s.app, Key::Return);
    type_text(&s.app, "Hi");
    alt(&s.app, "v");
    assert_eq!(
        s.app.get_local_menu_open_index(),
        2,
        "Alt+V opens View while editing text"
    );
    press(&s.app, Key::Escape);
    let content = s.state.session.borrow().document.slides[0]
        .elements
        .iter()
        .map(|e| e.content.clone())
        .collect::<Vec<_>>()
        .join("|");
    assert!(
        content.contains("Hi") && !content.contains("Hiv"),
        "no letter was typed: {content}"
    );
}
