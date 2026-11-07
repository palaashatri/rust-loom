//! Exact numeric entry for cell style and sheet geometry, alongside steppers.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::CellRef;
use slint::{ComponentHandle, SharedString};

use crate::formatting::{
    adjust_selection_font_size, set_selection_decimal_places_exact, set_selection_font_size,
};
use crate::{
    apply_sheet, commit_transaction, selection_from_app, sync_menu_state, GuiState,
    SheetTransaction, SheetsApp,
};

const FONT_MIN: u8 = 9;
const FONT_MAX: u8 = 32;
const ROW_MIN: f32 = 16.0;
const ROW_MAX: f32 = 160.0;
const COLUMN_MIN: f32 = 32.0;
const COLUMN_MAX: f32 = 400.0;

pub(super) fn register(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    register_font_actions(app, state, menu_service);
    register_geometry_actions(app, state, menu_service);
    register_decimal_action(app, state, menu_service);
}

fn register_decimal_action(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    let state = state.clone();
    let app_ref = app.as_weak();
    let menu_service = menu_service.clone();
    app.global::<crate::SheetInspectorActions>()
        .on_set_decimals(move |input| {
            let Some(app) = app_ref.upgrade() else {
                return false;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return false;
            }
            let Some(decimals) = parse_decimal_places(input.as_str()) else {
                app.set_status_left("Enter a whole number of decimal places from 0 to 10.".into());
                return false;
            };
            let selection = selection_from_app(&app);
            if set_selection_decimal_places_exact(
                &mut state.current.borrow_mut(),
                &mut state.undo_stack.borrow_mut(),
                &mut state.redo_stack.borrow_mut(),
                selection.range(),
                decimals,
            ) {
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
            }
            app.set_status_left(SharedString::from(format!("Decimals {decimals}")));
            true
        });
}

fn register_font_actions(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_adjust_font(move |delta| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return;
            }
            let selection = selection_from_app(&app);
            if adjust_selection_font_size(
                &mut state.current.borrow_mut(),
                &mut state.undo_stack.borrow_mut(),
                &mut state.redo_stack.borrow_mut(),
                selection.range(),
                delta,
            ) {
                let size = crate::formatting::effective_font_size(
                    state.current.borrow().cell_style(selection.anchor),
                );
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left(SharedString::from(format!("Font size {size} px")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_set_font_size(move |input| {
            let Some(app) = app_ref.upgrade() else {
                return false;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return false;
            }
            let Some(size) = parse_font_size(input.as_str()) else {
                app.set_status_left("Enter a whole font size from 9 to 32 px".into());
                return false;
            };
            let selection = selection_from_app(&app);
            if set_selection_font_size(
                &mut state.current.borrow_mut(),
                &mut state.undo_stack.borrow_mut(),
                &mut state.redo_stack.borrow_mut(),
                selection.range(),
                size,
            ) {
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
            }
            app.set_status_left(SharedString::from(format!("Font size {size} px")));
            true
        });
    }
}

fn register_geometry_actions(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_adjust_row_height(move |delta| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return;
            }
            let Some(cell) = selected_cell(&app) else {
                return;
            };
            let before = state.current.borrow().clone();
            let previous = before.row_height(cell.row);
            let height = (previous + delta as f32).clamp(ROW_MIN, ROW_MAX);
            if height == previous {
                app.set_status_left(SharedString::from(format!(
                    "Row {} height: {height} px",
                    cell.row + 1
                )));
                return;
            }
            let mut after = before.clone();
            after.set_row_height(cell.row, height);
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
                "Row {} height: {height} px",
                cell.row + 1
            )));
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_adjust_col_width(move |delta| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return;
            }
            let Some(cell) = selected_cell(&app) else {
                return;
            };
            let before = state.current.borrow().clone();
            let previous = before.col_width(cell.col);
            let width = (previous + delta as f32).clamp(COLUMN_MIN, COLUMN_MAX);
            if width == previous {
                app.set_status_left(SharedString::from(format!("Column width: {width} px")));
                return;
            }
            let mut after = before.clone();
            after.set_col_width(cell.col, width);
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
            app.set_status_left(SharedString::from(format!("Column width: {width} px")));
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_set_row_height(move |input| {
            let Some(app) = app_ref.upgrade() else {
                return false;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return false;
            }
            let Some(height) = parse_dimension(input.as_str(), ROW_MIN, ROW_MAX) else {
                app.set_status_left("Enter a row height from 16 to 160 px".into());
                return false;
            };
            set_geometry(&app, &state, &menu_service, Geometry::Row, height)
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_set_col_width(move |input| {
            let Some(app) = app_ref.upgrade() else {
                return false;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return false;
            }
            let Some(width) = parse_dimension(input.as_str(), COLUMN_MIN, COLUMN_MAX) else {
                app.set_status_left("Enter a column width from 32 to 400 px".into());
                return false;
            };
            set_geometry(&app, &state, &menu_service, Geometry::Column, width)
        });
    }
}

#[derive(Clone, Copy)]
enum Geometry {
    Row,
    Column,
}

fn set_geometry(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
    geometry: Geometry,
    value: f32,
) -> bool {
    let Some(cell) = selected_cell(app) else {
        app.set_status_left("Select a cell before editing worksheet geometry".into());
        return false;
    };
    let before = state.current.borrow().clone();
    let old_value = match geometry {
        Geometry::Row => before.row_height(cell.row),
        Geometry::Column => before.col_width(cell.col),
    };
    if old_value != value {
        let mut after = before.clone();
        match geometry {
            Geometry::Row => after.set_row_height(cell.row, value),
            Geometry::Column => after.set_col_width(cell.col, value),
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
        apply_sheet(app, state);
        sync_menu_state(menu_service, app, state);
    }
    let status = match geometry {
        Geometry::Row => format!("Row {} height: {value} px", cell.row + 1),
        Geometry::Column => format!("Column width: {value} px"),
    };
    app.set_status_left(SharedString::from(status));
    true
}

fn selected_cell(app: &SheetsApp) -> Option<CellRef> {
    let selected = app.get_selected_cell();
    CellRef::parse(selected.as_str())
}

fn parse_font_size(input: &str) -> Option<u8> {
    let size = input.trim().parse::<u8>().ok()?;
    (FONT_MIN..=FONT_MAX).contains(&size).then_some(size)
}

fn parse_decimal_places(input: &str) -> Option<u8> {
    let places = input.trim().parse::<u8>().ok()?;
    (places <= 10).then_some(places)
}

fn parse_dimension(input: &str, minimum: f32, maximum: f32) -> Option<f32> {
    let value = input.trim().parse::<f32>().ok()?;
    (value.is_finite() && (minimum..=maximum).contains(&value)).then_some(value)
}
