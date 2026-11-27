//! Wiring for P0 Sheets parity actions (autofill, visibility, format painter, formula audit).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{CellRange, CellRef, Sheet, SheetViewport};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{
    apply_sheet, commit_transaction, sync_menu_state, AuditHighlightRect, GuiState,
    SheetTransaction, SheetsApp,
};

thread_local! {
    static SHOW_FORMULAS: Cell<bool> = const { Cell::new(false) };
    static ACTIVE_AUDIT_CELLS: RefCell<(Vec<CellRef>, bool)> = const { RefCell::new((Vec::new(), true)) };
}

pub(crate) fn show_formulas_enabled() -> bool {
    SHOW_FORMULAS.with(Cell::get)
}

fn vis_col_width(sheet: &Sheet, col: u32, zoom: f32) -> f32 {
    if sheet.hidden_cols.contains(&col) {
        0.0
    } else {
        sheet.col_width(col) * zoom
    }
}

fn vis_row_height(sheet: &Sheet, row: u32, zoom: f32) -> f32 {
    if sheet.hidden_rows.contains(&row) {
        0.0
    } else {
        sheet.row_height(row) * zoom
    }
}

fn cell_rect_in_viewport(
    sheet: &Sheet,
    viewport: SheetViewport,
    zoom: f32,
    cell: CellRef,
) -> Option<(f32, f32, f32, f32)> {
    let col_end = viewport.first_col + viewport.visible_cols.max(1);
    let row_end = viewport.first_row + viewport.visible_rows.max(1);
    if cell.col < viewport.first_col
        || cell.col >= col_end
        || cell.row < viewport.first_row
        || cell.row >= row_end
        || sheet.hidden_cols.contains(&cell.col)
        || sheet.hidden_rows.contains(&cell.row)
    {
        return None;
    }
    let x: f32 = (viewport.first_col..cell.col)
        .map(|c| vis_col_width(sheet, c, zoom))
        .sum();
    let y: f32 = (viewport.first_row..cell.row)
        .map(|r| vis_row_height(sheet, r, zoom))
        .sum();
    let w = vis_col_width(sheet, cell.col, zoom);
    let h = vis_row_height(sheet, cell.row, zoom);
    Some((x, y, w, h))
}

fn hit_test_cell_in_viewport(
    sheet: &Sheet,
    viewport: SheetViewport,
    zoom: f32,
    x: f32,
    y: f32,
) -> CellRef {
    let mut cur_x = 0.0f32;
    let mut hit_col = viewport.first_col;
    let col_end = viewport.first_col + viewport.visible_cols.max(1);
    for c in viewport.first_col..col_end {
        let w = vis_col_width(sheet, c, zoom);
        if w > 0.0 {
            hit_col = c;
            if cur_x + w > x {
                break;
            }
            cur_x += w;
        }
    }
    let mut cur_y = 0.0f32;
    let mut hit_row = viewport.first_row;
    let row_end = viewport.first_row + viewport.visible_rows.max(1);
    for r in viewport.first_row..row_end {
        let h = vis_row_height(sheet, r, zoom);
        if h > 0.0 {
            hit_row = r;
            if cur_y + h > y {
                break;
            }
            cur_y += h;
        }
    }
    CellRef {
        row: hit_row,
        col: hit_col,
    }
}

pub(crate) fn sync_overlays(
    app: &SheetsApp,
    sheet: &Sheet,
    viewport: SheetViewport,
    selection: CellRange,
    zoom: f32,
) {
    let g = app.global::<crate::SheetsParityState>();
    if let Some((x, y, w, h)) = cell_rect_in_viewport(sheet, viewport, zoom, selection.end) {
        g.set_fill_handle_visible(true);
        g.set_fill_handle_x(x + w);
        g.set_fill_handle_y(y + h);
    } else {
        g.set_fill_handle_visible(false);
    }
    let rects: Vec<AuditHighlightRect> = ACTIVE_AUDIT_CELLS.with(|cells| {
        let (list, is_precedent) = &*cells.borrow();
        list.iter()
            .filter_map(|&c| cell_rect_in_viewport(sheet, viewport, zoom, c))
            .map(|(x, y, w, h)| AuditHighlightRect {
                x,
                y,
                w,
                h,
                is_precedent: *is_precedent,
            })
            .collect()
    });
    g.set_audit_highlights(ModelRc::new(VecModel::from(rects)));
}

