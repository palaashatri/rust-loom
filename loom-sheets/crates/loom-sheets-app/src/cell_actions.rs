//! Cell editing callbacks shared by the GUI bootstrap and callback tests.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::CellRef;
use slint::{ComponentHandle, SharedString};

use crate::{apply_sheet, commit_formula_edit, sync_menu_state, GuiState, SheetsApp};

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
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_formula_feedback(SharedString::from(format!(
                        "Cell {} updated",
                        cell.to_a1()
                    )));
                }
            }
        }
    });
}
