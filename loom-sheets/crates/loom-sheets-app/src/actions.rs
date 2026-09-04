//! Sheet and toolbar action handlers for Loom Sheets.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{CommandAction, DesktopError, NativeMenuBar};
use loom_sheets_core::{
    compute_pivot, evaluate, CellAlignment, CellRange, CellRef, ChartKind, ChartSeries, ChartSpec,
    PivotAggregation, Sheet, SheetModel,
};
use slint::{ComponentHandle, SharedString, VecModel};

use crate::{
    apply_sheet, cell_value, clear_selection, commit_formula_edit, select_cell, selection_from_app,
    set_selection_alignment, sync_menu_state, GuiState, SheetsApp,
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
                    let rel_col = sel.anchor.col;
                    if model.sort_rows(range, rel_col, true).is_ok() {
                        *state.current.borrow_mut() = model.sheet;
                        apply_sheet(&app, &state.current.borrow());
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_status_left("Sorted table rows ascending".into());
                    }
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
}

/// Dispatch canonical command IDs through the same Slint callbacks used by
/// Sheets toolbar and palette controls.
pub(crate) fn dispatch_command(app: &SheetsApp, id: &str) -> bool {
    match id {
        "file.new" | "sheets.new" => app.invoke_new_sheet(),
        "file.open" | "sheets.open" => app.invoke_open_sheet(),
        "file.save" | "sheets.save" => app.invoke_save_sheet(),
        "file.save_as" | "sheets.save-as" => app.invoke_save_as_sheet(),
        "file.export_csv" | "sheets.export-csv" => app.invoke_export_csv(),
        "file.export_xlsx" | "sheets.export-xlsx" => app.invoke_export_xlsx(),
        "edit.undo" | "sheets.undo" => app.invoke_undo(),
        "edit.redo" | "sheets.redo" => app.invoke_redo(),
        "app.palette" => app.invoke_open_palette(),
        "view.inspector" => app.invoke_toggle_inspector(),
        "table.add_row" => app.invoke_add_row(),
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
mod tests {
    use super::*;
    use crate::starter_workbook;
    use loom_desktop::{FileFilter, ScriptedFileDialogs};

    fn make_test_state() -> Rc<GuiState> {
        let dialogs = Rc::new(ScriptedFileDialogs::new([], []));
        Rc::new(GuiState::new(
            starter_workbook(),
            None,
            dialogs,
            FileFilter::new("Workbook", ["loomtable"]).unwrap(),
            FileFilter::new("CSV", ["csv"]).unwrap(),
            FileFilter::new("CSV", ["csv"]).unwrap(),
            FileFilter::new("Excel", ["xlsx"]).unwrap(),
        ))
    }

    #[test]
    fn test_multi_sheet_state_creation_and_switching() {
        let state = make_test_state();
        assert_eq!(state.sheets.borrow().len(), 1);
        assert_eq!(*state.active_sheet_index.borrow(), 0);
        assert_eq!(state.current.borrow().name, "Budget");

        // Add a second sheet
        let mut s2 = Sheet::new("Expenses");
        s2.set_str("A1", "Groceries");
        s2.set_str("B1", "250");
        state.sheets.borrow_mut().push(s2.clone());
        assert_eq!(state.sheets.borrow().len(), 2);

        // Switch to sheet 1
        let cur = state.current.borrow().clone();
        let active = *state.active_sheet_index.borrow();
        state.sheets.borrow_mut()[active] = cur;
        *state.active_sheet_index.borrow_mut() = 1;
        *state.current.borrow_mut() = s2;

        assert_eq!(*state.active_sheet_index.borrow(), 1);
        assert_eq!(state.current.borrow().name, "Expenses");
        assert_eq!(
            state.current.borrow().raw(CellRef::parse("A1").unwrap()),
            Some("Groceries")
        );

        // Switch back to sheet 0
        let cur = state.current.borrow().clone();
        let active = *state.active_sheet_index.borrow();
        state.sheets.borrow_mut()[active] = cur;
        *state.active_sheet_index.borrow_mut() = 0;
        *state.current.borrow_mut() = state.sheets.borrow()[0].clone();

        assert_eq!(*state.active_sheet_index.borrow(), 0);
        assert_eq!(state.current.borrow().name, "Budget");
    }

    #[test]
    fn test_add_row_and_undo() {
        let state = make_test_state();
        let initial_rows = state.current.borrow().dimensions().rows;
        let cell = CellRef {
            row: initial_rows,
            col: 0,
        };

        let committed = commit_formula_edit(
            &mut state.current.borrow_mut(),
            &mut state.undo_stack.borrow_mut(),
            &mut state.redo_stack.borrow_mut(),
            cell,
            "New Item",
        );
        assert!(committed);
        assert_eq!(state.current.borrow().raw(cell), Some("New Item"));
        assert!(state.current.borrow().dimensions().rows > initial_rows);

        // Undo
        let edit = state.undo_stack.borrow_mut().pop().expect("undo edit");
        edit.revert(&mut state.current.borrow_mut());
        assert_eq!(state.current.borrow().raw(cell), None);
    }

    #[test]
    fn test_organize_sorts_rows() {
        let mut sheet = Sheet::new("SortTest");
        sheet.set_str("A1", "Item");
        sheet.set_str("A2", "Zebra");
        sheet.set_str("A3", "Apple");
        sheet.set_str("A4", "Mango");

        let range = CellRange::parse("A2:A4").unwrap();
        let mut model = SheetModel::new(sheet);
        assert!(model.sort_rows(range, 0, true).is_ok());

        assert_eq!(
            model.sheet.raw(CellRef::parse("A2").unwrap()),
            Some("Apple")
        );
        assert_eq!(
            model.sheet.raw(CellRef::parse("A3").unwrap()),
            Some("Mango")
        );
        assert_eq!(
            model.sheet.raw(CellRef::parse("A4").unwrap()),
            Some("Zebra")
        );
    }

    #[test]
    fn test_pivot_table_aggregation() {
        let keys = vec!["Fruit".to_string(), "Veg".to_string(), "Fruit".to_string()];
        let values = vec![10.0, 5.0, 15.0];
        let pivot = compute_pivot(&keys, &values, PivotAggregation::Sum).expect("pivot table");

        assert_eq!(pivot.len(), 2);
        assert_eq!(pivot[0].0, "Fruit");
        assert_eq!(pivot[0].1, 25.0);
        assert_eq!(pivot[1].0, "Veg");
        assert_eq!(pivot[1].1, 5.0);
    }

    #[test]
    fn test_chart_spec_validation() {
        let spec = ChartSpec {
            kind: ChartKind::Bar,
            title: "Revenue Chart".into(),
            series: vec![ChartSeries {
                name: "Revenue".into(),
                categories: vec!["Q1".into(), "Q2".into()],
                values: vec![1000.0, 1500.0],
            }],
        };
        assert!(spec.validate().is_ok());
    }

    #[test]
    fn test_sheet_renaming() {
        let state = make_test_state();
        assert_eq!(state.current.borrow().name, "Budget");
        state.current.borrow_mut().name = "Financial Plan".to_string();
        let active_idx = *state.active_sheet_index.borrow();
        state.sheets.borrow_mut()[active_idx].name = "Financial Plan".to_string();
        assert_eq!(state.current.borrow().name, "Financial Plan");
        assert_eq!(state.sheets.borrow()[0].name, "Financial Plan");
    }

    #[test]
    fn test_add_table_col_and_undo() {
        let state = make_test_state();
        let initial_cols = state.current.borrow().dimensions().cols;
        let cell = CellRef {
            row: 0,
            col: initial_cols,
        };
        let col_letter = cell.to_a1().trim_end_matches('1').to_string();
        let committed = commit_formula_edit(
            &mut state.current.borrow_mut(),
            &mut state.undo_stack.borrow_mut(),
            &mut state.redo_stack.borrow_mut(),
            cell,
            &col_letter,
        );
        assert!(committed);
        assert_eq!(state.current.borrow().raw(cell), Some(col_letter.as_str()));
        assert!(state.current.borrow().dimensions().cols > initial_cols);

        // Undo
        let edit = state.undo_stack.borrow_mut().pop().expect("undo edit");
        edit.revert(&mut state.current.borrow_mut());
        assert_eq!(state.current.borrow().raw(cell), None);
    }

    #[test]
    fn test_cell_format_and_undo() {
        let state = make_test_state();
        let cell = CellRef { row: 1, col: 1 };
        state.current.borrow_mut().set_str("B2", "1250");

        // Format as Currency: $1250.00
        let current_raw = state.current.borrow().raw(cell).unwrap().to_string();
        let num = current_raw.parse::<f64>().unwrap();
        let formatted = format!("${num:.2}");
        let committed = commit_formula_edit(
            &mut state.current.borrow_mut(),
            &mut state.undo_stack.borrow_mut(),
            &mut state.redo_stack.borrow_mut(),
            cell,
            &formatted,
        );
        assert!(committed);
        assert_eq!(state.current.borrow().raw(cell), Some("$1250.00"));

        // Undo format
        let edit = state.undo_stack.borrow_mut().pop().expect("undo edit");
        edit.revert(&mut state.current.borrow_mut());
        assert_eq!(state.current.borrow().raw(cell), Some("1250"));
    }

    #[test]
    fn test_chart_spec_normalization_pipeline() {
        let mut sheet = Sheet::new("Sales");
        sheet.set_str("A1", "Region");
        sheet.set_str("B1", "Revenue");
        sheet.set_str("A2", "North");
        sheet.set_str("B2", "$100.00");
        sheet.set_str("A3", "South");
        sheet.set_str("B3", "$300.00");

        let vals = evaluate(&sheet);
        let dims = sheet.dimensions();
        let mut categories = Vec::new();
        let mut values = Vec::new();
        for r in 1..dims.rows {
            let cat = cell_value(&sheet, &vals, r, 0);
            let val_str = cell_value(&sheet, &vals, r, 1);
            let clean = val_str.trim().trim_start_matches('$').trim_end_matches('%');
            if let Ok(num) = clean.parse::<f64>() {
                categories.push(cat);
                values.push(num);
            }
        }
        assert_eq!(categories, vec!["North", "South"]);
        assert_eq!(values, vec![100.0, 300.0]);

        let spec = ChartSpec {
            kind: ChartKind::Bar,
            title: format!("{} Chart", sheet.name),
            series: vec![ChartSeries {
                name: "Series 1".into(),
                categories: categories.clone(),
                values: values.clone(),
            }],
        };
        let norm = spec.normalized_points().expect("normalized points");
        assert_eq!(norm.len(), 1);
        assert_eq!(norm[0].len(), 2);
        assert!((norm[0][0] - 0.0).abs() < 1e-5);
        assert!((norm[0][1] - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_clear_selection_and_undo() {
        let state = make_test_state();
        let a1 = CellRef::parse("A1").unwrap();
        let a2 = CellRef::parse("A2").unwrap();
        assert_eq!(state.current.borrow().raw(a1), Some("Item"));
        assert_eq!(state.current.borrow().raw(a2), Some("Rent"));

        let range = CellRange::new(a1, a2);
        let changed = clear_selection(
            &mut state.current.borrow_mut(),
            &mut state.undo_stack.borrow_mut(),
            &mut state.redo_stack.borrow_mut(),
            range,
        );
        assert!(changed);
        assert_eq!(state.current.borrow().raw(a1), None);
        assert_eq!(state.current.borrow().raw(a2), None);

        // Undo restores both cells
        let tx = state.undo_stack.borrow_mut().pop().expect("undo tx");
        tx.revert(&mut state.current.borrow_mut());
        assert_eq!(state.current.borrow().raw(a1), Some("Item"));
        assert_eq!(state.current.borrow().raw(a2), Some("Rent"));

        // Redo clears both cells again
        tx.apply(&mut state.current.borrow_mut());
        assert_eq!(state.current.borrow().raw(a1), None);
        assert_eq!(state.current.borrow().raw(a2), None);
    }

    #[test]
    fn test_set_selection_alignment_and_undo() {
        let state = make_test_state();
        let a1 = CellRef::parse("A1").unwrap();
        let a2 = CellRef::parse("A2").unwrap();
        assert_eq!(
            state.current.borrow().cell_alignment(a1),
            CellAlignment::General
        );
        assert_eq!(
            state.current.borrow().cell_alignment(a2),
            CellAlignment::General
        );

        let range = CellRange::new(a1, a2);
        let changed = set_selection_alignment(
            &mut state.current.borrow_mut(),
            &mut state.undo_stack.borrow_mut(),
            &mut state.redo_stack.borrow_mut(),
            range,
            CellAlignment::Center,
        );
        assert!(changed);
        assert_eq!(
            state.current.borrow().cell_alignment(a1),
            CellAlignment::Center
        );
        assert_eq!(
            state.current.borrow().cell_alignment(a2),
            CellAlignment::Center
        );

        // Undo restores previous alignment
        let tx = state.undo_stack.borrow_mut().pop().expect("undo tx");
        tx.revert(&mut state.current.borrow_mut());
        assert_eq!(
            state.current.borrow().cell_alignment(a1),
            CellAlignment::General
        );
        assert_eq!(
            state.current.borrow().cell_alignment(a2),
            CellAlignment::General
        );

        // Redo restores Center alignment
        tx.apply(&mut state.current.borrow_mut());
        assert_eq!(
            state.current.borrow().cell_alignment(a1),
            CellAlignment::Center
        );
        assert_eq!(
            state.current.borrow().cell_alignment(a2),
            CellAlignment::Center
        );
    }

    #[test]
    fn test_per_sheet_undo_stacks_preserved_across_tab_switches() {
        let state = make_test_state();
        let a1 = CellRef::parse("A1").unwrap();

        // Edit on Sheet 1 (Budget)
        commit_formula_edit(
            &mut state.current.borrow_mut(),
            &mut state.undo_stack.borrow_mut(),
            &mut state.redo_stack.borrow_mut(),
            a1,
            "Budget Header",
        );
        assert_eq!(state.undo_stack.borrow().len(), 1);

        // Save Sheet 1 history and add Sheet 2
        let cur_undo = std::mem::take(&mut *state.undo_stack.borrow_mut());
        let cur_redo = std::mem::take(&mut *state.redo_stack.borrow_mut());
        state.sheet_histories.borrow_mut()[0] = (cur_undo, cur_redo);

        let s2 = Sheet::new("Sheet 2");
        state.sheets.borrow_mut().push(s2.clone());
        state
            .sheet_histories
            .borrow_mut()
            .push((Vec::new(), Vec::new()));
        *state.active_sheet_index.borrow_mut() = 1;
        *state.current.borrow_mut() = s2;
        assert!(state.undo_stack.borrow().is_empty());

        // Edit on Sheet 2
        commit_formula_edit(
            &mut state.current.borrow_mut(),
            &mut state.undo_stack.borrow_mut(),
            &mut state.redo_stack.borrow_mut(),
            a1,
            "Sheet 2 Header",
        );
        assert_eq!(state.undo_stack.borrow().len(), 1);

        // Save Sheet 2 history and switch back to Sheet 1
        let s2_undo = std::mem::take(&mut *state.undo_stack.borrow_mut());
        let s2_redo = std::mem::take(&mut *state.redo_stack.borrow_mut());
        state.sheet_histories.borrow_mut()[1] = (s2_undo, s2_redo);

        *state.active_sheet_index.borrow_mut() = 0;
        let s1_sheet = state.sheets.borrow()[0].clone();
        *state.current.borrow_mut() = s1_sheet;
        let (s1_undo, s1_redo) = state.sheet_histories.borrow()[0].clone();
        *state.undo_stack.borrow_mut() = s1_undo;
        *state.redo_stack.borrow_mut() = s1_redo;

        // Sheet 1 undo stack is preserved and functional!
        assert_eq!(state.undo_stack.borrow().len(), 1);
        let tx = state.undo_stack.borrow_mut().pop().unwrap();
        tx.revert(&mut state.current.borrow_mut());
        assert_eq!(state.current.borrow().raw(a1), Some("Item"));
    }

    #[test]
    fn test_export_xlsx_from_sheet_grid() {
        let state = make_test_state();
        let grid = crate::sheet_to_grid(&state.current.borrow());
        assert!(!grid.is_empty());
        let bytes = loom_sheets_core::export_xlsx_from_grid(&grid).expect("export xlsx");
        // Check zip header signature PK\x03\x04
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }
}
