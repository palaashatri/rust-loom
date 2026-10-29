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

fn assert_table_action_is_a_recognized_noop_outside_table(app: &WriterApp, state: &GuiState) {
    let before_document = state.current.borrow().clone();
    let before_undo_depth = state.history.borrow().undo_len();

    assert!(
        dispatch_command(app, "table.insert_row_above"),
        "the known command ID is recognized even when its action has no context"
    );
    assert_eq!(
        *state.current.borrow(),
        before_document,
        "a structural action outside a table must not change the document"
    );
    assert_eq!(
        state.history.borrow().undo_len(),
        before_undo_depth,
        "a no-op structural action must not add undo history"
    );
    assert!(
        app.get_status_right().starts_with("Table action failed:"),
        "a rejected table action reports why it did not run: {}",
        app.get_status_right()
    );
}

fn press(app: &WriterApp, role: AccessibleRole, label: &str) {
    render(app);
    ElementHandle::find_by_accessible_label(app, label)
        .find(|e| e.accessible_role() == Some(role))
        .unwrap_or_else(|| panic!("no {role:?} named {label:?}"))
        .invoke_accessible_default_action();
}

fn toolbar_button(app: &WriterApp, label: &str) -> ElementHandle {
    let min_height = if label == "More actions" { 20.0 } else { 40.0 };
    ElementHandle::find_by_accessible_label(app, label)
        .find(|e| {
            e.accessible_role() == Some(AccessibleRole::Button) && e.size().height >= min_height
        })
        .unwrap_or_else(|| panic!("toolbar button {label:?}"))
}

fn button(app: &WriterApp, label: &str) {
    render(app);
    toolbar_button(app, label).invoke_accessible_default_action();
}

fn zoom_button(app: &WriterApp) {
    let label = format!("Zoom {}%", (app.get_page_zoom() * 100.0).round() as i32);
    button(app, &label);
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
    toolbar_button(app, label).absolute_position().x
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
    let left = ["View options", "Zoom 100%"].map(|l| toolbar_x(&app, l));
    let centre = ["Insert", "Text", "Style", "List"].map(|l| toolbar_x(&app, l));
    let right =
        ["Export", "Format inspector", "Document", "More actions"].map(|l| toolbar_x(&app, l));
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

    button(&app, "Format inspector");
    assert_eq!(app.get_inspector_tab(), 0);
    assert!(app.get_show_inspector());

    button(&app, "Format inspector");
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
    button(&app, "View options");
    assert_eq!(app.get_toolbar_menu(), 1);
    row(&app, "Outline Navigator");
    assert!(app.get_show_navigator());
    assert_eq!(app.get_toolbar_menu(), 0, "choosing a row closes the menu");

    button(&app, "View options");
    row(&app, "Format Inspector");
    assert!(!app.get_show_inspector());

    button(&app, "View options");
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
    assert!(
        checked(),
        "the menu follows the command: {}",
        app.get_status_right()
    );
    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"view.navigator"));
    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"file.export_md"));
}

