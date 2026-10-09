//! Keyboard-only operation of Sheets: menus, palette, toolbar, dialogs, the
//! template chooser and the type/formula/navigate/save workflow. Every step
//! sends real key events through `Window::dispatch_event`, so Slint's own
//! focus handling and key bubbling decide the result. Focus is read from the
//! window, then matched to the named accessibility element under it.

use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle, ElementRoot};
use slint::platform::{Key, WindowEvent};
use slint::private_unstable_api::re_exports::{ItemWeak, WindowInner};

const WIDTH: f32 = 1280.0;
const HEIGHT: f32 = 800.0;

/// Where keyboard focus is: its accessible name, role and window rectangle.
#[derive(Clone, Debug)]
struct Stop {
    name: String,
    role: Option<AccessibleRole>,
    /// Whether the control under focus reports itself enabled.
    enabled: Option<bool>,
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
    pub(super) app: SheetsApp,
    pub(super) state: Rc<GuiState>,
    actions: Rc<RefCell<Vec<String>>>,
    pub(super) menu: Arc<NativeMenuBar>,
}

/// The window as `run_gui` wires it, minus the background worker and the
/// native file pickers: custom chrome with the in-window menu, the real menu
/// bar, palette, toolbar, cell editing, tab-run navigation, objects, grid
/// pointer, history and sheet actions; inspector open; grid focused.
pub(super) fn launched(cells: &[(&str, &str)]) -> Session {
    let (app, state) = super::grid_pointer_tests::projected(cells);
    window_chrome::install(&app);
    app.set_local_menu_visible(true);
    toolbar_commands::start_with_inspector_open(&app);
    toolbar_commands::wire(&app);
    let weak = app.as_weak();
    app.on_toggle_inspector(move || {
        let app = weak.upgrade().expect("live window");
        let next = !app.get_show_inspector();
        app.set_inspector_preference(next);
        app.set_show_inspector(next);
    });
    let weak = app.as_weak();
    app.on_inspector_context_changed(move |tab| {
        weak.upgrade().expect("live window").set_inspector_tab(tab);
    });
    let menu = Arc::new(NativeMenuBar::new());
    tab_run::register_navigation(&app, &state);
    wire_selection_extension(&app, &state);
    register_cell_edit_action(&app, &state, &menu);
    // These two are wired inline by `run_gui`; the same behaviour is repeated here.
    let weak = app.as_weak();
    app.on_begin_edit(move |initial_text| {
        if let Some(app) = weak.upgrade() {
            app.set_formula_edit_buffer(initial_text);
            app.invoke_focus_formula_bar();
        }
    });
    let (weak, shared) = (app.as_weak(), state.clone());
    app.on_cancel_selected_cell(move || {
        if let Some(app) = weak.upgrade() {
            shared.tab_run.reset();
            app.invoke_reset_formula_edit_buffer();
            app.set_formula_feedback("Edit cancelled".into());
        }
    });
    object_actions::register_object_actions(&app, &state, &menu);
    menu.install_menu_bar(&local_menu::sheets_menu_bar())
        .expect("install menu bar");
    local_menu::wire_keyboard(&app);
    let actions = Rc::new(RefCell::new(Vec::new()));
    let seen = actions.clone();
    app.on_local_menu_action(move |id| seen.borrow_mut().push(id.to_string()));
    local_menu::sync(&app, &menu).expect("project menu");
    wire_palette(&app);
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    project_current(&app, &state);
    render(&app);
    app.invoke_focus_grid();
    render(&app);
    Session {
        app,
        state,
        actions,
        menu,
    }
}

fn render(app: &SheetsApp) {
    let _ = snapshot_component(app, WIDTH, HEIGHT, 1.0).expect("render");
}

fn key_down(app: &SheetsApp, text: impl Into<SharedString>) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.into() });
}

fn key_up(app: &SheetsApp, text: impl Into<SharedString>) {
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: text.into() });
}

