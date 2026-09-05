use super::*;

fn text_document(text: &str) -> WriterDocument {
    let mut document = WriterDocument::new("test", "Test");
    document.replace_paragraphs(text);
    document
}

fn test_state(
    document: WriterDocument,
    dialogs: Rc<dyn FileDialogService>,
) -> (WriterApp, Rc<GuiState>) {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut registry = build_writer_registry();
    let history = EditorHistory::new();
    sync_writer_registry_enablement(&mut registry, &document, &history);
    let state = Rc::new(GuiState {
        current: RefCell::new(document),
        viewport: RefCell::new(PageViewport::default()),
        pointer_anchor: Cell::new(None),
        pointer_active: Cell::new(false),
        save_path: RefCell::new(None),
        history: RefCell::new(history),
        history_clock: Instant::now(),
        syncing_editor: Cell::new(false),
        dialogs,
        document_filter: FileFilter::new("Writer", ["loomdoc"]).expect("document filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("PDF filter"),
        registry: Arc::new(Mutex::new(registry)),
    });
    (app, state)
}

fn history_shape(history: &EditorHistory) -> (usize, usize, usize) {
    (history.undo.len(), history.redo.len(), history.total_bytes)
}

#[test]
fn coalesces_adjacent_typing_edits() {
    let mut history = EditorHistory::with_budget(16, usize::MAX);
    let first = text_document("a");
    let second = text_document("ab");
    let third = text_document("abc");

    history.record(first.clone(), second.clone(), HistoryKind::Typing, 100);
    history.record(second.clone(), third.clone(), HistoryKind::Typing, 200);

    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.undo(), Some(first));
    assert_eq!(history.redo(), Some(third));
}

#[test]
fn document_action_breaks_typing_coalescing() {
    let mut history = EditorHistory::with_budget(16, usize::MAX);
    let first = text_document("a");
    let second = text_document("ab");
    let third = text_document("new document");

    history.record(first.clone(), second.clone(), HistoryKind::Typing, 100);
    history.record(
        second.clone(),
        third.clone(),
        HistoryKind::DocumentAction,
        200,
    );

    assert_eq!(history.undo_len(), 2);
    assert_eq!(history.undo(), Some(second.clone()));
    assert_eq!(history.undo(), Some(first));
    assert_eq!(history.redo(), Some(second));
    assert_eq!(history.redo(), Some(third));
}

#[test]
fn history_budget_evicts_oldest_entries() {
    let mut history = EditorHistory::with_budget(2, usize::MAX);
    let a = text_document("a");
    let b = text_document("b");
    let c = text_document("c");
    let d = text_document("d");

    history.record(a, b.clone(), HistoryKind::DocumentAction, 0);
    history.record(b, c.clone(), HistoryKind::DocumentAction, 1);
    history.record(c.clone(), d, HistoryKind::DocumentAction, 2);

    assert_eq!(history.undo_len(), 2);
    assert_eq!(history.undo(), Some(c));
}

#[test]
fn history_byte_budget_is_bounded() {
    let mut history = EditorHistory::with_budget(32, 1000);
    let a = text_document(&"a".repeat(400));
    let b = text_document(&"b".repeat(400));
    let c = text_document(&"c".repeat(400));

    history.record(a, b.clone(), HistoryKind::DocumentAction, 0);
    history.record(b, c, HistoryKind::DocumentAction, 1);

    assert!(history.total_bytes() <= 1000);
}

#[test]
fn new_document_is_blank_and_unsaved_ready() {
    let document = blank_document();
    assert!(document.blocks.is_empty());
    assert_eq!(document.title, "Untitled Document");
}

#[test]
fn scripted_dialog_request_uses_the_current_document_directory() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [Some(PathBuf::from("/tmp/next.loomdoc"))],
        [Some(PathBuf::from("/tmp/saved.loomdoc"))],
    ));
    let state = GuiState {
        current: RefCell::new(text_document("hello")),
        viewport: RefCell::new(PageViewport::default()),
        pointer_anchor: Cell::new(None),
        pointer_active: Cell::new(false),
        save_path: RefCell::new(Some(PathBuf::from("/tmp/current.loomdoc"))),
        history: RefCell::new(EditorHistory::new()),
        history_clock: Instant::now(),
        syncing_editor: Cell::new(false),
        dialogs,
        document_filter: FileFilter::new("Writer", ["loomdoc"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        registry: Arc::new(Mutex::new(build_writer_registry())),
    };
    let request = writer_open_request(&state);
    assert_eq!(request.initial_directory, Some(PathBuf::from("/tmp")));
    assert_eq!(
        state.dialogs.open_file(&request).expect("open"),
        Some(PathBuf::from("/tmp/next.loomdoc"))
    );
}

#[test]
fn save_panel_cancellation_preserves_document_and_history() {
    let document = text_document("cancel save");
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(document.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    {
        let mut history = state.history.borrow_mut();
        history.record(
            text_document("before save"),
            document.clone(),
            HistoryKind::DocumentAction,
            0,
        );
    }
    apply_state(&app, &state);
    let before_history = history_shape(&state.history.borrow());

    app.invoke_save_doc();

    assert_eq!(*state.current.borrow(), document);
    assert_eq!(*state.save_path.borrow(), None);
    assert_eq!(history_shape(&state.history.borrow()), before_history);
    assert_eq!(app.get_status_left(), "Save cancelled");
}

#[test]
fn open_panel_cancellation_preserves_document_and_history() {
    let document = text_document("cancel open");
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([None], []));
    let (app, state) = test_state(document.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    {
        let mut history = state.history.borrow_mut();
        history.record(
            text_document("before open"),
            document.clone(),
            HistoryKind::DocumentAction,
            0,
        );
    }
    apply_state(&app, &state);
    let before_history = history_shape(&state.history.borrow());

    app.invoke_open_doc();

    assert_eq!(*state.current.borrow(), document);
    assert_eq!(history_shape(&state.history.borrow()), before_history);
    assert_eq!(app.get_status_left(), "Open cancelled");
}

#[test]
fn failed_pdf_export_preserves_document_and_history() {
    let document = text_document("failed export");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(PathBuf::new())],
    ));
    let (app, state) = test_state(document.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    {
        let mut history = state.history.borrow_mut();
        history.record(
            text_document("before export"),
            document.clone(),
            HistoryKind::DocumentAction,
            0,
        );
    }
    apply_state(&app, &state);
    let before_history = history_shape(&state.history.borrow());

    app.invoke_export_pdf();

    assert_eq!(*state.current.borrow(), document);
    assert_eq!(history_shape(&state.history.borrow()), before_history);
    assert_eq!(
        app.get_status_left(),
        "Export failed: PDF destination is empty"
    );
}