/// Wires P0 parity callbacks on `SheetsParityState`.
pub(crate) fn register_parity_actions(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    let parity = app.global::<crate::SheetsParityState>();
    let fp = Rc::new(RefCell::new(
        crate::format_painter::SheetsFormatPainter::new(),
    ));
    let audit = Rc::new(RefCell::new(
        crate::formula_audit::AuditOverlayState::default(),
    ));
    let fill_state = Rc::new(RefCell::new(crate::fill_handle::FillHandleState::default()));

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let fill_state = fill_state.clone();
        parity.on_fill_drag_started(move |_x, _y| {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let sel = crate::selection_from_app(&app).range();
                fill_state.borrow_mut().begin_drag(sel);
                app.global::<crate::SheetsParityState>()
                    .set_fill_dragging(true);
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let fill_state = fill_state.clone();
        parity.on_fill_drag_moved(move |x, y| {
            if let Some(app) = app_ref.upgrade() {
                let g = app.global::<crate::SheetsParityState>();
                if !g.get_fill_dragging() {
                    return;
                }
                let sheet = state.current.borrow();
                let zoom = crate::grid_zoom(&app);
                let vp = crate::viewport_from_app(&app, &sheet, None);
                let hovered = hit_test_cell_in_viewport(&sheet, vp, zoom, x, y);
                fill_state.borrow_mut().update_drag(hovered);
                if let Some(target) = fill_state.borrow().target_range {
                    if let (Some((px, py, _, _)), Some((ex, ey, ew, eh))) = (
                        cell_rect_in_viewport(&sheet, vp, zoom, target.start),
                        cell_rect_in_viewport(&sheet, vp, zoom, target.end),
                    ) {
                        g.set_fill_preview_x(px);
                        g.set_fill_preview_y(py);
                        g.set_fill_preview_w((ex + ew - px).max(0.0));
                        g.set_fill_preview_h((ey + eh - py).max(0.0));
                    }
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        let fill_state = fill_state.clone();
        parity.on_fill_drag_ended(move |_x, _y| {
            if let Some(app) = app_ref.upgrade() {
                let g = app.global::<crate::SheetsParityState>();
                g.set_fill_dragging(false);
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                if let Some((source, target)) = fill_state.borrow_mut().finish_drag() {
                    let before = state.current.borrow().clone();
                    let mut after = before.clone();
                    let _undo = crate::fill_handle::apply_fill_range(&mut after, source, target);
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
                    app.set_status_left("Auto-filled range".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        parity.on_autofill_series(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let sel = crate::selection_from_app(&app).range();
                let before = state.current.borrow().clone();
                let mut after = before.clone();
                if crate::fill_handle::apply_double_click_fill(&mut after, sel).is_some() {
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
                    app.set_status_left("Auto-filled series".into());
                }
            }
        });
    }

    macro_rules! wire_vis {
        ($cb:ident, $fn_name:path, $msg:expr) => {{
            let state = state.clone();
            let app_ref = app.as_weak();
            let menu_service = menu_service.clone();
            parity.$cb(move || {
                if let Some(app) = app_ref.upgrade() {
                    if crate::mutation_guard::refused(&app, &state) {
                        return;
                    }
                    let sel = crate::selection_from_app(&app).range();
                    let before = state.current.borrow().clone();
                    let mut after = before.clone();
                    $fn_name(&mut after, sel);
                    if after.hidden_rows == before.hidden_rows
                        && after.hidden_cols == before.hidden_cols
                        && after.freeze_rows == before.freeze_rows
                        && after.freeze_cols == before.freeze_cols
                    {
                        app.set_status_left($msg.into());
                        return;
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
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left($msg.into());
                }
            });
        }};
    }

    wire_vis!(
        on_hide_rows,
        crate::visibility_actions::hide_selected_rows,
        "Hidden selected rows"
    );
    wire_vis!(
        on_unhide_rows,
        crate::visibility_actions::unhide_selected_rows,
        "Unhidden selected rows"
    );
    wire_vis!(
        on_hide_cols,
        crate::visibility_actions::hide_selected_cols,
        "Hidden selected columns"
    );
    wire_vis!(
        on_unhide_cols,
        crate::visibility_actions::unhide_selected_cols,
        "Unhidden selected columns"
    );
    wire_vis!(
        on_freeze_selection,
        crate::visibility_actions::freeze_panes_at_selection,
        "Frozen panes at selection"
    );

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let fp = fp.clone();
        parity.on_format_painter_copy(move || {
            if let Some(app) = app_ref.upgrade() {
                let active = crate::selection_from_app(&app).focus;
                fp.borrow_mut()
                    .pick_from_cell(&state.current.borrow(), active);
                let g = app.global::<crate::SheetsParityState>();
                g.set_format_painter_active(true);
                g.set_format_painter_locked(false);
                app.set_status_left("Format Painter ready (single-shot)".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let fp = fp.clone();
        parity.on_format_painter_lock(move || {
            if let Some(app) = app_ref.upgrade() {
                let g = app.global::<crate::SheetsParityState>();
                if fp.borrow().is_locked() {
                    fp.borrow_mut().cancel();
                    g.set_format_painter_active(false);
                    g.set_format_painter_locked(false);
                    app.set_status_left("Format Painter cleared".into());
                    return;
                }
                let active = crate::selection_from_app(&app).focus;
                fp.borrow_mut()
                    .pick_locked_from_cell(&state.current.borrow(), active);
                g.set_format_painter_active(true);
                g.set_format_painter_locked(true);
                app.set_status_left("Format Painter locked".into());
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        let fp = fp.clone();
        parity.on_format_painter_apply(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let sel = crate::selection_from_app(&app).range();
                let before = state.current.borrow().clone();
                let mut after = before.clone();
                if fp.borrow_mut().apply_to_range(&mut after, sel).is_some() {
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
                    let g = app.global::<crate::SheetsParityState>();
                    g.set_format_painter_active(fp.borrow().is_active());
                    g.set_format_painter_locked(fp.borrow().is_locked());
                    app.set_status_left("Applied copied format".into());
                }
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let audit = audit.clone();
        parity.on_toggle_show_formulas(move || {
            if let Some(app) = app_ref.upgrade() {
                let on = audit.borrow_mut().toggle_show_formulas();
                SHOW_FORMULAS.with(|s| s.set(on));
                app.global::<crate::SheetsParityState>()
                    .set_show_formulas(on);
                crate::project_current(&app, &state);
                app.set_status_left(
                    if on {
                        "Show Formulas: ON"
                    } else {
                        "Show Formulas: OFF"
                    }
                    .into(),
                );
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let audit = audit.clone();
        parity.on_trace_precedents(move || {
            if let Some(app) = app_ref.upgrade() {
                let sheet = state.current.borrow();
                let active = crate::selection_from_app(&app).focus;
                audit.borrow_mut().trace_precedents(&sheet, active);
                let cells = audit.borrow().precedents.clone();
                let count = cells.len();
                ACTIVE_AUDIT_CELLS.with(|c| *c.borrow_mut() = (cells, true));
                let vp = crate::viewport_from_app(&app, &sheet, None);
                let zoom = crate::grid_zoom(&app);
                sync_overlays(
                    &app,
                    &sheet,
                    vp,
                    crate::selection_from_app(&app).range(),
                    zoom,
                );
                app.set_status_left(SharedString::from(format!("Traced {count} precedent(s)")));
            }
        });
    }

    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let audit = audit.clone();
        parity.on_trace_dependents(move || {
            if let Some(app) = app_ref.upgrade() {
                let sheet = state.current.borrow();
                let active = crate::selection_from_app(&app).focus;
                audit.borrow_mut().trace_dependents(&sheet, active);
                let cells = audit.borrow().dependents.clone();
                let count = cells.len();
                ACTIVE_AUDIT_CELLS.with(|c| *c.borrow_mut() = (cells, false));
                let vp = crate::viewport_from_app(&app, &sheet, None);
                let zoom = crate::grid_zoom(&app);
                sync_overlays(
                    &app,
                    &sheet,
                    vp,
                    crate::selection_from_app(&app).range(),
                    zoom,
                );
                app.set_status_left(SharedString::from(format!("Traced {count} dependent(s)")));
            }
        });
    }

    {
        let app_ref = app.as_weak();
        let audit = audit.clone();
        parity.on_clear_audit(move || {
            if let Some(app) = app_ref.upgrade() {
                audit.borrow_mut().clear_traces();
                ACTIVE_AUDIT_CELLS.with(|c| c.borrow_mut().0.clear());
                app.global::<crate::SheetsParityState>()
                    .set_audit_highlights(ModelRc::new(VecModel::<AuditHighlightRect>::default()));
                app.set_status_left("Cleared formula audit".into());
            }
        });
    }
}
