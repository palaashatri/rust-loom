//! Rich formatting actions: per-edge borders, strikethrough, merge/unmerge, and
//! conditional formatting rules.

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::style::{BorderPreset, FillColor};
use loom_sheets_core::{
    CellConditionalRule, CellRange, CellRef, ConditionalOperator, GridSelection, Sheet,
    SheetViewport, Value,
};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::ink_actions::TEXT_PALETTE;
use crate::{
    apply_sheet, cell_value, formatting, push_history, selection_from_app, sync_menu_state,
    CellRichFormat, ConditionalRuleView, GuiState, MergedCellView, SheetTransaction, SheetsApp,
};

const FILL_ORDER: [FillColor; 7] = [
    FillColor::Red,
    FillColor::Orange,
    FillColor::Yellow,
    FillColor::Green,
    FillColor::Blue,
    FillColor::Purple,
    FillColor::Gray,
];

const TEXT_NAMES: [&str; 7] = ["Red", "Orange", "Green", "Blue", "Purple", "Gray", "Black"];

/// Publish per-cell strikethrough, per-edge borders, and merged span overlays
/// for the active viewport.
pub(crate) fn publish_viewport(
    app: &SheetsApp,
    sheet: &Sheet,
    values: &HashMap<CellRef, Value>,
    viewport: SheetViewport,
    selection: GridSelection,
    zoom: f32,
) {
    let mat_rows = crate::materialized_rows(sheet, viewport);
    let mat_cols = crate::materialized_cols(sheet, viewport);
    let cap = mat_rows.len() * mat_cols.len();
    let mut strikethroughs = Vec::with_capacity(cap);
    let mut border_tops = Vec::with_capacity(cap);
    let mut border_rights = Vec::with_capacity(cap);
    let mut border_bottoms = Vec::with_capacity(cap);
    let mut border_lefts = Vec::with_capacity(cap);
    let mut merged_covered = Vec::with_capacity(cap);

    for &row in &mat_rows {
        for &col in &mat_cols {
            let cell = CellRef { row, col };
            let style = sheet.cell_style(cell);
            strikethroughs.push(style.strikethrough);
            border_tops.push(style.borders.top);
            border_rights.push(style.borders.right);
            border_bottoms.push(style.borders.bottom);
            border_lefts.push(style.borders.left);
            merged_covered.push(sheet.is_merged_covered(cell));
        }
    }

    let merged_views = build_merged_views(sheet, values, viewport, selection, zoom);

    let rf = app.global::<CellRichFormat>();
    rf.set_strikethroughs(ModelRc::new(VecModel::from(strikethroughs)));
    rf.set_border_tops(ModelRc::new(VecModel::from(border_tops)));
    rf.set_border_rights(ModelRc::new(VecModel::from(border_rights)));
    rf.set_border_bottoms(ModelRc::new(VecModel::from(border_bottoms)));
    rf.set_border_lefts(ModelRc::new(VecModel::from(border_lefts)));
    rf.set_merged_covered(ModelRc::new(VecModel::from(merged_covered)));
    rf.set_merged_views(ModelRc::new(VecModel::from(merged_views)));
}

fn projected_row_height(sheet: &Sheet, row: u32, zoom: f32) -> f32 {
    if sheet.hidden_rows.contains(&row) {
        0.0
    } else {
        sheet.row_height(row) * zoom
    }
}

