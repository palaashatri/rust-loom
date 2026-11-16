//! Interactive Pivot Table Field Builder controller and Slint bindings.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{
    build_pivot_sheet, compute_pivot_table, detect_headers, extract_unique_values,
    plan_pivot_updates, Cell, CellRange, CellRef, CellStyle, PivotAggregation, PivotFieldConfig,
    Sheet,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::analysis::unique_sheet_name;
use crate::{
    apply_sheet, commit_transaction, commit_workbook_transaction, selection_from_app,
    sync_current_to_tabs, sync_menu_state, sync_sheet_tabs, GuiState, PivotBuilderState,
    PivotFieldItem, PivotPreviewRow, SheetTransaction, SheetsApp,
};

#[derive(Clone)]
struct PivotBuilderSession {
    config: PivotFieldConfig,
    headers: Vec<String>,
    filter_values: Vec<String>,
    filter_value_idx: usize,
    /// The destination text typed by the user is not a valid cell reference.
    dest_invalid: bool,
    /// The published field rows. Toggling a role updates these rows in place
    /// instead of replacing the model, so the focused role button survives.
    fields: Rc<VecModel<PivotFieldItem>>,
}

impl Default for PivotBuilderSession {
    fn default() -> Self {
        Self {
            config: PivotFieldConfig::default(),
            headers: Vec::new(),
            filter_values: vec!["(All)".to_string()],
            filter_value_idx: 0,
            dest_invalid: false,
            fields: Rc::new(VecModel::default()),
        }
    }
}

fn agg_to_int(agg: PivotAggregation) -> i32 {
    match agg {
        PivotAggregation::Sum => 0,
        PivotAggregation::Count => 1,
        PivotAggregation::Average => 2,
        PivotAggregation::Min => 3,
        PivotAggregation::Max => 4,
    }
}

fn cycle_agg(agg: PivotAggregation) -> PivotAggregation {
    match agg {
        PivotAggregation::Sum => PivotAggregation::Count,
        PivotAggregation::Count => PivotAggregation::Average,
        PivotAggregation::Average => PivotAggregation::Min,
        PivotAggregation::Min => PivotAggregation::Max,
        PivotAggregation::Max => PivotAggregation::Sum,
    }
}

fn detect_source_range(app: &SheetsApp, sheet: &Sheet) -> CellRange {
    let sel = selection_from_app(app).range();
    if sel.end.row > sel.start.row {
        return sel;
    }
    let dims = sheet.dimensions();
    if dims.rows >= 2 && dims.cols >= 1 {
        CellRange::new(
            CellRef { row: 0, col: 0 },
            CellRef {
                row: dims.rows - 1,
                col: dims.cols - 1,
            },
        )
    } else {
        CellRange::new(CellRef { row: 0, col: 0 }, CellRef { row: 0, col: 0 })
    }
}

fn init_session_for_range(sheet: &Sheet, range: CellRange) -> PivotBuilderSession {
    let headers = detect_headers(sheet, range);
    let row_fields = if headers.is_empty() {
        Vec::new()
    } else {
        vec![0]
    };
    let value_fields = if headers.len() >= 2 {
        vec![(1, PivotAggregation::Sum)]
    } else if !headers.is_empty() {
        vec![(0, PivotAggregation::Count)]
    } else {
        Vec::new()
    };
    PivotBuilderSession {
        config: PivotFieldConfig {
            source_range: range,
            row_fields,
            col_field: None,
            value_fields,
            filter_field: None,
            destination_new_sheet: true,
            destination_cell: Some(CellRef { row: 0, col: 0 }),
            title: format!("Pivot of {}", sheet.name),
        },
        headers,
        filter_values: vec!["(All)".to_string()],
        filter_value_idx: 0,
        dest_invalid: false,
        fields: Rc::new(VecModel::default()),
    }
}

const DEST_INVALID_MSG: &str = "Destination must be a valid cell reference such as A1.";
const DEST_OVERLAP_MSG: &str =
    "Pivot output would overwrite its source range. Choose a destination outside the source range.";

/// Whether an output block of `rows` x `cols` anchored at `anchor` touches `source`.
fn block_overlaps_source(source: CellRange, anchor: CellRef, rows: u32, cols: u32) -> bool {
    let end = CellRef {
        row: anchor.row.saturating_add(rows.saturating_sub(1)),
        col: anchor.col.saturating_add(cols.saturating_sub(1)),
    };
    source.intersects(CellRange::new(anchor, end))
}

