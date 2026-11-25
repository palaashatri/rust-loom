//! Multi-table freeform canvas projection, commands, and inspector wiring.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::sheet_tables::{insert_table, table_at, TableStylePreset};
use loom_sheets_core::{CellRef, GridSelection, Sheet, SheetViewport};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{
    apply_sheet, push_history, sync_menu_state, GuiState, SheetTableState, SheetTableView,
    SheetTransaction, SheetsApp, TableBandView,
};

fn active_cell_coords(app: &SheetsApp) -> (u32, u32) {
    CellRef::parse(app.get_selected_cell().as_str())
        .map(|cell| (cell.row, cell.col))
        .unwrap_or((0, 0))
}

fn col_x_and_width(sheet: &Sheet, viewport: SheetViewport, col: u32, zoom: f32) -> (f32, f32) {
    let mut x = 0.0_f32;
    for c in viewport.first_col..col {
        x += sheet.col_width(c) * zoom;
    }
    (x, sheet.col_width(col) * zoom)
}

fn row_y_and_height(sheet: &Sheet, viewport: SheetViewport, row: u32, zoom: f32) -> (f32, f32) {
    let mut y = 0.0_f32;
    for r in viewport.first_row..row {
        if !sheet.hidden_rows.contains(&r) {
            y += sheet.row_height(r) * zoom;
        }
    }
    let h = if sheet.hidden_rows.contains(&row) {
        0.0
    } else {
        sheet.row_height(row) * zoom
    };
    (y, h)
}

/// Project all [`SheetTable`](loom_sheets_core::SheetTable)s on `sheet` into
/// `SheetTableState` for the canvas overlay and Table Inspector.
pub(crate) fn project_tables(
    app: &SheetsApp,
    sheet: &Sheet,
    viewport: SheetViewport,
    selection: GridSelection,
    zoom: f32,
) {
    crate::parity_actions::sync_overlays(app, sheet, viewport, selection.range(), zoom);
    let state = app.global::<SheetTableState>();
    let active_idx = table_at(&sheet.tables, selection.focus.row, selection.focus.col);
    let view_end_row = viewport.first_row + viewport.visible_rows;
    let view_end_col = viewport.first_col + viewport.visible_cols;

    let mut table_views: Vec<SheetTableView> = Vec::new();
    let mut bands: Vec<TableBandView> = Vec::new();

    for (idx, table) in sheet.tables.iter().enumerate() {
        let t_end_row = table.end_row();
        let t_end_col = table.end_col();
        if t_end_row < viewport.first_row
            || table.start_row >= view_end_row
            || t_end_col < viewport.first_col
            || table.start_col >= view_end_col
        {
            continue;
        }

        let vis_start_row = table.start_row.max(viewport.first_row);
        let vis_end_row = t_end_row.min(view_end_row.saturating_sub(1));
        let vis_start_col = table.start_col.max(viewport.first_col);
        let vis_end_col = t_end_col.min(view_end_col.saturating_sub(1));

        let (x, _) = col_x_and_width(sheet, viewport, vis_start_col, zoom);
        let (end_x, end_w) = col_x_and_width(sheet, viewport, vis_end_col, zoom);
        let width = (end_x + end_w - x).max(0.0);

        let (y, _) = row_y_and_height(sheet, viewport, vis_start_row, zoom);
        let (end_y, end_h) = row_y_and_height(sheet, viewport, vis_end_row, zoom);
        let height = (end_y + end_h - y).max(0.0);

        let header_row_height = if table.header_rows > 0 && table.start_row >= viewport.first_row {
            let mut h = 0.0_f32;
            for offset in 0..table.header_rows {
                let r = table.start_row + offset;
                if r <= vis_end_row && !sheet.hidden_rows.contains(&r) {
                    h += sheet.row_height(r) * zoom;
                }
            }
            h
        } else {
            0.0
        };

        let header_col_width = if table.header_cols > 0 && table.start_col >= viewport.first_col {
            let mut w = 0.0_f32;
            for offset in 0..table.header_cols {
                let c = table.start_col + offset;
                if c <= vis_end_col {
                    w += sheet.col_width(c) * zoom;
                }
            }
            w
        } else {
            0.0
        };

        let style_idx = table.style.index();
        if table.alternating_rows && width > 0.0 {
            for r in vis_start_row..=vis_end_row {
                let rel_row = r - table.start_row;
                if rel_row >= table.header_rows && (rel_row - table.header_rows) % 2 == 1 {
                    let (ry, rh) = row_y_and_height(sheet, viewport, r, zoom);
                    if rh > 0.0 {
                        bands.push(TableBandView {
                            x,
                            y: ry,
                            width,
                            height: rh,
                            style_idx,
                        });
                    }
                }
            }
        }

        table_views.push(SheetTableView {
            id: SharedString::from(table.id.as_str()),
            name: SharedString::from(table.name.as_str()),
            range_label: SharedString::from(table.range_label()),
            x,
            y,
            width,
            height,
            header_row_height,
            header_col_width,
            style_idx,
            show_title: table.show_title,
            alternating_rows: table.alternating_rows,
            header_rows: table.header_rows as i32,
            header_cols: table.header_cols as i32,
            active: active_idx == Some(idx),
        });
    }

    state.set_tables(ModelRc::new(VecModel::from(table_views)));
    state.set_alternating_bands(ModelRc::new(VecModel::from(bands)));

    if let Some(idx) = active_idx {
        let t = &sheet.tables[idx];
        state.set_has_active_table(true);
        state.set_active_index(idx as i32);
        state.set_active_name(SharedString::from(t.name.as_str()));
        state.set_active_range(SharedString::from(t.range_label()));
        state.set_active_style(t.style.index());
        state.set_active_show_title(t.show_title);
        state.set_active_alternating_rows(t.alternating_rows);
    } else {
        state.set_has_active_table(false);
        state.set_active_index(-1);
        state.set_active_name(SharedString::default());
        state.set_active_range(SharedString::default());
        state.set_active_style(0);
        state.set_active_show_title(true);
        state.set_active_alternating_rows(true);
    }
}