fn build_merged_views(
    sheet: &Sheet,
    values: &HashMap<CellRef, Value>,
    viewport: SheetViewport,
    selection: GridSelection,
    zoom: f32,
) -> Vec<MergedCellView> {
    if sheet.merged_ranges.is_empty() {
        return Vec::new();
    }
    let row_end = viewport.first_row + viewport.visible_rows;
    let col_end = viewport.first_col + viewport.visible_cols;
    let sel_range = selection.range();
    let mut views = Vec::new();

    for &range in &sheet.merged_ranges {
        if range.end.row < viewport.first_row
            || range.start.row >= row_end
            || range.end.col < viewport.first_col
            || range.start.col >= col_end
        {
            continue;
        }
        let vis_start_row = range.start.row.max(viewport.first_row);
        let vis_end_row = range.end.row.min(row_end.saturating_sub(1));
        let vis_start_col = range.start.col.max(viewport.first_col);
        let vis_end_col = range.end.col.min(col_end.saturating_sub(1));

        let x: f32 = (viewport.first_col..vis_start_col)
            .map(|c| sheet.col_width(c) * zoom)
            .sum();
        let y: f32 = (viewport.first_row..vis_start_row)
            .map(|r| projected_row_height(sheet, r, zoom))
            .sum();
        let width: f32 = (vis_start_col..=vis_end_col)
            .map(|c| sheet.col_width(c) * zoom)
            .sum();
        let height: f32 = (vis_start_row..=vis_end_row)
            .map(|r| projected_row_height(sheet, r, zoom))
            .sum();
        if width <= 0.0 || height <= 0.0 {
            continue;
        }

        let anchor = range.start;
        let raw_val = cell_value(sheet, values, anchor.row, anchor.col);
        let base_style = sheet.cell_style(anchor);
        let formatted_val = base_style.format_value(&raw_val);
        let display_val = base_style.format_value_fit(
            &raw_val,
            width / zoom.max(0.1),
            formatting::effective_font_size(base_style) as f32,
        );
        let effective = sheet.effective_style(anchor, &formatted_val);
        let numeric = matches!(values.get(&anchor), Some(Value::Number(_)))
            || raw_val.trim().parse::<f64>().is_ok();
        let align_code = match sheet.cell_alignment(anchor) {
            loom_sheets_core::CellAlignment::General if numeric => 2,
            loom_sheets_core::CellAlignment::General | loom_sheets_core::CellAlignment::Left => 0,
            loom_sheets_core::CellAlignment::Center => 1,
            loom_sheets_core::CellAlignment::Right => 2,
        };
        let in_selection = sel_range.intersects(range);
        let is_active = range.contains(selection.focus);

        views.push(MergedCellView {
            x,
            y,
            width,
            height,
            text: SharedString::from(display_val),
            bold: effective.bold,
            italic: effective.italic,
            underline: effective.underline,
            strikethrough: effective.strikethrough,
            font_px: formatting::effective_font_size(effective) as i32,
            ink: effective
                .text_color
                .map_or(-1, |rgb| (rgb & 0x00FF_FFFF) as i32),
            fill_idx: effective.fill.swatch_index(),
            align: align_code,
            wraps: effective.wrap,
            border_top: effective.borders.top,
            border_right: effective.borders.right,
            border_bottom: effective.borders.bottom,
            border_left: effective.borders.left,
            in_selection,
            is_active,
        });
    }
    views
}

/// Update inspector selection flags for strikethrough and merged state.
pub(crate) fn publish_selected(app: &SheetsApp, sheet: &Sheet, range: CellRange) {
    let rf = app.global::<CellRichFormat>();
    let style = sheet.cell_style(range.start);
    rf.set_selected_strikethrough(style.strikethrough);
    let has_merge = sheet.merged_ranges.iter().any(|m| m.intersects(range));
    rf.set_selected_merged(has_merge);
}

/// Toggle strikethrough across the current selection with undo/redo.
pub(crate) fn toggle_strikethrough(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &NativeMenuBar,
) -> bool {
    let range = selection_from_app(app).range();
    let changed = formatting::toggle_selection_strikethrough(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        range,
    );
    if changed {
        apply_sheet(app, state);
        sync_menu_state(menu_service, app, state);
        let on = state.current.borrow().cell_style(range.start).strikethrough;
        app.set_status_left(SharedString::from(if on {
            "Strikethrough on"
        } else {
            "Strikethrough off"
        }));
    }
    changed
}

/// Apply a border preset (`all`, `outer`, `top`, `bottom`, `left`, `right`, `none`) to the selection.
pub(crate) fn apply_border_preset(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &NativeMenuBar,
    preset: BorderPreset,
) -> bool {
    let range = selection_from_app(app).range();
    let changed = formatting::apply_selection_border_preset(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        range,
        preset,
    );
    if changed {
        apply_sheet(app, state);
        sync_menu_state(menu_service, app, state);
        app.set_status_left(SharedString::from(format!("Applied {:?} borders", preset)));
    }
    changed
}