#[test]
fn table_menu_enablement_tracks_selection_for_native_and_in_window_menus() {
    set_platform();
    let mut document = WriterDocument::new("table-menu", "Table Menu");
    document.replace_paragraphs("Before table");
    document.insert_table_block(usize::MAX, 2, 2).unwrap();
    document.set_selection(TextSelection::caret(0));
    let table_offset = (0..=document.editor_text().len())
        .find(|offset| document.table_cell_at_offset(*offset).is_some())
        .expect("table cell offset");

    let (app, state) = test_state(document, scripted(vec![]));
    let menu = Arc::new(NativeMenuBar::new());
    menu.install_menu_bar(&local_menu::writer_menu_bar())
        .expect("install Writer menu");
    wire_writer_shared_callbacks(&app, &state, Some(menu.clone()));
    sync_menu_state(&menu, &app, &state);

    let structural_commands = [
        "table.insert_row_above",
        "table.insert_row_below",
        "table.insert_col_left",
        "table.insert_col_right",
        "table.delete_row",
        "table.delete_column",
        "table.merge_cells",
        "table.split_cell",
    ];
    let native_enabled = |id: &str| {
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item(id).cloned())
            .unwrap_or_else(|| panic!("native menu command {id}"))
            .is_enabled()
    };
    let window_enabled = |id: &str| {
        app.get_local_menu_items()
            .iter()
            .find(|item| item.command_id == id)
            .unwrap_or_else(|| panic!("in-window menu command {id}"))
            .enabled
    };
    let assert_table_commands = |enabled: bool| {
        for id in structural_commands {
            assert_eq!(native_enabled(id), enabled, "native menu: {id}");
            assert_eq!(window_enabled(id), enabled, "in-window menu: {id}");
        }
        assert!(
            native_enabled("insert.table"),
            "Insert Table stays available"
        );
        assert!(
            window_enabled("insert.table"),
            "Insert Table stays available"
        );
    };

    assert_table_commands(false);
    assert_table_action_is_a_recognized_noop_outside_table(&app, &state);
    app.invoke_selection_changed(table_offset as i32, table_offset as i32);
    assert_table_commands(true);
    app.invoke_selection_changed(0, 0);
    assert_table_commands(false);
    assert_table_action_is_a_recognized_noop_outside_table(&app, &state);
}