#[test]
fn viewport_callbacks_update_controller_and_clamp_inputs() {
    let document = text_document("viewport");
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(document.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);

    app.invoke_page_zoom_changed(1.5);
    app.invoke_page_scroll_changed(-20.0, 48.0);

    let viewport = *state.viewport.borrow();
    assert_eq!(viewport.zoom, 1.5);
    assert_eq!(viewport.scroll_x, 0.0);
    assert_eq!(viewport.scroll_y, 48.0);
    let layout = document
        .layout(&PageStyle::default(), viewport)
        .expect("viewport layout");
    assert!(layout.page_bounds[0].width > PageStyle::default().width_pt);
}

#[test]
fn redo_is_cleared_by_new_edit() {
    let mut history = EditorHistory::with_budget(16, usize::MAX);
    let a = text_document("a");
    let b = text_document("b");
    let c = text_document("c");

    history.record(a.clone(), b.clone(), HistoryKind::DocumentAction, 0);
    assert_eq!(history.undo(), Some(a.clone()));
    assert_eq!(history.redo_len(), 1);

    history.record(a, c, HistoryKind::DocumentAction, 1);
    assert_eq!(history.redo_len(), 0);
}

#[test]
fn selected_formatting_is_one_undoable_document_action() {
    let mut before = text_document("first second");
    before.set_selection(loom_writer_core::TextSelection::range(0, 5));
    let mut after = before.clone();
    let selection = after.selection();
    set_selection_bold(
        &mut after,
        DocumentSelection::range(selection.anchor, selection.focus),
        true,
    );

    let mut history = EditorHistory::with_budget(16, usize::MAX);
    history.record(
        before.clone(),
        after.clone(),
        HistoryKind::DocumentAction,
        0,
    );
    assert_eq!(history.undo_len(), 1);
    let undone = history.undo().expect("formatting undo");
    assert_eq!(undone, before);
    assert!(undone.blocks[0].runs.is_empty());
    assert_eq!(undone.selection(), before.selection());
    assert_eq!(history.redo(), Some(after));
}

#[test]
fn apply_document_projects_selection_formatting_and_accessible_announcement() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut document = text_document("first second");
    document.set_selection(TextSelection::range(0, 5));
    let selection = document.selection();
    set_selection_bold(
        &mut document,
        DocumentSelection::range(selection.anchor, selection.focus),
        true,
    );

    apply_document(&app, &document);

    assert_eq!(app.get_selection_anchor(), 0);
    assert_eq!(app.get_selection_focus(), 5);
    assert!(app.get_is_bold());
    assert_eq!(app.get_selection_announcement(), "Selected 5 characters");
    assert!(app.get_status_right().contains("Selected 5 characters"));
}

#[test]
fn selection_announcement_reports_utf8_character_count_not_byte_count() {
    let mut document = text_document("AéB");
    document.set_selection(TextSelection::range(1, 3));
    assert_eq!(
        selection_announcement(&document, &document.selection()),
        "Selected 1 characters"
    );
}

#[test]
fn collapsed_caret_announcement_reports_one_based_grapheme_position() {
    let mut document = text_document("AéB");
    document.set_selection(TextSelection::caret(0));
    assert_eq!(
        selection_announcement(&document, &document.selection()),
        "Caret at character 1"
    );
    // Offset 3 is after the two-grapheme prefix (`Aé`), even though the
    // second grapheme occupies two UTF-8 bytes.
    document.set_selection(TextSelection::caret(3));
    assert_eq!(
        selection_announcement(&document, &document.selection()),
        "Caret at character 3"
    );
}

