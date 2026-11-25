//! Wiring for P0 Sheets parity actions (autofill, visibility, format painter, formula audit).

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{CellRange, Sheet, SheetViewport};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{
    apply_sheet, commit_transaction, sync_menu_state, AuditHighlightRect, GuiState,
    SheetTransaction, SheetsApp,
};

thread_local! {
    static SHOW_FORMULAS: Cell<bool> = const { Cell::new(false) };
}

pub(crate) fn show_formulas_enabled() -> bool {
    SHOW_FORMULAS.with(Cell::get)
}

pub(crate) fn sync_overlays(
    app: &SheetsApp,
    _sheet: &Sheet,
    viewport: SheetViewport,
    selection: CellRange,
    zoom: f32,
) {
    let g = app.global::<crate::SheetsParityState>();
    let vis_col = selection.end.col >= viewport.first_col
        && selection.end.col < viewport.first_col + viewport.visible_cols.max(1);
    let vis_row = selection.end.row >= viewport.first_row
        && selection.end.row < viewport.first_row + viewport.visible_rows.max(1);
    if vis_col && vis_row {
        let cx =
            ((selection.end.col - viewport.first_col + 1) as f32) * crate::GRID_COL_WIDTH * zoom;
        let cy =
            ((selection.end.row - viewport.first_row + 1) as f32) * crate::GRID_ROW_HEIGHT * zoom;
        g.set_fill_handle_visible(true);
        g.set_fill_handle_x(cx);
        g.set_fill_handle_y(cy);
    } else {
        g.set_fill_handle_visible(false);
    }
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
                let vp = crate::viewport_from_app(&app, &state.current.borrow(), None);
                let row = ((y / crate::GRID_ROW_HEIGHT).floor().max(0.0) as u32)
                    .saturating_add(vp.first_row);
                let col = ((x / crate::GRID_COL_WIDTH).floor().max(0.0) as u32)
                    .saturating_add(vp.first_col);
                let hovered = loom_sheets_core::CellRef { row, col };
                fill_state.borrow_mut().update_drag(hovered);
                if let Some(target) = fill_state.borrow().target_range {
                    let fr = vp.first_row;
                    let fc = vp.first_col;
                    let px = (target.start.col.saturating_sub(fc) as f32) * crate::GRID_COL_WIDTH;
                    let py = (target.start.row.saturating_sub(fr) as f32) * crate::GRID_ROW_HEIGHT;
                    let pw = ((target.end.col.saturating_sub(target.start.col) + 1) as f32)
                        * crate::GRID_COL_WIDTH;
                    let ph = ((target.end.row.saturating_sub(target.start.row) + 1) as f32)
                        * crate::GRID_ROW_HEIGHT;
                    g.set_fill_preview_x(px);
                    g.set_fill_preview_y(py);
                    g.set_fill_preview_w(pw);
                    g.set_fill_preview_h(ph);
                }
                let _ = &state;
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
                let active = crate::selection_from_app(&app).focus;
                fp.borrow_mut()
                    .pick_locked_from_cell(&state.current.borrow(), active);
                let g = app.global::<crate::SheetsParityState>();
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
                apply_sheet(&app, &state);
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
                let active = crate::selection_from_app(&app).focus;
                audit
                    .borrow_mut()
                    .trace_precedents(&state.current.borrow(), active);
                let vp = crate::viewport_from_app(&app, &state.current.borrow(), None);
                let rects: Vec<AuditHighlightRect> = audit
                    .borrow()
                    .precedents
                    .iter()
                    .filter(|c| c.row >= vp.first_row && c.col >= vp.first_col)
                    .map(|c| AuditHighlightRect {
                        x: ((c.col - vp.first_col) as f32) * crate::GRID_COL_WIDTH,
                        y: ((c.row - vp.first_row) as f32) * crate::GRID_ROW_HEIGHT,
                        w: crate::GRID_COL_WIDTH,
                        h: crate::GRID_ROW_HEIGHT,
                        is_precedent: true,
                    })
                    .collect();
                let count = rects.len();
                app.global::<crate::SheetsParityState>()
                    .set_audit_highlights(ModelRc::new(VecModel::from(rects)));
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
                let active = crate::selection_from_app(&app).focus;
                audit
                    .borrow_mut()
                    .trace_dependents(&state.current.borrow(), active);
                let vp = crate::viewport_from_app(&app, &state.current.borrow(), None);
                let rects: Vec<AuditHighlightRect> = audit
                    .borrow()
                    .dependents
                    .iter()
                    .filter(|c| c.row >= vp.first_row && c.col >= vp.first_col)
                    .map(|c| AuditHighlightRect {
                        x: ((c.col - vp.first_col) as f32) * crate::GRID_COL_WIDTH,
                        y: ((c.row - vp.first_row) as f32) * crate::GRID_ROW_HEIGHT,
                        w: crate::GRID_COL_WIDTH,
                        h: crate::GRID_ROW_HEIGHT,
                        is_precedent: false,
                    })
                    .collect();
                let count = rects.len();
                app.global::<crate::SheetsParityState>()
                    .set_audit_highlights(ModelRc::new(VecModel::from(rects)));
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
                app.global::<crate::SheetsParityState>()
                    .set_audit_highlights(ModelRc::new(VecModel::<AuditHighlightRect>::default()));
                app.set_status_left("Cleared formula audit".into());
            }
        });
    }
}