fn tap(app: &SheetsApp, text: impl Into<SharedString>) {
    let text = text.into();
    key_down(app, text.clone());
    key_up(app, text);
    render(app);
}

fn press(app: &SheetsApp, key: Key) {
    tap(app, SharedString::from(key));
}

fn chord(app: &SheetsApp, modifiers: &[Key], text: &str) {
    for m in modifiers {
        key_down(app, SharedString::from(*m));
    }
    tap(app, text);
    for m in modifiers.iter().rev() {
        key_up(app, SharedString::from(*m));
    }
}

fn ctrl(app: &SheetsApp, text: &str) {
    chord(app, &[Key::Control], text);
}

fn alt(app: &SheetsApp, letter: &str) {
    chord(app, &[Key::Alt], letter);
}

fn shift_key(app: &SheetsApp, key: Key) {
    chord(app, &[Key::Shift], &String::from(char::from(key)));
}

fn type_text(app: &SheetsApp, text: &str) {
    for c in text.chars() {
        tap(app, c.to_string());
    }
}

fn tab(app: &SheetsApp) {
    press(app, Key::Tab);
}

fn shift_tab(app: &SheetsApp) {
    shift_key(app, Key::Tab);
}

fn focus_weak(app: &SheetsApp) -> ItemWeak {
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
fn stop(app: &SheetsApp) -> Option<Stop> {
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
    let (name, role, enabled) = best
        .map(|(_, e)| {
            (
                e.accessible_label().unwrap().to_string(),
                e.accessible_role(),
                e.accessible_enabled(),
            )
        })
        .unwrap_or_default();
    Some(Stop {
        name,
        role,
        enabled,
        x: origin.x,
        y: origin.y,
        w: geometry.size.width,
        h: geometry.size.height,
        visible: item.is_visible(),
    })
}

fn focus_name(app: &SheetsApp) -> String {
    stop(app).map(|s| s.name).unwrap_or_default()
}

fn labels(app: &SheetsApp) -> Vec<String> {
    app.get_local_menu_labels()
        .iter()
        .map(|l| l.to_string())
        .collect()
}

fn popup_labels(app: &SheetsApp) -> Vec<String> {
    app.get_local_menu_popup_items()
        .iter()
        .map(|i| i.label.to_string())
        .collect()
}

fn selected_row(app: &SheetsApp) -> String {
    let index = app.get_local_menu_popup_selected_index();
    app.get_local_menu_popup_items()
        .row_data(index as usize)
        .map(|i| i.label.to_string())
        .unwrap_or_default()
}

fn cell(s: &Session, reference: &str) -> String {
    s.state
        .current
        .borrow()
        .raw(CellRef::parse(reference).expect("cell reference"))
        .unwrap_or("")
        .to_string()
}

fn tab_to(app: &SheetsApp, name: &str) {
    for _ in 0..60 {
        if focus_name(app) == name {
            return;
        }
        tab(app);
    }
    panic!("Tab never reached {name:?}");
}

#[test]
fn alt_letters_open_every_menu_and_arrows_walk_them() {
    let s = launched(&[("A1", "1")]);
    let grid = focus_weak(&s.app);
    let menus = labels(&s.app);
    assert_eq!(menus, ["File", "Edit", "View", "Table", "Help"]);

    for (index, letter) in ["f", "e", "v", "t", "h"].iter().enumerate() {
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
            same_focus(&focus_weak(&s.app), &grid),
            "Escape returns to the grid"
        );
    }
    assert_eq!(cell(&s, "A1"), "1", "Alt+letter must not start a cell edit");
    assert!(!s.app.get_is_editing());

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
        4,
        "Left wraps to the last menu"
    );
    press(&s.app, Key::Escape);
}

