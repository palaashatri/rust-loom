//! Sheet and toolbar action handlers for Loom Sheets.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{CommandAction, DesktopError, NativeMenuBar};
use loom_sheets_core::{
    compute_pivot, evaluate, CellRange, CellRef, ChartKind, ChartSeries, ChartSpec,
    PivotAggregation, Sheet, SheetModel,
};
use slint::{ComponentHandle, SharedString, VecModel};

use crate::{
    apply_sheet, cell_value, commit_formula_edit, select_cell, selection_from_app, sync_menu_state,
    GuiState, SheetsApp,
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
                    *state.active_sheet_index.borrow_mut() = idx;
                    *state.current.borrow_mut() = state.sheets.borrow()[idx].clone();
                    state.undo_stack.borrow_mut().clear();
                    state.redo_stack.borrow_mut().clear();
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
                let count = state.sheets.borrow().len() + 1;
                let new_sheet = Sheet::new(&format!("Sheet {count}"));
                state.sheets.borrow_mut().push(new_sheet.clone());
                *state.active_sheet_index.borrow_mut() = count - 1;
                *state.current.borrow_mut() = new_sheet;
                state.undo_stack.borrow_mut().clear();
                state.redo_stack.borrow_mut().clear();
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
                let vals = evaluate(&sheet);
                let dims = sheet.dimensions();
                let mut categories = Vec::new();
                let mut values = Vec::new();
                for r in 1..dims.rows {
                    let cat = cell_value(&sheet, &vals, r, 0);
                    let val_str = cell_value(&sheet, &vals, r, 1);
                    if let Ok(num) = val_str.trim().parse::<f64>() {
                        categories.push(cat);
                        values.push(num);
                    }
                }
                let spec = ChartSpec {
                    kind: ChartKind::Bar,
                    title: format!("{} Chart", sheet.name),
                    series: vec![ChartSeries {
                        name: "Series 1".into(),
                        categories,
                        values,
                    }],
                };
                if spec.validate().is_ok() {
                    app.set_status_left(SharedString::from(format!(
                        "Inserted {} (Bar)",
                        spec.title
                    )));
                    app.set_formula_feedback(SharedString::from(format!(
                        "Chart created with {} data points",
                        spec.series[0].values.len()
                    )));
                } else {
                    app.set_status_left("Insert chart: enter numeric data in Column B".into());
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
}
