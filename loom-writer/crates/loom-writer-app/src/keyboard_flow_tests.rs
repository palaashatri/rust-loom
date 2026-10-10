//! Keyboard-only operation of Writer: menus, palette, toolbar, modals, find
//! bar and the type/format/find/save workflow. Every step sends real key
//! events through `Window::dispatch_event`, so Slint's own focus handling and
//! key bubbling decide the result. Focus is read from the window, then matched
//! to the smallest named accessibility element under it.

use super::actions_tests::{test_state, text_document};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::platform::{Key, WindowEvent};
use slint::private_unstable_api::re_exports::{ItemWeak, WindowInner};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;

/// Where keyboard focus is: its accessible name, role, and window rectangle.
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
    pub(super) app: WriterApp,
    pub(super) state: Rc<GuiState>,
    actions: Rc<RefCell<Vec<String>>>,
}

fn scripted(saves: Vec<Option<PathBuf>>) -> Rc<dyn FileDialogService> {
    Rc::new(loom_desktop::ScriptedFileDialogs::new([], saves))
}

/// The window as `run_gui` builds it: custom chrome with the in-window menu,
/// the real menu bar, palette and find wiring, inspector open, page focused.
fn launched_with(document: WriterDocument, dialogs: Rc<dyn FileDialogService>) -> Session {
    let (app, state) = test_state(document, dialogs);
    window_chrome::install(&app);
    let menu = Arc::new(NativeMenuBar::new());
    wire_writer_shared_callbacks(&app, &state, Some(menu.clone()));
    wire_writer_inspector_toggle(&app, &state, Some(menu.clone()));
    wire_close_guard(&app, &state);
    menu.install_menu_bar(&local_menu::writer_menu_bar())
        .expect("install menu bar");
    local_menu::wire_keyboard(&app);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let seen = actions.clone();
    app.on_local_menu_action(move |id| seen.borrow_mut().push(id.to_string()));
    local_menu::sync(&app, &menu).expect("project menu");
    wire_palette(&app);
    palette_wiring::wire(&app, &state);
    palette_wiring::wire_new_document(&app, &state);
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    toolbar_commands::start_with_inspector_open(&app, 1280);
    apply_state(&app, &state);
    render(&app);
    focus_page_at_launch(&app);
    render(&app);
    Session {
        app,
        state,
        actions,
    }
}

pub(super) fn launched(text: &str) -> Session {
    launched_with(text_document(text), scripted(vec![]))
}

fn render(app: &WriterApp) {
    let _ = snapshot_component(app, WIDTH, HEIGHT, 1.0).expect("render");
}

fn key_down(app: &WriterApp, text: impl Into<SharedString>) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
}

fn key_up(app: &WriterApp, text: impl Into<SharedString>) {
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
}

fn tap(app: &WriterApp, text: impl Into<SharedString>) {
    let text = text.into();
    key_down(app, text.clone());
    key_up(app, text);
    render(app);
}

fn press(app: &WriterApp, key: Key) {
    tap(app, SharedString::from(key));
}

fn chord(app: &WriterApp, modifiers: &[Key], text: &str) {
    for m in modifiers {
        key_down(app, SharedString::from(*m));
    }
    tap(app, text);
    for m in modifiers.iter().rev() {
        key_up(app, SharedString::from(*m));
    }
}

fn ctrl(app: &WriterApp, text: &str) {
    chord(app, &[Key::Control], text);
}

fn type_text(app: &WriterApp, text: &str) {
    for c in text.chars() {
        tap(app, c.to_string());
    }
}

fn tab(app: &WriterApp) {
    press(app, Key::Tab);
}

fn shift_tab(app: &WriterApp) {
    chord(app, &[Key::Shift], &String::from(char::from(Key::Tab)));
}

