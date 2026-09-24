//! Sheet and toolbar action handlers for Loom Sheets.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{CommandAction, DesktopError, NativeMenuBar};
use loom_sheets_core::style::FillColor;
use loom_sheets_core::{
    CellAlignment, CellRange, CellRef, NumberFormat, PivotAggregation, RangeEdit, Sheet,
    SheetChart, SheetModel, SheetObject,
};
use slint::{ComponentHandle, Image, Model, SharedString, VecModel};

pub(crate) use crate::chart_actions::sync_chart_to_app;

use crate::analysis::{
    label_value_columns, plan_chart_in_range, plan_pivot_sheet, unique_sheet_name,
};
use crate::formatting::{
    adjust_selection_font_size, cycle_selection_fill, set_selection_decimal_places,
    set_selection_fill, set_selection_number_format, toggle_selection_bold,
    toggle_selection_borders, toggle_selection_italic, toggle_selection_underline,
};
use crate::{
    apply_sheet, apply_sheet_view_change, clear_selection, commit_formula_edit, commit_transaction,
    commit_workbook_transaction, evaluate_current, image_open_request, project_current,
    select_cell, selection_from_app, set_selection_alignment, sync_current_to_tabs,
    sync_menu_state, update_selection_range, GridSelection, GuiState, SheetTransaction, SheetsApp,
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

pub(crate) use crate::template_navigation::wire_template_navigation;

/// Apply a zoom level: factor drives geometry, label drives the toolbar.
/// View-only (like scrolling): no undo transaction, but the projection and
/// menu state refresh so the new scale renders immediately.
pub(crate) fn set_zoom(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
    factor: f32,
) {
    let factor = factor.clamp(0.5, 3.0);
    app.set_zoom_factor(factor);
    let label = format!("{}%", (factor * 100.0).round() as i32);
    app.set_zoom_level(label.clone().into());
    project_current(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(format!("Zoom set to {label}")));
}

/// Build a template workbook into fresh single-tab state. Creating from a
/// template is not an undoable edit (matching New/Open): stacks reset.
pub(crate) fn create_template_workbook(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
    idx: i32,
) {
    let idx = idx.clamp(0, 10);
    let mut recents: Vec<i32> = app
        .get_template_recents()
        .iter()
        .filter(|recent| *recent != idx)
        .collect();
    recents.insert(0, idx);
    recents.truncate(3);
    app.set_template_recents(Rc::new(VecModel::from(recents)).into());
    let sheet = crate::template_sheet(idx);
    *state.current.borrow_mut() = sheet.clone();
    *state.sheets.borrow_mut() = vec![sheet];
    *state.active_sheet_index.borrow_mut() = 0;
    *state.save_path.borrow_mut() = None;
    state.undo_stack.borrow_mut().clear();
    state.redo_stack.borrow_mut().clear();
    *state.sheet_histories.borrow_mut() = vec![(Vec::new(), Vec::new())];
    apply_sheet(app, state);
    sync_sheet_tabs(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_template_chooser_open(false);
    app.set_status_left("Created template workbook".into());
}

/// Status-line confirmation for a style toggle, read back from the focused
/// cell so screen-reader and keyboard users learn the applied state.
fn announce_toggle(app: &SheetsApp, label: &str, on: bool) {
    app.set_status_left(SharedString::from(format!(
        "{label} {}",
        if on { "on" } else { "off" }
    )));
}

/// Register all toolbar, sheet-tab, and menu actions on the Slint window.
pub(crate) fn register_sheet_actions(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    wire_template_navigation(app);
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_create_template(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                create_template_workbook(&app, &state, &menu_service, idx);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_cancel_template(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_template_chooser_open(false);
            }
        });
    }
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

                    apply_sheet_view_change(&app, &state);
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
                sync_current_to_tabs(&state);
                let mut after_sheets = state.sheets.borrow().clone();
                // Generated tab names stay unique even after deletions.
                let mut count = after_sheets.len() + 1;
                while after_sheets
                    .iter()
                    .any(|sheet| sheet.name.eq_ignore_ascii_case(&format!("Sheet {count}")))
                {
                    count += 1;
                }
                after_sheets.push(Sheet::new(&format!("Sheet {count}")));
                commit_workbook_transaction(&state, after_sheets, count - 1, None);
                apply_sheet(&app, &state);
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
                    apply_sheet(&app, &state);
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
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left("Sorted table rows ascending".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_cycle_zoom(move || {
            if let Some(app) = app_ref.upgrade() {
                let next = match (app.get_zoom_factor() * 100.0).round() as i32 {
                    75 => 1.0,
                    100 => 1.25,
                    125 => 1.5,
                    _ => 0.75,
                };
                set_zoom(&app, &state, &menu_service, next);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_zoom_in(move || {
            if let Some(app) = app_ref.upgrade() {
                let current = crate::zoom_factor(&app);
                set_zoom(&app, &state, &menu_service, (current + 0.25).min(3.0));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_zoom_out(move || {
            if let Some(app) = app_ref.upgrade() {
                let current = crate::zoom_factor(&app);
                set_zoom(&app, &state, &menu_service, (current - 0.25).max(0.5));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_zoom_actual(move || {
            if let Some(app) = app_ref.upgrade() {
                set_zoom(&app, &state, &menu_service, 1.0);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_toggle_bold(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if toggle_selection_bold(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                ) {
                    let on = state.current.borrow().cell_style(sel.anchor).bold;
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    announce_toggle(&app, "Bold", on);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_toggle_italic(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if toggle_selection_italic(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                ) {
                    let on = state.current.borrow().cell_style(sel.anchor).italic;
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    announce_toggle(&app, "Italic", on);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_toggle_underline(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if toggle_selection_underline(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                ) {
                    let on = state.current.borrow().cell_style(sel.anchor).underline;
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    announce_toggle(&app, "Underline", on);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_adjust_decimals(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if set_selection_decimal_places(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                    delta,
                ) {
                    let decimals = state
                        .current
                        .borrow()
                        .cell_style(sel.anchor)
                        .decimal_places
                        .unwrap_or(2);
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!("Decimals: {decimals}")));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_toggle_borders(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if toggle_selection_borders(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                ) {
                    let on = state.current.borrow().cell_style(sel.anchor).border;
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    announce_toggle(&app, "Borders", on);
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_cycle_fill(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if cycle_selection_fill(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                ) {
                    let fill = state.current.borrow().cell_style(sel.anchor).fill;
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!("Fill {}", fill.as_str())));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_adjust_font(move |delta| {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                if adjust_selection_font_size(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                    delta,
                ) {
                    let size = crate::formatting::effective_font_size(
                        state.current.borrow().cell_style(sel.anchor),
                    );
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!("Font size {size}")));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_set_fill(move |swatch_idx| {
            if let Some(app) = app_ref.upgrade() {
                let fill = match swatch_idx {
                    0 => FillColor::Red,
                    1 => FillColor::Orange,
                    2 => FillColor::Yellow,
                    3 => FillColor::Green,
                    4 => FillColor::Blue,
                    5 => FillColor::Purple,
                    6 => FillColor::Gray,
                    _ => FillColor::None,
                };
                let sel = selection_from_app(&app);
                if set_selection_fill(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                    fill,
                ) {
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!("Fill {}", fill.as_str())));
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_insert_chart(move || {
            if let Some(app) = app_ref.upgrade() {
                let range = selection_from_app(&app).range();
                let explicit_range =
                    range.end.col == range.start.col + 1 && range.end.row > range.start.row;
                let existing = state.current.borrow().chart.clone();
                if let Some(existing) = existing.as_ref().filter(|_| !explicit_range) {
                    app.set_chart_visible(true);
                    sync_chart_to_app(&app, &state.current.borrow());
                    app.set_status_left(SharedString::from(format!(
                        "Showing {} ({})",
                        existing.title,
                        existing.kind.as_str()
                    )));
                    return;
                }
                if !explicit_range {
                    app.set_status_left(
                        "Select two columns including headers and data, then Insert Chart.".into(),
                    );
                    return;
                }
                let planned = plan_chart_in_range(
                    &state.current.borrow(),
                    range.start.col,
                    range.end.col,
                    range.start.row + 1,
                    range.end.row,
                );
                match planned {
                    Ok(mut chart) => {
                        if let Some(existing) = existing {
                            chart.kind = existing.kind;
                            chart.title = existing.title;
                        }
                        let before = state.current.borrow().clone();
                        let mut after = before.clone();
                        after.chart = Some(chart.clone());
                        commit_transaction(
                            &mut state.current.borrow_mut(),
                            &mut state.undo_stack.borrow_mut(),
                            &mut state.redo_stack.borrow_mut(),
                            SheetTransaction::Snapshot {
                                before: Box::new(before),
                                after: Box::new(after),
                            },
                        );
                        app.set_chart_visible(true);
                        sync_chart_to_app(&app, &state.current.borrow());
                        apply_sheet(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_status_left(SharedString::from(format!(
                            "Inserted {} ({})",
                            chart.title,
                            chart.kind.as_str()
                        )));
                    }
                    Err(hint) => {
                        app.set_status_left(SharedString::from(hint));
                    }
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        app.on_close_chart(move || {
            if let Some(app) = app_ref.upgrade() {
                app.set_chart_visible(false);
                app.set_status_left("Chart hidden".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_insert_shape(move || {
            if let Some(app) = app_ref.upgrade() {
                let before = state.current.borrow().clone();
                let mut after = before.clone();
                let anchor = selection_from_app(&app).focus;
                after.objects.push(SheetObject::shape(anchor, "Shape"));
                commit_transaction(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    SheetTransaction::Snapshot {
                        before: Box::new(before),
                        after: Box::new(after),
                    },
                );
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left("Inserted shape".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_insert_image(move || {
            if let Some(app) = app_ref.upgrade() {
                let request = match image_open_request(&state) {
                    Ok(request) => request,
                    Err(error) => {
                        app.set_status_left(SharedString::from(format!(
                            "Insert image failed: {error}"
                        )));
                        return;
                    }
                };
                match state.dialogs.open_file(&request) {
                    Ok(Some(path)) => {
                        if let Err(error) = Image::load_from_path(&path) {
                            app.set_status_left(SharedString::from(format!(
                                "Insert image failed: {error}"
                            )));
                            return;
                        }
                        let anchor = selection_from_app(&app).focus;
                        let object =
                            match SheetObject::image(anchor, path.to_string_lossy().into_owned()) {
                                Ok(object) => object,
                                Err(error) => {
                                    app.set_status_left(SharedString::from(format!(
                                        "Insert image failed: {error}"
                                    )));
                                    return;
                                }
                            };
                        let before = state.current.borrow().clone();
                        let mut after = before.clone();
                        after.objects.push(object);
                        commit_transaction(
                            &mut state.current.borrow_mut(),
                            &mut state.undo_stack.borrow_mut(),
                            &mut state.redo_stack.borrow_mut(),
                            SheetTransaction::Snapshot {
                                before: Box::new(before),
                                after: Box::new(after),
                            },
                        );
                        apply_sheet(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_status_left(SharedString::from(format!(
                            "Inserted image {}",
                            path.display()
                        )));
                    }
                    Ok(None) => app.set_status_left("Insert image cancelled".into()),
                    Err(error) => app.set_status_left(SharedString::from(format!(
                        "Insert image dialog failed: {error}"
                    ))),
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_cycle_chart_kind(move || {
            if let Some(app) = app_ref.upgrade() {
                let before = state.current.borrow().clone();
                let Some(current) = before.chart.clone() else {
                    app.set_status_left("Insert a chart first".into());
                    return;
                };
                let mut after = before.clone();
                after.chart = Some(SheetChart {
                    kind: current.kind.cycle(),
                    ..current
                });
                commit_transaction(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    SheetTransaction::Snapshot {
                        before: Box::new(before),
                        after: Box::new(after),
                    },
                );
                sync_chart_to_app(&app, &state.current.borrow());
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left(SharedString::from(format!(
                    "Chart kind: {}",
                    state
                        .current
                        .borrow()
                        .chart
                        .as_ref()
                        .map(|chart| chart.kind.as_str())
                        .unwrap_or("bar")
                )));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_pivot_summary(move |agg_idx| {
            if let Some(app) = app_ref.upgrade() {
                let aggregation = match agg_idx {
                    1 => PivotAggregation::Count,
                    2 => PivotAggregation::Average,
                    3 => PivotAggregation::Min,
                    4 => PivotAggregation::Max,
                    _ => PivotAggregation::Sum,
                };
                let (cat_col, val_col) = label_value_columns(&app, &state.current.borrow());
                let source_name = state.current.borrow().name.clone();
                match plan_pivot_sheet(&state.current.borrow(), cat_col, val_col, aggregation) {
                    Ok((pivot, groups)) => {
                        sync_current_to_tabs(&state);
                        let mut after_sheets = state.sheets.borrow().clone();
                        let mut pivot = pivot;
                        pivot.name = unique_sheet_name(&pivot.name, &after_sheets);
                        after_sheets.push(pivot);
                        commit_workbook_transaction(
                            &state,
                            after_sheets,
                            state.sheets.borrow().len(),
                            None,
                        );
                        apply_sheet(&app, &state);
                        sync_sheet_tabs(&app, &state);
                        sync_menu_state(&menu_service, &app, &state);
                        app.set_status_left(SharedString::from(format!(
                            "Pivot summary: {groups} groups from {source_name} (live formulas)"
                        )));
                    }
                    Err(hint) => {
                        app.set_status_left(SharedString::from(hint));
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_rename_sheet(move |new_name| {
            if let Some(app) = app_ref.upgrade() {
                let trimmed = new_name.trim().to_string();
                sync_current_to_tabs(&state);
                let active_idx = *state.active_sheet_index.borrow();
                let after_sheets = state.sheets.borrow().clone();
                if active_idx >= after_sheets.len() {
                    return;
                }
                let siblings: Vec<&str> = after_sheets
                    .iter()
                    .enumerate()
                    .filter(|(index, _)| *index != active_idx)
                    .map(|(_, sheet)| sheet.name.as_str())
                    .collect();
                if let Err(reason) =
                    loom_sheets_core::refs::validate_sheet_name(&trimmed, &siblings)
                {
                    app.set_status_left(SharedString::from(reason));
                    return;
                }
                let old_name = after_sheets[active_idx].name.clone();
                let mut after_sheets = after_sheets;
                after_sheets[active_idx].name = trimmed.clone();
                // Keep cross-sheet references pointing at the renamed tab.
                for sheet in &mut after_sheets {
                    for cell in sheet.cells.values_mut() {
                        if cell.is_formula() {
                            cell.raw = loom_sheets_core::refs::rename_sheet_in_formula(
                                &cell.raw, &old_name, &trimmed,
                            );
                        }
                    }
                }
                commit_workbook_transaction(&state, after_sheets, active_idx, None);
                apply_sheet(&app, &state);
                sync_sheet_tabs(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left(SharedString::from(format!("Renamed sheet to {trimmed}")));
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
                    apply_sheet(&app, &state);
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
                let fmt = match fmt_idx {
                    1 => NumberFormat::Currency,
                    2 => NumberFormat::Percentage,
                    3 => NumberFormat::Number,
                    _ => NumberFormat::General,
                };
                let sel = selection_from_app(&app);
                if set_selection_number_format(
                    &mut state.current.borrow_mut(),
                    &mut state.undo_stack.borrow_mut(),
                    &mut state.redo_stack.borrow_mut(),
                    sel.range(),
                    fmt,
                ) {
                    app.set_cell_format(fmt_idx);
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!(
                        "Set number format to {fmt:?}"
                    )));
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
                    apply_sheet(&app, &state);
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
                    apply_sheet(&app, &state);
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
                    apply_sheet(&app, &state);
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
                        apply_sheet(&app, &state);
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
                let vals = evaluate_current(&state);
                update_selection_range(&app, &sheet, &vals, sel);
                project_current(&app, &state);
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
                        apply_sheet(&app, &state);
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
                        apply_sheet(&app, &state);
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
                    apply_sheet(&app, &state);
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
                    apply_sheet(&app, &state);
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
                apply_sheet(&app, &state);
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
                apply_sheet(&app, &state);
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
                    apply_sheet(&app, &state);
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
                    apply_sheet(&app, &state);
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
    for (cell, style) in &sheet.styles {
        if cell.row < target_row {
            new_sheet.styles.insert(*cell, *style);
        } else if cell.row > target_row {
            new_sheet.styles.insert(
                CellRef {
                    row: cell.row - 1,
                    col: cell.col,
                },
                *style,
            );
        }
    }
    new_sheet.objects = sheet
        .objects
        .iter()
        .filter_map(|object| object.after_deleted_row(target_row))
        .collect();
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
    for (cell, style) in &sheet.styles {
        if cell.col < target_col {
            new_sheet.styles.insert(*cell, *style);
        } else if cell.col > target_col {
            new_sheet.styles.insert(
                CellRef {
                    row: cell.row,
                    col: cell.col - 1,
                },
                *style,
            );
        }
    }
    new_sheet.objects = sheet
        .objects
        .iter()
        .filter_map(|object| object.after_deleted_col(target_col))
        .collect();
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
        let mut after = model.sheet;
        after.objects = before.objects.clone();
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
    let mut after_sheets = state.sheets.borrow().clone();
    after_sheets.remove(active_idx);
    let new_idx = active_idx.min(after_sheets.len() - 1);
    commit_workbook_transaction(state, after_sheets, new_idx, Some(active_idx));
    sync_sheet_tabs(app, state);
    apply_sheet(app, state);
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
        "app.palette" | "help.shortcuts" => app.invoke_open_palette(),
        "view.inspector" => app.invoke_toggle_inspector(),
        "view.zoom_in" => app.invoke_zoom_in(),
        "view.zoom_out" => app.invoke_zoom_out(),
        "view.zoom_actual" => app.invoke_zoom_actual(),
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
        "format.bold" | "sheets.bold" => app.invoke_toggle_bold(),
        "format.italic" | "sheets.italic" => app.invoke_toggle_italic(),
        "format.underline" | "sheets.underline" => app.invoke_toggle_underline(),
        "format.align_left" | "sheets.align-left" => app.invoke_set_cell_alignment(0),
        "format.align_center" | "sheets.align-center" => app.invoke_set_cell_alignment(1),
        "format.align_right" | "sheets.align-right" => app.invoke_set_cell_alignment(2),
        "format.general" => app.invoke_set_cell_format(0),
        "format.currency" => app.invoke_set_cell_format(1),
        "format.percentage" => app.invoke_set_cell_format(2),
        "format.number" => app.invoke_set_cell_format(3),
        "table.adjust_decimals_increase" => app.invoke_adjust_decimals(1),
        "table.adjust_decimals_decrease" => app.invoke_adjust_decimals(-1),
        "format.borders" | "sheets.borders" => app.invoke_toggle_borders(),
        "format.fill_cycle" | "sheets.fill-cycle" => app.invoke_cycle_fill(),
        "format.font_increase" => app.invoke_adjust_font(1),
        "format.font_decrease" => app.invoke_adjust_font(-1),
        "table.fill_down" => app.invoke_fill_selection(),
        "sheets.insert_chart" => app.invoke_insert_chart(),
        "sheets.insert_shape" => app.invoke_insert_shape(),
        "sheets.insert_image" => app.invoke_insert_image(),
        "sheets.cycle_chart_kind" => app.invoke_cycle_chart_kind(),
        "table.pivot_sum" => app.invoke_pivot_summary(0),
        "table.pivot_count" => app.invoke_pivot_summary(1),
        "table.pivot_average" => app.invoke_pivot_summary(2),
        "table.pivot_min" => app.invoke_pivot_summary(3),
        "table.pivot_max" => app.invoke_pivot_summary(4),
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