#[test]
fn f10_opens_and_closes_the_first_menu_and_enter_runs_a_row() {
    let s = launched(&[]);
    let grid = focus_weak(&s.app);
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), 0);
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), -1, "F10 again closes");
    assert!(same_focus(&focus_weak(&s.app), &grid));

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
        same_focus(&focus_weak(&s.app), &grid),
        "focus is back on the grid"
    );
}

#[test]
fn menu_keys_do_nothing_while_a_modal_owns_the_keyboard() {
    let s = launched(&[]);
    s.app.invoke_open_palette();
    render(&s.app);
    alt(&s.app, "f");
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), -1);
    assert!(s.app.get_palette_open());
}

#[test]
fn ctrl_k_palette_runs_a_command_and_escape_closes_it_returning_focus() {
    let s = launched(&[("A1", "1"), ("B2", "2")]);
    let grid = focus_weak(&s.app);
    ctrl(&s.app, "k");
    assert!(s.app.get_palette_open(), "Ctrl+K opens the palette");
    assert!(focus_name(&s.app).starts_with("Command palette"));
    type_text(&s.app, "select all");
    assert!(s.app.get_palette_commands().row_count() > 0);
    press(&s.app, Key::Return);
    assert!(
        !s.app.get_palette_open(),
        "Enter runs the command and closes"
    );
    assert_eq!(
        s.app.get_selection_range(),
        "A1:B2",
        "the palette selected everything"
    );
    assert!(
        same_focus(&focus_weak(&s.app), &grid),
        "focus is back on the grid"
    );

    ctrl(&s.app, "k");
    press(&s.app, Key::DownArrow);
    press(&s.app, Key::Escape);
    assert!(!s.app.get_palette_open(), "Escape closes");
    assert!(same_focus(&focus_weak(&s.app), &grid));
}

/// Put focus on a toolbar item: F6 from the grid lands on the first one and
/// Tab walks the rest.
fn focus_toolbar_item(s: &Session, name: &str) {
    s.app.invoke_focus_grid();
    render(&s.app);
    press(&s.app, Key::F6);
    for _ in 0..14 {
        if focus_name(&s.app) == name && stop(&s.app).is_some_and(|x| x.y < 90.0) {
            return;
        }
        tab(&s.app);
    }
    panic!("Tab never reached toolbar item {name:?}");
}

#[test]
fn toolbar_menus_open_run_and_close_from_the_keyboard_returning_focus() {
    let s = launched(&[("A1", "1")]);
    let grid = focus_weak(&s.app);
    // Zoom: Enter opens, Enter runs the first row (Zoom In), focus goes back to the grid.
    focus_toolbar_item(&s, "Zoom");
    press(&s.app, Key::Return);
    assert_eq!(s.app.get_toolbar_menu(), 2, "Enter opened the Zoom menu");
    press(&s.app, Key::DownArrow);
    press(&s.app, Key::UpArrow);
    press(&s.app, Key::Return);
    assert_eq!(s.app.get_toolbar_menu(), 0, "a row closes the menu");
    assert!(s.app.get_zoom_factor() > 1.0, "Zoom In ran");
    assert!(
        same_focus(&focus_weak(&s.app), &grid),
        "focus is back on the grid"
    );

    // Escape closes the menu and returns focus to the trigger.
    for (name, kind) in [("View", 1), ("Table", 3), ("Text", 4), ("Export", 5)] {
        focus_toolbar_item(&s, name);
        press(&s.app, Key::Return);
        assert_eq!(s.app.get_toolbar_menu(), kind, "{name} opens menu {kind}");
        press(&s.app, Key::Escape);
        assert_eq!(s.app.get_toolbar_menu(), 0);
        assert_eq!(focus_name(&s.app), name, "Escape returns focus to {name}");
    }

    // The overflow menu returns to its trigger too.
    focus_toolbar_item(&s, "More actions");
    press(&s.app, Key::Return);
    assert!(s.app.get_toolbar_overflow_open());
    press(&s.app, Key::Escape);
    assert!(!s.app.get_toolbar_overflow_open());
    assert_eq!(focus_name(&s.app), "More actions");
}