fn focus_weak(app: &WriterApp) -> ItemWeak {
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

/// The focused item, described by the smallest named accessible element that
/// contains its centre. An empty name means nothing under the focus is named.
fn stop(app: &WriterApp) -> Option<Stop> {
    let item = focus_weak(app).upgrade()?;
    let geometry = item.geometry();
    let origin = item.map_to_window(geometry.origin);
    let (cx, cy) = (
        origin.x + geometry.size.width / 2.0,
        origin.y + geometry.size.height / 2.0,
    );
    // Prefer the named element whose rectangle is closest to the focused item's
    // (a FocusScope usually fills the control it serves); among equals, the smaller.
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

fn focus_name(app: &WriterApp) -> String {
    stop(app).map(|s| s.name).unwrap_or_default()
}

fn alt(app: &WriterApp, letter: &str) {
    chord(app, &[Key::Alt], letter);
}

fn shift_key(app: &WriterApp, key: Key) {
    chord(app, &[Key::Shift], &String::from(char::from(key)));
}

fn labels(app: &WriterApp) -> Vec<String> {
    app.get_local_menu_labels()
        .iter()
        .map(|l| l.to_string())
        .collect()
}

fn popup_labels(app: &WriterApp) -> Vec<String> {
    app.get_local_menu_popup_items()
        .iter()
        .map(|i| i.label.to_string())
        .collect()
}

fn selected_row(app: &WriterApp) -> String {
    let index = app.get_local_menu_popup_selected_index();
    app.get_local_menu_popup_items()
        .row_data(index as usize)
        .map(|i| i.label.to_string())
        .unwrap_or_default()
}

#[test]
fn alt_letters_open_every_menu_and_arrows_walk_them() {
    let s = launched("Hello");
    let page = focus_weak(&s.app);
    let menus = labels(&s.app);
    assert_eq!(menus, ["File", "Edit", "View", "Format"]);

    // Each menu answers to the first letter no earlier menu used: Format is O.
    for (index, letter) in ["f", "e", "v", "o"].iter().enumerate() {
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
            "Escape returns to the page"
        );
    }

    assert_eq!(
        s.state.current.borrow().plain_text(),
        "Hello",
        "Alt+letter must not type the letter into the page"
    );

    // Arrows: Down/Up move the highlight over enabled rows, Left/Right change menus.
    alt(&s.app, "f");
    let first = selected_row(&s.app);
    assert!(
        !first.is_empty(),
        "a row is highlighted when the menu opens"
    );
    press(&s.app, Key::DownArrow);
    let second = selected_row(&s.app);
    assert_ne!(first, second);
    press(&s.app, Key::UpArrow);
    assert_eq!(selected_row(&s.app), first);
    press(&s.app, Key::RightArrow);
    assert_eq!(s.app.get_local_menu_open_index(), 1);
    press(&s.app, Key::LeftArrow);
    assert_eq!(s.app.get_local_menu_open_index(), 0);
    press(&s.app, Key::LeftArrow);
    assert_eq!(
        s.app.get_local_menu_open_index(),
        3,
        "Left wraps to the last menu"
    );
    press(&s.app, Key::Escape);
}

#[test]
fn f10_opens_and_closes_the_first_menu_and_enter_runs_the_row() {
    let s = launched("Hello");
    let page = focus_weak(&s.app);
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), 0);
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), -1, "F10 again closes");
    assert!(same_focus(&focus_weak(&s.app), &page));

    press(&s.app, Key::F10);
    let row = selected_row(&s.app);
    press(&s.app, Key::Return);
    assert_eq!(
        s.app.get_local_menu_open_index(),
        -1,
        "Enter closes the menu"
    );
    assert_eq!(s.actions.borrow().len(), 1, "Enter ran {row:?} once");
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus is back on the page"
    );
}

#[test]
fn menu_keys_do_nothing_while_a_modal_owns_the_keyboard() {
    let s = launched("Hello");
    s.app.invoke_open_palette();
    render(&s.app);
    alt(&s.app, "f");
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), -1);
    assert!(s.app.get_palette_open());
}

