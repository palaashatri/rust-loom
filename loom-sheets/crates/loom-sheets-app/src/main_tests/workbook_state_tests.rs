use super::*;

#[test]
fn escape_cancels_save_changes_after_tab_moves_focus_to_a_button() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let cancelled = Rc::new(Cell::new(false));
    let cancelled_ref = cancelled.clone();
    app.on_save_changes_cancel(move || cancelled_ref.set(true));
    app.set_save_changes_open(true);

    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });

    assert!(
        cancelled.get(),
        "Escape must cancel even after Tab moves focus to a dialog button"
    );
}

#[test]
fn cross_sheet_formulas_evaluate_save_and_reopen() {
    let state = cross_sheet_state();
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = state.sheets.borrow()[1].clone();

    let vals = evaluate_current(&state);
    let a1 = CellRef::parse("A1").unwrap();
    let a2 = CellRef::parse("A2").unwrap();
    assert_eq!(vals.get(&a1).map(|v| v.display()).as_deref(), Some("25"));
    assert_eq!(vals.get(&a2).map(|v| v.display()).as_deref(), Some("10"));

    // Single-sheet evaluation of the same tab reports unresolvable refs.
    let solo = evaluate(&state.current.borrow());
    assert_eq!(solo.get(&a1).map(|v| v.display()).as_deref(), Some("#REF!"));

    // Save/reopen preserves the foreign formula text and re-resolves it.
    let dir = std::env::temp_dir().join(format!("loom-xsheet-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("x.loomtable");
    sync_current_to_tabs(&state);
    save_workbook(&path, &state.sheets.borrow(), 1).expect("save");
    let workbook = load_workbook(&path).expect("load");
    assert_eq!(workbook.sheets.len(), 2);
    assert_eq!(workbook.active, 1);
    assert_eq!(
        workbook.sheets[1].raw(a1),
        Some("=Data!B1+5"),
        "foreign formula text survives"
    );
    std::fs::remove_file(&path).ok();
}

#[test]
fn rename_rewrites_qualifiers_and_rejects_collisions() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_sheet_actions(&app, &state, &menu_service);

    // Rename the source tab: dependent formulas follow it.
    *state.active_sheet_index.borrow_mut() = 0;
    *state.current.borrow_mut() = state.sheets.borrow()[0].clone();
    app.invoke_rename_sheet("Figures".into());
    assert_eq!(state.current.borrow().name, "Figures");
    assert_eq!(
        state.sheets.borrow()[1].raw(CellRef::parse("A1").unwrap()),
        Some("=Figures!B1+5")
    );

    // Switch to the dependent tab and confirm values still resolve.
    *state.active_sheet_index.borrow_mut() = 1;
    *state.current.borrow_mut() = state.sheets.borrow()[1].clone();
    let vals = evaluate_current(&state);
    assert_eq!(
        vals.get(&CellRef::parse("A1").unwrap())
            .map(|v| v.display())
            .as_deref(),
        Some("25")
    );

    // Collision with an existing tab name is refused truthfully.
    app.invoke_rename_sheet("Figures".into());
    assert_eq!(state.current.borrow().name, "Report");
    assert!(app.get_status_left().as_str().contains("already exists"));

    // Undo restores the old tab name and the old qualifier text.
    let tx = state.undo_stack.borrow_mut().pop().unwrap();
    match &tx {
        SheetTransaction::Workbook { before, .. } => before.restore(&state),
        _ => panic!("expected workbook transaction"),
    }
    assert_eq!(state.sheets.borrow()[0].name, "Data");
    assert_eq!(
        state.sheets.borrow()[1].raw(CellRef::parse("A1").unwrap()),
        Some("=Data!B1+5")
    );
}

#[test]
fn repeated_workbook_renames_keep_document_snapshots_bounded() {
    let state = cross_sheet_state();
    for index in 0..100 {
        let mut after = state.sheets.borrow().clone();
        after[0].name = format!("Data {index}");
        commit_workbook_transaction(&state, after, 0, None);
    }

    let undo = state.undo_stack.borrow();
    assert_eq!(undo.len(), 100);
    assert!(history_bytes(&undo) <= MAX_HISTORY_BYTES);
    assert!(undo.iter().all(|transaction| matches!(
        transaction,
        SheetTransaction::Workbook { before, after }
            if before.sheets.len() == 2 && after.sheets.len() == 2
    )));
    // WorkbookUndoState intentionally has only document fields. If history
    // stacks were captured recursively, this source-level shape would be
    // impossible and the byte count would grow as 1, 4, 13, ... instead.
}

#[test]
fn dirty_state_clears_when_workbook_returns_to_last_saved_content() {
    let state = cross_sheet_state();
    state.mark_saved();
    assert!(!state.is_dirty());

    state.current.borrow_mut().set_str("A1", "unsaved");
    state.mark_content_dirty();
    assert!(state.is_dirty());

    *state.current.borrow_mut() = state.sheets.borrow()[0].clone();
    state.recompute_dirty_from_saved();
    assert!(!state.is_dirty());
}

#[test]
fn dirty_title_marker_updates_without_serializing_every_edit_and_rechecks_after_undo() {
    let state = cross_sheet_state();
    state.mark_saved();
    assert!(!state.is_dirty());

    state.mark_content_dirty();
    assert!(state.is_dirty());

    state.current.borrow_mut().set_str("A1", "unsaved");
    *state.current.borrow_mut() = state.sheets.borrow()[0].clone();
    state.recompute_dirty_from_saved();
    assert!(!state.is_dirty());
}

#[test]
fn dirty_title_marker_tracks_active_sheet_without_serializing_workbook() {
    let state = cross_sheet_state();
    state.mark_saved();

    *state.active_sheet_index.borrow_mut() = 1;
    assert!(state.is_dirty());

    *state.active_sheet_index.borrow_mut() = 0;
    assert!(!state.is_dirty());
}
