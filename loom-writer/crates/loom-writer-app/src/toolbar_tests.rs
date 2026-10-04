//! The icon-over-label toolbar, its menus, the inspector tabs, the View menu
//! toggles and the outline navigator all drive real window and document state.

use super::actions_tests::{test_state, text_document};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_test_support::capture::snapshot_component;
use slint::platform::{Key, WindowEvent};

fn scripted(saves: Vec<Option<PathBuf>>) -> Rc<dyn FileDialogService> {
    Rc::new(loom_desktop::ScriptedFileDialogs::new([], saves))
}

fn launched_with(
    document: WriterDocument,
    dialogs: Rc<dyn FileDialogService>,
) -> (WriterApp, Rc<GuiState>) {
    let (app, state) = test_state(document, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    wire_writer_inspector_toggle(&app, &state, None);
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    toolbar_commands::start_with_inspector_open(&app, 1280);
    apply_state(&app, &state);
    render(&app);
    (app, state)
}

fn launched(text: &str) -> (WriterApp, Rc<GuiState>) {
    launched_with(text_document(text), scripted(vec![]))
}

fn render(app: &WriterApp) {
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render");
}

fn press(app: &WriterApp, role: AccessibleRole, label: &str) {
    render(app);
    ElementHandle::find_by_accessible_label(app, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} named {label:?}"))
        .invoke_accessible_default_action();
}

fn button(app: &WriterApp, label: &str) {
    press(app, AccessibleRole::Button, label);
}

fn row(app: &WriterApp, label: &str) {
    press(app, AccessibleRole::ListItem, label);
}

fn key(app: &WriterApp, key: Key) {
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: key.into() });
}

fn toolbar_x(app: &WriterApp, label: &str) -> f32 {
    render(app);
    ElementHandle::find_by_accessible_label(app, label)
        .filter(|e| e.accessible_role() == Some(AccessibleRole::Button))
        .map(|e| (e.absolute_position(), e.size()))
        .find(|(p, s)| p.y < 100.0 && s.width > 0.0)
        .unwrap_or_else(|| panic!("toolbar button {label:?}"))
        .0
        .x
}

fn select_all(app: &WriterApp, state: &GuiState) {
    let mut doc = state.current.borrow().clone();
    let end = doc.editor_text().len();
    doc.set_selection(TextSelection::range(0, end));
    *state.current.borrow_mut() = doc;
    apply_state(app, state);
}

#[test]
fn every_toolbar_item_is_a_named_button_in_three_groups() {
    let (app, _state) = launched("Hello");
    let left = ["View", "Zoom"].map(|l| toolbar_x(&app, l));
    let centre = ["Insert", "Text", "Style", "List"].map(|l| toolbar_x(&app, l));
    let right = ["Export", "Format", "Document", "More actions"].map(|l| toolbar_x(&app, l));
    assert!(left.windows(2).all(|w| w[0] < w[1]));
    assert!(centre.windows(2).all(|w| w[0] < w[1]));
    assert!(right.windows(2).all(|w| w[0] < w[1]));
    assert!(left[1] < centre[0] && centre[3] < right[0]);
}

#[test]
fn inspector_is_open_by_default_when_docked_and_closed_when_compact() {
    let (app, _state) = launched("Hello");
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 0);

    let narrow = WriterApp::new().expect("app");
    toolbar_commands::start_with_inspector_open(&narrow, 1024);
    assert!(!narrow.get_show_inspector(), "a drawer starts closed");

    let scaled = WriterApp::new().expect("app");
    Theme::get(&scaled).set_text_scale(1.5);
    toolbar_commands::start_with_inspector_open(&scaled, 1280);
    assert!(
        !scaled.get_show_inspector(),
        "scaled text counts as narrower"
    );
}

#[test]
fn format_and_document_buttons_switch_tabs_and_close_the_inspector() {
    let (app, _state) = launched("Hello");
    button(&app, "Document");
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 1);

    button(&app, "Format");
    assert_eq!(app.get_inspector_tab(), 0);
    assert!(app.get_show_inspector());

    button(&app, "Format");
    assert!(!app.get_show_inspector(), "second press closes it");

    button(&app, "Document");
    assert!(app.get_show_inspector());
    assert_eq!(app.get_inspector_tab(), 1);
}

#[test]
fn the_inspector_tab_strip_shows_the_matching_sections() {
    let (app, _state) = launched("Hello");
    let has = |label: &str| {
        render(&app);
        ElementHandle::find_by_accessible_label(&app, label).count() > 0
    };
    assert!(has("Paragraph Style") || has("Heading Style"));
    assert!(!has("Paper Size"));
    app.set_inspector_tab(1);
    assert!(has("Paper Size"));
    assert!(!has("Heading Style"));
}

