#![allow(dead_code)]
use std::cell::RefCell;
use loom_sheets_core::{Sheet,CellRef,CellRange,CellAlignment,CellStyle,RangeEdit};
struct GuiState {
 current:RefCell<Sheet>, sheets:RefCell<Vec<Sheet>>, active_sheet_index:RefCell<usize>,
 undo_stack:RefCell<Vec<SheetTransaction>>, redo_stack:RefCell<Vec<SheetTransaction>>,
 sheet_histories:RefCell<Vec<(Vec<SheetTransaction>,Vec<SheetTransaction>)>>,
}
#[derive(Debug, Clone)]
pub(crate) enum SheetTransaction {
    Range(RangeEdit),
    Batch(Vec<RangeEdit>),
    Alignment {
        range: CellRange,
        before: Vec<(CellRef, CellAlignment)>,
        after: CellAlignment,
    },
    Style {
        before: Vec<(CellRef, CellStyle)>,
        after: Vec<(CellRef, CellStyle)>,
    },
    Snapshot {
        before: Box<Sheet>,
        after: Box<Sheet>,
    },
    /// Workbook-level tab operation (add/delete/rename sheet). Boxed to keep
    /// the enum size bounded; applied only via `restore_workbook_state`, never
    /// through the single-sheet `apply`/`revert` below.
    Workbook {
        before: Box<WorkbookUndoState>,
        after: Box<WorkbookUndoState>,
    },
}

/// Full tab-strip state captured for workbook-level undo/redo.
#[derive(Debug, Clone)]
pub(crate) struct WorkbookUndoState {
    sheets: Vec<Sheet>,
    active: usize,
    histories: Vec<(Vec<SheetTransaction>, Vec<SheetTransaction>)>,
}

impl WorkbookUndoState {
    fn capture(state: &GuiState) -> Self {
        Self {
            sheets: state.sheets.borrow().clone(),
            active: *state.active_sheet_index.borrow(),
            histories: state.sheet_histories.borrow().clone(),
        }
    }

    fn restore(&self, state: &GuiState) {
        *state.sheets.borrow_mut() = self.sheets.clone();
        *state.active_sheet_index.borrow_mut() = self.active;
        let active = self.active.min(self.sheets.len().saturating_sub(1));
        *state.current.borrow_mut() = self.sheets[active].clone();
        *state.sheet_histories.borrow_mut() = self.histories.clone();
        let (undo, redo) = self.histories.get(active).cloned().unwrap_or_default();
        *state.undo_stack.borrow_mut() = undo;
        *state.redo_stack.borrow_mut() = redo;
    }
}

pub(crate) const MAX_HISTORY_ENTRIES: usize = 200;

/// Push a transaction, evicting the oldest entry past the bound.
pub(crate) fn push_history(stack: &mut Vec<SheetTransaction>, tx: SheetTransaction) {
    stack.push(tx);
    if stack.len() > MAX_HISTORY_ENTRIES {
        stack.remove(0);
    }
}


pub(crate) fn commit_workbook_transaction(
    state: &GuiState,
    after_sheets: Vec<Sheet>,
    after_active: usize,
    drop_history: Option<usize>,
) {
    sync_current_to_tabs(state);
    if after_sheets.is_empty() {
        return;
    }
    let old_active = *state.active_sheet_index.borrow();

    // Stash the outgoing tab's live stacks, mirroring sheet switching.
    let live_undo = std::mem::take(&mut *state.undo_stack.borrow_mut());
    let live_redo = std::mem::take(&mut *state.redo_stack.borrow_mut());
    {
        let mut histories = state.sheet_histories.borrow_mut();
        if histories.len() <= old_active {
            histories.resize_with(old_active + 1, || (Vec::new(), Vec::new()));
        }
        histories[old_active] = (live_undo, live_redo);
    }
    // Workbook snapshots keep per-tab histories as data, rather than taking
    // a recursive copy of the live workbook undo/redo stacks. This keeps tab
    // operations bounded even after long editing sessions.
    let before = WorkbookUndoState::capture(state);

    let after_active = after_active.min(after_sheets.len().saturating_sub(1));
    *state.sheets.borrow_mut() = after_sheets;
    *state.active_sheet_index.borrow_mut() = after_active;
    *state.current.borrow_mut() = state.sheets.borrow()[after_active].clone();
    {
        let mut histories = state.sheet_histories.borrow_mut();
        let tabs = state.sheets.borrow().len();
        if let Some(dropped) = drop_history {
            if dropped < histories.len() {
                histories.remove(dropped);
            }
        }
        if histories.len() < tabs {
            histories.resize_with(tabs, || (Vec::new(), Vec::new()));
        }
        if !histories.is_empty() {
            let landed = after_active.min(histories.len() - 1);
            let (landed_undo, landed_redo) = histories[landed].clone();
            *state.undo_stack.borrow_mut() = landed_undo;
            *state.redo_stack.borrow_mut() = landed_redo;
        }
    }

    let after = WorkbookUndoState::capture(state);
    push_history(
        &mut state.undo_stack.borrow_mut(),
        SheetTransaction::Workbook {
            before: Box::new(before),
            after: Box::new(after),
        },
    );
    state.redo_stack.borrow_mut().clear();
}


fn sync_current_to_tabs(state: &GuiState) {
    let current = state.current.borrow().clone();
    let active = *state.active_sheet_index.borrow();
    let mut sheets = state.sheets.borrow_mut();
    if active >= sheets.len() {
        sheets.resize_with(active + 1, || Sheet::new("Untitled"));
    }
    sheets[active] = current;
}


fn count(stack:&[SheetTransaction])->usize {stack.iter().map(|tx|match tx {SheetTransaction::Workbook{before,after}=>1+before.histories.iter().map(|(a,b)|count(a)+count(b)).sum::<usize>()+after.histories.iter().map(|(a,b)|count(a)+count(b)).sum::<usize>(),_=>1}).sum()}
fn main(){let sheet=Sheet::new("Before");let state=GuiState {current:RefCell::new(sheet.clone()),sheets:RefCell::new(vec![sheet]),active_sheet_index:RefCell::new(0),undo_stack:RefCell::new(vec![]),redo_stack:RefCell::new(vec![]),sheet_histories:RefCell::new(vec![(vec![],vec![])])};
for i in 1..=9 {let mut sheets=state.sheets.borrow().clone();sheets[0].name=format!("Rename{i}"); let t=std::time::Instant::now();commit_workbook_transaction(&state,sheets,0,None);println!("renames={i} top-level={} nested_transactions={} elapsed_ms={}",state.undo_stack.borrow().len(),count(&state.undo_stack.borrow()),t.elapsed().as_millis());}}
