//! Wiring for P0 Sheets parity actions (autofill, visibility, format painter, formula audit).

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use slint::{ComponentHandle, SharedString};

use crate::{
    apply_sheet, commit_transaction, sync_menu_state, GuiState, SheetTransaction, SheetsApp,
};

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
        let app_ref = app.as_weak();
        let audit = audit.clone();
        parity.on_toggle_show_formulas(move || {
            if let Some(app) = app_ref.upgrade() {
                let on = audit.borrow_mut().toggle_show_formulas();
                app.global::<crate::SheetsParityState>()
                    .set_show_formulas(on);
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
                let count = audit.borrow().precedents.len();
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
                let count = audit.borrow().dependents.len();
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
                app.set_status_left("Cleared formula audit".into());
            }
        });
    }
}
