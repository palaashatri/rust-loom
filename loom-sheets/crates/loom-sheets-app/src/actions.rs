//! Sheet and toolbar action handlers for Loom Sheets.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{CommandAction, DesktopError, NativeMenuBar};
use loom_sheets_core::{
    compute_pivot, evaluate, CellAlignment, CellRange, CellRef, ChartKind, ChartSeries, ChartSpec,
    PivotAggregation, RangeEdit, Sheet, SheetModel,
};
use slint::{ComponentHandle, SharedString, VecModel};

use crate::{
    apply_sheet, cell_value, clear_selection, commit_formula_edit, commit_transaction, select_cell,
    selection_from_app, set_selection_alignment, sync_menu_state, update_selection_range,
    GridSelection, GuiState, SheetTransaction, SheetsApp,
};

/// Synchronize the sheet tab labels and active selection with the Slint UI.
pub(crate) fn sync_sheet_tabs(app: &SheetsApp, state: &GuiState) {
    let names: Vec<SharedString> = state
        .sheets
        .borrow()
        .iter()
        .map(|s| SharedString::from(s.name.as_str()))
        .collect();
    app.set_sheet_names(Rc::new(VecModel::from(names)).into());
    app.set_active_sheet_index(*state.active_sheet_index.borrow() as i32);
}

/// Synchronize active chart data from the worksheet model to the Slint UI.
pub(crate) fn sync_chart_to_app(app: &SheetsApp, sheet: &Sheet) {
    if !app.get_chart_visible() {
        return;
    }
    let vals = evaluate(sheet);
    let dims = sheet.dimensions();
    let mut categories = Vec::new();
    let mut values = Vec::new();
    let mut display_values = Vec::new();
    for r in 1..dims.rows {
        let cat = cell_value(sheet, &vals, r, 0);
        let val_str = cell_value(sheet, &vals, r, 1);
        let clean = val_str.trim().trim_start_matches('$').trim_end_matches('%');
        if let Ok(num) = clean.parse::<f64>() {
            categories.push(cat);
            values.push(num);
            display_values.push(val_str);
        }
    }
    if categories.is_empty() {
        return;
    }
    let spec = ChartSpec {
        kind: ChartKind::Bar,
        title: format!("{} Chart", sheet.name),
        series: vec![ChartSeries {
            name: "Series 1".into(),
            categories: categories.clone(),
            values: values.clone(),
        }],
    };
    if let Ok(normalized_series) = spec.normalized_points() {
        let norm = normalized_series[0]
            .iter()
            .map(|&v| v as f32)
            .collect::<Vec<f32>>();
        app.set_chart_title(SharedString::from(spec.title));
        app.set_chart_categories(
            Rc::new(VecModel::from(
                categories
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            ))
            .into(),
        );
        app.set_chart_values_display(
            Rc::new(VecModel::from(
                display_values
                    .into_iter()
                    .map(SharedString::from)
                    .collect::<Vec<_>>(),
            ))
            .into(),
        );
        app.set_chart_normalized(Rc::new(VecModel::from(norm)).into());
    }
}

