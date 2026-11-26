//! Data Analysis Tools UI/state bindings for Loom Sheets:
//! 1. Find & Replace bar/modal
//! 2. Column AutoFilter toggle & filter-by-value modal
//! 3. Named Ranges manager modal & resolution
//! 4. Cell Data Validation rules & inspector feedback

use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::data_tools::{
    collect_sheet_replacements, find_matches_in_workbook, find_next_match, recompute_hidden_rows,
    replace_cell_raw, unique_column_values,
};
use loom_sheets_core::{
    evaluate, CellRef, FindOptions, GridSelection, RangeEdit, Sheet, ValidationCriteria, Value,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::{
    apply_sheet, commit_formula_edit, commit_transaction, commit_workbook_transaction,
    evaluate_current, project_current, selection_from_app, sync_current_to_tabs, sync_menu_state,
    update_selection_range, DataToolsState, FilterValueItem, GuiState, SheetTransaction, SheetsApp,
};

mod named_ranges;
mod validation;

pub(crate) use named_ranges::open_named_ranges;
use named_ranges::sync_named_ranges_model;

pub(super) fn commit_sheet_metadata_edit(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
    mutate: impl FnOnce(&mut Sheet) -> bool,
) -> bool {
    let before = state.current.borrow().clone();
    let mut after = before.clone();
    if !mutate(&mut after) {
        return false;
    }
    commit_transaction(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        SheetTransaction::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        },
    );
    state.mark_content_dirty();
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
    true
}

pub(crate) fn open_find_replace(app: &SheetsApp, replace_mode: bool) {
    if app.global::<crate::PrintPreviewState>().get_open() {
        return;
    }
    let dt = app.global::<DataToolsState>();
    dt.set_find_replace_mode(replace_mode);
    dt.set_find_open(true);
    let next = dt.get_find_focus_tick().wrapping_add(1).max(1);
    dt.set_find_focus_tick(next);
    dt.set_find_status(SharedString::default());
}

pub(crate) fn toggle_auto_filter(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    if crate::mutation_guard::refused(app, state) {
        return;
    }
    let selection = selection_from_app(app);
    let header_row = selection.range().start.row;
    let mut enabled = false;
    commit_sheet_metadata_edit(app, state, menu_service, |current| {
        if current.auto_filter.enabled {
            current.auto_filter.enabled = false;
            current.auto_filter.column_filters.clear();
            current.hidden_rows.clear();
            enabled = false;
        } else {
            current.auto_filter.enabled = true;
            current.auto_filter.header_row = header_row;
            enabled = true;
        }
        true
    });
    let msg = if enabled {
        format!("AutoFilter enabled on row {}", header_row + 1)
    } else {
        "AutoFilter turned off".to_string()
    };
    app.set_status_left(SharedString::from(msg));
}

pub(crate) fn clear_all_filters(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    if crate::mutation_guard::refused(app, state) {
        return;
    }
    commit_sheet_metadata_edit(app, state, menu_service, |current| {
        current.auto_filter.column_filters.clear();
        current.hidden_rows.clear();
        true
    });
    app.set_status_left("Cleared all column filters".into());
}

pub(crate) fn open_column_filter(app: &SheetsApp, state: &Rc<GuiState>, col: u32) {
    let values = evaluate_current(state);
    let (header_row, unique, existing) = {
        let current = state.current.borrow();
        let header_row = if current.auto_filter.enabled {
            current.auto_filter.header_row
        } else {
            0
        };
        let unique = unique_column_values(&current, &values, header_row, col);
        let existing = current.auto_filter.column_filters.get(&col).cloned();
        (header_row, unique, existing)
    };
    let _ = header_row;
    let items: Vec<FilterValueItem> = unique
        .into_iter()
        .map(|value| {
            let checked = existing
                .as_ref()
                .map(|set| set.contains(&value))
                .unwrap_or(true);
            FilterValueItem {
                value: SharedString::from(value),
                checked,
            }
        })
        .collect();
    let col_name = CellRef { row: 0, col }
        .to_a1()
        .trim_end_matches('1')
        .to_string();
    let dt = app.global::<DataToolsState>();
    dt.set_filter_col_index(col as i32);
    dt.set_filter_col_name(SharedString::from(col_name));
    dt.set_filter_search(SharedString::default());
    dt.set_filter_values(ModelRc::new(VecModel::from(items)));
    dt.set_filter_popup_open(true);
}