/// Merge the current selection into one spanned cell with full snapshot undo/redo.
pub(crate) fn merge_selection(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &NativeMenuBar,
) -> bool {
    let range = selection_from_app(app).range();
    if range.start == range.end {
        app.set_status_left(SharedString::from("Select at least 2 cells to merge"));
        return false;
    }
    let before = Box::new(state.current.borrow().clone());
    let changed = state.current.borrow_mut().merge_cells(range);
    if !changed {
        return false;
    }
    let after = Box::new(state.current.borrow().clone());
    push_history(
        &mut state.undo_stack.borrow_mut(),
        SheetTransaction::Snapshot { before, after },
    );
    state.redo_stack.borrow_mut().clear();
    {
        let sheet = state.current.borrow();
        let vals = crate::values_for_projection(state);
        crate::selection_projection::update_selection_range(
            app,
            &sheet,
            &vals,
            loom_sheets_core::GridSelection::new(range.end, range.start),
        );
    }
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(format!("Merged {}", range.to_a1())));
    true
}

/// Unmerge any merged spans intersecting the current selection with full snapshot undo/redo.
pub(crate) fn unmerge_selection(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &NativeMenuBar,
) -> bool {
    let range = selection_from_app(app).range();
    let before = Box::new(state.current.borrow().clone());
    let changed = state.current.borrow_mut().unmerge_cells(range);
    if !changed {
        app.set_status_left(SharedString::from("No merged cells in selection"));
        return false;
    }
    let after = Box::new(state.current.borrow().clone());
    push_history(
        &mut state.undo_stack.borrow_mut(),
        SheetTransaction::Snapshot { before, after },
    );
    state.redo_stack.borrow_mut().clear();
    apply_sheet(app, state);
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(format!("Unmerged {}", range.to_a1())));
    true
}

/// Open the Conditional Formatting modal for the current selection.
pub(crate) fn open_conditional_formatting(app: &SheetsApp, state: &GuiState) {
    app.set_welcome_open(false);
    let range = selection_from_app(app).range();
    refresh_cf_modal(app, &state.current.borrow(), range);
    app.global::<CellRichFormat>().set_cf_modal_open(true);
}

fn refresh_cf_modal(app: &SheetsApp, sheet: &Sheet, range: CellRange) {
    let rf = app.global::<CellRichFormat>();
    rf.set_cf_range(SharedString::from(range.to_a1()));
    let views: Vec<ConditionalRuleView> = sheet
        .conditional_rules
        .iter()
        .map(|rule| ConditionalRuleView {
            range: SharedString::from(rule.range.to_a1()),
            operator: SharedString::from(rule.operator.label()),
            value: SharedString::from(rule.value.as_str()),
            fill_name: SharedString::from(rule.fill.as_str()),
            text_color_name: SharedString::from(
                rule.text_color
                    .and_then(|c| {
                        TEXT_PALETTE
                            .iter()
                            .position(|&p| p == (c & 0x00FF_FFFF))
                            .map(|idx| TEXT_NAMES[idx])
                    })
                    .unwrap_or("Auto"),
            ),
        })
        .collect();
    rf.set_cf_rules(ModelRc::new(VecModel::from(views)));
}

pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    let rf = app.global::<CellRichFormat>();
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_toggle_strikethrough(move || {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    toggle_strikethrough(&app, &state, &menu_service);
                }
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        rf.on_toggle_wrap_text(move || {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    app.global::<crate::CellInk>().invoke_wrap_toggled();
                }
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        rf.on_cycle_text_color(move || {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    let sel = selection_from_app(&app);
                    let current_slot = crate::ink_actions::slot_for(
                        state.current.borrow().cell_style(sel.focus).text_color,
                    );
                    let next_slot = if current_slot + 1 >= TEXT_PALETTE.len() as i32 {
                        -1
                    } else {
                        current_slot + 1
                    };
                    app.global::<crate::CellInk>()
                        .invoke_color_requested(next_slot);
                }
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        rf.on_auto_text_color(move || {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    app.global::<crate::CellInk>().invoke_color_requested(-1);
                }
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_paste_values_only(move || {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    crate::clipboard_actions::paste_values_only(&app, &state, &menu_service);
                }
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_paste_formats_only(move || {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    crate::clipboard_actions::paste_formats_only(&app, &state, &menu_service);
                }
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_apply_border_preset(move |raw| {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    if let Some(preset) = BorderPreset::parse(&raw) {
                        apply_border_preset(&app, &state, &menu_service, preset);
                    }
                }
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_merge_cells(move || {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    merge_selection(&app, &state, &menu_service);
                }
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_unmerge_cells(move || {
            if let Some(app) = app_ref.upgrade() {
                if !crate::mutation_guard::refused(&app, &state) {
                    unmerge_selection(&app, &state, &menu_service);
                }
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        rf.on_open_conditional_formatting(move || {
            if let Some(app) = app_ref.upgrade() {
                open_conditional_formatting(&app, &state);
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_cf_add_rule(move |op_idx, value, fill_idx, text_idx| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return;
            }
            let operator = match op_idx {
                1 => ConditionalOperator::LessThan,
                2 => ConditionalOperator::EqualTo,
                3 => ConditionalOperator::Contains,
                _ => ConditionalOperator::GreaterThan,
            };
            let fill = usize::try_from(fill_idx)
                .ok()
                .and_then(|i| FILL_ORDER.get(i).copied())
                .unwrap_or(FillColor::None);
            let text_color = usize::try_from(text_idx)
                .ok()
                .and_then(|i| TEXT_PALETTE.get(i).copied());
            let range = selection_from_app(&app).range();
            let before = Box::new(state.current.borrow().clone());
            state
                .current
                .borrow_mut()
                .add_conditional_rule(CellConditionalRule {
                    range,
                    operator,
                    value: value.to_string(),
                    fill,
                    text_color,
                });
            let after = Box::new(state.current.borrow().clone());
            push_history(
                &mut state.undo_stack.borrow_mut(),
                SheetTransaction::Snapshot { before, after },
            );
            state.redo_stack.borrow_mut().clear();
            refresh_cf_modal(&app, &state.current.borrow(), range);
            apply_sheet(&app, &state);
            sync_menu_state(&menu_service, &app, &state);
            app.set_status_left(SharedString::from("Added conditional formatting rule"));
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_cf_remove_rule(move |idx| {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return;
            }
            let Ok(index) = usize::try_from(idx) else {
                return;
            };
            let before = Box::new(state.current.borrow().clone());
            if state.current.borrow_mut().remove_conditional_rule(index) {
                let after = Box::new(state.current.borrow().clone());
                push_history(
                    &mut state.undo_stack.borrow_mut(),
                    SheetTransaction::Snapshot { before, after },
                );
                state.redo_stack.borrow_mut().clear();
                let range = selection_from_app(&app).range();
                refresh_cf_modal(&app, &state.current.borrow(), range);
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left(SharedString::from("Removed conditional formatting rule"));
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        rf.on_cf_clear_selection(move || {
            let Some(app) = app_ref.upgrade() else {
                return;
            };
            if crate::mutation_guard::refused(&app, &state) {
                return;
            }
            let range = selection_from_app(&app).range();
            let before = Box::new(state.current.borrow().clone());
            let removed = state.current.borrow_mut().clear_conditional_rules(range);
            if removed > 0 {
                let after = Box::new(state.current.borrow().clone());
                push_history(
                    &mut state.undo_stack.borrow_mut(),
                    SheetTransaction::Snapshot { before, after },
                );
                state.redo_stack.borrow_mut().clear();
                refresh_cf_modal(&app, &state.current.borrow(), range);
                apply_sheet(&app, &state);
                sync_menu_state(&menu_service, &app, &state);
                app.set_status_left(SharedString::from(format!(
                    "Cleared {removed} conditional rule(s)"
                )));
            }
        });
    }
    {
        let app_ref = app.as_weak();
        rf.on_cf_close(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<CellRichFormat>().set_cf_modal_open(false);
            }
        });
    }
}