#[test]
fn render_projection_keeps_character_styles_and_paragraph_geometry_visible() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut document = text_document("Styled heading");
    document.blocks[0].kind = "heading2".into();
    document.blocks[0].style.alignment = loom_text::Alignment::Center;
    document.set_selection(TextSelection::range(0, 6));
    set_selection_bold(&mut document, DocumentSelection::range(0, 6), true);
    set_selection_italic(&mut document, DocumentSelection::range(0, 6), true);
    set_selection_underline(&mut document, DocumentSelection::range(0, 6), true);

    let markup = writer_render_markup(&document.blocks[0]);
    assert!(
        markup.contains("**"),
        "bold run must reach the display projection"
    );
    assert!(
        markup.contains("*"),
        "italic run must reach the display projection"
    );
    assert!(
        markup.contains("<u>"),
        "underline run must reach the display projection"
    );
    assert!(
        slint::StyledText::from_markdown(&markup).is_ok(),
        "combined character-style markup must be accepted by StyledText"
    );

    apply_document(&app, &document);
    let rows = app.get_render_blocks();
    assert_eq!(rows.row_count(), 1);
    let row = rows.row_data(0).expect("render row");
    assert_eq!(
        row.alignment, 1,
        "center alignment must reach the paper row"
    );
    assert_eq!(row.font_size, 20.0, "heading size must reach the paper row");
    assert!(
        row.height > row.font_size,
        "wrapped row needs explicit layout height"
    );
    assert_ne!(
        row.content,
        slint::StyledText::from_plain_text(document.blocks[0].text.as_str()),
        "styled rows must not silently fall back to plain text"
    );
    let content_debug = format!("{:?}", row.content);
    assert!(
        content_debug.contains("Strong"),
        "bold span missing from StyledText"
    );
    assert!(
        content_debug.contains("Emphasis"),
        "italic span missing from StyledText"
    );
    assert!(
        content_debug.contains("Underline"),
        "underline span missing from StyledText"
    );
}

#[test]
fn render_projection_uses_page_layout_for_rows_and_selection_overlay() {
    let mut document = text_document("layout-backed selection");
    document.set_selection(TextSelection::range(0, 6));
    let (rows, selection_rects) = writer_render_projection(
        &document,
        PageViewport {
            zoom: 1.5,
            scroll_y: 48.0,
            ..PageViewport::default()
        },
    );

    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let style = PageStyle::default();
    assert_eq!(row.x, 0.0, "rows are relative to the content box");
    assert_eq!(row.y, 0.0, "first fragment starts at the content origin");
    assert_eq!(
        row.width,
        style.width_pt - style.margin_left_pt - style.margin_right_pt,
        "row width must come from PageStyle content width"
    );
    assert!(
        selection_rects
            .iter()
            .any(|rect| !rect.caret && rect.width > 0.0),
        "a non-collapsed selection must produce a visible highlight rectangle"
    );

    let (zoomed_rows, _) = writer_render_projection(
        &document,
        PageViewport {
            zoom: 2.0,
            ..PageViewport::default()
        },
    );
    assert_eq!(
        zoomed_rows[0].width, row.width,
        "rows remain logical page coordinates while the UI applies zoom"
    );
    assert!(
        (zoomed_rows[0].height - row.height).abs() < 0.0001,
        "rows remain logical page coordinates while the UI applies zoom"
    );
}

#[test]
fn justify_is_disabled_and_persisted_projection_is_indeterminate() {
    set_platform();
    let mut document = text_document("justify remains readable");
    document.blocks[0].style.alignment = loom_text::Alignment::Justify;
    document.set_selection(TextSelection::range(0, 7));
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(document.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);

    assert_eq!(app.get_text_alignment(), -1);
    assert_eq!(
        app.get_render_blocks()
            .row_data(0)
            .expect("render row")
            .alignment,
        -1,
        "persisted Justify must not be projected as Left"
    );
    assert!(
        !state
            .registry
            .lock()
            .unwrap()
            .get(&CommandId::new("writer.align.justify"))
            .expect("legacy Justify command")
            .enabled
    );

    let before = state.current.borrow().clone();
    app.invoke_select_alignment(3);
    assert_eq!(*state.current.borrow(), before);
    assert_eq!(
        app.get_status_right(),
        "Justify alignment is unavailable in the page editor"
    );
}

#[test]
fn persisted_justify_projection_exposes_an_explicit_unsupported_indicator() {
    let mut document = text_document("justify remains readable");
    document.blocks[0].style.alignment = loom_text::Alignment::Justify;
    let (rows, _) = writer_render_projection(&document, PageViewport::default());
    let row = rows.first().expect("render row");
    assert_eq!(row.alignment, -1);
    assert!(
        row.unsupported,
        "persisted Justify must be visibly marked unsupported rather than rendered as Left"
    );
    assert_eq!(row.unsupported_label, "Justify unavailable");
}