fn record_table_snapshot(state: &GuiState, before: Sheet, after: Sheet) {
    push_history(
        &mut state.undo_stack.borrow_mut(),
        SheetTransaction::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        },
    );
    state.redo_stack.borrow_mut().clear();
}

/// A1 anchor of the table with stable identity `id` (not a filtered model index).
fn table_anchor_by_id(tables: &[loom_sheets_core::SheetTable], id: &str) -> Option<String> {
    tables.iter().find(|t| t.id == id).map(|t| {
        CellRef {
            row: t.start_row,
            col: t.start_col,
        }
        .to_a1()
    })
}

/// Insert a new non-overlapping 5x4 canvas table near the active cell.
pub(crate) fn insert_canvas_table(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<NativeMenuBar>,
) {
    let (row, col) = active_cell_coords(app);
    let before = state.current.borrow().clone();
    let created = {
        let mut sheet = state.current.borrow_mut();
        insert_table(&mut sheet.tables, row, col, 5, 4)
    };
    let Some(created) = created else {
        app.set_status_left("No free space to insert a table".into());
        return;
    };
    let after = state.current.borrow().clone();
    record_table_snapshot(state, before, after);
    let anchor = CellRef {
        row: created.start_row,
        col: created.start_col,
    };
    app.set_selected_cell(SharedString::from(anchor.to_a1()));
    app.set_selection_range(SharedString::from(anchor.to_a1()));
    state.mark_content_dirty();
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(format!(
        "Inserted {} ({})",
        created.name,
        created.range_label()
    )));
}

fn mutate_active_table<F>(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<NativeMenuBar>,
    status_label: &str,
    mutate: F,
) where
    F: FnOnce(&mut loom_sheets_core::SheetTable),
{
    let (row, col) = active_cell_coords(app);
    let idx = {
        let sheet = state.current.borrow();
        table_at(&sheet.tables, row, col)
    };
    let Some(idx) = idx else {
        app.set_status_left("Select a cell inside a canvas table first".into());
        return;
    };
    let before = state.current.borrow().clone();
    {
        let mut sheet = state.current.borrow_mut();
        if let Some(table) = sheet.tables.get_mut(idx) {
            mutate(table);
        }
    }
    let after = state.current.borrow().clone();
    record_table_snapshot(state, before, after);
    state.mark_content_dirty();
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(status_label));
}

pub(crate) fn delete_active_table(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<NativeMenuBar>,
) {
    let (row, col) = active_cell_coords(app);
    let idx = {
        let sheet = state.current.borrow();
        table_at(&sheet.tables, row, col)
    };
    let Some(idx) = idx else {
        app.set_status_left("Select a cell inside a canvas table first".into());
        return;
    };
    let before = state.current.borrow().clone();
    let removed_name = {
        let mut sheet = state.current.borrow_mut();
        sheet.tables.remove(idx).name
    };
    let after = state.current.borrow().clone();
    record_table_snapshot(state, before, after);
    state.mark_content_dirty();
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(format!("Deleted {removed_name}")));
}