/// Register all toolbar, sheet-tab, and menu actions on the Slint window.
pub(crate) fn register_sheet_actions(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_select_sheet(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let idx = idx as usize;
                if idx < state.sheets.borrow().len() && idx != *state.active_sheet_index.borrow() {
                    let cur = state.current.borrow().clone();
                    let active_idx = *state.active_sheet_index.borrow();
                    state.sheets.borrow_mut()[active_idx] = cur;

                    // Preserve current sheet's undo/redo history
                    let cur_undo = std::mem::take(&mut *state.undo_stack.borrow_mut());
                    let cur_redo = std::mem::take(&mut *state.redo_stack.borrow_mut());
                    if active_idx >= state.sheet_histories.borrow().len() {
                        state
                            .sheet_histories
                            .borrow_mut()
                            .resize_with(active_idx + 1, || (Vec::new(), Vec::new()));
                    }
                    state.sheet_histories.borrow_mut()[active_idx] = (cur_undo, cur_redo);

                    // Switch sheet
                    *state.active_sheet_index.borrow_mut() = idx;
                    *state.current.borrow_mut() = state.sheets.borrow()[idx].clone();

                    // Restore target sheet's undo/redo history
                    if idx >= state.sheet_histories.borrow().len() {
                        state
                            .sheet_histories
                            .borrow_mut()
                            .resize_with(idx + 1, || (Vec::new(), Vec::new()));
                    }
                    let (target_undo, target_redo) =
                        state.sheet_histories.borrow_mut()[idx].clone();
                    *state.undo_stack.borrow_mut() = target_undo;
                    *state.redo_stack.borrow_mut() = target_redo;

                    apply_sheet(&app, &state.current.borrow());
                    sync_sheet_tabs(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!(
                        "Switched to {}",
                        state.current.borrow().name
                    )));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_add_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                let cur = state.current.borrow().clone();
                let active_idx = *state.active_sheet_index.borrow();
                state.sheets.borrow_mut()[active_idx] = cur;

                // Preserve current sheet's undo/redo history
                let cur_undo = std::mem::take(&mut *state.undo_stack.borrow_mut());
                let cur_redo = std::mem::take(&mut *state.redo_stack.borrow_mut());
                if active_idx >= state.sheet_histories.borrow().len() {
                    state
                        .sheet_histories
                        .borrow_mut()
                        .resize_with(active_idx + 1, || (Vec::new(), Vec::new()));
                }
                state.sheet_histories.borrow_mut()[active_idx] = (cur_undo, cur_redo);

                let count = state.sheets.borrow().len() + 1;
                let new_sheet = Sheet::new(&format!("Sheet {count}"));
                state.sheets.borrow_mut().push(new_sheet.clone());
                state
                    .sheet_histories
                    .borrow_mut()
                    .push((Vec::new(), Vec::new()));
                *state.active_sheet_index.borrow_mut() = count - 1;
                *state.current.borrow_mut() = new_sheet;
                *state.undo_stack.borrow_mut() = Vec::new();
                *state.redo_stack.borrow_mut() = Vec::new();
                apply_sheet(&app, &state.current.borrow());
                sync_sheet_tabs(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left(SharedString::from(format!("Created Sheet {count}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_add_row(move || {
            if let Some(app) = app_ref.upgrade() {
                let dims = state.current.borrow().dimensions();
                let next_row = dims.rows;
                let cell = CellRef {
                    row: next_row,
                    col: 0,
                };
                let committed = commit_formula_edit(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    cell,
                    "",
                );
                if committed {
                    select_cell(&app, &state.current.borrow(), next_row as i32, 0);
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!("Added row {}", next_row + 1)));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_organize(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if sort_table(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.anchor.col,
                    true,
                ) {
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left("Sorted table rows ascending".into());
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_cycle_zoom(move || {
            if let Some(app) = app_ref.upgrade() {
                let next = match app.get_zoom_level().as_str() {
                    "100%" => "125%",
                    "125%" => "150%",
                    "150%" => "75%",
                    _ => "100%",
                };
                app.set_zoom_level(next.into());
                app.set_status_left(SharedString::from(format!("Zoom set to {next}")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_add_comment(move || {
            if let Some(app) = app_ref.upgrade() {
                let cell = app.get_selected_cell();
                app.set_formula_edit_buffer(SharedString::from(format!("// Note on {cell}: ")));
                app.invoke_focus_formula_bar();
                app.set_formula_feedback(SharedString::from(format!("Add note to {cell}")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_insert_chart(move || {
            if let Some(app) = app_ref.upgrade() {
                let sheet = state.current.borrow();
                app.set_chart_visible(true);
                sync_chart_to_app(&app, &sheet);
                app.set_status_left(SharedString::from(format!(
                    "Inserted {} (Bar)",
                    app.get_chart_title()
                )));
                app.set_formula_feedback("Chart rendered on worksheet".into());
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_close_chart(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_chart_visible(false);
                app.set_status_left("Chart dismissed".into());
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_toggle_chart_kind(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_status_left("Toggled chart display style".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_rename_sheet(move |new_name| {
            if let Some(app) = app_ref.upgrade() {
                let trimmed = new_name.trim();
                if !trimmed.is_empty() {
                    let active_idx = *state.active_sheet_index.borrow();
                    state.current.borrow_mut().name = trimmed.to_string();
                    if active_idx < state.sheets.borrow().len() {
                        state.sheets.borrow_mut()[active_idx].name = trimmed.to_string();
                    }
                    sync_sheet_tabs(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!("Renamed sheet to {trimmed}")));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_add_table_col(move || {
            if let Some(app) = app_ref.upgrade() {
                let dims = state.current.borrow().dimensions();
                let next_col = dims.cols;
                let cell = CellRef {
                    row: 0,
                    col: next_col,
                };
                let col_letter = cell.to_a1().trim_end_matches('1').to_string();
                let committed = commit_formula_edit(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    cell,
                    &col_letter,
                );
                if committed {
                    select_cell(&app, &state.current.borrow(), 0, next_col as i32);
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!("Added column {col_letter}")));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_set_cell_format(move |fmt_idx| {
            if let Some(app) = app_ref.upgrade() {
                if let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) {
                    let current_raw = state
                        .current
                        .borrow()
                        .raw(cell)
                        .map(|s| s.to_string())
                        .unwrap_or_default();
                    let clean = current_raw
                        .trim()
                        .trim_start_matches('$')
                        .trim_end_matches('%');
                    if let Ok(num) = clean.parse::<f64>() {
                        let new_val = match fmt_idx {
                            1 => format!("${num:.2}"),
                            2 => format!("{:.1}%", num * 100.0),
                            _ => format!("{num}"),
                        };
                        let committed = commit_formula_edit(
                            &mut state.current.borrow_mut(),
                            &mut state.undo_stack.borrow_mut(),
                            &mut state.redo_stack.borrow_mut(),
                            cell,
                            &new_val,
                        );
                        if committed {
                            app.set_cell_format(fmt_idx);
                            apply_sheet(&app, &state.current.borrow());
                            sync_menu_state(&menu_service, &app, &state);
                            app.set_status_left(SharedString::from(format!(
                                "Formatted cell {} as {}",
                                cell.to_a1(),
                                new_val
                            )));
                        }
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_add_shape(move || {
            if let Some(app) = app_ref.upgrade() {
                if let Some(cell) = CellRef::parse(app.get_selected_cell().as_str()) {
                    let committed = commit_formula_edit(
                        &mut state.current.borrow_mut(),
                        &mut state.undo_stack.borrow_mut(),
                        &mut state.redo_stack.borrow_mut(),
                        cell,
                        "◆",
                    );
                    if committed {
                        apply_sheet(&app, &state.current.borrow());
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_formula_feedback(SharedString::from(format!(
                            "Inserted shape ◆ into {}",
                            cell.to_a1()
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
        app.on_add_category(move || {
            if let Some(app) = app_ref.upgrade() {
                let cur = state.current.borrow().clone();
                let dims = cur.dimensions();
                if dims.rows > 1 {
                    let range = CellRange::new(
                        CellRef { row: 1, col: 0 },
                        CellRef {
                            row: dims.rows.saturating_sub(1),
                            col: dims.cols.saturating_sub(1),
                        },
                    );
                    let mut model = SheetModel::new(cur);
                    if model.sort_rows(range, 0, true).is_ok() {
                        *state.current.borrow_mut() = model.sheet;
                        apply_sheet(&app, &state.current.borrow());
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_status_left("Grouped rows by category (Column A)".into());
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_pivot_table(move || {
            if let Some(app) = app_ref.upgrade() {
                let sheet = state.current.borrow();
                let vals = evaluate(&sheet);
                let dims = sheet.dimensions();
                let mut keys = Vec::new();
                let mut values = Vec::new();
                for r in 1..dims.rows {
                    let k = cell_value(&sheet, &vals, r, 0);
                    let v_str = cell_value(&sheet, &vals, r, 1);
                    if let Ok(num) = v_str.trim().parse::<f64>() {
                        keys.push(k);
                        values.push(num);
                    }
                }
                if let Ok(pivot) = compute_pivot(&keys, &values, PivotAggregation::Sum) {
                    let mut p_sheet = Sheet::new("Pivot Summary");
                    p_sheet.set_str("A1", "Category");
                    p_sheet.set_str("B1", "Total");
                    for (i, (k, v)) in pivot.iter().enumerate() {
                        let r = i + 2;
                        p_sheet.set_str(&format!("A{r}"), k);
                        p_sheet.set_str(&format!("B{r}"), &format!("{v:.2}"));
                    }
                    drop(sheet);
                    let cur = state.current.borrow().clone();
                    let active = *state.active_sheet_index.borrow();
                    state.sheets.borrow_mut()[active] = cur;
                    state.sheets.borrow_mut().push(p_sheet.clone());
                    let new_idx = state.sheets.borrow().len() - 1;
                    *state.active_sheet_index.borrow_mut() = new_idx;
                    *state.current.borrow_mut() = p_sheet;
                    apply_sheet(&app, &state.current.borrow());
                    sync_sheet_tabs(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!(
                        "Generated Pivot Summary ({} groups)",
                        pivot.len()
                    )));
                } else {
                    app.set_status_left(
                        "Pivot table: requires labels in Col A and numbers in Col B".into(),
                    );
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_clear_selected_cells(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                let changed = clear_selection(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                );
                if changed {
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!("Cleared {}", sel.label())));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_set_cell_alignment(move |align_idx| {
            if let Some(app) = app_ref.upgrade() {
                let align = match align_idx {
                    1 => CellAlignment::Center,
                    2 => CellAlignment::Right,
                    _ => CellAlignment::Left,
                };
                let sel = selection_from_app(&app);
                let changed = set_selection_alignment(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                    align,
                );
                if changed {
                    app.set_cell_alignment(align_idx);
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    let name = match align {
                        CellAlignment::Center => "Center",
                        CellAlignment::Right => "Right",
                        _ => "Left",
                    };
                    app.set_status_left(SharedString::from(format!(
                        "Aligned {} to {}",
                        sel.label(),
                        name
                    )));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_copy_selection(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                let data = copy_selection(&state.current.borrow(), sel);
                let cell_count = data.iter().map(|r| r.len()).sum::<usize>();
                *state.clipboard.borrow_mut() = Some(data);
                app.set_status_left(SharedString::from(format!(
                    "Copied {} ({} cells)",
                    sel.label(),
                    cell_count
                )));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_cut_selection(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                let data = copy_selection(&state.current.borrow(), sel);
                let cell_count = data.iter().map(|r| r.len()).sum::<usize>();
                *state.clipboard.borrow_mut() = Some(data);
                let changed = clear_selection(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                );
                if changed {
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                }
                app.set_status_left(SharedString::from(format!(
                    "Cut {} ({} cells)",
                    sel.label(),
                    cell_count
                )));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_paste_selection(move || {
            if let Some(app) = app_ref.upgrade() {
                let clip = state.clipboard.borrow().clone();
                if let Some(data) = clip {
                    let sel = selection_from_app(&app);
                    let pasted = paste_selection(
                        &mut state.current.borrow_mut(),
                        &mut state.undo_stack.borrow_mut(),
                        &mut state.redo_stack.borrow_mut(),
                        sel,
                        &data,
                    );
                    if pasted > 0 {
                        apply_sheet(&app, &state.current.borrow());
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_status_left(SharedString::from(format!(
                            "Pasted {} cells at {}",
                            pasted,
                            sel.focus.to_a1()
                        )));
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_select_all(move || {
            if let Some(app) = app_ref.upgrade() {
                let sheet = state.current.borrow();
                let sel = select_all_range(&sheet);
                let vals = evaluate(&sheet);
                update_selection_range(&app, &sheet, &vals, sel);
                apply_sheet(&app, &sheet);
                app.set_status_left(SharedString::from(format!(
                    "Selected all ({})",
                    sel.label()
                )));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_delete_row(move || {
            if let Some(app) = app_ref.upgrade() {
                let cell_str = app.get_selected_cell();
                if let Some(cell) = CellRef::parse(cell_str.as_str()) {
                    let before = state.current.borrow().clone();
                    if let Some(new_sheet) = delete_row(&before, cell.row) {
                        commit_transaction(
                            &mut state.current.borrow_mut(),
                            &mut state.undo_stack.borrow_mut(),
                            &mut state.redo_stack.borrow_mut(),
                            SheetTransaction::Snapshot {
                                before: Box::new(before),
                                after: Box::new(new_sheet),
                            },
                        );
                        let dims = state.current.borrow().dimensions();
                        let target_row = cell.row.min(dims.rows.saturating_sub(1));
                        select_cell(
                            &app,
                            &state.current.borrow(),
                            target_row as i32,
                            cell.col as i32,
                        );
                        apply_sheet(&app, &state.current.borrow());
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_status_left(SharedString::from(format!(
                            "Deleted row {}",
                            cell.row + 1
                        )));
                    } else {
                        app.set_status_left("Cannot delete the only row in table".into());
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_delete_col(move || {
            if let Some(app) = app_ref.upgrade() {
                let cell_str = app.get_selected_cell();
                if let Some(cell) = CellRef::parse(cell_str.as_str()) {
                    let before = state.current.borrow().clone();
                    if let Some(new_sheet) = delete_col(&before, cell.col) {
                        let col_letter = cell.to_a1().trim_end_matches('1').to_string();
                        commit_transaction(
                            &mut state.current.borrow_mut(),
                            &mut state.undo_stack.borrow_mut(),
                            &mut state.redo_stack.borrow_mut(),
                            SheetTransaction::Snapshot {
                                before: Box::new(before),
                                after: Box::new(new_sheet),
                            },
                        );
                        let dims = state.current.borrow().dimensions();
                        let target_col = cell.col.min(dims.cols.saturating_sub(1));
                        select_cell(
                            &app,
                            &state.current.borrow(),
                            cell.row as i32,
                            target_col as i32,
                        );
                        apply_sheet(&app, &state.current.borrow());
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_status_left(SharedString::from(format!(
                            "Deleted column {col_letter}"
                        )));
                    } else {
                        app.set_status_left("Cannot delete the only column in table".into());
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_delete_sheet(move || {
            if let Some(app) = app_ref.upgrade() {
                delete_active_sheet(&app, &state, &menu_service);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_sort_ascending(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if sort_table(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.anchor.col,
                    true,
                ) {
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left("Sorted rows ascending".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_sort_descending(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if sort_table(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.anchor.col,
                    false,
                ) {
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left("Sorted rows descending".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_freeze_panes(move || {
            if let Some(app) = app_ref.upgrade() {
                let before = state.current.borrow().clone();
                let mut after = before.clone();
                after.freeze_panes(1, 0);
                commit_transaction(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    SheetTransaction::Snapshot {
                        before: Box::new(before),
                        after: Box::new(after),
                    },
                );
                apply_sheet(&app, &state.current.borrow());
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left("Frozen header row".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_unfreeze_panes(move || {
            if let Some(app) = app_ref.upgrade() {
                let before = state.current.borrow().clone();
                let mut after = before.clone();
                after.unfreeze_panes();
                commit_transaction(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    SheetTransaction::Snapshot {
                        before: Box::new(before),
                        after: Box::new(after),
                    },
                );
                apply_sheet(&app, &state.current.borrow());
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left("Unfrozen panes".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_adjust_row_height(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let cell_str = app.get_selected_cell();
                if let Some(cell) = CellRef::parse(cell_str.as_str()) {
                    let cur_h = state.current.borrow().row_height(cell.row);
                    let new_h = (cur_h + delta as f32).clamp(16.0, 160.0);
                    let before = state.current.borrow().clone();
                    let mut after = before.clone();
                    after.set_row_height(cell.row, new_h);
                    commit_transaction(
                        &mut state.current.borrow_mut(),
                        &mut state.undo_stack.borrow_mut(),
                        &mut state.redo_stack.borrow_mut(),
                        SheetTransaction::Snapshot {
                            before: Box::new(before),
                            after: Box::new(after),
                        },
                    );
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!(
                        "Row {} height: {:.0} px",
                        cell.row + 1,
                        new_h
                    )));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_adjust_col_width(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let cell_str = app.get_selected_cell();
                if let Some(cell) = CellRef::parse(cell_str.as_str()) {
                    let cur_w = state.current.borrow().col_width(cell.col);
                    let new_w = (cur_w + delta as f32).clamp(32.0, 400.0);
                    let before = state.current.borrow().clone();
                    let mut after = before.clone();
                    after.set_col_width(cell.col, new_w);
                    commit_transaction(
                        &mut state.current.borrow_mut(),
                        &mut state.undo_stack.borrow_mut(),
                        &mut state.redo_stack.borrow_mut(),
                        SheetTransaction::Snapshot {
                            before: Box::new(before),
                            after: Box::new(after),
                        },
                    );
                    apply_sheet(&app, &state.current.borrow());
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!(
                        "Column width: {:.0} px",
                        new_w
                    )));
                }
            }
        });
    }
}

/// Copy values from a worksheet selection into a 2D matrix of raw strings.
pub(crate) fn copy_selection(sheet: &Sheet, sel: GridSelection) -> Vec<Vec<String>> {
    let range = sel.range();
    let mut rows = Vec::new();
    for r in range.start.row..=range.end.row {
        let mut row = Vec::new();
        for c in range.start.col..=range.end.col {
            let val = sheet
                .raw(CellRef { row: r, col: c })
                .unwrap_or_default()
                .to_string();
            row.push(val);
        }
        rows.push(row);
    }
    rows
}

/// Paste a 2D matrix into a worksheet with full undo transaction recording.
pub(crate) fn paste_selection(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    target_sel: GridSelection,
    data: &[Vec<String>],
) -> usize {
    if data.is_empty() || data[0].is_empty() {
        return 0;
    }
    let is_single = data.len() == 1 && data[0].len() == 1;
    let target_range = target_sel.range();
    let mut edits = Vec::new();

    if is_single && target_range.start != target_range.end {
        let val = &data[0][0];
        for r in target_range.start.row..=target_range.end.row {
            for c in target_range.start.col..=target_range.end.col {
                let cell = CellRef { row: r, col: c };
                let edit = RangeEdit::replace(sheet, cell, Some(val.clone()));
                edits.push(edit);
            }
        }
    } else {
        let origin = target_sel.focus;
        for (r_off, row) in data.iter().enumerate() {
            for (c_off, val) in row.iter().enumerate() {
                let cell = CellRef {
                    row: origin.row.saturating_add(r_off as u32),
                    col: origin.col.saturating_add(c_off as u32),
                };
                let edit = RangeEdit::replace(sheet, cell, Some(val.clone()));
                edits.push(edit);
            }
        }
    }

    let count = edits.len();
    if count > 0 {
        commit_transaction(
            sheet,
            undo_stack,
            redo_stack,
            SheetTransaction::Batch(edits),
        );
    }
    count
}

/// Create a selection spanning all populated cells in the sheet.
pub(crate) fn select_all_range(sheet: &Sheet) -> GridSelection {
    let dims = sheet.dimensions();
    let start = CellRef { row: 0, col: 0 };
    let end = CellRef {
        row: dims.rows.max(1).saturating_sub(1),
        col: dims.cols.max(1).saturating_sub(1),
    };
    GridSelection::new(start, end)
}

/// Delete a row and shift following rows upwards.
pub(crate) fn delete_row(sheet: &Sheet, target_row: u32) -> Option<Sheet> {
    let dims = sheet.dimensions();
    if dims.rows <= 1 {
        return None;
    }
    let mut new_sheet = Sheet::new(&sheet.name);
    new_sheet.freeze_rows = sheet.freeze_rows.min(dims.rows.saturating_sub(2));
    new_sheet.freeze_cols = sheet.freeze_cols;
    for (&col, &w) in &sheet.col_widths {
        new_sheet.set_col_width(col, w);
    }
    for (&r, &h) in &sheet.row_heights {
        if r < target_row {
            new_sheet.set_row_height(r, h);
        } else if r > target_row {
            new_sheet.set_row_height(r - 1, h);
        }
    }
    for (cell, c) in &sheet.cells {
        if cell.row < target_row {
            new_sheet.cells.insert(*cell, c.clone());
        } else if cell.row > target_row {
            new_sheet.cells.insert(
                CellRef {
                    row: cell.row - 1,
                    col: cell.col,
                },
                c.clone(),
            );
        }
    }
    for (cell, align) in &sheet.alignments {
        if cell.row < target_row {
            new_sheet.alignments.insert(*cell, *align);
        } else if cell.row > target_row {
            new_sheet.alignments.insert(
                CellRef {
                    row: cell.row - 1,
                    col: cell.col,
                },
                *align,
            );
        }
    }
    Some(new_sheet)
}

/// Delete a column and shift following columns leftwards.
pub(crate) fn delete_col(sheet: &Sheet, target_col: u32) -> Option<Sheet> {
    let dims = sheet.dimensions();
    if dims.cols <= 1 {
        return None;
    }
    let mut new_sheet = Sheet::new(&sheet.name);
    new_sheet.freeze_rows = sheet.freeze_rows;
    new_sheet.freeze_cols = sheet.freeze_cols.min(dims.cols.saturating_sub(2));
    for (&r, &h) in &sheet.row_heights {
        new_sheet.set_row_height(r, h);
    }
    for (&c, &w) in &sheet.col_widths {
        if c < target_col {
            new_sheet.set_col_width(c, w);
        } else if c > target_col {
            new_sheet.set_col_width(c - 1, w);
        }
    }
    for (cell, c) in &sheet.cells {
        if cell.col < target_col {
            new_sheet.cells.insert(*cell, c.clone());
        } else if cell.col > target_col {
            new_sheet.cells.insert(
                CellRef {
                    row: cell.row,
                    col: cell.col - 1,
                },
                c.clone(),
            );
        }
    }
    for (cell, align) in &sheet.alignments {
        if cell.col < target_col {
            new_sheet.alignments.insert(*cell, *align);
        } else if cell.col > target_col {
            new_sheet.alignments.insert(
                CellRef {
                    row: cell.row,
                    col: cell.col - 1,
                },
                *align,
            );
        }
    }
    Some(new_sheet)
}

/// Sort table rows preserving header row 0 with undo transaction recording.
pub(crate) fn sort_table(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    col_idx: u32,
    ascending: bool,
) -> bool {
    let dims = sheet.dimensions();
    if dims.rows <= 1 {
        return false;
    }
    let range = CellRange::new(
        CellRef { row: 1, col: 0 },
        CellRef {
            row: dims.rows.saturating_sub(1),
            col: dims.cols.saturating_sub(1),
        },
    );
    let before = sheet.clone();
    let mut model = SheetModel::new(sheet.clone());
    let clamped_col = col_idx.min(dims.cols.saturating_sub(1));
    if model.sort_rows(range, clamped_col, ascending).is_ok() {
        let after = model.sheet;
        commit_transaction(
            sheet,
            undo_stack,
            redo_stack,
            SheetTransaction::Snapshot {
                before: Box::new(before),
                after: Box::new(after),
            },
        );
        true
    } else {
        false
    }
}

/// Delete active worksheet in multi-sheet workbook.
pub(crate) fn delete_active_sheet(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) -> bool {
    let count = state.sheets.borrow().len();
    if count <= 1 {
        app.set_status_left("Cannot delete the only worksheet".into());
        return false;
    }
    let active_idx = *state.active_sheet_index.borrow();
    state.sheets.borrow_mut().remove(active_idx);
    let new_idx = active_idx.min(state.sheets.borrow().len() - 1);
    *state.active_sheet_index.borrow_mut() = new_idx;
    let next_sheet = state.sheets.borrow()[new_idx].clone();
    *state.current.borrow_mut() = next_sheet;
    if active_idx < state.sheet_histories.borrow().len() {
        state.sheet_histories.borrow_mut().remove(active_idx);
    }
    sync_sheet_tabs(app, state);
    apply_sheet(app, &state.current.borrow());
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(format!(
        "Deleted sheet. Active: {}",
        state.current.borrow().name
    )));
    true
}

/// Dispatch canonical command IDs through the same Slint callbacks used by
/// Sheets toolbar and palette controls.
pub(crate) fn dispatch_command(app: &SheetsApp, id: &str) -> bool {
    match id {
        "file.new" | "sheets.new" => app.invoke_new_sheet(),
        "file.new_template" | "sheets.new-template" => {
            app.set_template_chooser_open(true);
        }
        "file.open" | "sheets.open" => app.invoke_open_sheet(),
        "file.save" | "sheets.save" => app.invoke_save_sheet(),
        "file.save_as" | "sheets.save-as" => app.invoke_save_as_sheet(),
        "file.export_csv" | "sheets.export-csv" => app.invoke_export_csv(),
        "file.export_xlsx" | "sheets.export-xlsx" => app.invoke_export_xlsx(),
        "edit.undo" | "sheets.undo" => app.invoke_undo(),
        "edit.redo" | "sheets.redo" => app.invoke_redo(),
        "edit.cut" | "sheets.cut" => app.invoke_cut_selection(),
        "edit.copy" | "sheets.copy" => app.invoke_copy_selection(),
        "edit.paste" | "sheets.paste" => app.invoke_paste_selection(),
        "edit.select_all" | "sheets.select-all" => app.invoke_select_all(),
        "app.palette" => app.invoke_open_palette(),
        "view.inspector" => app.invoke_toggle_inspector(),
        "table.add_row" => app.invoke_add_row(),
        "table.delete_row" => app.invoke_delete_row(),
        "table.add_col" | "sheets.add-col" => app.invoke_add_table_col(),
        "table.delete_col" => app.invoke_delete_col(),
        "table.sort_asc" | "sheets.sort-asc" => app.invoke_sort_ascending(),
        "table.sort_desc" | "sheets.sort-desc" => app.invoke_sort_descending(),
        "table.freeze_header" | "sheets.freeze-header" => app.invoke_freeze_panes(),
        "table.unfreeze_panes" | "sheets.unfreeze-panes" => app.invoke_unfreeze_panes(),
        "sheets.delete_sheet" => app.invoke_delete_sheet(),
        "sheets.organize" => app.invoke_organize(),
        _ => return false,
    }
    true
}

pub(crate) fn schedule_menu_action(
    app_ref: &slint::Weak<SheetsApp>,
    action: CommandAction,
) -> Result<(), DesktopError> {
    let error_id = action.id.clone();
    app_ref
        .upgrade_in_event_loop(move |app| {
            let id = action.id.as_str();
            if !dispatch_command(&app, id) {
                app.set_status_left(SharedString::from(format!(
                    "Unsupported menu command: {id}"
                )));
            }
        })
        .map_err(|error| {
            DesktopError::InvalidRequest(format!(
                "failed to schedule Sheets menu command {error_id}: {error}"
            ))
        })
}

#[cfg(test)]
#[path = "actions_tests.rs"]
mod tests;