fn count_save_changes(app: &SheetsApp) -> Rc<RefCell<Vec<&'static str>>> {
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
    let s = launched(&[("A1", "1")]);
    let grid = focus_weak(&s.app);
    let log = count_save_changes(&s.app);
    s.app.set_save_changes_document("Budget".into());
    s.app.set_save_changes_open(true);
    render(&s.app);
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
    assert_eq!(*log.borrow(), ["cancel"], "Escape cancels from a button");
    assert!(
        same_focus(&focus_weak(&s.app), &grid),
        "focus returns to the grid"
    );

    s.app.set_save_changes_open(true);
    render(&s.app);
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel", "save"], "Enter saves by default");
    s.app.set_save_changes_open(true);
    render(&s.app);
    shift_tab(&s.app);
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel", "save", "discard"]);
}

#[test]
fn xlsx_import_warning_traps_tab_and_cancel_is_the_default() {
    let s = launched(&[("A1", "1")]);
    let grid = focus_weak(&s.app);
    let log: Rc<RefCell<Vec<&'static str>>> = Rc::default();
    let (weak, seen) = (s.app.as_weak(), log.clone());
    s.app.on_xlsx_import_cancel(move || {
        seen.borrow_mut().push("cancel");
        weak.upgrade().unwrap().set_xlsx_import_warning_open(false);
    });
    let (weak, seen) = (s.app.as_weak(), log.clone());
    s.app.on_xlsx_import_continue(move || {
        seen.borrow_mut().push("continue");
        weak.upgrade().unwrap().set_xlsx_import_warning_open(false);
    });
    s.app
        .set_xlsx_import_warning_message("Pivot tables will be flattened.".into());
    s.app.set_xlsx_import_warning_open(true);
    render(&s.app);
    assert_eq!(
        focus_name(&s.app),
        "Cancel import",
        "the safe choice has focus"
    );
    tab(&s.app);
    assert_eq!(focus_name(&s.app), "Import with feature loss");
    tab(&s.app);
    assert_eq!(
        focus_name(&s.app),
        "Cancel import",
        "Tab wraps inside the dialog"
    );
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Import with feature loss");
    assert!(s.app.get_xlsx_import_warning_open());
    press(&s.app, Key::Escape);
    assert_eq!(*log.borrow(), ["cancel"], "Escape cancels from a button");
    assert!(
        same_focus(&focus_weak(&s.app), &grid),
        "focus returns to the grid"
    );

    s.app.set_xlsx_import_warning_open(true);
    render(&s.app);
    press(&s.app, Key::Return);
    assert_eq!(
        *log.borrow(),
        ["cancel", "cancel"],
        "Enter keeps the current workbook"
    );
    s.app.set_xlsx_import_warning_open(true);
    render(&s.app);
    tab(&s.app);
    press(&s.app, Key::Return);
    assert_eq!(*log.borrow(), ["cancel", "cancel", "continue"]);
}