/// Builds one undoable transaction covering both pivot values and styles.
fn pivot_snapshot_transaction(
    sheet: &Sheet,
    updates: &[(CellRef, Cell, bool)],
) -> SheetTransaction {
    let mut after = sheet.clone();
    for (cell, value, is_bold) in updates {
        if value.raw.is_empty() {
            after.clear_cell(*cell);
        } else if value.text {
            after.set_literal(*cell, &value.raw);
        } else {
            after.set_raw(*cell, &value.raw);
        }
        if *is_bold {
            let style = CellStyle {
                bold: true,
                ..after.cell_style(*cell)
            };
            after.set_cell_style(*cell, style);
        }
    }
    SheetTransaction::Snapshot {
        before: Box::new(sheet.clone()),
        after: Box::new(after),
    }
}

fn publish_session(app: &SheetsApp, sheet: &Sheet, session: &PivotBuilderSession) {
    let pb = app.global::<PivotBuilderState>();
    let range = session.config.source_range;
    pb.set_source_range(SharedString::from(format!(
        "{}:{}",
        range.start.to_a1(),
        range.end.to_a1()
    )));
    pb.set_title(SharedString::from(&session.config.title));
    pb.set_dest_new_sheet(session.config.destination_new_sheet);
    let dest = session
        .config
        .destination_cell
        .unwrap_or(CellRef { row: 0, col: 0 });
    // Never overwrite text the user is still typing (e.g. an invalid cell).
    if !session.dest_invalid && CellRef::parse(pb.get_dest_cell().as_str()) != Some(dest) {
        pb.set_dest_cell(SharedString::from(dest.to_a1()));
    }

    let items: Vec<PivotFieldItem> = session
        .headers
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            let val_entry = session
                .config
                .value_fields
                .iter()
                .find(|(vf, _)| *vf == idx);
            PivotFieldItem {
                index: idx as i32,
                name: SharedString::from(name),
                is_row: session.config.row_fields.contains(&idx),
                is_col: session.config.col_field == Some(idx),
                is_val: val_entry.is_some(),
                val_agg: val_entry.map(|(_, agg)| agg_to_int(*agg)).unwrap_or(0),
                is_filter: session
                    .config
                    .filter_field
                    .as_ref()
                    .is_some_and(|(ff, _)| *ff == idx),
            }
        })
        .collect();
    // Update existing rows in place when the field set is unchanged. Replacing
    // the whole model would re-create every row and drop focus from the role
    // button the user just activated with the keyboard.
    if session.fields.row_count() == items.len() {
        for (row, item) in items.into_iter().enumerate() {
            if session.fields.row_data(row).as_ref() != Some(&item) {
                session.fields.set_row_data(row, item);
            }
        }
    } else {
        session.fields.set_vec(items);
    }
    pb.set_fields(ModelRc::from(session.fields.clone()));

    pb.set_filter_field_idx(
        session
            .config
            .filter_field
            .as_ref()
            .map(|(f, _)| *f as i32)
            .unwrap_or(-1),
    );
    let fvals: Vec<SharedString> = session
        .filter_values
        .iter()
        .map(SharedString::from)
        .collect();
    pb.set_filter_values(ModelRc::new(VecModel::from(fvals)));
    pb.set_filter_value_idx(session.filter_value_idx as i32);

    match compute_pivot_table(sheet, &session.config) {
        Ok(result) => {
            let preview: Vec<PivotPreviewRow> = result
                .rows
                .iter()
                .take(50)
                .map(|row| {
                    let is_header = row.first().is_some_and(|c| c.is_header);
                    let is_total = row.first().is_some_and(|c| c.is_total);
                    let cells: Vec<SharedString> =
                        row.iter().map(|c| SharedString::from(&c.display)).collect();
                    PivotPreviewRow {
                        cells: ModelRc::new(VecModel::from(cells)),
                        is_header,
                        is_total,
                    }
                })
                .collect();
            pb.set_preview_rows(ModelRc::new(VecModel::from(preview)));
            let dest_error = if session.config.destination_new_sheet {
                None
            } else if session.dest_invalid {
                Some(DEST_INVALID_MSG)
            } else if block_overlaps_source(
                session.config.source_range,
                dest,
                result.row_count as u32,
                result.col_count as u32,
            ) {
                Some(DEST_OVERLAP_MSG)
            } else {
                None
            };
            pb.set_error_text(SharedString::from(dest_error.unwrap_or_default()));
            pb.set_summary_text(SharedString::from(format!(
                "{} rows × {} cols",
                result.row_count, result.col_count
            )));
        }
        Err(err) => {
            pb.set_preview_rows(ModelRc::new(VecModel::default()));
            pb.set_error_text(SharedString::from(err));
            pb.set_summary_text(SharedString::default());
        }
    }
}

