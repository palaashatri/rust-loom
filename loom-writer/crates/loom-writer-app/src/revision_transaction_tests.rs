use super::actions_tests::{test_state, text_document};
use super::*;

fn tracked_document(text: &str) -> WriterDocument {
    let mut document = text_document(text);
    document
        .tracked_changes
        .set_author("Revision test")
        .expect("valid revision author");
    document.tracked_changes.set_recording(true);
    document
}

fn setup(document: WriterDocument) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(document, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    apply_state(&app, &state);
    (app, state)
}

fn assert_document_with_valid_revisions_serializes(document: &WriterDocument) {
    // The public save boundary validates revision anchors against document blocks.
    let _serialized = loom_writer_core::save_document(document)
        .expect("document with valid revisions serializes");
}

fn history_shape(state: &GuiState) -> (usize, usize, usize) {
    let history = state.history.borrow();
    (
        history.undo_len(),
        history.redo_len(),
        history.total_bytes(),
    )
}

#[test]
fn tracked_typing_is_one_undo_step_and_undo_redo_restore_revisions() {
    let mut document = tracked_document("ac");
    document.set_selection(TextSelection::caret(1));
    let (app, state) = setup(document);

    app.invoke_document_edited("aXc".into(), 2, 2);

    let after = state.current.borrow().clone();
    assert_eq!(after.editor_text(), "aXc");
    assert_eq!(state.history.borrow().undo_len(), 1);
    let revisions = after.tracked_changes.revisions();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0].kind, loom_writer_core::RevisionKind::Insertion);
    assert_eq!((revisions[0].anchor.start, revisions[0].anchor.end), (1, 2));
    assert_document_with_valid_revisions_serializes(&after);

    app.invoke_undo();
    let undone = state.current.borrow().clone();
    assert_eq!(undone.editor_text(), "ac");
    assert!(undone.tracked_changes.revisions().is_empty());

    app.invoke_redo();
    assert_eq!(*state.current.borrow(), after);
}

#[test]
fn tracked_deletion_records_original_text_and_is_undoable() {
    let mut document = tracked_document("remove this");
    document.set_selection(TextSelection::range(0, 7));
    let (app, state) = setup(document);

    app.invoke_document_edited("this".into(), 0, 0);

    let after = state.current.borrow().clone();
    assert_eq!(after.editor_text(), "this");
    assert_eq!(state.history.borrow().undo_len(), 1);
    let revisions = after.tracked_changes.revisions();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0].kind, loom_writer_core::RevisionKind::Deletion);
    assert_eq!(revisions[0].before.as_ref().unwrap().text, "remove ");
    assert_document_with_valid_revisions_serializes(&after);

    app.invoke_undo();
    assert_eq!(state.current.borrow().editor_text(), "remove this");
    assert!(state
        .current
        .borrow()
        .tracked_changes
        .revisions()
        .is_empty());
    app.invoke_redo();
    assert_eq!(*state.current.borrow(), after);
}

#[test]
fn tracked_find_replacement_records_deletion_and_insertion_in_one_history_step() {
    let mut document = tracked_document("cat sat");
    document.set_selection(TextSelection::range(0, 3));
    let (app, state) = setup(document);
    let bar = app.global::<FindBar>();
    bar.set_query("cat".into());
    bar.set_replacement("dog".into());

    bar.invoke_replace_current();

    let after = state.current.borrow().clone();
    assert_eq!(after.editor_text(), "dog sat");
    assert_eq!(state.history.borrow().undo_len(), 1);
    let revisions = after.tracked_changes.revisions();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].kind, loom_writer_core::RevisionKind::Deletion);
    assert_eq!(revisions[0].before.as_ref().unwrap().text, "cat");
    assert_eq!(revisions[1].kind, loom_writer_core::RevisionKind::Insertion);
    assert_eq!(revisions[1].after.as_ref().unwrap().text, "dog");

    app.invoke_undo();
    assert_eq!(state.current.borrow().editor_text(), "cat sat");
    assert!(state
        .current
        .borrow()
        .tracked_changes
        .revisions()
        .is_empty());
    app.invoke_redo();
    assert_eq!(*state.current.borrow(), after);
}