#[test]
fn ctrl_k_palette_runs_a_command_and_escape_closes_it_returning_focus() {
    let s = launched("Hello world");
    let page = focus_weak(&s.app);
    ctrl(&s.app, "a");
    ctrl(&s.app, "k");
    assert!(s.app.get_palette_open(), "Ctrl+K opens the palette");
    assert!(focus_name(&s.app).starts_with("Command palette"));
    type_text(&s.app, "bold");
    assert_eq!(s.app.get_palette_query(), "bold");
    assert!(s.app.get_palette_commands().row_count() > 0);
    press(&s.app, Key::Return);
    assert!(
        !s.app.get_palette_open(),
        "Enter runs the command and closes"
    );
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus is back on the page"
    );
    assert!(
        s.app.get_is_bold(),
        "the palette command bolded the selection"
    );

    ctrl(&s.app, "k");
    assert!(s.app.get_palette_open());
    press(&s.app, Key::DownArrow);
    press(&s.app, Key::UpArrow);
    press(&s.app, Key::Escape);
    assert!(!s.app.get_palette_open(), "Escape closes");
    assert!(same_focus(&focus_weak(&s.app), &page));
}

/// Types `text` as a keyboard does: a capital or a shifted symbol is pressed
/// with Shift held, so the palette receives the letter together with the
/// modifier state.
fn type_as_keyboard(app: &WriterApp, text: &str) {
    for c in text.chars() {
        if c.is_uppercase() || "!?@#$%^&*()_+{}|:\"<>~".contains(c) {
            chord(app, &[Key::Shift], &c.to_string());
        } else {
            tap(app, c.to_string());
        }
    }
}

#[test]
fn palette_query_keeps_capital_letters_and_shifted_symbols() {
    let s = launched("Hello world");
    ctrl(&s.app, "k");
    assert!(s.app.get_palette_open(), "Ctrl+K opens the palette");
    type_as_keyboard(&s.app, "Export PDF");
    assert_eq!(s.app.get_palette_query(), "Export PDF");
    assert!(
        s.app.get_palette_commands().row_count() > 0,
        "the capitalised query finds its command"
    );
    type_as_keyboard(&s.app, "!");
    assert_eq!(s.app.get_palette_query(), "Export PDF!");
}

#[test]
fn palette_says_so_when_no_command_matches() {
    let s = launched("Hello world");
    ctrl(&s.app, "k");
    type_as_keyboard(&s.app, "Zqxv?");
    assert_eq!(s.app.get_palette_query(), "Zqxv?");
    assert_eq!(s.app.get_palette_commands().row_count(), 0);
    let line = ElementHandle::find_by_accessible_label(&s.app, "No matching commands")
        .find(|element| element.size().height > 0.0)
        .expect("an empty result shows a 'No matching commands' line");
    assert!(line.computed_opacity() > 0.0, "the line is visible");
}

#[test]
fn palette_ignores_arrow_and_tab_keys_in_its_query() {
    let s = launched("Hello world");
    ctrl(&s.app, "k");
    type_as_keyboard(&s.app, "Bold");
    press(&s.app, Key::LeftArrow);
    press(&s.app, Key::RightArrow);
    press(&s.app, Key::Tab);
    assert_eq!(
        s.app.get_palette_query(),
        "Bold",
        "navigation keys are not query text"
    );
    assert!(
        s.app.get_palette_open(),
        "navigation keys do not close the palette"
    );
}

#[test]
fn ctrl_a_in_the_palette_search_selects_the_query_so_typing_replaces_it() {
    let s = launched("Hello world");
    let page_selection = s.state.current.borrow().selection();
    ctrl(&s.app, "k");
    type_as_keyboard(&s.app, "Bold");
    ctrl(&s.app, "a");
    assert_eq!(s.app.get_palette_query(), "Bold", "Ctrl+A changes no text");
    assert!(
        s.app.get_palette_query_selected(),
        "Ctrl+A selects the search text"
    );
    assert_eq!(
        s.state.current.borrow().selection(),
        page_selection,
        "the page selection is untouched"
    );
    type_text(&s.app, "Export");
    assert_eq!(
        s.app.get_palette_query(),
        "Export",
        "typing replaces the selected search text"
    );
    assert!(!s.app.get_palette_query_selected());
}

