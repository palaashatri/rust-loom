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
fn filesystem_write_failure_reports_the_cause_without_mutating_document_or_history() {
    let document = text_document("keep me");
    let root = scratch_dir("write-failure");
    let blocker = root.join("regular-file");
    std::fs::write(&blocker, b"not a directory")
        .expect("create regular file as destination parent");
    let destination = blocker.join("failed.docx");
    let (app, state) = test_state(document.clone(), scripted(vec![Some(destination.clone())]));
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
    let before_document = state.current.borrow().clone();
    let before_history = {
        let history = state.history.borrow();
        (history.undo_len(), history.redo_len())
    };

    app.invoke_export_docx();

    let status = app.get_status_left();
    let prefix = format!("Export failed: atomic write {}: ", destination.display());
    assert!(
        status.as_str().starts_with(&prefix),
        "filesystem failure detail missing from status: {}",
        status.as_str()
    );
    assert!(
        status.as_str().len() > prefix.len(),
        "filesystem cause missing from status: {}",
        status.as_str()
    );
    assert_eq!(*state.current.borrow(), before_document);
    assert_eq!(
        {
            let history = state.history.borrow();
            (history.undo_len(), history.redo_len())
        },
        before_history
    );
    assert!(
        !destination.exists(),
        "failed export must not create a DOCX"
    );

    std::fs::remove_dir_all(&root).expect("remove isolated DOCX failure root");
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
    assert_eq!(message, "Warning: 1 comment was left out. Exported a.docx");
}

#[test]
fn long_path_comment_warning_is_readable_in_the_compact_export_report() {
    let root = scratch_dir("long-warning");
    let mut parent = root.clone();
    let separator = std::path::MAIN_SEPARATOR.len_utf8();
    let path_chars = |path: &Path| path.display().to_string().chars().count();
    let mut target_chars = path_chars(&parent) + separator + "review.docx".len();
    while target_chars < 245 {
        let segment_chars = (245 - target_chars - separator).min(64);
        if segment_chars == 0 {
            break;
        }
        parent = parent.join("x".repeat(segment_chars));
        target_chars = path_chars(&parent) + separator + "review.docx".len();
    }
    std::fs::create_dir_all(&parent).unwrap();
    let target = parent.join("review.docx");
    let target_len = path_chars(&target);
    assert!((241..=246).contains(&target_len));

    let mut document = text_document("one two");
    document.comments.push(loom_writer_core::CommentThread {
        id: "orphaned-comment".into(),
        author: "A".into(),
        block_id: document.blocks[0].id,
        start: 0,
        end: 3,
        body: "This comment cannot be anchored in Word.".into(),
        resolved: false,
        orphaned: true,
    });
    let (app, state) = test_state(document, scripted(vec![Some(target.clone())]));
    wire_writer_shared_callbacks(&app, &state, None);
    app.window().set_size(slint::PhysicalSize::new(480, 360));

    app.invoke_export_docx();

    assert!(
        target.is_file(),
        "the requested long destination was written"
    );
    let status = app.get_status_left().to_string();
    let warning_position = status.find("Warning:").expect("warning leads the status");
    let path_position = status
        .find(&target.display().to_string())
        .expect("the status still names the destination");
    assert!(warning_position < path_position);
    assert!(app.get_docx_export_report_open());

    let report = app.get_docx_export_report_message().to_string();
    assert!(report.starts_with("1 comment could not be placed in Word and was left out."));
    let (_, displayed_path) = report
        .split_once("Saved to:\n")
        .expect("the full destination follows the loss details");
    assert_eq!(
        displayed_path.replace('\n', ""),
        target.display().to_string(),
        "visual line breaks preserve every character of a long path"
    );
    assert!(
        !displayed_path.contains('…'),
        "the report does not elide the path"
    );

    let _ = std::fs::remove_dir_all(&root);
}
