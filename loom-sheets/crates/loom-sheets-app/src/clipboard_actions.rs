//! Copy, cut and paste for the grid, through the operating system clipboard.
//!
//! * Copy and Cut put tab-separated text on the system clipboard, with
//!   formulas shown as their calculated values, as other spreadsheets do. The
//!   cells' raw contents (formulas included) are also kept privately.
//! * Paste uses the private copy when the system clipboard still holds exactly
//!   what Copy wrote, so formulas survive a round trip inside Sheets and their
//!   relative references move with them. Anything else on the clipboard (text
//!   from another program) is read as tab-separated cells.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{parse_csv_records, shift_formula_references, CellRef, Sheet};
use slint::{ComponentHandle, SharedString};

use crate::actions::{copy_selection, paste_selection};
use crate::{
    apply_sheet, clear_selection, evaluate_current, selection_from_app, sync_menu_state,
    GridSelection, GuiState, SheetsApp,
};

/// What the last Copy or Cut wrote, so Paste can tell its own text from text
/// that came from elsewhere.
struct LastCopy {
    origin: CellRef,
    /// The text Copy wrote to the system clipboard; `None` when that write failed.
    external_text: Option<String>,
}

/// "1 cell" or "N cells".
fn cells_label(count: usize) -> String {
    if count == 1 {
        "1 cell".to_string()
    } else {
        format!("{count} cells")
    }
}

thread_local! {
    static LAST_COPY: RefCell<Option<LastCopy>> = const { RefCell::new(None) };
}

fn quote_if_needed(cell: &str) -> String {
    if cell.contains(['\t', '\n', '\r', '"']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell.to_string()
    }
}

/// Tab-separated text, one line per row, quoting cells that need it.
pub(crate) fn encode_tsv(rows: &[Vec<String>]) -> String {
    rows.iter()
        .map(|row| {
            row.iter()
                .map(|cell| quote_if_needed(cell))
                .collect::<Vec<_>>()
                .join("\t")
        })
        .collect::<Vec<_>>()
        .join("\r\n")
}

/// Cells from tab-separated text. A single trailing line break (which other
/// spreadsheets add) does not create an extra row.
pub(crate) fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    parse_csv_records(text, '\t')
}

/// What other programs should see: formulas as their calculated values.
fn external_rows(
    sheet: &Sheet,
    values: &std::collections::HashMap<CellRef, loom_sheets_core::Value>,
    sel: GridSelection,
) -> Vec<Vec<String>> {
    let range = sel.range();
    (range.start.row..=range.end.row)
        .map(|row| {
            (range.start.col..=range.end.col)
                .map(|col| {
                    let cell = CellRef { row, col };
                    let raw = sheet.raw(cell).unwrap_or_default().to_string();
                    if raw.starts_with('=') {
                        values.get(&cell).map(|v| v.display()).unwrap_or_default()
                    } else {
                        raw
                    }
                })
                .collect()
        })
        .collect()
}

/// Copy the selection privately and onto the system clipboard.
fn copy_to_clipboards(state: &GuiState, sel: GridSelection) -> usize {
    let data = copy_selection(&state.current.borrow(), sel);
    let count = data.iter().map(Vec::len).sum();
    let values = evaluate_current(state);
    let text = encode_tsv(&external_rows(&state.current.borrow(), &values, sel));
    let origin = sel.range().start;
    let written = crate::system_clipboard::set_text(&text);
    LAST_COPY.with(|last| {
        *last.borrow_mut() = Some(LastCopy {
            origin,
            external_text: written.then_some(text),
        });
    });
    *state.clipboard.borrow_mut() = Some(data);
    count
}

/// The cells to paste and, when they are Sheets' own copy, the cell they were
/// copied from (so formulas can follow). That private copy is used when the
/// system clipboard still matches what Copy wrote; otherwise the clipboard's
/// tab-separated text is read.
fn clipboard_cells(state: &GuiState) -> Option<(Vec<Vec<String>>, Option<CellRef>)> {
    let system = crate::system_clipboard::get_text();
    let internal = state.clipboard.borrow().clone();
    let from_us = LAST_COPY.with(|last| {
        let last = last.borrow();
        last.as_ref()
            .filter(|last| match (&last.external_text, system.as_deref()) {
                // Our write failed, so the system clipboard cannot hold this copy.
                (None, _) => true,
                // Nothing readable (no clipboard, an empty string, or a failed
                // read): the private copy is all there is.
                (Some(_), None | Some("")) => true,
                // Another program has not replaced what Copy wrote.
                (Some(written), Some(text)) => text == written,
            })
            .map(|last| last.origin)
    });
    if let (Some(origin), Some(raw)) = (from_us, internal) {
        return Some((raw, Some(origin)));
    }
    let text = system.filter(|text| !text.is_empty())?;
    let rows = parse_tsv(&text);
    (!rows.is_empty() && rows.iter().any(|row| !row.is_empty())).then_some((rows, None))
}