#[test]
fn backspace_after_ctrl_a_clears_the_palette_search() {
    let s = launched("Hello world");
    ctrl(&s.app, "k");
    type_as_keyboard(&s.app, "Bold");
    ctrl(&s.app, "a");
    press(&s.app, Key::Backspace);
    assert_eq!(
        s.app.get_palette_query(),
        "",
        "Backspace clears the selection"
    );
    assert!(!s.app.get_palette_query_selected());
}

#[test]
fn f6_cycles_page_toolbar_inspector_and_shift_f6_runs_it_backwards() {
    let s = launched("Hello world");
    let page = focus_weak(&s.app);
    assert!(
        s.app.get_show_inspector(),
        "the inspector is open at this size"
    );

    // Forward: page -> toolbar -> inspector -> page.
    press(&s.app, Key::F6);
    assert_eq!(
        focus_name(&s.app),
        "View",
        "F6 from the page lands on the first toolbar item"
    );
    press(&s.app, Key::F6);
    assert_ne!(
        focus_name(&s.app),
        "View",
        "F6 from the toolbar goes to the inspector"
    );
    assert!(!same_focus(&focus_weak(&s.app), &page));
    press(&s.app, Key::F6);
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "F6 from the inspector returns to the page"
    );

    // Backward: page -> inspector -> toolbar -> page.
    shift_key(&s.app, Key::F6);
    assert!(
        !same_focus(&focus_weak(&s.app), &page),
        "Shift+F6 from the page goes to the inspector"
    );
    shift_key(&s.app, Key::F6);
    assert_eq!(
        focus_name(&s.app),
        "View",
        "Shift+F6 from the inspector goes to the toolbar"
    );
    shift_key(&s.app, Key::F6);
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "Shift+F6 from the toolbar returns to the page"
    );
}

#[test]
fn ctrl_u_with_no_selection_underlines_the_next_typed_text_from_the_keyboard() {
    let s = launched("Hello");
    press(&s.app, Key::End);
    assert_eq!(
        s.state.current.borrow().selection(),
        TextSelection::caret(5),
        "End puts the caret after the text"
    );
    ctrl(&s.app, "u");
    assert_eq!(
        s.app.get_status_right().as_str(),
        "Underline on for new text",
        "after Ctrl+U (menu actions {:?}, left {:?}, state {:?})",
        s.actions.borrow(),
        s.app.get_status_left().as_str(),
        s.state.current.borrow().selection()
    );
    type_text(&s.app, "!");
    let document = s.state.current.borrow();
    assert_eq!(document.plain_text(), "Hello!");
    assert!(
        document.blocks[0]
            .runs
            .iter()
            .any(|run| run.start <= 5 && 5 < run.end && run.style.underline),
        "the typed ! is underlined"
    );
}

fn open_toolbar_menu(s: &Session, name: &str) {
    s.app.invoke_focus_page();
    press(&s.app, Key::F6);
    for _ in 0..12 {
        if focus_name(&s.app) == name {
            press(&s.app, Key::Return);
            return;
        }
        tab(&s.app);
    }
    panic!("Tab never reached {name:?}");
}

#[test]
fn toolbar_is_reachable_in_order_and_every_menu_works_from_the_keyboard() {
    let s = launched("Hello");
    // F6 leaves the page for the toolbar's first item.
    press(&s.app, Key::F6);
    assert_eq!(focus_name(&s.app), "View");
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
            "Insert",
            "Text",
            "Style",
            "List",
            "Export",
            "Format",
            "Document",
            "More actions"
        ]
    );
    // Shift+Tab walks back.
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Document");

    // Zoom: Enter opens, Down + Enter runs "Zoom In", focus returns to the page.
    let t = launched("Hello");
    let page = focus_weak(&t.app);
    open_toolbar_menu(&t, "Zoom");
    assert_eq!(t.app.get_toolbar_menu(), 2, "Enter opened the Zoom menu");
    press(&t.app, Key::Return);
    assert_eq!(t.app.get_toolbar_menu(), 0, "Enter ran a row and closed it");
    assert!(
        same_focus(&focus_weak(&t.app), &page),
        "focus is back on the page"
    );
    assert!(
        (t.state.viewport.borrow().zoom - 1.25).abs() < 1e-3,
        "the first Zoom row is Zoom In"
    );

    // Escape closes a menu and puts focus back on the button that opened it.
    open_toolbar_menu(&t, "Insert");
    assert_eq!(t.app.get_toolbar_menu(), 3);
    press(&t.app, Key::Escape);
    assert_eq!(t.app.get_toolbar_menu(), 0);
    assert_eq!(focus_name(&t.app), "Insert", "focus returns to the trigger");

    // The overflow menu behaves the same.
    open_toolbar_menu(&t, "More actions");
    assert!(t.app.get_toolbar_overflow_open());
    press(&t.app, Key::Escape);
    assert!(!t.app.get_toolbar_overflow_open());
    assert_eq!(focus_name(&t.app), "More actions");
}