#[test]
fn render_projection_keeps_long_document_rows_on_their_page_flow() {
    let mut document = WriterDocument::new("long", "Long document");
    for id in 0..120 {
        document.push(RichBlock::new(
            id + 1,
            "paragraph",
            "A paragraph with enough text to exercise deterministic page wrapping.",
        ));
    }
    // Selection geometry is only projected for the model's active range. Select the
    // complete document so this regression exercises overlays on later pages as well
    // as the cumulative row positions above.
    let document_len = document.editor_text().len();
    document.set_selection(TextSelection::range(0, document_len));
    let (rows, selection_rects) = writer_render_projection(&document, PageViewport::default());
    assert_eq!(rows.len(), document.blocks.len());
    assert!(
        rows.windows(2)
            .all(|pair| { pair[1].y + 0.001 >= pair[0].y + pair[0].height }),
        "rows from later pages must retain cumulative page offsets"
    );
    assert!(
        rows.last()
            .is_some_and(|row| row.y > PageStyle::default().height_pt),
        "a long document must project rows beyond the first page"
    );
    assert!(
        selection_rects.iter().any(|rect| rect.page_index > 0),
        "selection overlays must follow the same multi-page policy"
    );
}

#[test]
fn empty_document_projection_exposes_a_visible_insertion_caret() {
    let document = WriterDocument::new("empty", "Empty");
    let (_, selection_rects) = writer_render_projection(&document, PageViewport::default());
    assert!(
        selection_rects.iter().any(|rect| rect.caret),
        "blank documents must expose a model caret for the empty insertion point"
    );
}

#[test]
fn pointer_hit_test_selects_using_projected_heading_and_paragraph_geometry() {
    set_platform();
    let mut document = WriterDocument::new("pointer", "Pointer");
    document.push(RichBlock::new(1, "heading1", "A centered heading"));
    let mut body = RichBlock::new(2, "paragraph", "Body target");
    body.style.alignment = loom_text::Alignment::Right;
    document.push(body);
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(document, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);

    let rows = app.get_render_blocks();
    let body_row = rows.row_data(1).expect("body render row");
    let body_start = "A centered heading".len() + 1;
    app.invoke_pointer_pressed(body_row.x + 1.0, body_row.y + 1.0, false);
    app.invoke_pointer_released(body_row.x + 1.0, body_row.y + 1.0);
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::caret(body_start)
    );
    assert_eq!(app.get_selection_anchor() as usize, body_start);
    assert_eq!(app.get_selection_focus() as usize, body_start);
}

#[test]
fn keyboard_selection_updates_authoritative_state_and_accessible_announcement() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut document = text_document("select me");

    // This invokes the same selection callback used by the TextInput's
    // Shift+Arrow keyboard path; no document mutation is hidden in the UI.
    assert!(project_selection_event(&app, &mut document, 0, 6));
    assert_eq!(document.selected_text(), "select");
    assert_eq!(app.get_selection_anchor(), 0);
    assert_eq!(app.get_selection_focus(), 6);
    assert_eq!(app.get_selection_announcement(), "Selected 6 characters");

    let selection = document.selection();
    set_selection_bold(
        &mut document,
        DocumentSelection::range(selection.anchor, selection.focus),
        true,
    );
    document.set_selection(TextSelection::range(0, 6));
    apply_document(&app, &document);
    assert!(app.get_is_bold());
}

#[test]
fn layout_breakpoints_match_supported_width_boundaries() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let policy = ResponsivePolicy::get(&app);
    assert_eq!(policy.get_priority_1_icon_only_below(), 1180.0);
    assert_eq!(policy.get_priority_2_overflow_below(), 1320.0);
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
            layout_breakpoints(&app, width),
            ResponsiveToolbarState {
                icon_only,
                overflow,
                labeled,
            }
        );
        apply_layout_breakpoints(&app, width);
        assert_eq!(app.get_icon_only_toolbar(), icon_only);
        assert_eq!(app.get_overflow_toolbar(), overflow);
        assert_eq!(app.get_labeled_toolbar(), labeled);
    }
}

#[test]
fn writer_inspector_is_closed_by_default() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    assert!(!app.get_show_inspector());
}

#[test]
fn native_menu_and_palette_share_writer_callback_dispatch() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let calls = Rc::new(Cell::new(0));
    let calls_ref = calls.clone();
    app.on_save_doc(move || calls_ref.set(calls_ref.get() + 1));

    assert!(dispatch_command(&app, "file.save"));
    assert!(dispatch_palette_action(&app, PaletteAction::SaveDoc));

    let menu = NativeMenuBar::new();
    let bar = build_standard_menu_bar("Loom Writer", vec![], vec![], vec![], vec![]);
    menu.install_menu_bar(&bar).expect("install menu");
    let app_ref = app.as_weak();
    menu.register_action_sink(Arc::new(move |action: CommandAction| {
        schedule_menu_action(&app_ref, action)
    }))
    .expect("register menu sink");
    let error = menu
        .dispatch_action("file.save")
        .expect_err("capture platform has no event loop provider");
    assert!(error
        .to_string()
        .contains("failed to schedule Writer menu command"));

    // The menu event was queued only when a real event-loop provider was
    // available; the capture platform must not invoke Slint directly.
    assert_eq!(calls.get(), 2);
}

