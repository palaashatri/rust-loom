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
    evaluate, CellRange, CellRef, DataValidationRule, FindOptions, GridSelection, RangeEdit, Sheet,
    ValidationCriteria, Value,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::{
    apply_sheet, commit_formula_edit, commit_transaction, commit_workbook_transaction,
    evaluate_current, project_current, selection_from_app, sync_current_to_tabs, sync_menu_state,
    update_selection_range, DataToolsState, FilterValueItem, GuiState, NamedRangeItem,
    SheetTransaction, SheetsApp,
};

pub(crate) fn open_find_replace(app: &SheetsApp, replace_mode: bool) {
    let dt = app.global::<DataToolsState>();
    dt.set_find_replace_mode(replace_mode);
    dt.set_find_open(true);
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
    let enabled = {
        let mut current = state.current.borrow_mut();
        if current.auto_filter.enabled {
            current.auto_filter.enabled = false;
            current.auto_filter.column_filters.clear();
            current.hidden_rows.clear();
            false
        } else {
            current.auto_filter.enabled = true;
            current.auto_filter.header_row = header_row;
            true
        }
    };
    state.mark_content_dirty();
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
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
    {
        let mut current = state.current.borrow_mut();
        current.auto_filter.column_filters.clear();
        current.hidden_rows.clear();
    }
    state.mark_content_dirty();
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left("Cleared all column filters".into());
}