/// Syncs AutoFilter indicators, Named Ranges list, and active cell Data Validation into `DataToolsState`.
pub(crate) fn sync_data_tools_state(app: &SheetsApp, sheet: &Sheet) {
    let dt = app.global::<DataToolsState>();
    dt.set_autofilter_enabled(sheet.auto_filter.enabled);
    let filtered: Vec<bool> = app
        .get_cols()
        .iter()
        .map(|col| col >= 0 && sheet.auto_filter.column_filters.contains_key(&(col as u32)))
        .collect();
    dt.set_col_filtered(ModelRc::new(VecModel::from(filtered)));
    sync_named_ranges_model(app, sheet);

    if let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) {
        if let Some(v) = sheet.validation_for_cell(cell) {
            match &v.rule.criteria {
                ValidationCriteria::List(items) => {
                    dt.set_val_kind(1);
                    dt.set_val_list_or_range(SharedString::from(items.join(", ")));
                    dt.set_val_min(SharedString::default());
                    dt.set_val_max(SharedString::default());
                }
                ValidationCriteria::RangeList(range) => {
                    dt.set_val_kind(2);
                    dt.set_val_list_or_range(SharedString::from(range.to_a1()));
                    dt.set_val_min(SharedString::default());
                    dt.set_val_max(SharedString::default());
                }
                ValidationCriteria::WholeNumberBetween(min, max) => {
                    dt.set_val_kind(3);
                    dt.set_val_list_or_range(SharedString::default());
                    dt.set_val_min(SharedString::from(min.to_string()));
                    dt.set_val_max(SharedString::from(max.to_string()));
                }
                ValidationCriteria::DecimalBetween(min, max) => {
                    dt.set_val_kind(4);
                    dt.set_val_list_or_range(SharedString::default());
                    dt.set_val_min(SharedString::from(min.to_string()));
                    dt.set_val_max(SharedString::from(max.to_string()));
                }
                ValidationCriteria::TextLengthBetween(min, max) => {
                    dt.set_val_kind(5);
                    dt.set_val_list_or_range(SharedString::default());
                    dt.set_val_min(SharedString::from(min.to_string()));
                    dt.set_val_max(SharedString::from(max.to_string()));
                }
            }
            dt.set_val_allow_blank(v.rule.allow_blank);
            dt.set_val_error_message(SharedString::from(v.rule.error_message.as_str()));
        } else {
            dt.set_val_kind(0);
            dt.set_val_list_or_range(SharedString::default());
            dt.set_val_min(SharedString::default());
            dt.set_val_max(SharedString::default());
            dt.set_val_allow_blank(true);
            dt.set_val_error_message("Invalid value".into());
        }
    }
}

fn read_find_options(dt: &DataToolsState) -> FindOptions {
    FindOptions {
        query: dt.get_find_query().to_string(),
        match_case: dt.get_find_match_case(),
        whole_cell: dt.get_find_whole_cell(),
        workbook_scope: dt.get_find_workbook_scope(),
    }
}

fn find_step(app: &SheetsApp, state: &Rc<GuiState>, forward: bool) {
    let dt = app.global::<DataToolsState>();
    let opts = read_find_options(&dt);
    if opts.query.is_empty() {
        dt.set_find_status("Enter search text".into());
        return;
    }
    sync_current_to_tabs(state);
    let sheets = state.sheets.borrow().clone();
    let active_sheet = *state.active_sheet_index.borrow();
    let values_by_sheet: Vec<HashMap<CellRef, Value>> = if opts.workbook_scope {
        loom_sheets_core::workbook::evaluate_workbook(&sheets)
    } else {
        let mut v = vec![HashMap::new(); sheets.len()];
        v[active_sheet] = (*evaluate_current(state)).clone();
        v
    };
    let matches = find_matches_in_workbook(&sheets, &values_by_sheet, active_sheet, &opts);
    if matches.is_empty() {
        dt.set_find_status("No matches".into());
        return;
    }
    let current_cell =
        CellRef::parse(app.get_selected_cell().as_str()).unwrap_or(CellRef { row: 0, col: 0 });
    let Some((idx, m)) = find_next_match(&matches, active_sheet, current_cell, forward) else {
        return;
    };
    let target_sheet = m.sheet_index;
    let target_cell = m.cell;
    let total = matches.len();
    drop(sheets);
    if target_sheet != active_sheet {
        app.invoke_select_sheet(target_sheet as i32);
    }
    let sheet_ref = state.current.borrow();
    let vals = evaluate(&sheet_ref);
    update_selection_range(
        app,
        &sheet_ref,
        &vals,
        GridSelection::new(target_cell, target_cell),
    );
    drop(sheet_ref);
    project_current(app, state);
    dt.set_find_status(SharedString::from(format!(
        "{} of {} ({})",
        idx + 1,
        total,
        target_cell.to_a1()
    )));
}