fn request_close(app: &WriterApp) {
    app.window().dispatch_event(WindowEvent::CloseRequested);
    render(app);
}

/// Counts for the three Save Changes outcomes; closing the dialog the way the
/// live window's handlers do, so focus restoration is exercised too.
fn count_save_changes(app: &WriterApp) -> Rc<RefCell<Vec<&'static str>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let (weak, seen) = (app.as_weak(), log.clone());
    app.on_save_changes_cancel(move || {
        seen.borrow_mut().push("cancel");
        weak.upgrade().unwrap().set_save_changes_open(false);
    });
    let (weak, seen) = (app.as_weak(), log.clone());
    app.on_save_changes_discard(move || {
        seen.borrow_mut().push("discard");
        weak.upgrade().unwrap().set_save_changes_open(false);
    });
    let (weak, seen) = (app.as_weak(), log.clone());
    app.on_save_changes_save(move || {
        seen.borrow_mut().push("save");
        weak.upgrade().unwrap().set_save_changes_open(false);
    });
    log
}

#[test]
fn save_changes_traps_tab_defaults_to_save_and_escape_cancels() {
    let s = launched("Hello");
    let page = focus_weak(&s.app);
    type_text(&s.app, "!");
    let log = count_save_changes(&s.app);
    request_close(&s.app);
    assert!(
        s.app.get_save_changes_open(),
        "closing a dirty document asks first"
    );
    assert_eq!(focus_name(&s.app), "Save", "the primary button has focus");

    // Tab cycles Cancel, Discard, Save and never leaves the dialog.
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

    // Escape cancels from any button, and focus returns to the page.
    press(&s.app, Key::Escape);
    assert_eq!(*log.borrow(), ["cancel"]);
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus returns to the page"
    );

    // Enter activates the focused button: Save by default, Discard after a Tab.
    request_close(&s.app);
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel", "save"]);
    request_close(&s.app);
    shift_tab(&s.app);
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel", "save", "discard"]);
}