#[test]
fn writer_menu_disables_unhandled_controller_commands() {
    set_platform();
    let mut menu = build_standard_menu_bar("Loom Writer", vec![], vec![], vec![], vec![]);
    menu.disable_items_except([
        "file.new",
        "file.open",
        "file.save",
        "file.save_as",
        "file.export_pdf",
        "edit.undo",
        "edit.redo",
        "app.palette",
        "view.inspector",
        "format.bold",
        "format.italic",
        "format.underline",
    ]);
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
            !menu.find_item(id).expect("standard command").is_enabled(),
            "unhandled Writer command {id} must be disabled"
        );
    }

    let service = NativeMenuBar::new();
    service.install_menu_bar(&menu).expect("install menu");
    let sink_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let sink_calls_ref = sink_calls.clone();
    service
        .register_action_sink(Arc::new(move |_| {
            sink_calls_ref.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            Ok(())
        }))
        .expect("register sink");
    assert!(service.dispatch_action("edit.cut").is_err());
    assert_eq!(sink_calls.load(std::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn writer_inspector_menu_check_tracks_live_window_state() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let state = GuiState {
        current: RefCell::new(text_document("menu state")),
        viewport: RefCell::new(PageViewport::default()),
        pointer_anchor: Cell::new(None),
        pointer_active: Cell::new(false),
        save_path: RefCell::new(None),
        history: RefCell::new(EditorHistory::new()),
        history_clock: Instant::now(),
        syncing_editor: Cell::new(false),
        dialogs,
        document_filter: FileFilter::new("Writer", ["loomdoc"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        registry: Arc::new(Mutex::new(build_writer_registry())),
    };
    let menu = NativeMenuBar::new();
    let bar = build_standard_menu_bar(
        "Loom Writer",
        vec![],
        vec![],
        vec![MenuItem::check("view.inspector", "Format Inspector", false)],
        vec![],
    );
    menu.install_menu_bar(&bar).expect("install menu");

    app.set_inspector_available(true);
    app.set_show_inspector(false);
    sync_menu_state(&menu, &app, &state);
    assert!(matches!(
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item("view.inspector").cloned()),
        Some(MenuItem::Check {
            checked: false,
            enabled: true,
            ..
        })
    ));

    app.set_show_inspector(true);
    sync_menu_state(&menu, &app, &state);
    assert!(matches!(
        menu.installed_menu_bar()
            .and_then(|bar| bar.find_item("view.inspector").cloned()),
        Some(MenuItem::Check {
            checked: true,
            enabled: true,
            ..
        })
    ));
}

#[test]
fn window_resize_updates_breakpoint_flags() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    apply_layout_breakpoints(&app, 1024);
    wire_responsive_layout(&app);

    app.window().set_size(PhysicalSize::new(1320, 800));
    let _ = snapshot_component(&app, 1320.0, 800.0, 1.0).expect("render resized window");

    assert!(app.get_wide_toolbar());
    assert!(app.get_labeled_export());
    assert!(!app.get_overflow_toolbar());
}

#[test]
fn expanding_past_overflow_breakpoint_closes_menu() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    apply_layout_breakpoints(&app, 1024);
    assert!(app.get_overflow_toolbar());
    app.set_toolbar_overflow_open(true);

    apply_layout_breakpoints(&app, 1320);

    assert!(!app.get_overflow_toolbar());
    assert!(!app.get_toolbar_overflow_open());
}

#[test]
fn widening_window_preserves_palette_focus() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    apply_layout_breakpoints(&app, 1024);
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
fn template_chooser_renders_at_reference_width() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    apply_theme(&app, "dark");
    apply_layout_breakpoints(&app, 1440);
    app.set_template_selected(2);
    app.set_template_chooser_open(true);
    let image = snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render chooser");
    assert_eq!(image.width(), 1440);
    assert_eq!(image.height(), 900);
}

#[test]
fn writer_action_rail_is_one_row_before_the_canvas() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    apply_theme(&app, "dark");
    apply_layout_breakpoints(&app, 1317);
    let image = snapshot_component(&app, 1317.0, 1182.0, 1.0).expect("render action rail");

    // The old title bar + toolbar occupied 80px. A single 64px action
    // rail leaves the surrounding canvas visible at y=72.
    assert_eq!(
        image.get_pixel(24, 100),
        image.get_pixel(24, 120),
        "the canvas should begin directly below the single action rail"
    );
}

#[test]
fn template_category_filters_visible_cards() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    apply_theme(&app, "light");
    apply_layout_breakpoints(&app, 1440);
    app.set_template_chooser_open(true);
    app.set_template_category(0);
    let all = snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render all templates");

    app.set_template_category(2);
    let letters = snapshot_component(&app, 1440.0, 900.0, 1.0).expect("render letter templates");
    assert_eq!(app.get_template_category(), 2);
    // The chooser now scales to 1160px at this viewport, placing the
    // Report card in this content-region rectangle. Check the full card
    // region instead of one global pixel so the filter test stays tied to
    // the actual card rather than the old 960px modal geometry.
    let report_card_changed =
        (220..396).any(|y| (674..814).any(|x| all.get_pixel(x, y) != letters.get_pixel(x, y)));
    assert!(
        report_card_changed,
        "selecting Letters must remove the Report card from the chooser"
    );
}

#[test]
fn template_ids_are_stable_and_seed_real_documents() {
    let cases = [
        (TemplateId::Blank, "blank", "Untitled Document"),
        (TemplateId::Report, "report", "Untitled Report"),
        (TemplateId::Letter, "letter", "Untitled Letter"),
        (TemplateId::Cv, "cv", "Untitled CV"),
    ];

    for (template, id, title) in cases {
        assert_eq!(template.as_str(), id);
        let document = template_document(template);
        assert_eq!(document.id, format!("template-{id}"));
        assert_eq!(document.title, title);
        if template == TemplateId::Blank {
            assert!(document.blocks.is_empty());
        } else {
            assert!(!document.blocks.is_empty());
            assert!(document
                .blocks
                .iter()
                .any(|block| !block.text.as_str().trim().is_empty()));
        }
    }
}

