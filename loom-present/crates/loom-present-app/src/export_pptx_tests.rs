//! The "Export PowerPoint (.pptx)" command: menu, palette, native-dialog flow and result.

use super::*;
use loom_desktop::ScriptedFileDialogs;
use loom_present_core::extract_pptx_titles;

fn state_with(dialogs: ScriptedFileDialogs) -> Rc<GuiState> {
    Rc::new(GuiState {
        session: RefCell::new(sample_session()),
        last_saved: RefCell::new(sample_session().document.clone()),
        last_saved_transitions: RefCell::default(),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(dialogs),
        deck_filter: FileFilter::new("Deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    })
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("loom-present-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

#[test]
fn export_command_writes_a_real_pptx_through_the_save_dialog() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dir = temp_dir("pptx-export");
    let path = dir.join("deck.pptx");
    let state = state_with(ScriptedFileDialogs::new([], [Some(path.clone())]));
    state
        .session
        .borrow_mut()
        .document
        .add_slide("Last slide", "content");
    let last = state.session.borrow().document.slides.len();
    state.session.borrow_mut().document.slides[last - 1].speaker_notes = "Say hello".into();
    wire_app_callbacks(&app, &state);

    app.invoke_export_pptx();

    assert!(
        app.get_status_left().starts_with("Exported "),
        "{}",
        app.get_status_left()
    );
    let bytes = std::fs::read(&path).expect("pptx written");
    assert!(bytes.starts_with(b"PK"));
    let titles = extract_pptx_titles(&bytes).expect("titles");
    assert_eq!(titles.len(), last);
    assert_eq!(titles[0], "Create without compromise");
    assert_eq!(titles[last - 1], "Last slide");
    // Stored (uncompressed) ZIP entries keep their names readable in the bytes.
    let needle = format!("ppt/notesSlides/notesSlide{last}.xml").into_bytes();
    assert!(bytes.windows(needle.len()).any(|w| w == needle));
    // The export is what the core produces for the live session.
    assert_eq!(
        bytes,
        loom_present_core::export_pptx(&state.session.borrow()).expect("export")
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancelling_or_failing_the_export_is_reported_and_writes_nothing() {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let dir = temp_dir("pptx-export-fail");
    let blocker = dir.join("not-a-folder");
    std::fs::write(&blocker, b"x").expect("write blocker");
    let state = state_with(ScriptedFileDialogs::new(
        [],
        [None, Some(blocker.join("deck.pptx"))],
    ));
    wire_app_callbacks(&app, &state);

    app.invoke_export_pptx();
    assert_eq!(app.get_status_left(), "Export cancelled");

    app.invoke_export_pptx();
    assert!(
        app.get_status_left().starts_with("Export failed"),
        "{}",
        app.get_status_left()
    );
    assert_eq!(std::fs::read(&blocker).unwrap(), b"x");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn menu_palette_and_command_dispatch_all_reach_the_export() {
    set_platform();
    // File menu item exists, is a supported command and dispatches.
    let menu_bar = build_present_menu_bar();
    let item = menu_bar.find_item("file.export_pptx").expect("menu item");
    assert_eq!(item.label(), Some("Export to PowerPoint..."));
    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"file.export_pptx"));
    assert!(is_present_menu_command("file.export_pptx"));

    let app = PresentApp::new().expect("create PresentApp");
    let dir = temp_dir("pptx-export-routes");
    let first = dir.join("menu.pptx");
    let second = dir.join("palette.pptx");
    let state = state_with(ScriptedFileDialogs::new(
        [],
        [Some(first.clone()), Some(second.clone())],
    ));
    wire_app_callbacks(&app, &state);
    wire_palette(&app);

    assert!(dispatch_command(&app, "file.export_pptx"));
    assert!(first.is_file());

    rebuild_palette(&app, "powerpoint");
    assert_eq!(app.get_palette_commands().row_count(), 1);
    app.set_palette_query("powerpoint".into());
    app.invoke_palette_invoked(0);
    assert!(second.is_file());
    let _ = std::fs::remove_dir_all(&dir);
}