#[test]
fn template_chooser_is_keyboard_complete_and_traps_tab() {
    let s = launched("Hello");
    let page = focus_weak(&s.app);
    let log: Rc<RefCell<Vec<String>>> = Rc::default();
    let (weak, seen) = (s.app.as_weak(), log.clone());
    s.app.on_create_template(move |index| {
        seen.borrow_mut().push(format!("create {index}"));
        weak.upgrade().unwrap().set_template_chooser_open(false);
    });
    let (weak, seen) = (s.app.as_weak(), log.clone());
    s.app.on_cancel_template(move || {
        seen.borrow_mut().push("cancel".into());
        weak.upgrade().unwrap().set_template_chooser_open(false);
    });

    ctrl(&s.app, "n");
    assert!(
        s.app.get_template_chooser_open(),
        "Ctrl+N opens the chooser"
    );
    assert_eq!(s.app.get_template_selected(), 0);

    // Left/Right choose a template, Up/Down a category.
    press(&s.app, Key::RightArrow);
    press(&s.app, Key::RightArrow);
    assert_eq!(s.app.get_template_selected(), 2);
    press(&s.app, Key::LeftArrow);
    assert_eq!(s.app.get_template_selected(), 1);
    press(&s.app, Key::LeftArrow);
    press(&s.app, Key::LeftArrow);
    assert_eq!(
        s.app.get_template_selected(),
        0,
        "Left stops at the first template"
    );
    press(&s.app, Key::DownArrow);
    assert_eq!(s.app.get_template_category(), 1);
    press(&s.app, Key::DownArrow);
    assert_eq!(s.app.get_template_category(), 2);
    assert_eq!(
        s.app.get_template_selected(),
        2,
        "Reports selects the report template"
    );
    press(&s.app, Key::UpArrow);
    press(&s.app, Key::UpArrow);
    assert_eq!(s.app.get_template_category(), 0);

    // Tab cycles chooser -> Cancel -> Create Document -> chooser, both ways.
    let chooser = focus_weak(&s.app);
    tab(&s.app);
    assert_eq!(focus_name(&s.app), "Cancel");
    tab(&s.app);
    assert_eq!(focus_name(&s.app), "Create Document");
    tab(&s.app);
    assert!(
        same_focus(&focus_weak(&s.app), &chooser),
        "Tab wrapped inside the dialog"
    );
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Create Document");
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Cancel");
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel"], "Enter on Cancel cancels");
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus returns to the page"
    );

    // Return on the chooser itself creates the selected template.
    ctrl(&s.app, "n");
    press(&s.app, Key::RightArrow);
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel", "create 1"]);
    assert!(same_focus(&focus_weak(&s.app), &page));

    // Escape cancels.
    ctrl(&s.app, "n");
    press(&s.app, Key::Escape);
    assert_eq!(log.borrow().last().map(String::as_str), Some("cancel"));
    assert!(same_focus(&focus_weak(&s.app), &page));
}

fn selected_text(state: &GuiState) -> String {
    let document = state.current.borrow();
    let selection = document.selection();
    let (low, high) = (
        selection.anchor.min(selection.focus),
        selection.anchor.max(selection.focus),
    );
    document.editor_text()[low..high].to_string()
}

#[test]
fn find_bar_opens_steps_replaces_and_closes_from_the_keyboard() {
    let s = launched("one cat, two cats, three cats");
    let page = focus_weak(&s.app);
    let bar = s.app.global::<FindBar>();

    ctrl(&s.app, "f");
    assert!(bar.get_open(), "Ctrl+F opens the bar");
    assert_eq!(focus_name(&s.app), "Find", "the query field has focus");
    type_text(&s.app, "cat");
    assert_eq!(bar.get_query(), "cat");
    assert!(
        bar.get_status().contains("of 3"),
        "status: {}",
        bar.get_status()
    );
    let first = selected_text(&s.state);
    assert_eq!(first, "cat");
    let first_range = s.state.current.borrow().selection();

    // Enter steps forward, Shift+Enter back.
    press(&s.app, Key::Return);
    let second_range = s.state.current.borrow().selection();
    assert_ne!(first_range, second_range, "Enter moved to the next match");
    shift_key(&s.app, Key::Return);
    assert_eq!(
        s.state.current.borrow().selection(),
        first_range,
        "Shift+Enter went back"
    );

    // Tab reaches Match case and every button, in reading order, inside the bar.
    let mut names = Vec::new();
    for _ in 0..4 {
        tab(&s.app);
        names.push(focus_name(&s.app));
    }
    assert_eq!(names, ["Previous", "Next", "Done", "Match case"]);

    // Escape closes from a button, not only from the field, and returns to the page.
    press(&s.app, Key::Escape);
    assert!(!bar.get_open());
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus returns to the page"
    );

    // Ctrl+H adds the replace row; Enter there replaces the current match.
    ctrl(&s.app, "h");
    assert!(bar.get_open() && bar.get_show_replace());
    assert_eq!(focus_name(&s.app), "Find");
    tab_to(&s.app, "Replace with");
    type_text(&s.app, "dog");
    press(&s.app, Key::Return);
    assert!(
        s.state.current.borrow().plain_text().contains("dog"),
        "Enter in the replace field replaced a match"
    );
    press(&s.app, Key::Escape);
    assert!(same_focus(&focus_weak(&s.app), &page));
}

