//! Cell editing callbacks shared by the GUI bootstrap and callback tests.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::CellRef;
use slint::{ComponentHandle, SharedString};

use crate::workbook_worker::CellUpdate;
use crate::{
    apply_sheet, commit_formula_edit, evaluate, project_current, sync_menu_state, GuiState,
    SheetsApp,
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
            if let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) {
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
                                state.pending_cell_commit.set(Some((revision, cell)));
                                app.set_formula_feedback("Calculating…".into());
                                app.set_status_left("Calculating…".into());
                            }
                            Err(error) => {
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