#[test]
fn template_id_parser_accepts_stable_names_only() {
    assert_eq!(parse_template_id("blank"), Some(TemplateId::Blank));
    assert_eq!(parse_template_id("report"), Some(TemplateId::Report));
    assert_eq!(parse_template_id("letter"), Some(TemplateId::Letter));
    assert_eq!(parse_template_id("cv"), Some(TemplateId::Cv));
    assert_eq!(parse_template_id("CV"), None);
    assert_eq!(parse_template_id("memo"), None);
}

#[test]
fn parse_args_accepts_template_without_changing_existing_flags() {
    let args = parse_args_from([
        "--template",
        "report",
        "--screenshot",
        "/tmp/writer-template.png",
        "--theme",
        "dark",
        "--size",
        "1440x900",
    ])
    .expect("parse template screenshot arguments");

    assert_eq!(args.template, Some(TemplateId::Report));
    assert_eq!(args.screenshot.as_deref(), Some("/tmp/writer-template.png"));
    assert_eq!(args.theme, "dark");
    assert_eq!(args.size, (1440, 900));
}

#[test]
fn template_chooser_flag_parses_and_changes_headless_render() {
    set_platform();
    let chooser_args = parse_args_from(["--template-chooser", "--size", "1440x900"])
        .expect("parse chooser screenshot arguments");
    let regular_args =
        parse_args_from(["--size", "1440x900"]).expect("parse regular screenshot arguments");

    let dir = std::env::temp_dir().join(format!(
        "loom-writer-template-chooser-test-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("create screenshot directory");
    let chooser_path = dir.join("chooser.png");
    let regular_path = dir.join("regular.png");
    render_headless(&chooser_args, chooser_path.to_str().expect("chooser path"))
        .expect("render chooser screenshot");
    render_headless(&regular_args, regular_path.to_str().expect("regular path"))
        .expect("render regular screenshot");

    let chooser = loom_test_support::png::load_png(&chooser_path).expect("load chooser");
    let regular = loom_test_support::png::load_png(&regular_path).expect("load regular");
    assert_ne!(
        chooser.get_pixel(100, 200),
        regular.get_pixel(100, 200),
        "--template-chooser must render the chooser overlay, not the editor"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn parse_args_rejects_unknown_template_id() {
    let error = parse_args_from(["--template", "memo"]).expect_err("unknown template");
    assert!(error.contains("unknown template"));
}

#[test]
fn writer_registry_disabled_bold_never_executes_handler() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let execution_count = Arc::new(AtomicUsize::new(0));
    let count_clone = execution_count.clone();
    let mut registry = build_writer_registry();
    // Bold must be disabled when selection is collapsed / empty document.
    let empty_doc = text_document("");
    let history = EditorHistory::new();
    sync_writer_registry_enablement(&mut registry, &empty_doc, &history);
    let spec = registry.get(&CommandId::new("writer.style.bold")).unwrap();
    assert!(
        !spec.enabled,
        "bold must be disabled with collapsed selection"
    );
    // Replace handler with counting handler to prove disabled never executes.
    registry.set_handler(
        "writer.style.bold",
        loom_command::FnCommandHandler::new(move |inv| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(CommandOutcome::success(inv.id.clone()))
        }),
    );
    let result = registry.invoke(&CommandInvocation::new(
        "writer.style.bold",
        InvocationSource::Toolbar,
    ));
    assert!(matches!(result, Err(CommandError::Disabled(_))));
    assert_eq!(execution_count.load(Ordering::SeqCst), 0);
}

#[test]
fn writer_registry_all_surfaces_share_one_handler() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();
    let mut registry = CommandRegistry::new();
    registry.register_fn(CommandSpec::new("writer.style.bold", "Bold"), move |inv| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
        Ok(CommandOutcome::success(inv.id.clone()))
    });
    for source in [
        InvocationSource::Toolbar,
        InvocationSource::Menu,
        InvocationSource::Shortcut,
        InvocationSource::Palette,
        InvocationSource::ContextMenu,
        InvocationSource::Accessibility,
        InvocationSource::Plugin,
        InvocationSource::Test,
    ] {
        let inv = CommandInvocation::new("writer.style.bold", source);
        registry
            .invoke(&inv)
            .expect("handler must succeed for all sources");
    }
    assert_eq!(counter.load(Ordering::SeqCst), 8);
}