/// Wire `SheetTableState` callbacks for canvas table operations.
pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    let table_state = app.global::<SheetTableState>();

    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        let menu_service = Arc::clone(menu_service);
        table_state.on_insert_table(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                insert_canvas_table(&app, &state, &menu_service);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        let menu_service = Arc::clone(menu_service);
        table_state.on_rename_active_table(move |name| {
            let trimmed = name.trim().to_string();
            if trimmed.is_empty() {
                return;
            }
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                mutate_active_table(
                    &app,
                    &state,
                    &menu_service,
                    &format!("Renamed table to {trimmed}"),
                    |table| table.name = trimmed,
                );
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        let menu_service = Arc::clone(menu_service);
        table_state.on_set_active_table_style(move |style_idx| {
            let preset = TableStylePreset::from_index(style_idx);
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                mutate_active_table(
                    &app,
                    &state,
                    &menu_service,
                    &format!("Table style: {}", preset.as_str()),
                    |table| table.style = preset,
                );
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        let menu_service = Arc::clone(menu_service);
        table_state.on_toggle_active_table_title(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                mutate_active_table(
                    &app,
                    &state,
                    &menu_service,
                    "Toggled table title",
                    |table| table.show_title = !table.show_title,
                );
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        let menu_service = Arc::clone(menu_service);
        table_state.on_toggle_active_table_alternating(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                mutate_active_table(
                    &app,
                    &state,
                    &menu_service,
                    "Toggled alternating rows",
                    |table| table.alternating_rows = !table.alternating_rows,
                );
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        let menu_service = Arc::clone(menu_service);
        table_state.on_delete_active_table(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                delete_active_table(&app, &state, &menu_service);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = Rc::clone(state);
        table_state.on_select_table(move |id| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            let anchor = {
                let sheet = state.current.borrow();
                table_anchor_by_id(&sheet.tables, id.as_str())
            };
            if let Some(a1) = anchor {
                app.set_selected_cell(SharedString::from(&a1));
                app.set_selection_range(SharedString::from(&a1));
                crate::project_current(&app, &state);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{register_sheet_actions, start_hub, FileFilter, NativeFileDialogs, StartHubState};
    use loom_test_support::capture::set_platform;

    #[test]
    fn select_resolves_table_identity_not_model_index() {
        let tables = vec![
            loom_sheets_core::SheetTable::new("a", "A", 0, 0, 2, 2),
            loom_sheets_core::SheetTable::new("b", "B", 50, 3, 2, 2),
        ];
        // Table "b" would be model index 0 if "a" were outside the viewport.
        assert_eq!(table_anchor_by_id(&tables, "b").as_deref(), Some("D51"));
        assert_eq!(table_anchor_by_id(&tables, "missing"), None);
    }

    fn test_state() -> Rc<GuiState> {
        let filter = FileFilter::new("Loom", ["looms"]).expect("filter");
        Rc::new(GuiState::new(
            Sheet::new("Sheet 1"),
            None,
            Rc::new(NativeFileDialogs),
            filter.clone(),
            filter.clone(),
            filter.clone(),
            filter,
        ))
    }

    #[test]
    fn canvas_table_insert_style_rename_delete_and_undo_work() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let state = test_state();
        let menu_service = Arc::new(NativeMenuBar::new());
        register_sheet_actions(&app, &state, &menu_service);
        crate::register_history_actions(&app, &state, &menu_service);
        wire(&app, &state, &menu_service);

        let ts = app.global::<SheetTableState>();
        ts.invoke_insert_table();
        assert_eq!(state.current.borrow().tables.len(), 1);
        assert!(ts.get_has_active_table());
        assert_eq!(ts.get_active_name().as_str(), "Table 1");

        ts.invoke_rename_active_table("Revenue".into());
        assert_eq!(state.current.borrow().tables[0].name, "Revenue");

        ts.invoke_set_active_table_style(TableStylePreset::Jade.index());
        assert_eq!(
            state.current.borrow().tables[0].style,
            TableStylePreset::Jade
        );

        ts.invoke_toggle_active_table_title();
        assert!(!state.current.borrow().tables[0].show_title);

        ts.invoke_toggle_active_table_alternating();
        assert!(!state.current.borrow().tables[0].alternating_rows);

        ts.invoke_delete_active_table();
        assert!(state.current.borrow().tables.is_empty());
        assert!(!ts.get_has_active_table());

        // Undo restores the deleted table.
        app.invoke_undo();
        assert_eq!(state.current.borrow().tables.len(), 1);
        assert_eq!(state.current.borrow().tables[0].name, "Revenue");
    }

    #[test]
    fn start_hub_new_document_transitions_to_template_chooser() {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        let state = test_state();
        let menu_service = Arc::new(NativeMenuBar::new());
        start_hub::wire(&app, &state, &menu_service);
        start_hub::open_hub(&app);

        let hub = app.global::<StartHubState>();
        assert!(hub.get_open());
        assert!(!app.get_template_chooser_open());

        hub.invoke_new_document();
        assert!(!hub.get_open());
        assert!(app.get_template_chooser_open());
    }
}