#[test]
fn view_menu_toggles_inspector_and_outline_with_check_marks() {
    let (app, _state) = launched("Hello");
    button(&app, "View");
    assert_eq!(app.get_toolbar_menu(), 1);
    row(&app, "Outline Navigator");
    assert!(app.get_show_navigator());
    assert_eq!(app.get_toolbar_menu(), 0, "choosing a row closes the menu");

    button(&app, "View");
    row(&app, "Format Inspector");
    assert!(!app.get_show_inspector());

    button(&app, "View");
    row(&app, "Format Inspector");
    assert!(app.get_show_inspector());
}

#[test]
fn navigator_menu_check_follows_every_control() {
    let menu = Arc::new(NativeMenuBar::new());
    let (app, state) = test_state(text_document("Hello"), scripted(vec![]));
    wire_writer_shared_callbacks(&app, &state, Some(menu.clone()));
    let mut bar = build_standard_menu_bar(
        "Loom Writer",
        vec![],
        vec![],
        vec![
            MenuItem::check("view.inspector", "Format Inspector", false),
            MenuItem::check("view.navigator", "Outline Navigator", false),
        ],
        vec![],
    );
    bar.disable_items_except(local_menu::SUPPORTED_COMMANDS);
    menu.install_menu_bar(&bar).expect("install");
    sync_menu_state(&menu, &app, &state);
    let checked = || match menu
        .installed_menu_bar()
        .and_then(|bar| bar.find_item("view.navigator").cloned())
    {
        Some(MenuItem::Check { checked, .. }) => checked,
        other => panic!("not a check item: {other:?}"),
    };
    assert!(!checked());
    assert!(dispatch_command(&app, "view.navigator"));
    assert!(app.get_show_navigator());
    assert!(checked(), "the menu follows the command");
    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"view.navigator"));
    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"file.export_md"));
}

#[test]
fn text_menu_rows_format_the_selection() {
    let (app, state) = launched("Hello world");
    select_all(&app, &state);
    button(&app, "Text");
    row(&app, "Bold");
    assert!(app.get_is_bold());
    // Strikethrough is wired by the live window only; count the call here.
    let strikes = Rc::new(Cell::new(0));
    let s = strikes.clone();
    app.on_toggle_strikethrough(move || s.set(s.get() + 1));
    button(&app, "Text");
    row(&app, "Strikethrough");
    assert_eq!(strikes.get(), 1);
    button(&app, "Text");
    row(&app, "Align Center");
    assert_eq!(app.get_text_alignment(), 1);
}

#[test]
fn style_and_list_menus_change_the_paragraphs() {
    let (app, state) = launched("Chapter");
    button(&app, "Style");
    row(&app, "Heading 2");
    assert_eq!(state.current.borrow().blocks[0].kind, "heading2");
    assert_eq!(app.get_heading_level(), 2);

    button(&app, "List");
    row(&app, "Bulleted List");
    assert_eq!(state.current.borrow().blocks[0].kind, "list-bulleted");
}

#[test]
fn insert_menu_inserts_a_table_and_opens_the_comment_field() {
    let (app, state) = launched("Hello");
    let before = state.current.borrow().blocks.len();
    button(&app, "Insert");
    row(&app, "Table");
    assert!(state.current.borrow().blocks.len() > before);

    app.set_show_inspector(false);
    button(&app, "Insert");
    row(&app, "Comment");
    assert!(app.get_show_inspector());
    assert_eq!(
        app.get_inspector_tab(),
        1,
        "comments live on the Document tab"
    );
}

#[test]
fn zoom_menu_changes_the_page_zoom() {
    let (app, state) = launched("Hello");
    button(&app, "Zoom");
    row(&app, "Zoom In");
    assert!((state.viewport.borrow().zoom - 1.25).abs() < 1e-3);
    button(&app, "Zoom");
    row(&app, "Zoom Out");
    button(&app, "Zoom");
    row(&app, "Zoom Out");
    assert!((state.viewport.borrow().zoom - 0.75).abs() < 1e-3);
    button(&app, "Zoom");
    row(&app, "Actual Size");
    assert!((state.viewport.borrow().zoom - 1.0).abs() < 1e-3);
}

