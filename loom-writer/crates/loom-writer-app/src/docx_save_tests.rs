//! Writer Save/Open format selection and no-loss preflight regressions.

use crate::actions_tests::test_state;
use crate::*;
use std::path::PathBuf;
use std::rc::Rc;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("loom-docxsave-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create test directory");
    dir
}

fn document(title: &str, text: &str) -> WriterDocument {
    let mut document = WriterDocument::new("docx-save-test", title);
    document.blocks.push(RichBlock::new(1, "paragraph", text));
    document
}

fn edit_text(app: &WriterApp, text: &str) {
    app.invoke_document_edited(
        SharedString::from(text),
        text.len().min(i32::MAX as usize) as i32,
        text.len().min(i32::MAX as usize) as i32,
    );
}

fn docx_bytes(document: &WriterDocument) -> Vec<u8> {
    loom_writer_core::export_docx(document)
        .expect("export test DOCX")
        .bytes
}

fn document_with_unrepresentable_comment() -> WriterDocument {
    let mut document = document("Comment source", "Keep this paragraph");
    let block_id = document.blocks[0].id;
    document
        .add_comment_thread(block_id, 0, 4, "This comment has no DOCX anchor")
        .expect("add comment");
    document.comments[0].orphaned = true;
    document
}

fn document_with_invalid_docx_color() -> WriterDocument {
    let mut document = document("Invalid color", "Invalid style");
    document.blocks[0].runs.push(loom_text::StyleRun {
        start: 0,
        end: 3,
        style: loom_text::CharacterStyle {
            color: Some("red".into()),
            ..Default::default()
        },
    });
    document
}