fn replace_current_match(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    if crate::mutation_guard::refused(app, state) {
        return;
    }
    let dt = app.global::<DataToolsState>();
    let opts = read_find_options(&dt);
    if opts.query.is_empty() {
        dt.set_find_status("Enter search text".into());
        return;
    }
    let replacement = dt.get_replace_text().to_string();
    let selected =
        CellRef::parse(app.get_selected_cell().as_str()).unwrap_or(CellRef { row: 0, col: 0 });
    let maybe_new = {
        let current = state.current.borrow();
        current.raw(selected).and_then(|raw| {
            replace_cell_raw(
                raw,
                &opts.query,
                &replacement,
                opts.match_case,
                opts.whole_cell,
            )
        })
    };
    if let Some(new_raw) = maybe_new {
        let committed = {
            let mut current = state.current.borrow_mut();
            let mut undo = state.undo_stack.borrow_mut();
            let mut redo = state.redo_stack.borrow_mut();
            commit_formula_edit(&mut current, &mut undo, &mut redo, selected, &new_raw)
        };
        if committed {
            state.mark_content_dirty();
            apply_sheet(app, state);
            sync_menu_state(menu_service, app, state);
            dt.set_find_status(SharedString::from(format!("Replaced {}", selected.to_a1())));
        }
    }
    find_step(app, state, true);
}

fn replace_all_matches(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    if crate::mutation_guard::refused(app, state) {
        return;
    }
    let dt = app.global::<DataToolsState>();
    let opts = read_find_options(&dt);
    if opts.query.is_empty() {
        dt.set_find_status("Enter search text".into());
        return;
    }
    let replacement = dt.get_replace_text().to_string();
    if !opts.workbook_scope {
        let changes = {
            let current = state.current.borrow();
            collect_sheet_replacements(
                &current,
                &opts.query,
                &replacement,
                opts.match_case,
                opts.whole_cell,
            )
        };
        if changes.is_empty() {
            dt.set_find_status("No matches to replace".into());
            return;
        }
        let count = changes.len();
        let committed = {
            let mut current = state.current.borrow_mut();
            let edits: Vec<RangeEdit> = changes
                .into_iter()
                .map(|(cell, _, new_raw)| RangeEdit::replace(&current, cell, Some(new_raw)))
                .collect();
            let mut undo = state.undo_stack.borrow_mut();
            let mut redo = state.redo_stack.borrow_mut();
            commit_transaction(
                &mut current,
                &mut undo,
                &mut redo,
                SheetTransaction::Batch(edits),
            )
        };
        if committed {
            state.mark_content_dirty();
            apply_sheet(app, state);
            sync_menu_state(menu_service, app, state);
            dt.set_find_status(SharedString::from(format!("Replaced {count} cell(s)")));
        }
    } else {
        sync_current_to_tabs(state);
        let mut after_sheets = state.sheets.borrow().clone();
        let active = *state.active_sheet_index.borrow();
        let mut count = 0usize;
        for sheet in &mut after_sheets {
            let changes = collect_sheet_replacements(
                sheet,
                &opts.query,
                &replacement,
                opts.match_case,
                opts.whole_cell,
            );
            count += changes.len();
            for (cell, _, new_raw) in changes {
                sheet.set_raw(cell, new_raw);
            }
        }
        if count == 0 {
            dt.set_find_status("No matches to replace".into());
            return;
        }
        commit_workbook_transaction(state, after_sheets, active, None);
        state.mark_content_dirty();
        apply_sheet(app, state);
        sync_menu_state(menu_service, app, state);
        dt.set_find_status(SharedString::from(format!(
            "Replaced {count} cell(s) across workbook"
        )));
    }
}