fn tab_to(app: &WriterApp, name: &str) {
    for _ in 0..16 {
        if focus_name(app) == name {
            return;
        }
        tab(app);
    }
    panic!("Tab never reached {name:?}");
}

/// Walk Tab from the current focus until it reaches `until` (the page) again,
/// returning every stop on the way.
fn walk_until(app: &WriterApp, until: &ItemWeak, limit: usize) -> Vec<(ItemWeak, Stop)> {
    let mut stops = Vec::new();
    for _ in 0..limit {
        tab(app);
        if same_focus(&focus_weak(app), until) {
            return stops;
        }
        stops.push((focus_weak(app), stop(app).expect("a stop")));
    }
    panic!("Tab never came back to the page within {limit} presses");
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

fn assert_each_stop_is_usable(stops: &[(ItemWeak, Stop)]) {
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
                "stop {i} and {j} are the same control"
            );
        }
    }
}

#[test]
fn tab_order_is_logical_named_visible_and_free_of_traps() {
    for tab_index in [0, 1] {
        let s = launched("Hello world");
        s.app.set_inspector_tab(tab_index);
        render(&s.app);
        let page = focus_weak(&s.app);

        // The page keeps Tab for itself (it types a tab stop), so it cannot
        // trap: F6 leaves it for the inspector.
        let before = stop(&s.app).unwrap().name;
        tab(&s.app);
        assert!(
            same_focus(&focus_weak(&s.app), &page),
            "Tab stays in the page"
        );
        assert_eq!(
            s.state.current.borrow().plain_text().len(),
            "Hello world".len() + 4
        );
        assert_eq!(
            stop(&s.app).unwrap().name.split(',').next(),
            before.split(',').next()
        );
        press(&s.app, Key::F6);
        assert!(
            !same_focus(&focus_weak(&s.app), &page),
            "F6 leaves the page"
        );
        // The first F6 lands on the toolbar; the second hands over to the inspector.
        press(&s.app, Key::F6);

        let stops = walk_until(&s.app, &page, 120);
        assert_each_stop_is_usable(&stops);
        let first = stop(&s.app);
        let _ = first;

        // Order: the rest of the inspector, then the menu bar, then the toolbar.
        let infos: Vec<&Stop> = stops.iter().map(|(_, s)| s).collect();
        let menu_start = infos.iter().position(|s| s.y < 36.0).expect("menu stops");
        let toolbar_start = infos
            .iter()
            .position(|s| s.x < 900.0 && (36.0..90.0).contains(&s.y))
            .expect("toolbar stops");
        assert!(menu_start < toolbar_start);
        let (inspector, rest) = infos.split_at(menu_start);
        assert!(
            inspector.iter().all(|s| s.x >= 900.0),
            "inspector docks at the reading end"
        );
        assert!(
            reads_in_order(inspector),
            "inspector order: {:?}",
            names(inspector)
        );
        let (menu, toolbar) = rest.split_at(toolbar_start - menu_start);
        assert_eq!(names(menu), ["File", "Edit", "View", "Format"]);
        assert!(reads_in_order(menu));
        assert!(
            reads_in_order(toolbar),
            "toolbar order: {:?}",
            names(toolbar)
        );
        assert_eq!(
            names(toolbar),
            [
                "View",
                "Zoom",
                "Insert",
                "Text",
                "Style",
                "List",
                "Export",
                "Format",
                "Document",
                "More actions"
            ]
        );
    }
}

fn names(stops: &[&Stop]) -> Vec<String> {
    stops.iter().map(|s| s.name.clone()).collect()
}