#[test]
fn template_chooser_is_keyboard_complete_and_traps_tab() {
    let s = launched(&[("A1", "1")]);
    let grid = focus_weak(&s.app);
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

    // Open it from the File menu without a pointer.
    alt(&s.app, "f");
    for _ in 0..8 {
        if selected_row(&s.app).starts_with("New from Template") {
            break;
        }
        press(&s.app, Key::DownArrow);
    }
    assert!(selected_row(&s.app).starts_with("New from Template"));
    press(&s.app, Key::Return);
    assert_eq!(*s.actions.borrow(), ["file.new_template"]);
    // The menu action is routed by the live window; open the chooser as it does.
    s.app.set_template_chooser_open(true);
    s.app.invoke_focus_template_chooser();
    render(&s.app);
    let chooser = focus_weak(&s.app);

    let before = s.app.get_template_selected();
    press(&s.app, Key::RightArrow);
    assert_ne!(
        s.app.get_template_selected(),
        before,
        "Right picks the next template"
    );
    press(&s.app, Key::LeftArrow);
    assert_eq!(s.app.get_template_selected(), before);
    press(&s.app, Key::DownArrow);
    assert_eq!(s.app.get_template_category(), 1);
    press(&s.app, Key::DownArrow);
    assert_eq!(s.app.get_template_category(), 2);
    press(&s.app, Key::UpArrow);
    press(&s.app, Key::UpArrow);
    assert_eq!(s.app.get_template_category(), 0);

    tab(&s.app);
    assert_eq!(focus_name(&s.app), "Cancel");
    tab(&s.app);
    assert_eq!(focus_name(&s.app), "Create");
    tab(&s.app);
    assert!(
        same_focus(&focus_weak(&s.app), &chooser),
        "Tab wrapped inside the dialog"
    );
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Create", "Shift+Tab wraps backwards");
    shift_tab(&s.app);
    assert_eq!(focus_name(&s.app), "Cancel");
    press(&s.app, Key::Escape);
    assert_eq!(*log.borrow(), ["cancel"], "Escape cancels from a button");
    assert!(
        same_focus(&focus_weak(&s.app), &grid),
        "focus returns to the grid"
    );
}