/// Move the relative references of every formula by a whole-grid offset.
fn shift_cells(mut cells: Vec<Vec<String>>, dc: i64, dr: i64) -> Vec<Vec<String>> {
    let (dc, dr) = (
        dc.clamp(-1_000_000, 1_000_000) as i32,
        dr.clamp(-1_000_000, 1_000_000) as i32,
    );
    for cell in cells.iter_mut().flatten() {
        if cell.starts_with('=') {
            *cell = shift_formula_references(cell, dc, dr);
        }
    }
    cells
}

/// Paste `cells` at the selection, moving formulas copied from `origin` so their
/// relative references keep their meaning. One cell pasted over a larger
/// selection fills it, each copy with its own shifted references.
fn paste_cells(
    state: &GuiState,
    sel: GridSelection,
    cells: Vec<Vec<String>>,
    origin: Option<CellRef>,
) -> usize {
    let range = sel.range();
    let single = cells.len() == 1 && cells[0].len() == 1;
    let fill = single && range.start != range.end;
    let anchor = if fill { range.start } else { sel.focus };
    let cells = match origin {
        Some(origin) => shift_cells(
            cells,
            anchor.col as i64 - origin.col as i64,
            anchor.row as i64 - origin.row as i64,
        ),
        None => cells,
    };
    let (target, data) = if fill {
        let source = cells[0][0].clone();
        let rows = (range.end.row - range.start.row + 1) as usize;
        let cols = (range.end.col - range.start.col + 1) as usize;
        let filled = (0..rows)
            .map(|r| {
                (0..cols)
                    .map(|c| {
                        if source.starts_with('=') {
                            shift_formula_references(&source, c as i32, r as i32)
                        } else {
                            source.clone()
                        }
                    })
                    .collect()
            })
            .collect();
        (sel.collapse(range.start), filled)
    } else {
        (sel, cells)
    };
    paste_selection(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        target,
        &data,
    )
}

pub(crate) fn wire(app: &SheetsApp, state: &Rc<GuiState>, menu_service: &Arc<NativeMenuBar>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_copy_selection(move || {
            if let Some(app) = app_ref.upgrade() {
                let sel = selection_from_app(&app);
                let count = copy_to_clipboards(&state, sel);
                app.set_status_left(SharedString::from(format!(
                    "Copied {} ({})",
                    sel.label(),
                    cells_label(count)
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
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let sel = selection_from_app(&app);
                let count = copy_to_clipboards(&state, sel);
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
                    "Cut {} ({})",
                    sel.label(),
                    cells_label(count)
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
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let sel = selection_from_app(&app);
                let Some((cells, origin)) = clipboard_cells(&state) else {
                    app.set_status_left("Nothing to paste".into());
                    return;
                };
                let pasted = paste_cells(&state, sel, cells, origin);
                if pasted > 0 {
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    app.set_status_left(SharedString::from(format!(
                        "Pasted {} at {}",
                        cells_label(pasted),
                        sel.focus.to_a1()
                    )));
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(cells: &[&[&str]]) -> Vec<Vec<String>> {
        cells
            .iter()
            .map(|row| row.iter().map(|c| c.to_string()).collect())
            .collect()
    }

    #[test]
    fn tsv_round_trips_cells_with_tabs_quotes_and_line_breaks() {
        let cells = rows(&[
            &["plain", "with\ttab", "say \"hi\""],
            &["two\nlines", "", "end"],
        ]);
        let text = encode_tsv(&cells);
        assert!(text.contains("\"with\ttab\""));
        assert!(text.contains("\"say \"\"hi\"\"\""));
        assert_eq!(parse_tsv(&text), cells);
    }

    #[test]
    fn tsv_from_other_programs_parses_with_either_line_ending_and_a_trailing_break() {
        let expected = rows(&[&["a", "1", "2.5"], &["b", "2", "3.5"]]);
        assert_eq!(parse_tsv("a\t1\t2.5\r\nb\t2\t3.5\r\n"), expected);
        assert_eq!(parse_tsv("a\t1\t2.5\nb\t2\t3.5"), expected);
        assert!(parse_tsv("").is_empty());
    }

    #[test]
    fn formulas_move_their_relative_references_but_not_absolute_ones() {
        let cells = shift_cells(rows(&[&["=A1+$B$1", "text", "7"]]), 2, 3);
        assert_eq!(cells[0][0], "=C4+$B$1");
        assert_eq!(cells[0][1], "text");
        assert_eq!(cells[0][2], "7");
    }
}