#[test]
fn tracked_inline_formatting_is_one_undoable_revision() {
    let mut document = tracked_document("plain");
    document.set_selection(TextSelection::range(0, 5));
    let (app, state) = setup(document);

    app.invoke_toggle_bold();

    let after = state.current.borrow().clone();
    assert_eq!(state.history.borrow().undo_len(), 1);
    let revisions = after.tracked_changes.revisions();
    assert_eq!(revisions.len(), 1);
    assert_eq!(
        revisions[0].kind,
        loom_writer_core::RevisionKind::Formatting
    );
    assert_eq!((revisions[0].anchor.start, revisions[0].anchor.end), (0, 5));
    assert_ne!(
        revisions[0].before.as_ref().unwrap().runs,
        revisions[0].after.as_ref().unwrap().runs
    );

    app.invoke_undo();
    assert!(state
        .current
        .borrow()
        .tracked_changes
        .revisions()
        .is_empty());
    assert!(state.current.borrow().blocks[0].runs.is_empty());
    app.invoke_redo();
    assert_eq!(*state.current.borrow(), after);
}

#[test]
fn replace_all_uses_exact_hunks_and_keeps_an_anchor_between_matches() {
    let mut document = tracked_document("hit keep hit");
    document.set_selection(TextSelection::caret(3));
    let (app, state) = setup(document);
    app.invoke_document_edited("hitX keep hit".into(), 4, 4);
    let original_id = state.current.borrow().tracked_changes.revisions()[0].id;

    let bar = app.global::<FindBar>();
    bar.set_query("hit".into());
    bar.set_replacement("long".into());
    bar.invoke_replace_all();

    let after = state.current.borrow().clone();
    assert_eq!(after.editor_text(), "longX keep long");
    assert_eq!(bar.get_status().as_str(), "2 replaced");
    let stable = after
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| revision.id == original_id)
        .expect("original insertion revision remains present");
    assert_eq!((stable.anchor.start, stable.anchor.end), (4, 5));
    assert_eq!(
        after.blocks[0]
            .text
            .as_str()
            .get(stable.anchor.start..stable.anchor.end),
        Some("X")
    );
    assert_eq!(after.tracked_changes.revisions().len(), 5);
    assert_document_with_valid_revisions_serializes(&after);

    app.invoke_undo();
    let undone = state.current.borrow().clone();
    assert_eq!(undone.editor_text(), "hitX keep hit");
    assert_eq!(undone.tracked_changes.revisions().len(), 1);
    app.invoke_redo();
    assert_eq!(*state.current.borrow(), after);
}

#[test]
fn recording_off_edits_rebase_existing_revision_anchors() {
    let mut document = tracked_document("abcd");
    document.set_selection(TextSelection::caret(2));
    let (app, state) = setup(document);
    app.invoke_document_edited("abXcd".into(), 3, 3);
    let revision_id = state.current.borrow().tracked_changes.revisions()[0].id;

    app.global::<WriterReviewState>().invoke_toggle_recording();
    assert!(!state.current.borrow().tracked_changes.recording());
    {
        let mut current = state.current.borrow_mut();
        current.set_selection(TextSelection::caret(0));
        apply_document(&app, &current);
    }
    app.invoke_document_edited("YabXcd".into(), 1, 1);

    let after = state.current.borrow();
    assert_eq!(after.editor_text(), "YabXcd");
    assert!(!after.tracked_changes.recording());
    let revisions = after.tracked_changes.revisions();
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0].id, revision_id);
    assert_eq!((revisions[0].anchor.start, revisions[0].anchor.end), (3, 4));
    assert_eq!(after.blocks[0].text.as_str().get(3..4), Some("X"));
    assert_document_with_valid_revisions_serializes(&after);
}