#[test]
fn picture_description_menu_command_reaches_the_dialog_and_keeps_the_empty_state_guard() {
    set_platform();
    let mut document = WriterDocument::new("picture-menu", "Picture Menu");
    document.replace_paragraphs("Before the picture");
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(40, 20, image::Rgba([1, 2, 3, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
    let image = loom_writer_core::InlineImage::from_bytes("photo.png", png).unwrap();
    let picture_id = document.insert_image_block(usize::MAX, image).unwrap();
    let picture_offset = document
        .blocks
        .iter()
        .take_while(|block| block.id != picture_id)
        .map(|block| block.text.len_bytes() + 1)
        .sum::<usize>();
    document.set_selection(TextSelection::caret(picture_offset));

    let (app, _state) = launched_with(document, scripted(vec![]));
    let menu_item = local_menu::writer_menu_bar()
        .find_item("picture.alt_text")
        .cloned()
        .expect("Picture Description is in the Table menu");
    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"picture.alt_text"));
    assert!(
        menu_item.is_enabled(),
        "the menu keeps the supported action enabled"
    );

    button(&app, "Table");
    row(&app, "Picture Description...");
    let dialog = app.global::<PictureAlt>();
    assert!(
        dialog.get_open(),
        "the menu command opens the production dialog"
    );
    assert_eq!(
        dialog.get_value(),
        "photo.png",
        "the picture name prefills it"
    );

    dialog.invoke_cancel();
    app.invoke_selection_changed(0, 0);
    button(&app, "Table");
    row(&app, "Picture Description...");
    assert!(
        !dialog.get_open(),
        "no picture at the caret keeps the dialog closed"
    );
    assert!(
        app.get_status_left()
            .contains("Place the caret on a picture first"),
        "{}",
        app.get_status_left()
    );
}

#[test]
fn table_insert_size_callback_refreshes_both_menus_without_manual_sync() {
    let (app, state) = test_state(text_document("Body text"), scripted(vec![]));
    let menu = Arc::new(NativeMenuBar::new());
    menu.install_menu_bar(&local_menu::writer_menu_bar())
        .expect("install Writer menu");
    wire_writer_shared_callbacks(&app, &state, Some(menu.clone()));
    apply_state(&app, &state);
    sync_menu_state(&menu, &app, &state);

    let structural_commands = [
        "table.insert_row_above",
        "table.insert_row_below",
        "table.insert_col_left",
        "table.insert_col_right",
        "table.delete_row",
        "table.delete_column",
        "table.merge_cells",
        "table.split_cell",
    ];
    let assert_table_commands = |enabled: bool| {
        for id in structural_commands {
            let native_enabled = menu
                .installed_menu_bar()
                .and_then(|bar| bar.find_item(id).cloned())
                .unwrap_or_else(|| panic!("native menu command {id}"))
                .is_enabled();
            let window_enabled = app
                .get_local_menu_items()
                .iter()
                .find(|item| item.command_id == id)
                .unwrap_or_else(|| panic!("in-window menu command {id}"))
                .enabled;
            assert_eq!(native_enabled, enabled, "native menu: {id}");
            assert_eq!(window_enabled, enabled, "in-window menu: {id}");
        }
    };

    assert_table_commands(false);
    assert!(!app.get_is_table_selected());

    // This is the callback used by the table-size dialog. The app-state
    // mutation must refresh the menus without an explicit post-insert sync.
    app.invoke_insert_table_size(2, 2);

    assert!(
        app.get_is_table_selected(),
        "the inserted table owns the caret"
    );
    assert_table_commands(true);
}

#[test]
fn same_table_context_mutation_refreshes_native_and_window_history_menu_state() {
    set_platform();
    let mut document = WriterDocument::new("table-history-menu", "Table History Menu");
    document.replace_paragraphs("Body text");
    document.insert_table_block(usize::MAX, 2, 2).unwrap();
    document.set_selection(TextSelection::caret(0));
    let table_offset = (0..=document.editor_text().len())
        .find(|offset| document.table_cell_at_offset(*offset).is_some())
        .expect("table cell offset");

    let (app, state) = test_state(document, scripted(vec![]));
    let menu = Arc::new(NativeMenuBar::new());
    menu.install_menu_bar(&local_menu::writer_menu_bar())
        .expect("install Writer menu");
    wire_writer_shared_callbacks(&app, &state, Some(menu.clone()));
    apply_state(&app, &state);
    sync_menu_state(&menu, &app, &state);

    // Create a real redo entry outside the table, then enter the table. The
    // selection callback synchronizes both menu surfaces, so redo starts in
    // an enabled state before the same-context table mutation below.
    app.invoke_selection_changed(0, 4);
    app.invoke_toggle_bold();
    assert!(app.get_can_undo(), "bold creates the undo entry");
    app.invoke_undo();
    assert!(app.get_can_redo(), "undo creates the redo entry");
    app.invoke_selection_changed(table_offset as i32, table_offset as i32);
    assert!(app.get_is_table_selected());

    let native_enabled = |id: &str| {
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item(id).cloned())
            .unwrap_or_else(|| panic!("native menu command {id}"))
            .is_enabled()
    };
    let window_enabled = |id: &str| {
        app.get_local_menu_items()
            .iter()
            .find(|item| item.command_id == id)
            .unwrap_or_else(|| panic!("in-window menu command {id}"))
            .enabled
    };
    assert!(
        native_enabled("edit.redo"),
        "native menu initially shows redo"
    );
    assert!(
        window_enabled("edit.redo"),
        "window menu initially shows redo"
    );

    app.invoke_table_action("insert-row-below".into());
    assert!(app.get_is_table_selected(), "the caret stays in the table");
    assert!(app.get_can_undo(), "table insertion creates an undo entry");
    assert!(!app.get_can_redo(), "a new table edit clears redo");
    assert!(
        native_enabled("edit.undo"),
        "native menu keeps undo enabled"
    );
    assert!(
        window_enabled("edit.undo"),
        "window menu keeps undo enabled"
    );
    assert!(
        !native_enabled("edit.redo"),
        "native menu disables redo after the same-context table edit"
    );
    assert!(
        !window_enabled("edit.redo"),
        "in-window menu disables redo after the same-context table edit"
    );
}