/// Wires all `PivotBuilderState` Slint callbacks for the Pivot Table Field Builder.
pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    let session = Rc::new(RefCell::new(PivotBuilderSession::default()));
    let pb = app.global::<PivotBuilderState>();

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_open_builder(move || {
            if let Some(app) = app_ref.upgrade() {
                let sheet = state.current.borrow();
                let range = detect_source_range(&app, &sheet);
                let next = init_session_for_range(&sheet, range);
                *session.borrow_mut() = next;
                publish_session(&app, &sheet, &session.borrow());
                app.global::<PivotBuilderState>().set_open(true);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        pb.on_close_builder(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<PivotBuilderState>().set_open(false);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_toggle_row_field(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let idx = idx as usize;
                let mut sess = session.borrow_mut();
                if let Some(pos) = sess.config.row_fields.iter().position(|&r| r == idx) {
                    sess.config.row_fields.remove(pos);
                } else {
                    sess.config.row_fields.push(idx);
                    if sess.config.col_field == Some(idx) {
                        sess.config.col_field = None;
                    }
                }
                publish_session(&app, &state.current.borrow(), &sess);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_toggle_col_field(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let idx = idx as usize;
                let mut sess = session.borrow_mut();
                if sess.config.col_field == Some(idx) {
                    sess.config.col_field = None;
                } else {
                    sess.config.col_field = Some(idx);
                    sess.config.row_fields.retain(|&r| r != idx);
                }
                publish_session(&app, &state.current.borrow(), &sess);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_toggle_val_field(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let idx = idx as usize;
                let mut sess = session.borrow_mut();
                if let Some(pos) = sess.config.value_fields.iter().position(|&(v, _)| v == idx) {
                    sess.config.value_fields.remove(pos);
                } else {
                    sess.config.value_fields.push((idx, PivotAggregation::Sum));
                }
                publish_session(&app, &state.current.borrow(), &sess);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_cycle_val_agg(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let idx = idx as usize;
                let mut sess = session.borrow_mut();
                if let Some((_, agg)) = sess.config.value_fields.iter_mut().find(|(v, _)| *v == idx)
                {
                    *agg = cycle_agg(*agg);
                }
                publish_session(&app, &state.current.borrow(), &sess);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_select_filter_field(move |idx| {
            if let Some(app) = app_ref.upgrade() {
                let idx = idx as usize;
                let sheet = state.current.borrow();
                let mut sess = session.borrow_mut();
                if sess
                    .config
                    .filter_field
                    .as_ref()
                    .is_some_and(|(f, _)| *f == idx)
                {
                    sess.config.filter_field = None;
                    sess.filter_values = vec!["(All)".to_string()];
                    sess.filter_value_idx = 0;
                } else {
                    let mut vals = vec!["(All)".to_string()];
                    vals.extend(extract_unique_values(&sheet, sess.config.source_range, idx));
                    sess.filter_values = vals;
                    sess.filter_value_idx = 0;
                    sess.config.filter_field = Some((idx, "(All)".to_string()));
                }
                publish_session(&app, &sheet, &sess);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_select_filter_value(move |val_idx| {
            if let Some(app) = app_ref.upgrade() {
                let val_idx = val_idx as usize;
                let mut sess = session.borrow_mut();
                if let Some(chosen) = sess.filter_values.get(val_idx).cloned() {
                    sess.filter_value_idx = val_idx;
                    if let Some((f, _)) = sess.config.filter_field {
                        sess.config.filter_field = Some((f, chosen));
                    }
                }
                publish_session(&app, &state.current.borrow(), &sess);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_set_dest_new_sheet(move |is_new| {
            if let Some(app) = app_ref.upgrade() {
                let mut sess = session.borrow_mut();
                sess.config.destination_new_sheet = is_new;
                publish_session(&app, &state.current.borrow(), &sess);
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_set_dest_cell(move |text| {
            if let Some(app) = app_ref.upgrade() {
                let mut sess = session.borrow_mut();
                match CellRef::parse(text.as_str()) {
                    Some(cell) => {
                        sess.config.destination_cell = Some(cell);
                        sess.dest_invalid = false;
                    }
                    None => sess.dest_invalid = true,
                }
                publish_session(&app, &state.current.borrow(), &sess);
            }
        });
    }

    {
        let session = session.clone();
        pb.on_set_title(move |text| {
            session.borrow_mut().config.title = text.to_string();
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        pb.on_set_source_range(move |text| {
            if let Some(app) = app_ref.upgrade() {
                if let Some(range) = CellRange::parse(text.as_str()) {
                    let sheet = state.current.borrow();
                    let next = init_session_for_range(&sheet, range);
                    *session.borrow_mut() = next;
                    publish_session(&app, &sheet, &session.borrow());
                }
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let session = session.clone();
        let menu_service = menu_service.clone();
        pb.on_apply_pivot_builder(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                crate::object_actions::cancel_active_gesture(&app, &state);
                let (cfg, dest_invalid) = {
                    let sess = session.borrow();
                    (sess.config.clone(), sess.dest_invalid)
                };
                if !cfg.destination_new_sheet && dest_invalid {
                    app.global::<PivotBuilderState>()
                        .set_error_text(SharedString::from(DEST_INVALID_MSG));
                    return;
                }
                if cfg.destination_new_sheet {
                    let built = {
                        let current = state.current.borrow();
                        build_pivot_sheet(&current, &cfg)
                    };
                    match built {
                        Ok(mut pivot) => {
                            sync_current_to_tabs(&state);
                            let mut after_sheets = state.sheets.borrow().clone();
                            pivot.name = unique_sheet_name(&pivot.name, &after_sheets);
                            let target_name = pivot.name.clone();
                            after_sheets.push(pivot);
                            let new_idx = state.sheets.borrow().len();
                            commit_workbook_transaction(&state, after_sheets, new_idx, None);
                            apply_sheet(&app, &state);
                            sync_sheet_tabs(&app, &state);
                            sync_menu_state(&menu_service, &app, &state);
                            app.global::<PivotBuilderState>().set_open(false);
                            app.set_status_left(SharedString::from(format!(
                                "Created pivot table on sheet \"{target_name}\""
                            )));
                        }
                        Err(err) => {
                            app.global::<PivotBuilderState>()
                                .set_error_text(SharedString::from(err));
                        }
                    }
                } else {
                    let anchor = cfg.destination_cell.unwrap_or(CellRef { row: 0, col: 0 });
                    let planned = {
                        let current = state.current.borrow();
                        plan_pivot_updates(&current, &cfg, anchor, false)
                    };
                    match planned {
                        Ok(updates) => {
                            if updates
                                .iter()
                                .any(|(cell, _, _)| cfg.source_range.contains(*cell))
                            {
                                app.global::<PivotBuilderState>()
                                    .set_error_text(SharedString::from(DEST_OVERLAP_MSG));
                                return;
                            }
                            let mut sheet = state.current.borrow_mut();
                            let tx = pivot_snapshot_transaction(&sheet, &updates);
                            let mut undo = state.undo_stack.borrow_mut();
                            let mut redo = state.redo_stack.borrow_mut();
                            commit_transaction(&mut sheet, &mut undo, &mut redo, tx);
                            drop(undo);
                            drop(redo);
                            drop(sheet);
                            apply_sheet(&app, &state);
                            sync_menu_state(&menu_service, &app, &state);
                            app.global::<PivotBuilderState>().set_open(false);
                            app.set_status_left(SharedString::from(format!(
                                "Created pivot table at {}",
                                anchor.to_a1()
                            )));
                        }
                        Err(err) => {
                            app.global::<PivotBuilderState>()
                                .set_error_text(SharedString::from(err));
                        }
                    }
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlap_detects_source_collisions() {
        let src = CellRange::parse("A1:D5").unwrap();
        let at = |a: &str| CellRef::parse(a).unwrap();
        assert!(block_overlaps_source(src, at("A1"), 3, 2));
        assert!(block_overlaps_source(src, at("D5"), 1, 1));
        assert!(!block_overlaps_source(src, at("F1"), 1, 1));
        assert!(block_overlaps_source(src, at("C4"), 4, 4));
        assert!(!block_overlaps_source(src, at("F1"), 5, 3));
    }

    #[test]
    fn pivot_transaction_undoes_values_and_styles() {
        let mut sheet = Sheet::new("S");
        sheet.set_str("F1", "keep");
        let before_style = sheet.cell_style(CellRef::parse("F1").unwrap());
        let updates = vec![
            (CellRef::parse("F1").unwrap(), Cell::new("Region"), true),
            (CellRef::parse("G1").unwrap(), Cell::new("1"), false),
        ];
        let tx = pivot_snapshot_transaction(&sheet, &updates);
        tx.apply(&mut sheet);
        assert!(sheet.cell_style(CellRef::parse("F1").unwrap()).bold);
        assert_eq!(sheet.raw(CellRef::parse("F1").unwrap()), Some("Region"));
        tx.revert(&mut sheet);
        assert_eq!(sheet.raw(CellRef::parse("F1").unwrap()), Some("keep"));
        assert_eq!(
            sheet.cell_style(CellRef::parse("F1").unwrap()),
            before_style
        );
        assert_eq!(sheet.raw(CellRef::parse("G1").unwrap()), None);
    }
}
