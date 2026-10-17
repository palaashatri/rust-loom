//! Text colour and wrapped text: the inspector's swatches and wrap switch.
//!
//! Both are cell styles, so each choice is one undoable step over the whole
//! selection. Turning wrap on also grows the rows it needs, in the same step,
//! so wrapped text is never cut off by the default row height.

use super::*;
use loom_sheets_core::style::CellStyle;

/// The seven text colours the inspector offers, as `0xRRGGBB`. Dark enough to
/// read on the white paper in every theme; the eighth swatch is "Automatic".
pub(crate) const TEXT_PALETTE: [u32; 7] = [
    0x00C6_2828,
    0x00E6_5100,
    0x002E_7D32,
    0x0015_65C0,
    0x006A_1B9A,
    0x0061_6161,
    0x0000_0000,
];

/// Names for the status line, in palette order.
const NAMES: [&str; 7] = ["red", "orange", "green", "blue", "purple", "gray", "black"];

/// Palette slot of a style's text colour, or -1 for automatic or a colour that
/// came from an imported file and is not one of the seven.
pub(crate) fn slot_for(color: Option<u32>) -> i32 {
    color
        .and_then(|rgb| TEXT_PALETTE.iter().position(|&c| c == rgb & 0x00FF_FFFF))
        .map_or(-1, |slot| slot as i32)
}

/// Show the selected cell's text colour and wrap in the inspector.
pub(crate) fn publish_selected(app: &SheetsApp, style: CellStyle) {
    let ink = app.global::<CellInk>();
    ink.set_selected_slot(slot_for(style.text_color));
    ink.set_selected_wrap(style.wrap);
    ink.set_palette(ModelRc::new(VecModel::from(
        TEXT_PALETTE
            .iter()
            .map(|&rgb| slint::Color::from_rgb_u8((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8))
            .collect::<Vec<_>>(),
    )));
}

fn restyle(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
    change: impl Fn(&mut CellStyle),
) -> bool {
    let cells = range.cells();
    let mut before = Vec::with_capacity(cells.len());
    let mut after = Vec::with_capacity(cells.len());
    let mut any_change = false;
    for &cell in &cells {
        let prev = sheet.cell_style(cell);
        let mut next = prev;
        change(&mut next);
        any_change |= next != prev;
        before.push((cell, prev));
        after.push((cell, next));
    }
    if !any_change {
        return false;
    }
    commit_transaction(
        sheet,
        undo_stack,
        redo_stack,
        SheetTransaction::Style { before, after },
    )
}

/// Set (or with `None` clear) the text colour of the range.
pub(crate) fn set_selection_text_color(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
    color: Option<u32>,
) -> bool {
    restyle(sheet, undo_stack, redo_stack, range, |style| {
        style.text_color = color;
    })
}

/// Rough advance of a character as a fraction of the font size, with the same
/// constants the overflow layer uses.
const CHAR_EM: f32 = 0.6;
const BOLD_EXTRA: f32 = 0.06;
const CELL_PADDING: f32 = 16.0;
/// Space a text line takes, as a multiple of the font size.
const LINE_EM: f32 = 1.35;
const ROW_PADDING: f32 = 8.0;
/// A row never grows past this many pixels from wrapping alone.
const MAX_WRAPPED_ROW: f32 = 400.0;

/// Lines `text` needs when wrapped into `width` pixels.
fn wrapped_lines(text: &str, width: f32, size: f32, bold: bool) -> usize {
    let advance = size * (CHAR_EM + if bold { BOLD_EXTRA } else { 0.0 });
    let per_line = (((width - CELL_PADDING) / advance).floor() as usize).max(1);
    text.split('\n')
        .map(|line| line.chars().count().div_ceil(per_line).max(1))
        .sum()
}

/// Row heights that fit the wrapped cells of `range`, for rows the user has not
/// sized by hand. Returns the changed `(row, height)` pairs.
fn wrapped_row_heights(sheet: &Sheet, range: CellRange) -> Vec<(u32, f32)> {
    let mut needed: std::collections::BTreeMap<u32, f32> = std::collections::BTreeMap::new();
    for cell in range.cells() {
        let style = sheet.cell_style(cell);
        let Some(text) = sheet
            .raw(cell)
            .filter(|text| !text.is_empty() && !text.starts_with('='))
        else {
            continue;
        };
        let size = formatting::effective_font_size(style) as f32;
        let lines = wrapped_lines(text, sheet.col_width(cell.col), size, style.bold);
        let height = (lines as f32 * size * LINE_EM + ROW_PADDING).min(MAX_WRAPPED_ROW);
        let slot = needed.entry(cell.row).or_insert(0.0);
        *slot = slot.max(height);
    }
    needed
        .into_iter()
        .filter(|(row, height)| {
            !sheet.row_heights.contains_key(row) && *height > sheet.row_height(*row)
        })
        .collect()
}

/// Turn wrapping on for the whole range, or off if every cell already wraps.
/// Switching it on grows the rows the wrapped text needs, in the same step.
pub(crate) fn toggle_selection_wrap(
    sheet: &mut Sheet,
    undo_stack: &mut Vec<SheetTransaction>,
    redo_stack: &mut Vec<SheetTransaction>,
    range: CellRange,
) -> bool {
    let cells = range.cells();
    if cells.is_empty() {
        return false;
    }
    let target = !cells.iter().all(|&cell| sheet.cell_style(cell).wrap);
    let before = Box::new(sheet.clone());
    let changed = restyle(sheet, &mut Vec::new(), &mut Vec::new(), range, |style| {
        style.wrap = target;
    });
    if !changed {
        return false;
    }
    if target {
        for (row, height) in wrapped_row_heights(sheet, range) {
            sheet.set_row_height(row, height);
        }
    }
    let after = Box::new(sheet.clone());
    push_history(undo_stack, SheetTransaction::Snapshot { before, after });
    redo_stack.clear();
    true
}

fn apply_choice(app: &SheetsApp, state: &GuiState, menu_service: &NativeMenuBar, slot: i32) {
    let color = usize::try_from(slot)
        .ok()
        .and_then(|slot| TEXT_PALETTE.get(slot).copied());
    let selection = selection_from_app(app);
    if set_selection_text_color(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        selection.range(),
        color,
    ) {
        apply_sheet(app, state);
        sync_menu_state(menu_service, app, state);
        let name = usize::try_from(slot)
            .ok()
            .and_then(|slot| NAMES.get(slot))
            .map_or("automatic", |name| name);
        app.set_status_left(SharedString::from(format!("Text colour {name}")));
    }
}

fn apply_wrap(app: &SheetsApp, state: &GuiState, menu_service: &NativeMenuBar) {
    let selection = selection_from_app(app);
    if toggle_selection_wrap(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        selection.range(),
    ) {
        apply_sheet(app, state);
        sync_menu_state(menu_service, app, state);
        let wraps = state.current.borrow().cell_style(selection.focus).wrap;
        app.set_status_left(SharedString::from(if wraps {
            "Wrap text on"
        } else {
            "Wrap text off"
        }));
    }
}

pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    let ink = app.global::<CellInk>();
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        ink.on_color_requested(move |slot| {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                apply_choice(&app, &state, &menu_service, slot);
            }
        });
    }
    {
        let (state, app_ref, menu_service) = (state.clone(), app.as_weak(), menu_service.clone());
        ink.on_wrap_toggled(move || {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                apply_wrap(&app, &state, &menu_service);
            }
        });
    }
}