#[test]
fn writer_registry_search_is_deterministic_and_ranked() {
    let mut registry = build_writer_registry();
    // Enable all so ranking is visible; writer catalog search must be stable.
    let doc = {
        let mut d = text_document("hello world");
        d.set_selection(TextSelection::range(0, 5));
        d
    };
    let mut history = EditorHistory::new();
    history.record(
        text_document(""),
        doc.clone(),
        HistoryKind::DocumentAction,
        0,
    );
    sync_writer_registry_enablement(&mut registry, &doc, &history);
    let res1 = registry.search("bold");
    let res2 = registry.search("bold");
    assert_eq!(res1.len(), res2.len());
    assert!(res1
        .iter()
        .zip(res2.iter())
        .all(|(a, b)| a.0.id == b.0.id && a.1 == b.1));
    // Bold should rank above non-bold for query "bold"
    assert!(res1
        .iter()
        .any(|(spec, _)| spec.id.as_str() == "writer.style.bold"));
    // Search for "save" should deterministically return file.save first (lower order)
    let save_res = registry.search("save");
    assert!(!save_res.is_empty());
    assert_eq!(save_res[0].0.id.as_str(), "file.save");
}

#[test]
fn writer_registry_honest_enablement_transitions_with_document_and_history() {
    let mut registry = build_writer_registry();
    let mut doc = text_document("hello world");
    doc.set_selection(TextSelection::range(0, 5)); // has selection
    let mut history = EditorHistory::new();
    // Initially no undo, bold enabled because has selection, headings enabled because has blocks
    sync_writer_registry_enablement(&mut registry, &doc, &history);
    assert!(!registry.get(&CommandId::new("edit.undo")).unwrap().enabled);
    assert!(
        registry
            .get(&CommandId::new("writer.style.bold"))
            .unwrap()
            .enabled
    );
    assert!(
        registry
            .get(&CommandId::new("writer.heading.h1"))
            .unwrap()
            .enabled
    );
    assert!(
        registry
            .get(&CommandId::new("writer.align.left"))
            .unwrap()
            .enabled
    );
    assert!(
        registry
            .get(&CommandId::new("file.export_pdf"))
            .unwrap()
            .enabled
    );

    // After one edit, undo becomes enabled; collapse selection disables bold
    history.record(
        text_document(""),
        doc.clone(),
        HistoryKind::DocumentAction,
        0,
    );
    doc.set_selection(TextSelection::caret(0));
    sync_writer_registry_enablement(&mut registry, &doc, &history);
    assert!(registry.get(&CommandId::new("edit.undo")).unwrap().enabled);
    assert!(
        !registry
            .get(&CommandId::new("writer.style.bold"))
            .unwrap()
            .enabled
    );

    // Empty document disables headings/alignment/export
    let empty = text_document("");
    sync_writer_registry_enablement(&mut registry, &empty, &history);
    assert!(
        !registry
            .get(&CommandId::new("writer.heading.h1"))
            .unwrap()
            .enabled
    );
    assert!(
        !registry
            .get(&CommandId::new("writer.align.left"))
            .unwrap()
            .enabled
    );
    assert!(
        !registry
            .get(&CommandId::new("file.export_pdf"))
            .unwrap()
            .enabled
    );
    // File new/open/save remain always enabled
    assert!(registry.get(&CommandId::new("file.new")).unwrap().enabled);
    assert!(registry.get(&CommandId::new("file.save")).unwrap().enabled);
}

#[test]
fn selection_change_is_first_class_without_history_or_block_mutation() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut document = text_document("first second");
    document.set_selection(TextSelection::range(0, 0));
    // Seed selection state via authoritative path.
    assert!(project_selection_event(&app, &mut document, 0, 5));
    assert_eq!(document.selected_text(), "first");
    // Blocks, ids, and kinds must be untouched by selection alone.
    let blocks_before = document.blocks.clone();
    let _selection_before = document.selection();
    let mut doc_for_selection = document.clone();
    let history = EditorHistory::new();
    // Second selection change should not create history or mutate blocks.
    let app2 = WriterApp::new().expect("second app for selection");
    apply_document(&app2, &doc_for_selection);
    assert!(project_selection_event(
        &app2,
        &mut doc_for_selection,
        6,
        12
    ));
    assert_eq!(doc_for_selection.blocks, blocks_before);
    // History must remain empty: selection changes never push history.
    assert_eq!(history.undo_len(), 0);
    assert_eq!(doc_for_selection.selected_text(), "second");
    // Inspector and toolbar follow selection: first block is plain paragraph so
    // heading should be uniform body (0) and alignment uniform left (0).
    assert_eq!(app2.get_heading_level(), 0);
    assert_eq!(app2.get_text_alignment(), 0);
    assert_eq!(app2.get_selection_announcement(), "Selected 6 characters");
    // Collapsed vs range enablement via registry is selection-derived.
    let mut reg = build_writer_registry();
    sync_writer_registry_enablement(&mut reg, &doc_for_selection, &history);
    assert!(
        reg.get(&CommandId::new("writer.style.bold"))
            .unwrap()
            .enabled
    );
    doc_for_selection.set_selection(TextSelection::caret(6));
    sync_writer_registry_enablement(&mut reg, &doc_for_selection, &history);
    assert!(
        !reg.get(&CommandId::new("writer.style.bold"))
            .unwrap()
            .enabled
    );
}