#[test]
fn table_inspector_delete_row_button_stays_inside_its_reading_edge_padding() {
    let ui_font = loom_desktop::ui_font::for_this_run(false);
    println!("Native UI font for inspector geometry: {ui_font}");
    let mut violations = Vec::new();
    for width in [1024_u32, 1280] {
        for text_scale in [1.0_f32, 1.5] {
            let mut document = WriterDocument::new("table-inspector", "Table Inspector");
            document.replace_paragraphs("Before table");
            document.insert_table_block(usize::MAX, 2, 2).unwrap();
            let table_offset = (0..=document.editor_text().len())
                .find(|offset| document.table_cell_at_offset(*offset).is_some())
                .expect("table cell offset");
            document.set_selection(TextSelection::caret(table_offset));

            let (app, state) = test_state(document, scripted(vec![]));
            Theme::get(&app).set_ui_font_family(ui_font.clone().into());
            Theme::get(&app).set_text_scale(text_scale);
            toolbar_commands::apply_layout_breakpoints(&app, width);
            app.window().set_size(PhysicalSize::new(width, 800));
            app.set_inspector_available(true);
            app.set_show_inspector(true);
            app.set_inspector_quiet_open(true);
            wire_writer_shared_callbacks(&app, &state, None);
            apply_state(&app, &state);

            let _ = snapshot_component(&app, width as f32, 800.0, 1.0).expect("render inspector");
            for label in ["Del Row", "Del Col"] {
                let button = ElementHandle::find_by_accessible_label(&app, label)
                    .find(|element| element.accessible_role() == Some(AccessibleRole::Button))
                    .unwrap_or_else(|| panic!("{label} remains accessible"));
                let position = button.absolute_position();
                let size = button.size();
                let right = position.x + size.width;
                println!(
                    "{label} geometry: width={width}, text_scale={text_scale}, x={}, y={}, width={}, height={}, right={right}",
                    position.x, position.y, size.width, size.height,
                );
                if right > width as f32 - 16.0 + 1.0 {
                    violations.push(format!(
                        "{label} right edge {right} exceeds the 16px inset at {width}px, text scale {text_scale}"
                    ));
                }
                if size.height + 0.5 < Theme::get(&app).get_scaled_control_height() {
                    violations.push(format!(
                        "{label} height {:?} is below the themed control height at {width}px, text scale {text_scale}",
                        size,
                    ));
                }
            }
        }
    }
    assert!(
        violations.is_empty(),
        "inspector geometry violations: {violations:#?}"
    );
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
fn insert_menu_offers_page_and_line_breaks_with_their_shortcuts() {
    let (app, _state) = launched("AB");
    button(&app, "Insert");
    render(&app);
    let row_of = |label: &str| {
        ElementHandle::find_by_accessible_label(&app, label)
            .find(|e| e.accessible_role() == Some(AccessibleRole::ListItem))
            .unwrap_or_else(|| panic!("the Insert menu offers {label:?}"))
    };
    assert_eq!(
        row_of("Page Break").accessible_description().as_deref(),
        Some("Ctrl+Enter")
    );
    assert_eq!(
        row_of("Line Break").accessible_description().as_deref(),
        Some("Shift+Enter")
    );
}

#[test]
fn insert_menu_page_and_line_break_rows_run_the_breaks_at_the_caret() {
    let texts = |state: &GuiState| -> Vec<String> {
        state
            .current
            .borrow()
            .blocks
            .iter()
            .map(|b| b.text.as_str().to_string())
            .collect()
    };

    let (app, state) = launched("AB");
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::caret(1));
    button(&app, "Insert");
    row(&app, "Page Break");
    assert_eq!(texts(&state), ["A\u{c}", "B"]);

    let (app, state) = launched("AB");
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::caret(1));
    button(&app, "Insert");
    row(&app, "Line Break");
    assert_eq!(texts(&state), ["A\u{b}B"]);
}

#[test]
fn zoom_menu_changes_the_page_zoom() {
    let (app, state) = launched("Hello");
    zoom_button(&app);
    row(&app, "Zoom In");
    assert!((state.viewport.borrow().zoom - 1.25).abs() < 1e-3);
    zoom_button(&app);
    row(&app, "Zoom Out");
    zoom_button(&app);
    row(&app, "Zoom Out");
    assert!((state.viewport.borrow().zoom - 0.75).abs() < 1e-3);
    zoom_button(&app);
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