pub(crate) fn open_column_filter(app: &SheetsApp, state: &Rc<GuiState>, col: u32) {
    let values = evaluate_current(state);
    let (header_row, unique, existing) = {
        let mut current = state.current.borrow_mut();
        if !current.auto_filter.enabled {
            current.auto_filter.enabled = true;
            current.auto_filter.header_row = 0;
        }
        let header_row = current.auto_filter.header_row;
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
    dt.set_autofilter_enabled(true);
    dt.set_filter_col_index(col as i32);
    dt.set_filter_col_name(SharedString::from(col_name));
    dt.set_filter_search(SharedString::default());
    dt.set_filter_values(ModelRc::new(VecModel::from(items)));
    dt.set_filter_popup_open(true);
}

pub(crate) fn open_named_ranges(app: &SheetsApp, state: &Rc<GuiState>) {
    let selection = selection_from_app(app);
    let dt = app.global::<DataToolsState>();
    dt.set_new_range_name(SharedString::default());
    dt.set_new_range_ref(SharedString::from(selection.range().to_a1()));
    dt.set_named_range_error(SharedString::default());
    sync_named_ranges_model(app, &state.current.borrow());
    dt.set_named_ranges_open(true);
}

fn sync_named_ranges_model(app: &SheetsApp, sheet: &Sheet) {
    let items: Vec<NamedRangeItem> = sheet
        .named_ranges
        .iter()
        .map(|(name, range)| NamedRangeItem {
            name: SharedString::from(name.as_str()),
            range: SharedString::from(range.to_a1()),
        })
        .collect();
    app.global::<DataToolsState>()
        .set_named_ranges(ModelRc::new(VecModel::from(items)));
}

/// Syncs AutoFilter indicators, Named Ranges list, and active cell Data Validation into `DataToolsState`.
pub(crate) fn sync_data_tools_state(app: &SheetsApp, sheet: &Sheet) {
    let dt = app.global::<DataToolsState>();
    dt.set_autofilter_enabled(sheet.auto_filter.enabled);
    let col_origin = app.get_view_col_origin().max(0) as u32;
    let visible_cols = app.get_cols().row_count() as u32;
    let filtered: Vec<bool> = (0..visible_cols)
        .map(|i| {
            sheet
                .auto_filter
                .column_filters
                .contains_key(&(col_origin + i))
        })
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
                {
                    let mut current = state.current.borrow_mut();
                    current.auto_filter.enabled = true;
                    if all_checked {
                        current.auto_filter.column_filters.remove(&col);
                    } else {
                        current.auto_filter.column_filters.insert(col, allowed);
                    }
                    recompute_hidden_rows(&mut current, &values);
                }
                dt.set_filter_popup_open(false);
                state.mark_content_dirty();
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
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
                {
                    let mut current = state.current.borrow_mut();
                    current.auto_filter.column_filters.remove(&col);
                    recompute_hidden_rows(&mut current, &values);
                }
                dt.set_filter_popup_open(false);
                state.mark_content_dirty();
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
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

    // Named Ranges
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_save_named_range(move |name, range_str| {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let dt = app.global::<DataToolsState>();
                let Some(range) = CellRange::parse(range_str.as_str().trim()) else {
                    dt.set_named_range_error("Invalid A1 range (e.g. A1:B10)".into());
                    return;
                };
                let res = state
                    .current
                    .borrow_mut()
                    .set_named_range(name.as_str(), range);
                match res {
                    Ok(()) => {
                        dt.set_named_range_error(SharedString::default());
                        dt.set_new_range_name(SharedString::default());
                        state.mark_content_dirty();
                        apply_sheet(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                    }
                    Err(err) => {
                        dt.set_named_range_error(SharedString::from(err));
                    }
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_delete_named_range(move |name| {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                if state.current.borrow_mut().remove_named_range(name.as_str()) {
                    state.mark_content_dirty();
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        dt.on_goto_named_range(move |name| {
            if let Some(app) = app_ref.upgrade() {
                let target = state.current.borrow().resolve_named_range(name.as_str());
                if let Some(range) = target {
                    let vals = evaluate_current(&state);
                    let sheet = state.current.borrow();
                    update_selection_range(
                        &app,
                        &sheet,
                        &vals,
                        GridSelection {
                            anchor: range.start,
                            focus: range.end,
                        },
                    );
                    drop(sheet);
                    project_current(&app, &state);
                    app.global::<DataToolsState>().set_named_ranges_open(false);
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        dt.on_close_named_ranges(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<DataToolsState>().set_named_ranges_open(false);
            }
        });
    }

    // Cell Data Validation
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_apply_validation(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let dt = app.global::<DataToolsState>();
                let selection = selection_from_app(&app).range();
                let kind = dt.get_val_kind();
                if kind == 0 {
                    state.current.borrow_mut().clear_validation(selection);
                    dt.set_val_feedback("Validation rule cleared".into());
                    dt.set_val_feedback_error(false);
                    state.mark_content_dirty();
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    return;
                }
                let criteria = match kind {
                    1 => {
                        let items: Vec<String> = dt
                            .get_val_list_or_range()
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        if items.is_empty() {
                            dt.set_val_feedback("Enter comma-separated list items".into());
                            dt.set_val_feedback_error(true);
                            return;
                        }
                        ValidationCriteria::List(items)
                    }
                    2 => {
                        let Some(src) =
                            CellRange::parse(dt.get_val_list_or_range().as_str().trim())
                        else {
                            dt.set_val_feedback("Invalid source range (e.g. E1:E5)".into());
                            dt.set_val_feedback_error(true);
                            return;
                        };
                        ValidationCriteria::RangeList(src)
                    }
                    3 => {
                        let (Ok(min), Ok(max)) = (
                            dt.get_val_min().trim().parse::<i64>(),
                            dt.get_val_max().trim().parse::<i64>(),
                        ) else {
                            dt.set_val_feedback("Enter integer Min and Max".into());
                            dt.set_val_feedback_error(true);
                            return;
                        };
                        ValidationCriteria::WholeNumberBetween(min.min(max), min.max(max))
                    }
                    4 => {
                        let (Ok(min), Ok(max)) = (
                            dt.get_val_min().trim().parse::<f64>(),
                            dt.get_val_max().trim().parse::<f64>(),
                        ) else {
                            dt.set_val_feedback("Enter numeric Min and Max".into());
                            dt.set_val_feedback_error(true);
                            return;
                        };
                        ValidationCriteria::DecimalBetween(min.min(max), min.max(max))
                    }
                    5 => {
                        let (Ok(min), Ok(max)) = (
                            dt.get_val_min().trim().parse::<usize>(),
                            dt.get_val_max().trim().parse::<usize>(),
                        ) else {
                            dt.set_val_feedback("Enter non-negative Min and Max length".into());
                            dt.set_val_feedback_error(true);
                            return;
                        };
                        ValidationCriteria::TextLengthBetween(min.min(max), min.max(max))
                    }
                    _ => return,
                };
                let err_msg = {
                    let msg = dt.get_val_error_message().to_string();
                    if msg.trim().is_empty() {
                        "Invalid value".to_string()
                    } else {
                        msg
                    }
                };
                let mut rule = DataValidationRule::new(criteria, err_msg);
                rule.allow_blank = dt.get_val_allow_blank();
                state.current.borrow_mut().set_validation(selection, rule);
                dt.set_val_feedback(SharedString::from(format!(
                    "Validation applied to {}",
                    selection.to_a1()
                )));
                dt.set_val_feedback_error(false);
                state.mark_content_dirty();
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_clear_validation(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let selection = selection_from_app(&app).range();
                state.current.borrow_mut().clear_validation(selection);
                let dt = app.global::<DataToolsState>();
                dt.set_val_feedback("Validation rule cleared".into());
                dt.set_val_feedback_error(false);
                state.mark_content_dirty();
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
            }
        });
    }
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
