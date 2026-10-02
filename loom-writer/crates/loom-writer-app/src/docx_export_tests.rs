use super::actions_tests::{test_state, text_document};
use super::docx_export::{self, suggested_name};
use super::*;

fn scratch_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("loom-writer-docx-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn scripted(saves: Vec<Option<PathBuf>>) -> Rc<dyn FileDialogService> {
    Rc::new(loom_desktop::ScriptedFileDialogs::new([], saves))
}

#[test]
fn suggested_name_uses_the_title_and_stays_a_valid_file_name() {
    assert_eq!(suggested_name("Quarterly Report"), "Quarterly Report.docx");
    assert_eq!(suggested_name("A/B: C?"), "A-B- C-.docx");
    assert_eq!(suggested_name("  "), "loom-writer-export.docx");
    assert_eq!(suggested_name("Draft..."), "Draft.docx");
}

#[test]
fn export_writes_a_word_package_and_reports_it() {
    let dir = scratch_dir("ok");
    let target = dir.join("out.docx");
    let mut document = text_document("Hello Word");
    document.title = "Title".into();
    let (app, state) = test_state(document.clone(), scripted(vec![Some(target.clone())]));
    wire_writer_shared_callbacks(&app, &state, None);

    app.invoke_export_docx();

    assert_eq!(
        app.get_status_left().as_str(),
        format!("Exported {}", target.display())
    );
    let bytes = std::fs::read(&target).unwrap();
    assert!(bytes.starts_with(b"PK"));
    let blocks = loom_writer_core::extract_docx_blocks(&bytes).unwrap();
    assert_eq!(blocks[0].text.as_str(), "Hello Word");
    assert_eq!(
        *state.current.borrow(),
        document,
        "export never edits the document"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_destination_without_extension_gets_docx() {
    let dir = scratch_dir("ext");
    let (path, _) =
        docx_export::export_docx_file(&dir.join("report"), &text_document("x")).unwrap();
    assert_eq!(path, dir.join("report.docx"));
    assert!(path.is_file());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cancel_and_empty_destination_leave_the_document_alone() {
    let document = text_document("keep me");
    let (app, state) = test_state(document.clone(), scripted(vec![None, Some(PathBuf::new())]));
    wire_writer_shared_callbacks(&app, &state, None);
    let before = (
        state.history.borrow().undo.len(),
        state.history.borrow().redo.len(),
    );

    app.invoke_export_docx();
    assert_eq!(app.get_status_left().as_str(), "Export cancelled");

    app.invoke_export_docx();
    assert_eq!(
        app.get_status_left().as_str(),
        "Export failed: Word destination is empty"
    );
    assert_eq!(*state.current.borrow(), document);
    assert_eq!(
        before,
        (
            state.history.borrow().undo.len(),
            state.history.borrow().redo.len()
        )
    );
}

#[test]
fn an_empty_document_cannot_be_exported() {
    let dir = scratch_dir("empty");
    let target = dir.join("never.docx");
    let empty = WriterDocument::new("e", "Empty");
    let (app, state) = test_state(empty, scripted(vec![Some(target.clone())]));
    wire_writer_shared_callbacks(&app, &state, None);

    app.invoke_export_docx();

    assert!(!target.exists());
    assert_eq!(
        app.get_status_right().as_str(),
        "Add content before exporting a Word document"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn menu_palette_and_registry_reach_the_same_export() {
    let dir = scratch_dir("surfaces");
    let first = dir.join("menu.docx");
    let second = dir.join("palette.docx");
    let (app, state) = test_state(
        text_document("surfaces"),
        scripted(vec![Some(first.clone()), Some(second.clone())]),
    );
    wire_writer_shared_callbacks(&app, &state, None);

    {
        let registry = state.registry.lock().unwrap();
        for id in ["file.export_docx", "writer.export-docx"] {
            assert!(registry.get(&CommandId::new(id)).unwrap().enabled, "{id}");
        }
    }
    assert!(local_menu::SUPPORTED_COMMANDS.contains(&"file.export_docx"));
    assert!(dispatch_command(&app, "file.export_docx"));
    assert!(first.is_file());
    assert!(dispatch_command(&app, "writer.export-docx"));
    assert!(second.is_file());
    assert!(master_palette(&app)
        .iter()
        .any(|command| command.id == "writer.export-docx"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn skipped_comments_are_named_in_the_status() {
    let mut document = text_document("one two");
    document.comments.push(loom_writer_core::CommentThread {
        id: "c".into(),
        author: "A".into(),
        block_id: document.blocks[0].id,
        start: 0,
        end: 3,
        body: "x".into(),
        resolved: false,
        orphaned: true,
    });
    let export = loom_writer_core::export_docx(&document).unwrap();
    let message = docx_export::success_message(Path::new("a.docx"), &export);
    assert_eq!(
        message,
        "Exported a.docx (1 comment could not be placed and was left out)"
    );
}