#[test]
fn type_values_and_formulas_navigate_undo_and_save_without_a_pointer() {
    let s = launched(&[]);
    let saves = Rc::new(Cell::new(0));
    let seen = saves.clone();
    s.app.on_save_sheet(move || seen.set(seen.get() + 1));

    // Typing on a selected cell starts an edit; Enter commits and moves down.
    type_text(&s.app, "10");
    assert!(s.app.get_is_editing(), "typing started an edit");
    press(&s.app, Key::Return);
    assert_eq!(cell(&s, "A1"), "10", "Enter committed the value");
    assert_eq!(s.app.get_selected_cell(), "A2", "and moved down");

    // A formula is typed the same way.
    type_text(&s.app, "=A1*2");
    press(&s.app, Key::Return);
    assert_eq!(cell(&s, "A2"), "=A1*2");
    assert_eq!(s.app.get_selected_cell(), "A3");
    let a2 = s
        .state
        .current
        .borrow()
        .raw(CellRef::parse("A2").unwrap())
        .map(str::to_string);
    assert_eq!(a2.as_deref(), Some("=A1*2"));

    // Tab runs across a row; Enter returns to the column the run began in.
    type_text(&s.app, "5");
    press(&s.app, Key::Tab);
    type_text(&s.app, "6");
    press(&s.app, Key::Return);
    assert_eq!(cell(&s, "A3"), "5");
    assert_eq!(cell(&s, "B3"), "6");
    assert_eq!(
        s.app.get_selected_cell(),
        "A4",
        "Enter after a Tab run returns to column A"
    );

    // Arrow keys move the cursor; Escape abandons an edit without changing the cell.
    press(&s.app, Key::UpArrow);
    press(&s.app, Key::UpArrow);
    assert_eq!(s.app.get_selected_cell(), "A2");
    type_text(&s.app, "oops");
    press(&s.app, Key::Escape);
    assert_eq!(cell(&s, "A2"), "=A1*2", "Escape cancelled the edit");
    assert!(!s.app.get_is_editing());
    press(&s.app, Key::LeftArrow);
    assert_eq!(
        s.app.get_selected_cell(),
        "A2",
        "arrows still move after Escape"
    );

    // Shift+Arrow selects a range; Delete clears it.
    press(&s.app, Key::UpArrow);
    shift_key(&s.app, Key::DownArrow);
    shift_key(&s.app, Key::DownArrow);
    assert_eq!(s.app.get_selection_range(), "A1:A3");
    press(&s.app, Key::Delete);
    assert_eq!(cell(&s, "A1"), "");
    assert_eq!(cell(&s, "A3"), "");
    ctrl(&s.app, "z");
    assert_eq!(cell(&s, "A1"), "10", "Ctrl+Z restored the cleared cells");

    // Ctrl+S asks to save; the grid still has focus afterwards.
    ctrl(&s.app, "s");
    assert_eq!(saves.get(), 1, "Ctrl+S reached the save command");
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum Region {
    Inspector,
    Menu,
    Toolbar,
    SheetTabs,
    FormulaBar,
}

fn region(stop: &Stop) -> Region {
    if stop.y < 36.0 {
        Region::Menu
    } else if stop.y < 84.0 {
        Region::Toolbar
    } else if stop.y < 114.0 {
        Region::SheetTabs
    } else if stop.y < 146.0 {
        Region::FormulaBar
    } else {
        Region::Inspector
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
fn a_typed_value_is_stored_exactly_whether_the_grid_or_the_formula_bar_has_focus() {
    // Typing "42" right after launch must store "42": no seeded space, and
    // the same result whichever control holds focus when the first key lands.
    for formula_bar_first in [false, true] {
        let s = launched(&[]);
        if formula_bar_first {
            s.app.invoke_focus_formula_bar();
            render(&s.app);
        }
        type_text(&s.app, "42");
        press(&s.app, Key::Return);
        let stored = s
            .state
            .current
            .borrow()
            .raw(CellRef::parse("A1").unwrap())
            .map(str::to_owned);
        assert_eq!(
            stored.as_deref(),
            Some("42"),
            "formula bar focused first: {formula_bar_first}"
        );
    }
}

#[test]
fn a_disabled_control_never_takes_keyboard_focus_in_the_inspector() {
    for tab_index in [0, 1] {
        let s = launched(&[("A1", "1")]);
        s.app.set_inspector_tab(tab_index);
        render(&s.app);
        let grid = focus_weak(&s.app);
        press(&s.app, Key::F6);
        shift_key(&s.app, Key::F6);
        let mut visited = Vec::new();
        for _ in 0..120 {
            visited.push(stop(&s.app).expect("a focused control"));
            tab(&s.app);
            if same_focus(&focus_weak(&s.app), &grid) {
                break;
            }
        }
        for visit in &visited {
            assert_ne!(
                visit.enabled,
                Some(false),
                "tab {tab_index}: disabled control {:?} took keyboard focus",
                visit.name
            );
        }
    }
}

#[test]
fn tab_order_is_logical_named_visible_and_free_of_traps() {
    for tab_index in [0, 1] {
        let s = launched(&[("A1", "1")]);
        s.app.set_inspector_tab(tab_index);
        render(&s.app);
        let grid = focus_weak(&s.app);

        // The grid keeps Tab for the cell cursor, so F6 is how the keyboard leaves
        // it: toolbar first, then Shift+F6 for the inspector.
        press(&s.app, Key::F6);
        shift_key(&s.app, Key::F6);
        assert!(focus_name(&s.app).starts_with("Inspector"));
        let mut stops = vec![(focus_weak(&s.app), stop(&s.app).unwrap())];
        loop {
            tab(&s.app);
            if same_focus(&focus_weak(&s.app), &grid) {
                break;
            }
            assert!(stops.len() < 120, "Tab never came back to the grid");
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

        // One block per region, in the fixed order inspector, menu bar, toolbar,
        // sheet tabs, formula bar; each reads left to right, top to bottom.
        let mut blocks: Vec<(Region, Vec<&Stop>)> = Vec::new();
        for (_, stop) in &stops {
            let r = region(stop);
            match blocks.last_mut() {
                Some((last, group)) if *last == r => group.push(stop),
                _ => blocks.push((r, vec![stop])),
            }
        }
        let order: Vec<Region> = blocks.iter().map(|(r, _)| *r).collect();
        assert_eq!(
            order,
            [
                Region::Inspector,
                Region::Menu,
                Region::Toolbar,
                Region::SheetTabs,
                Region::FormulaBar
            ],
            "regions in tab order: {blocks:#?}"
        );
        // The inspector scrolls to the focused control, so only its tree order
        // (checked above through the block order) applies, not its positions.
        for (r, group) in &blocks {
            if *r != Region::Inspector {
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
        assert_eq!(
            names(Region::Menu),
            ["File", "Edit", "View", "Table", "Help"]
        );
        assert_eq!(
            names(Region::Toolbar),
            [
                "View",
                "Zoom",
                "Add Sheet",
                "Chart",
                "Table",
                "Text",
                "Shape",
                "Image",
                "Export",
                "Format",
                "Organize",
                "More actions"
            ]
        );
        assert_eq!(names(Region::FormulaBar), ["Name box", "Formula bar"]);
    }
}

#[test]
fn alt_letters_and_f10_open_menus_while_editing_without_typing_into_the_cell() {
    let s = launched(&[]);
    type_text(&s.app, "12");
    assert!(s.app.get_is_editing());
    alt(&s.app, "f");
    assert_eq!(
        s.app.get_local_menu_open_index(),
        0,
        "Alt+F opens File from the formula bar"
    );
    assert_eq!(
        s.app.get_formula_edit_buffer(),
        "12",
        "the letter was not typed into the cell"
    );
    press(&s.app, Key::Escape);

    // The inspector search field is a text field too.
    let s = launched(&[]);
    press(&s.app, Key::F6);
    shift_key(&s.app, Key::F6);
    tab_to(&s.app, "Search");
    alt(&s.app, "e");
    assert_eq!(
        s.app.get_local_menu_open_index(),
        1,
        "Alt+E opens Edit from the inspector search"
    );
    assert_eq!(
        s.app.get_inspector_search(),
        "",
        "nothing was typed into the search field"
    );
}

#[test]
fn menu_keys_do_nothing_while_a_dialog_is_open() {
    let s = launched(&[]);
    s.app.set_save_changes_open(true);
    render(&s.app);
    alt(&s.app, "f");
    press(&s.app, Key::F10);
    assert_eq!(s.app.get_local_menu_open_index(), -1);
    assert!(s.app.get_save_changes_open());
}

#[test]
fn f6_leaves_the_grid_for_the_toolbar_and_the_inspector_and_comes_back() {
    let s = launched(&[("A1", "1")]);
    let grid = focus_weak(&s.app);
    // Tab moves the cell cursor inside the grid, so it cannot leave it; F6 does.
    tab(&s.app);
    assert!(
        same_focus(&focus_weak(&s.app), &grid),
        "Tab stays on the grid"
    );
    assert_eq!(s.app.get_selected_cell(), "B1", "Tab moved the cell cursor");
    press(&s.app, Key::F6);
    assert_eq!(
        focus_name(&s.app),
        "View",
        "F6 goes to the first toolbar item"
    );
    press(&s.app, Key::F6);
    assert!(
        same_focus(&focus_weak(&s.app), &grid),
        "F6 from the toolbar returns to the grid"
    );

    // Shift+F6 walks the panels: grid, toolbar, inspector, grid.
    shift_key(&s.app, Key::F6);
    assert_eq!(focus_name(&s.app), "View");
    shift_key(&s.app, Key::F6);
    assert!(
        focus_name(&s.app).starts_with("Inspector"),
        "Shift+F6 goes on to the open inspector, not {:?}",
        focus_name(&s.app)
    );
    shift_key(&s.app, Key::F6);
    assert!(
        same_focus(&focus_weak(&s.app), &grid),
        "Shift+F6 returns to the grid"
    );

    // Without an inspector the panel walk skips straight back.
    s.app.set_show_inspector(false);
    shift_key(&s.app, Key::F6);
    assert_eq!(focus_name(&s.app), "View");
    shift_key(&s.app, Key::F6);
    assert!(same_focus(&focus_weak(&s.app), &grid));
}