#[test]
fn anchors_survive_edits_across_paragraphs_and_within_one_table_cell() {
    let mut document = tracked_document("alpha\nbeta");
    document.set_selection(TextSelection::range(3, 8));
    let (app, state) = setup(document);
    let mut next = state.current.borrow().clone();
    next.replace_selection_text("X")
        .expect("replace across ordinary paragraphs");
    assert!(apply_with_history(
        &app,
        &state,
        next,
        HistoryKind::DocumentAction
    ));

    let after_paragraph_edit = state.current.borrow().clone();
    assert!(after_paragraph_edit
        .tracked_changes
        .revisions()
        .iter()
        .any(|revision| {
            revision.kind == loom_writer_core::RevisionKind::Deletion
                && revision.before.as_ref().unwrap().text == "ha\nbe"
        }));
    assert_document_with_valid_revisions_serializes(&after_paragraph_edit);

    let mut table = loom_writer_core::WriterTable::new("table-1", 2, 2);
    table.set(1, 0, "North");
    table.set(1, 1, "10");
    let mut document = WriterDocument::new("table-edit", "Table edit");
    document
        .blocks
        .push(loom_writer_core::RichBlock::new(1, "paragraph", "Intro"));
    let table_block_id = 2;
    document.blocks.push(loom_writer_core::markdown_table_block(
        table_block_id,
        &table.to_markdown(),
    ));
    document
        .blocks
        .push(loom_writer_core::RichBlock::new(3, "paragraph", "Outro"));
    document.tracked_changes.set_recording(true);
    let before = document.editor_text();
    let insert_at = before.find("North").expect("table cell text") + 2;
    document.set_selection(TextSelection::caret(insert_at));
    let (app, state) = setup(document);
    let mut edited = before;
    edited.insert(insert_at, 'X');
    app.invoke_document_edited(
        edited.into(),
        (insert_at + 1) as i32,
        (insert_at + 1) as i32,
    );

    let revision_id = state.current.borrow().tracked_changes.revisions()[0].id;
    let mut prefix = state.current.borrow().editor_text();
    prefix.insert(1, '!');
    app.invoke_document_edited(prefix.into(), 2, 2);

    let after_table_edit = state.current.borrow();
    let cell_revision = after_table_edit
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| revision.id == revision_id)
        .expect("table-cell revision remains present");
    assert_eq!(cell_revision.anchor.block_id, table_block_id);
    assert_document_with_valid_revisions_serializes(&after_table_edit);
}

#[test]
fn rejected_revision_finalization_keeps_document_history_recovery_and_projection_unchanged() {
    let (app, state) = setup(tracked_document("unchanged"));
    let before = state.current.borrow().clone();
    let projected_text = app.get_doc_content().to_string();
    let projected_selection = (app.get_selection_anchor(), app.get_selection_focus());
    let history_before = history_shape(&state);
    let writes_before = recovery::WRITES.with(Cell::get);
    let mut unsupported = before.clone();
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encode the fixture picture");
    unsupported.blocks[0].image = Some(
        loom_writer_core::InlineImage::from_bytes("photo.png", png)
            .expect("decode the fixture picture"),
    );

    assert!(!apply_with_history(
        &app,
        &state,
        unsupported.clone(),
        HistoryKind::DocumentAction,
    ));
    assert_eq!(*state.current.borrow(), before);
    assert_eq!(history_shape(&state), history_before);
    assert_eq!(recovery::WRITES.with(Cell::get), writes_before);
    assert_eq!(app.get_doc_content().as_str(), projected_text);
    assert_eq!(
        (app.get_selection_anchor(), app.get_selection_focus()),
        projected_selection
    );
    assert!(app.get_status_left().as_str().contains("Edit not applied:"));

    apply_typing(&app, &state, unsupported);
    assert_eq!(*state.current.borrow(), before);
    assert_eq!(history_shape(&state), history_before);
    assert_eq!(recovery::WRITES.with(Cell::get), writes_before);
    assert_eq!(app.get_doc_content().as_str(), projected_text);
    assert!(app.get_status_left().as_str().contains("Edit not applied:"));
}
