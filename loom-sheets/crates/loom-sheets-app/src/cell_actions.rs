//! Cell editing callbacks shared by the GUI bootstrap and callback tests.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{CellRange, CellRef};
use slint::{ComponentHandle, SharedString};

use crate::workbook_worker::CellUpdate;
use crate::{
    apply_sheet, commit_formula_edit, evaluate, fill_selection_down, fill_target_range,
    project_current, selection_from_app, sync_menu_state, update_selection_range, GridSelection,
    GuiState, SheetsApp,
};

pub(crate) fn register_cell_edit_action(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    let state = state.clone();
    let app_ref = app.as_weak();
    let menu_service = menu_service.clone();
    app.on_commit_selected_cell(move |draft| {
        if let Some(app) = app_ref.upgrade() {
            if crate::close_operations::reject_admission(&app, &state) {
                return;
            }
            if crate::mutation_guard::refused(&app, &state) {
                return;
            }
            crate::object_actions::cancel_active_gesture(&app, &state);
            if let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) {
                state.tab_run.note_commit(cell);
                let committed = {
                    let mut current = state.current.borrow_mut();
                    let mut undo = state.undo_stack.borrow_mut();
                    let mut redo = state.redo_stack.borrow_mut();
                    commit_formula_edit(&mut current, &mut undo, &mut redo, cell, draft.as_str())
                };
                if committed {
                    let worker_update = {
                        let worker = state.workbook_worker.borrow();
                        worker.as_ref().map(|worker| {
                            let active_sheet = *state.active_sheet_index.borrow();
                            let raw = state.current.borrow().raw(cell).map(str::to_owned);
                            let revision = state.next_worker_revision();
                            let submitted = worker.submit_cell(CellUpdate {
                                revision,
                                active_sheet,
                                sheet: active_sheet,
                                cell,
                                raw,
                            });
                            (revision, submitted)
                        })
                    };
                    if let Some((revision, submitted)) = worker_update {
                        state.mark_content_dirty();
                        project_current(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                        match submitted {
                            Ok(()) => {
                                state.last_queued_worker_revision.set(revision);
                                state.pending_cell_commit.set(Some((revision, cell)));
                                app.set_formula_feedback("Calculating…".into());
                                app.set_status_left("Calculating…".into());
                            }
                            Err(error) => {
                                state.mark_worker_submission_failure(revision, error.clone());
                                state.pending_cell_commit.set(None);
                                let feedback = format!("Calculation unavailable: {error}");
                                app.set_formula_feedback(SharedString::from(feedback.clone()));
                                app.set_status_left(SharedString::from(feedback));
                            }
                        }
                    } else {
                        apply_sheet(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                        let feedback = match evaluate(&state.current.borrow()).get(&cell) {
                            Some(loom_sheets_core::Value::Error(error)) => {
                                format!("Formula error in {}: #{}", cell.to_a1(), error.code())
                            }
                            _ => format!("Cell {} updated", cell.to_a1()),
                        };
                        app.set_formula_feedback(SharedString::from(feedback.clone()));
                        app.set_status_left(SharedString::from(feedback));
                    }
                }
            }
        }
    });
}

/// Quick-formula (Sum, Average, ...) and Fill Down. Both change the workbook,
/// so both start with the shared admission guard.
pub(crate) fn register_quick_edit_actions(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_quick_formula(move |func| {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                if let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) {
                    let range_str = app.get_selection_range();
                    let formula_text = if range_str.contains(':') {
                        format!("={func}({range_str})")
                    } else {
                        let col = cell
                            .to_a1()
                            .trim_end_matches(|c: char| c.is_ascii_digit())
                            .to_string();
                        format!("={func}({col}1:{col}5)")
                    };
                    let committed = {
                        let mut current = state.current.borrow_mut();
                        let mut undo = state.undo_stack.borrow_mut();
                        let mut redo = state.redo_stack.borrow_mut();
                        commit_formula_edit(&mut current, &mut undo, &mut redo, cell, &formula_text)
                    };
                    if committed {
                        apply_sheet(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_formula_feedback(SharedString::from(format!(
                            "Inserted {func} formula"
                        )));
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_fill_selection(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let changed = {
                    let selection = selection_from_app(&app);
                    let mut current = state.current.borrow_mut();
                    let mut undo = state.undo_stack.borrow_mut();
                    let mut redo = state.redo_stack.borrow_mut();
                    fill_selection_down(&mut current, &mut undo, &mut redo, selection)
                };
                if changed {
                    let source = selection_from_app(&app).range();
                    if let Some(target) = fill_target_range(source) {
                        let expanded = CellRange::new(source.start, target.end);
                        update_selection_range(
                            &app,
                            &state.current.borrow(),
                            &evaluate(&state.current.borrow()),
                            GridSelection::new(source.start, expanded.end),
                        );
                    }
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_formula_feedback(SharedString::from(format!(
                        "Filled {} down",
                        app.get_selection_range()
                    )));
                }
            }
        });
    }
}