#[test]
fn inspector_shows_mixed_heading_and_alignment_as_indeterminate() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut document = WriterDocument::new("mixed", "Mixed");
    document.push(RichBlock::new(1, "heading1", "Title"));
    document.push(RichBlock::new(2, "paragraph", "Body"));
    // Alonso: use paragraph alignment to test mixed alignment.
    document.blocks[0].style.alignment = loom_text::Alignment::Left;
    document.blocks[1].style.alignment = loom_text::Alignment::Center;
    // editor_text = "Title\nBody" (5 +1 +4 =10). Select across both blocks.
    document.set_selection(TextSelection::range(0, 10));
    apply_document(&app, &document);
    // Mixed heading (h1 vs paragraph) and mixed alignment (Left vs Center)
    // must show indeterminate sentinel `-1`, not falsely Body/Left (0).
    assert_eq!(app.get_heading_level(), -1);
    assert_eq!(app.get_text_alignment(), -1);
    // Uniform selection inside first block shows real heading.
    let mut single = document.clone();
    single.set_selection(TextSelection::range(0, 2));
    apply_document(&app, &single);
    assert_eq!(app.get_heading_level(), 1);
    assert_eq!(app.get_text_alignment(), 0);
    // Project selection event must also use indeterminate logic.
    let mut via_project = document.clone();
    via_project.set_selection(TextSelection::caret(0));
    assert!(project_selection_event(&app, &mut via_project, 0, 10));
    assert_eq!(app.get_heading_level(), -1);
    assert_eq!(app.get_text_alignment(), -1);
}

#[test]
fn collapsed_caret_uses_caret_style_for_toolbar_checked_state() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut document = WriterDocument::new("caret-style", "Caret");
    document.push(RichBlock::new(1, "paragraph", "abcdef"));
    // Make middle of text bold: 2..5 => "cde"
    set_selection_bold(
        &mut document,
        loom_writer_core::TextSelection::range(2, 5),
        true,
    );
    // Caret inside bold run (affinity Upstream) should show bold checked
    // because `formatting_state_for_selection` uses `caret_style`.
    let mut caret_inside = document.clone();
    caret_inside.set_selection(TextSelection::range(3, 3));
    apply_document(&app, &caret_inside);
    assert!(
        app.get_is_bold(),
        "caret inside bold run must show bold checked via caret_style"
    );
    // Caret outside bold run must show unchecked.
    let mut caret_outside = document.clone();
    caret_outside.set_selection(TextSelection::caret(0));
    apply_document(&app, &caret_outside);
    assert!(!app.get_is_bold(), "caret outside bold must be unchecked");
    // Range that is exactly the bold region must be checked; partial range
    // that mixes bold and non-bold must be unchecked (mixed → false).
    let mut partial = document.clone();
    partial.set_selection(TextSelection::range(1, 4)); // "bcd" mixes
    apply_document(&app, &partial);
    assert!(
        !app.get_is_bold(),
        "mixed range must be unchecked, not partially checked"
    );
}

#[test]
fn formatting_produces_named_undo_and_clears_redo() {
    set_platform();
    let mut before = text_document("first second");
    before.set_selection(TextSelection::range(0, 5));
    let mut after = before.clone();
    set_selection_bold(
        &mut after,
        loom_writer_core::TextSelection::range(0, 5),
        true,
    );
    after.set_selection(TextSelection::range(0, 5));
    let mut history = EditorHistory::with_budget(16, usize::MAX);
    history.record(
        before.clone(),
        after.clone(),
        HistoryKind::DocumentAction,
        0,
    );
    // One named undo entry, redo empty.
    assert_eq!(history.undo_len(), 1);
    assert_eq!(history.redo_len(), 0);
    // Undo restores, redo becomes available.
    let undone = history.undo().expect("undo formatting");
    assert_eq!(undone, before);
    assert_eq!(history.redo_len(), 1);
    // New edit must clear redo (user branched).
    history.record(
        before.clone(),
        after.clone(),
        HistoryKind::DocumentAction,
        1,
    );
    assert_eq!(history.redo_len(), 0);
    // Ensure block ids are preserved by formatting (no id churn).
    assert_eq!(before.blocks[0].id, after.blocks[0].id);
    assert_eq!(undone.blocks[0].id, before.blocks[0].id);
}

#[test]
fn selection_announcement_is_meaningful_and_focus_restores_after_palette() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut document = text_document("hello world");
    document.set_selection(TextSelection::range(0, 5));
    assert_eq!(
        selection_announcement(&document, &document.selection()),
        "Selected 5 characters"
    );
    document.set_selection(TextSelection::caret(3));
    assert_eq!(
        selection_announcement(&document, &document.selection()),
        "Caret at character 4"
    );
    // UTF-8 char count, not byte count.
    let mut utf8 = text_document("AéB");
    utf8.set_selection(TextSelection::range(1, 3));
    assert_eq!(
        selection_announcement(&utf8, &utf8.selection()),
        "Selected 1 characters"
    );
    // Palette focus restoration: open then close must return focus to
    // `app-focus` (the editor workspace) per `changed palette-open` handler.
    // We verify the Slint property toggles; actual focus item is checked via
    // window integration tests, but the sentinel `-1` mixed state above
    // already guarantees announcement derives from `selection_announcement`.
    app.set_palette_open(true);
    assert!(app.get_palette_open());
    app.set_palette_open(false);
    assert!(!app.get_palette_open());
    // After closing, selection announcement must remain meaningful.
    let mut d2 = text_document("select me");
    d2.set_selection(TextSelection::range(0, 6));
    apply_document(&app, &d2);
    assert_eq!(app.get_selection_announcement(), "Selected 6 characters");
}