#[test]
fn untitled_docx_save_then_edit_save_and_reopen_uses_word_format() {
    let dir = temp_dir("untitled");
    let word = dir.join("first.docx");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(word.clone())],
    ));
    let initial = document("First document", "Before first save");
    let (app, state) = test_state(initial, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);

    app.invoke_save_doc();

    assert_eq!(state.save_path.borrow().as_deref(), Some(word.as_path()));
    assert!(!state.compatibility_save.get());
    assert!(std::fs::read(&word)
        .expect("read first DOCX")
        .starts_with(b"PK"));

    edit_text(&app, "Edited before the second save");
    assert!(document_is_dirty(&state));
    app.invoke_save_doc();

    let reopened = load_file(&word).expect("reopen saved DOCX");
    assert_eq!(reopened.editor_text(), "Edited before the second save");
    assert!(!document_is_dirty(&state));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_opened_docx_ordinary_save_replaces_that_docx_without_a_second_picker() {
    let dir = temp_dir("ordinary-save");
    let word = dir.join("report.docx");
    let original = document("Report", "Original body");
    std::fs::write(&word, docx_bytes(&original)).expect("write original DOCX");
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(original.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    replace_opened_document(&app, &state, word.clone(), original);

    edit_text(&app, "Updated body");
    app.invoke_save_doc();

    let reopened = load_file(&word).expect("reopen overwritten DOCX");
    assert_eq!(reopened.editor_text(), "Updated body");
    assert_eq!(state.save_path.borrow().as_deref(), Some(word.as_path()));
    assert!(!document_is_dirty(&state));
    assert!(!app.global::<DocxSaveWarningState>().get_dialog_open());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn opening_loomdoc_migrates_to_docx_without_overwriting_and_loomdoc_is_explicit() {
    let dir = temp_dir("migration");
    let legacy = dir.join("legacy.loomdoc");
    let migrated = dir.join("legacy.docx");
    let compatibility_copy = dir.join("compatibility.loomdoc");
    let original = document("Legacy", "Full-fidelity source");
    let legacy_bytes = loom_writer_core::save_document(&original).expect("save Loom package");
    std::fs::write(&legacy, &legacy_bytes).expect("write legacy source");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [
            None,
            Some(migrated.clone()),
            Some(compatibility_copy.clone()),
        ],
    ));
    let (app, state) = test_state(original.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    replace_opened_document(&app, &state, legacy.clone(), original);
    edit_text(&app, "Unsaved migration text");
    let saved_before_migration = state.last_saved.borrow().clone();
    let history_before_cancel = state.history.borrow().undo_len();

    app.invoke_save_doc();

    assert_eq!(
        std::fs::read(&legacy).expect("legacy bytes remain"),
        legacy_bytes
    );
    assert_eq!(state.save_path.borrow().as_deref(), Some(legacy.as_path()));
    assert!(document_is_dirty(&state));
    assert_eq!(*state.last_saved.borrow(), saved_before_migration);
    assert_eq!(state.history.borrow().undo_len(), history_before_cancel);

    app.invoke_save_doc();

    assert_eq!(
        std::fs::read(&legacy).expect("legacy bytes remain"),
        legacy_bytes
    );
    assert_eq!(
        load_file(&migrated)
            .expect("open migrated DOCX")
            .editor_text(),
        "Unsaved migration text"
    );
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(migrated.as_path())
    );
    assert!(!state.compatibility_save.get());

    app.invoke_save_as_doc();

    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(compatibility_copy.as_path())
    );
    assert!(state.compatibility_save.get());
    edit_text(&app, "Edited in explicit Loom compatibility mode");
    app.invoke_save_doc();
    assert_eq!(
        load_file(&compatibility_copy)
            .expect("reopen explicit Loom file")
            .editor_text(),
        "Edited in explicit Loom compatibility mode"
    );
    assert_eq!(
        std::fs::read(&legacy).expect("legacy bytes remain"),
        legacy_bytes
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn cancelling_docx_loss_confirmation_preserves_target_document_path_history_and_baseline() {
    let dir = temp_dir("loss-cancel");
    let word = dir.join("source.docx");
    let disk_document = document("On disk", "Unchanged source bytes");
    let original_bytes = docx_bytes(&disk_document);
    std::fs::write(&word, &original_bytes).expect("write source DOCX");
    let current = document_with_unrepresentable_comment();
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(current.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    replace_opened_document(&app, &state, word.clone(), current.clone());
    state.history.borrow_mut().record(
        document("Earlier", "Earlier content"),
        current.clone(),
        HistoryKind::DocumentAction,
        1,
    );
    let before_history = state.history.borrow().undo_len();
    let before_saved = state.last_saved.borrow().clone();

    app.invoke_save_doc();

    let warning = app.global::<DocxSaveWarningState>();
    assert!(warning.get_dialog_open());
    assert!(warning
        .get_warning_message()
        .contains("1 comment thread cannot be represented"));
    assert_eq!(
        std::fs::read(&word).expect("target unchanged"),
        original_bytes
    );
    warning.invoke_cancel_docx();

    assert!(!warning.get_dialog_open());
    assert_eq!(
        std::fs::read(&word).expect("target remains unchanged"),
        original_bytes
    );
    assert_eq!(*state.current.borrow(), current);
    assert_eq!(*state.last_saved.borrow(), before_saved);
    assert_eq!(state.save_path.borrow().as_deref(), Some(word.as_path()));
    assert_eq!(state.history.borrow().undo_len(), before_history);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn continuing_docx_loss_confirmation_writes_only_after_confirmation() {
    let dir = temp_dir("loss-continue");
    let word = dir.join("source.docx");
    let original = document("On disk", "Original bytes");
    let original_bytes = docx_bytes(&original);
    std::fs::write(&word, &original_bytes).expect("write source DOCX");
    let current = document_with_unrepresentable_comment();
    let dialogs: Rc<dyn FileDialogService> =
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(current.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    replace_opened_document(&app, &state, word.clone(), current.clone());

    app.invoke_save_doc();

    assert_eq!(
        std::fs::read(&word).expect("no write before choice"),
        original_bytes
    );
    app.global::<DocxSaveWarningState>().invoke_continue_docx();

    let reopened = load_file(&word).expect("open confirmed DOCX save");
    assert_eq!(reopened.editor_text(), current.editor_text());
    assert!(reopened.comments.is_empty());
    assert!(!document_is_dirty(&state));
    assert_eq!(*state.last_saved.borrow(), current);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn rejecting_docx_loss_during_loomdoc_migration_keeps_source_and_dirty_draft() {
    let dir = temp_dir("loomdoc-loss-reject");
    let legacy = dir.join("review-source.loomdoc");
    let first_docx = dir.join("migration-cancelled.docx");
    let retry_docx = dir.join("migration-confirmed.docx");
    let source_document = document_with_unrepresentable_comment();
    let legacy_bytes =
        loom_writer_core::save_document(&source_document).expect("save full-fidelity source");
    std::fs::write(&legacy, &legacy_bytes).expect("write source Loom package");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(first_docx.clone()), Some(retry_docx.clone())],
    ));
    let (app, state) = test_state(source_document.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    replace_opened_document(&app, &state, legacy.clone(), source_document.clone());
    edit_text(&app, "Edited while retaining the orphan comment");
    let current_before = state.current.borrow().clone();
    let baseline_before = state.last_saved.borrow().clone();
    let history_before = state.history.borrow().undo_len();

    app.invoke_save_doc();

    let warning = app.global::<DocxSaveWarningState>();
    assert!(warning.get_dialog_open());
    assert_eq!(
        std::fs::read(&legacy).expect("source remains before choice"),
        legacy_bytes
    );
    assert!(!first_docx.exists());
    warning.invoke_cancel_docx();

    assert!(!warning.get_dialog_open());
    assert_eq!(
        std::fs::read(&legacy).expect("source remains after rejection"),
        legacy_bytes
    );
    assert!(!first_docx.exists());
    assert_eq!(state.save_path.borrow().as_deref(), Some(legacy.as_path()));
    assert!(!state.compatibility_save.get());
    assert!(document_is_dirty(&state));
    assert_eq!(*state.current.borrow(), current_before);
    assert_eq!(*state.last_saved.borrow(), baseline_before);
    assert_eq!(state.history.borrow().undo_len(), history_before);

    app.invoke_save_doc();
    assert!(warning.get_dialog_open());
    assert_eq!(
        std::fs::read(&legacy).expect("source remains on retry"),
        legacy_bytes
    );
    warning.invoke_continue_docx();

    assert_eq!(
        std::fs::read(&legacy).expect("source remains after migration"),
        legacy_bytes
    );
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(retry_docx.as_path())
    );
    assert!(!document_is_dirty(&state));
    assert_eq!(
        load_file(&retry_docx)
            .expect("open confirmed migration")
            .editor_text(),
        "Edited while retaining the orphan comment"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn docx_preflight_failure_keeps_loomdoc_source_and_retry_can_commit() {
    let dir = temp_dir("preflight-failure");
    let legacy = dir.join("source.loomdoc");
    let migrated = dir.join("retry.docx");
    let original = document("Source", "Valid original");
    let legacy_bytes = loom_writer_core::save_document(&original).expect("save Loom package");
    std::fs::write(&legacy, &legacy_bytes).expect("write source package");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(migrated.clone()), Some(migrated.clone())],
    ));
    let invalid = document_with_invalid_docx_color();
    let (app, state) = test_state(invalid.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    replace_opened_document(&app, &state, legacy.clone(), invalid.clone());
    let mut dirty = invalid.clone();
    dirty.title = "Dirty invalid color".into();
    state.history.borrow_mut().record(
        invalid.clone(),
        dirty.clone(),
        HistoryKind::DocumentAction,
        1,
    );
    *state.current.borrow_mut() = dirty.clone();
    apply_state(&app, &state);
    let before_history = state.history.borrow().undo_len();
    let baseline_before = state.last_saved.borrow().clone();

    app.invoke_save_doc();

    assert!(!migrated.exists());
    assert_eq!(
        std::fs::read(&legacy).expect("source remains on preflight error"),
        legacy_bytes
    );
    assert_eq!(state.save_path.borrow().as_deref(), Some(legacy.as_path()));
    assert!(!state.compatibility_save.get());
    assert!(document_is_dirty(&state));
    assert_eq!(*state.current.borrow(), dirty);
    assert_eq!(*state.last_saved.borrow(), baseline_before);
    assert_eq!(state.history.borrow().undo_len(), before_history);
    assert!(!app.global::<DocxSaveWarningState>().get_dialog_open());

    state.current.borrow_mut().blocks[0].runs.clear();
    apply_state(&app, &state);
    app.invoke_save_doc();

    assert_eq!(
        std::fs::read(&legacy).expect("source remains after retry"),
        legacy_bytes
    );
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(migrated.as_path())
    );
    assert!(!document_is_dirty(&state));
    assert_eq!(
        load_file(&migrated).expect("open preflight retry").title,
        "Dirty invalid color"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn conflicting_save_extension_fails_before_any_path_or_document_change() {
    let dir = temp_dir("conflicting-extension");
    let conflicting = dir.join("report.docx.loomdoc");
    let original = document("Conflict", "Keep state");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(conflicting.clone())],
    ));
    let (app, state) = test_state(original.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    state.history.borrow_mut().record(
        document("Earlier", "Earlier content"),
        original.clone(),
        HistoryKind::DocumentAction,
        1,
    );
    let before_history = state.history.borrow().undo_len();

    app.invoke_save_doc();

    assert!(!conflicting.exists());
    assert_eq!(*state.current.borrow(), original);
    assert_eq!(*state.save_path.borrow(), None);
    assert_eq!(state.history.borrow().undo_len(), before_history);
    assert!(app
        .get_status_left()
        .contains("Conflicting Writer extensions"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn storage_write_failure_keeps_the_open_path_document_and_history() {
    let dir = temp_dir("write-failure");
    let legacy = dir.join("source.loomdoc");
    let legacy_retry = dir.join("retry.docx");
    let blocked_path = dir.join("blocking.docx");
    std::fs::create_dir_all(&blocked_path).expect("create non-file destination");
    std::fs::write(blocked_path.join("source.marker"), b"original bytes")
        .expect("write destination marker");
    let original = document("Failure", "Keep original state");
    let legacy_bytes = loom_writer_core::save_document(&original).expect("save Loom source");
    std::fs::write(&legacy, &legacy_bytes).expect("write source package");
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(blocked_path.clone()), Some(legacy_retry.clone())],
    ));
    let (app, state) = test_state(original.clone(), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    replace_opened_document(&app, &state, legacy.clone(), original.clone());
    edit_text(&app, "Edited before failed migration");
    let before_history = state.history.borrow().undo_len();
    let before_saved = state.last_saved.borrow().clone();
    let current_before = state.current.borrow().clone();

    app.invoke_save_doc();

    assert_eq!(
        std::fs::read(blocked_path.join("source.marker")).expect("marker remains"),
        b"original bytes"
    );
    assert_eq!(
        std::fs::read(&legacy).expect("Loom source remains"),
        legacy_bytes
    );
    assert_eq!(*state.current.borrow(), current_before);
    assert_eq!(*state.last_saved.borrow(), before_saved);
    assert_eq!(state.save_path.borrow().as_deref(), Some(legacy.as_path()));
    assert!(!state.compatibility_save.get());
    assert!(document_is_dirty(&state));
    assert_eq!(state.history.borrow().undo_len(), before_history);
    assert!(app.get_status_left().starts_with("Save failed:"));

    app.invoke_save_doc();

    assert_eq!(
        std::fs::read(&legacy).expect("Loom source remains after retry"),
        legacy_bytes
    );
    assert_eq!(
        load_file(&legacy_retry)
            .expect("open migration retry")
            .editor_text(),
        "Edited before failed migration"
    );
    assert_eq!(
        state.save_path.borrow().as_deref(),
        Some(legacy_retry.as_path())
    );
    assert!(!document_is_dirty(&state));
    let _ = std::fs::remove_dir_all(dir);
}