#[test]
fn type_format_find_and_save_without_a_pointer() {
    let dir = std::env::temp_dir().join(format!("loom-writer-keyboard-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("keyboard.loomdoc");
    let s = launched_with(blank_document(), scripted(vec![Some(target.clone())]));

    type_text(&s.app, "Hello keyboard world");
    assert_eq!(
        s.state.current.borrow().plain_text(),
        "Hello keyboard world"
    );

    // Select the last word with Ctrl+Shift+Left, then format it with shortcuts.
    chord(
        &s.app,
        &[Key::Control, Key::Shift],
        &String::from(char::from(Key::LeftArrow)),
    );
    assert_eq!(selected_text(&s.state), "world");
    ctrl(&s.app, "b");
    assert!(s.app.get_is_bold(), "Ctrl+B bolds the selection");
    ctrl(&s.app, "i");
    assert!(s.app.get_is_italic(), "Ctrl+I italicises it");
    // Count the request here; the handler itself is covered by typing_style_tests.
    let underlines = Rc::new(Cell::new(0));
    let seen = underlines.clone();
    s.app.on_toggle_underline(move || seen.set(seen.get() + 1));
    ctrl(&s.app, "u");
    assert_eq!(underlines.get(), 1, "Ctrl+U asks to underline");
    ctrl(&s.app, "z");
    assert!(!s.app.get_is_italic(), "Ctrl+Z undoes the last format");
    chord(&s.app, &[Key::Control, Key::Shift], "z");
    assert!(s.app.get_is_italic(), "Ctrl+Shift+Z redoes it");

    // Find, step, and come back to type.
    ctrl(&s.app, "f");
    assert_eq!(
        s.app.global::<FindBar>().get_query(),
        "world",
        "Ctrl+F searches for the selected word"
    );
    ctrl(&s.app, "a");
    type_text(&s.app, "keyboard");
    assert_eq!(s.app.global::<FindBar>().get_query(), "keyboard");
    assert_eq!(selected_text(&s.state), "keyboard");
    press(&s.app, Key::Escape);
    press(&s.app, Key::RightArrow);
    type_text(&s.app, "!");
    assert_eq!(
        s.state.current.borrow().plain_text(),
        "Hello keyboard! world"
    );

    // Save with Ctrl+S: the scripted dialog picks the path, the file is written.
    ctrl(&s.app, "s");
    assert!(target.exists(), "Ctrl+S saved {}", target.display());
    assert_eq!(
        *s.state.save_path.borrow(),
        Some(target.clone()),
        "the document remembers where it was saved"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn outline_headings_are_keyboard_reachable_and_escape_closes_the_pane() {
    let mut doc = text_document("Intro\nBody one\nMiddle\nBody two\nEnd");
    for (index, kind) in [(0usize, "heading1"), (2, "heading2"), (4, "heading1")] {
        doc.blocks[index].kind = kind.to_string();
    }
    let s = launched_with(doc, scripted(vec![]));
    let page = focus_weak(&s.app);
    s.app.set_show_navigator(true);
    apply_state(&s.app, &s.state);
    render(&s.app);

    // Every heading is a Tab stop with its title as the name.
    s.app.invoke_focus_page();
    press(&s.app, Key::F6);
    tab_to(&s.app, "Intro");
    tab(&s.app);
    assert_eq!(focus_name(&s.app), "Middle");
    assert!(stop(&s.app).unwrap().on_screen());
    press(&s.app, Key::Return);
    let expected = "Intro\nBody one\n".len();
    assert_eq!(
        s.state.current.borrow().selection(),
        TextSelection::caret(expected),
        "Enter moved the caret to the heading"
    );
    assert!(
        same_focus(&focus_weak(&s.app), &page),
        "focus is back on the page"
    );

    // Escape closes the pane from a heading.
    press(&s.app, Key::F6);
    tab_to(&s.app, "End");
    press(&s.app, Key::Escape);
    assert!(!s.app.get_show_navigator(), "Escape closed the outline");
    assert!(same_focus(&focus_weak(&s.app), &page));
}

#[test]
fn alt_letters_open_menus_from_the_find_field_without_typing_into_it() {
    let s = launched("Hello world");
    ctrl(&s.app, "f");
    type_text(&s.app, "wor");
    alt(&s.app, "e");
    assert_eq!(
        s.app.get_local_menu_open_index(),
        1,
        "Alt+E opens Edit from the find field"
    );
    assert_eq!(
        s.app.global::<FindBar>().get_query(),
        "wor",
        "no letter was typed"
    );
    press(&s.app, Key::Escape);
}
