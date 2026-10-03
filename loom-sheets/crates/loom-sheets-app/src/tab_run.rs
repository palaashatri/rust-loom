//! Excel's Tab/Enter rule: after typing across a row with Tab, Enter returns
//! to the column where the Tab run started, one row down.
//!
//! The grid and the formula bar report Tab and Enter as plain selection
//! moves, so a run is recognised from the commit that precedes the move.

use std::cell::Cell;

use loom_sheets_core::CellRef;

/// Where a navigation move after a commit should land.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TabRunMove {
    /// Move by the requested delta as usual.
    Plain,
    /// Enter ended a Tab run: select this cell instead.
    Return(CellRef),
}

#[derive(Default)]
pub(crate) struct TabRun {
    /// Cell whose edit was just committed; the next move may be Tab/Enter.
    committed: Cell<Option<CellRef>>,
    /// Column where the live run began.
    start_column: Cell<Option<u32>>,
    /// Cell where the run left the selection; any other selection ends it.
    expected: Cell<Option<CellRef>>,
}

impl TabRun {
    pub(crate) fn note_commit(&self, cell: CellRef) {
        self.committed.set(Some(cell));
    }

    /// Escape or any other non-navigation action ends the run.
    pub(crate) fn reset(&self) {
        self.committed.set(None);
        self.start_column.set(None);
        self.expected.set(None);
    }

    /// Decide what a key move from `focus` means.
    pub(crate) fn before_move(&self, focus: CellRef, delta: (i32, i32)) -> TabRunMove {
        let committed = self.committed.take();
        let live_start = if self.expected.get() == Some(focus) {
            self.start_column.get()
        } else {
            None
        };
        self.start_column.set(None);
        self.expected.set(None);
        if committed != Some(focus) {
            return TabRunMove::Plain;
        }
        match delta {
            (0, 1) | (0, -1) => {
                self.start_column.set(Some(live_start.unwrap_or(focus.col)));
                TabRunMove::Plain
            }
            (1, 0) => match live_start {
                Some(col) => TabRunMove::Return(CellRef {
                    row: focus.row.saturating_add(1),
                    col,
                }),
                None => TabRunMove::Plain,
            },
            _ => TabRunMove::Plain,
        }
    }

    /// Record where a sideways move ended so the run stays live.
    pub(crate) fn after_move(&self, landed: CellRef) {
        if self.start_column.get().is_some() {
            self.expected.set(Some(landed));
        }
    }
}

/// Wire the selection-move callback shared by arrow keys, Tab and Enter.
pub(crate) fn register_navigation(app: &crate::SheetsApp, state: &std::rc::Rc<crate::GuiState>) {
    use crate::{navigate_selection, project_current, select_cell, selection_from_app};
    use slint::ComponentHandle;
    let state = state.clone();
    let app_ref = app.as_weak();
    app.on_navigate_selection(move |row_delta, col_delta| {
        let Some(app) = app_ref.upgrade() else { return };
        let focus = selection_from_app(&app).focus;
        match state.tab_run.before_move(focus, (row_delta, col_delta)) {
            TabRunMove::Return(cell) => select_cell(
                &app,
                &state.current.borrow(),
                cell.row as i32,
                cell.col as i32,
            ),
            TabRunMove::Plain => {
                navigate_selection(&app, &state.current.borrow(), row_delta, col_delta);
                state.tab_run.after_move(selection_from_app(&app).focus);
            }
        }
        project_current(&app, &state);
    });
}