/// Registers all Data Analysis Tools callbacks on `DataToolsState`.
pub(crate) fn register_data_tools_actions(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    let dt = app.global::<DataToolsState>();

    // Find & Replace
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        dt.on_find_next(move || {
            if let Some(app) = app_ref.upgrade() {
                find_step(&app, &state, true);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        dt.on_find_prev(move || {
            if let Some(app) = app_ref.upgrade() {
                find_step(&app, &state, false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_replace_current(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                replace_current_match(&app, &state, &menu_service);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_replace_all(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                replace_all_matches(&app, &state, &menu_service);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        dt.on_close_find(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<DataToolsState>().set_find_open(false);
            }
        });
    }

    // AutoFilter
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        dt.on_open_column_filter(move |col| {
            if let Some(app) = app_ref.upgrade() {
                if col >= 0 {
                    open_column_filter(&app, &state, col as u32);
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        dt.on_toggle_filter_value(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let dt = app.global::<DataToolsState>();
                let model = dt.get_filter_values();
                if let Some(mut item) = model.row_data(idx as usize) {
                    item.checked = !item.checked;
                    model.set_row_data(idx as usize, item);
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        dt.on_select_all_filter_values(move |checked| {
            if let Some(app) = app_ref.upgrade() {
                let dt = app.global::<DataToolsState>();
                let model = dt.get_filter_values();
                for i in 0..model.row_count() {
                    if let Some(mut item) = model.row_data(i) {
                        item.checked = checked;
                        model.set_row_data(i, item);
                    }
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_apply_column_filter(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let dt = app.global::<DataToolsState>();
                let col = dt.get_filter_col_index().max(0) as u32;
                let model = dt.get_filter_values();
                let mut allowed = BTreeSet::new();
                let mut all_checked = true;
                for i in 0..model.row_count() {
                    if let Some(item) = model.row_data(i) {
                        if item.checked {
                            allowed.insert(item.value.to_string());
                        } else {
                            all_checked = false;
                        }
                    }
                }
                let values = evaluate_current(&state);
                commit_sheet_metadata_edit(&app, &state, &menu_service, |current| {
                    if !current.auto_filter.enabled {
                        current.auto_filter.header_row = 0;
                        current.auto_filter.enabled = true;
                    }
                    if all_checked {
                        current.auto_filter.column_filters.remove(&col);
                    } else {
                        current.auto_filter.column_filters.insert(col, allowed);
                    }
                    recompute_hidden_rows(current, &values);
                    true
                });
                dt.set_filter_popup_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_clear_column_filter(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let dt = app.global::<DataToolsState>();
                let col = dt.get_filter_col_index().max(0) as u32;
                let values = evaluate_current(&state);
                commit_sheet_metadata_edit(&app, &state, &menu_service, |current| {
                    current.auto_filter.column_filters.remove(&col);
                    recompute_hidden_rows(current, &values);
                    true
                });
                dt.set_filter_popup_open(false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        dt.on_close_column_filter(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<DataToolsState>().set_filter_popup_open(false);
            }
        });
    }

    dt.on_filter_matches(|value, query| {
        query.is_empty() || value.to_lowercase().contains(&query.to_lowercase())
    });
    named_ranges::register_callbacks(app, state, menu_service);
    validation::register_callbacks(app, state, menu_service);
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_toggle_auto_filter(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                toggle_auto_filter(&app, &state, &menu_service);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        dt.on_open_filter_column(move || {
            if let Some(app) = app_ref.upgrade() {
                let col = selection_from_app(&app).focus.col;
                open_column_filter(&app, &state, col);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_clear_all_filters(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                clear_all_filters(&app, &state, &menu_service);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        dt.on_open_named_ranges(move || {
            if let Some(app) = app_ref.upgrade() {
                open_named_ranges(&app, &state);
            }
        });
    }
}

#[cfg(test)]
#[path = "data_tools_tests.rs"]
mod tests;