#[test]
fn export_menu_writes_markdown_and_routes_pdf_and_word() {
    let dir = std::env::temp_dir().join(format!("loom-writer-toolbar-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let target = dir.join("out.md");
    let (app, _state) = launched_with(
        text_document("Exported text"),
        scripted(vec![Some(target.clone())]),
    );
    button(&app, "Export");
    row(&app, "Markdown");
    let written = std::fs::read_to_string(&target).expect("markdown file");
    assert!(written.contains("Exported text"));
    let _ = std::fs::remove_dir_all(&dir);

    let (app, _state) = launched("Hello");
    let calls = Rc::new(Cell::new((0, 0)));
    let c = calls.clone();
    app.on_export_pdf(move || c.set((c.get().0 + 1, c.get().1)));
    let c = calls.clone();
    app.on_export_docx(move || c.set((c.get().0, c.get().1 + 1)));
    button(&app, "Export");
    row(&app, "PDF");
    button(&app, "Export");
    row(&app, "Word (.docx)");
    assert_eq!(calls.get(), (1, 1));
}

#[test]
fn markdown_export_is_disabled_for_an_empty_document() {
    let (app, state) = launched_with(blank_document(), scripted(vec![]));
    app.invoke_export_markdown();
    assert_eq!(
        app.get_status_right().as_str(),
        "Add content before exporting Markdown"
    );
    drop(state);
}

#[test]
fn more_menu_runs_undo_redo_file_commands_and_find() {
    let (app, state) = launched("Hello");
    let saves = Rc::new(Cell::new(0));
    let s = saves.clone();
    app.on_save_doc(move || s.set(s.get() + 1));
    let opens = Rc::new(Cell::new(0));
    let o = opens.clone();
    app.on_open_doc(move || o.set(o.get() + 1));

    button(&app, "More actions");
    row(&app, "Save");
    button(&app, "More actions");
    row(&app, "Open");
    assert_eq!((saves.get(), opens.get()), (1, 1));

    button(&app, "More actions");
    row(&app, "Find");
    assert!(app.global::<FindBar>().get_open());

    // Undo is offered only when there is something to undo.
    app.global::<FindBar>().invoke_close();
    let mut next = state.current.borrow().clone();
    next.replace_paragraphs("Changed");
    apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
    button(&app, "More actions");
    row(&app, "Undo");
    assert_eq!(state.current.borrow().plain_text(), "Hello");
    button(&app, "More actions");
    row(&app, "Redo");
    assert_eq!(state.current.borrow().plain_text(), "Changed");
}

#[test]
fn keyboard_moves_through_a_toolbar_menu_and_escape_closes_it() {
    let (app, state) = launched("Chapter");
    button(&app, "Style");
    assert_eq!(app.get_toolbar_menu(), 5);
    // Body is first; Down moves to Heading 1; Return runs it.
    key(&app, Key::DownArrow);
    key(&app, Key::Return);
    assert_eq!(state.current.borrow().blocks[0].kind, "heading1");
    assert_eq!(app.get_toolbar_menu(), 0);

    button(&app, "Style");
    key(&app, Key::Escape);
    assert_eq!(app.get_toolbar_menu(), 0);
}

#[test]
fn outline_lists_headings_and_a_click_moves_the_caret_there() {
    let mut doc = text_document("Intro\nBody one\nMiddle\nBody two\nEnd");
    for (index, kind) in [(0usize, "heading1"), (2, "heading2"), (4, "heading1")] {
        doc.blocks[index].kind = kind.to_string();
    }
    let (app, state) = launched_with(doc, scripted(vec![]));
    assert_eq!(
        app.get_outline_entries().row_count(),
        0,
        "hidden until shown"
    );
    dispatch_command(&app, "view.navigator");
    render(&app);
    let entries = app.get_outline_entries();
    let titles: Vec<String> = entries.iter().map(|e| e.title.to_string()).collect();
    assert_eq!(titles, ["Intro", "Middle", "End"]);
    assert_eq!(entries.row_data(1).unwrap().level, 2);

    press(&app, AccessibleRole::Button, "Middle");
    let expected = "Intro\nBody one\n".len();
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::caret(expected)
    );
    assert!(app.get_outline_entries().row_data(1).unwrap().current);
}

#[test]
fn outline_follows_edits_while_showing() {
    let mut doc = text_document("Title\nText");
    doc.blocks[0].kind = "heading1".to_string();
    let (app, state) = launched_with(doc, scripted(vec![]));
    dispatch_command(&app, "view.navigator");
    assert_eq!(app.get_outline_entries().row_count(), 1);
    let mut next = state.current.borrow().clone();
    next.blocks[1].kind = "heading2".to_string();
    apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
    assert_eq!(app.get_outline_entries().row_count(), 2);
}
