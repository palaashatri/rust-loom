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

use crate::actions::{copy_selection_with_values, paste_selection};
use crate::paste_formats::{self, CellFormat};
use crate::{
    apply_sheet, clear_selection, evaluate_current, selection_from_app, sync_menu_state,
    GridSelection, GuiState, SheetsApp,
};

/// What the last Copy or Cut wrote, so Paste can tell its own text from text
/// that came from elsewhere.
struct LastCopy {
    origin: CellRef,
    /// Each copied cell's style and alignment, pasted along with its value.
    formats: Vec<Vec<CellFormat>>,
    /// The text Copy wrote to the system clipboard; `None` when that write failed.
    external_text: Option<String>,
}

/// What a Paste will write: the cells, where they were copied from when that
/// was Sheets itself (so formulas follow), and their formats when the source
/// carried any (plain text does not).
struct Pasteable {
    cells: Vec<Vec<String>>,
    origin: Option<CellRef>,
    formats: Option<Vec<Vec<CellFormat>>>,
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
                    if raw.starts_with('=') || raw.is_empty() {
                        let disp = values.get(&cell).map(|v| v.display()).unwrap_or_default();
                        if !disp.is_empty() {
                            disp
                        } else {
                            raw
                        }
                    } else {
                        raw
                    }
                })
                .collect()
        })
        .collect()
}

fn external_cells_data(
    sheet: &Sheet,
    values: &std::collections::HashMap<CellRef, loom_sheets_core::Value>,
    sel: GridSelection,
) -> Vec<Vec<loom_clipboard::SheetsCellData>> {
    let range = sel.range();
    (range.start.row..=range.end.row)
        .map(|row| {
            (range.start.col..=range.end.col)
                .map(|col| {
                    let cell = CellRef { row, col };
                    let raw = sheet.raw(cell).unwrap_or_default().to_string();
                    let display_val = if raw.starts_with('=') || raw.is_empty() {
                        let disp = values.get(&cell).map(|v| v.display()).unwrap_or_default();
                        if !disp.is_empty() {
                            disp
                        } else {
                            raw.clone()
                        }
                    } else {
                        raw.clone()
                    };
                    let style = sheet.cell_style(cell);
                    let alignment = sheet.cell_alignment(cell);
                    let width = sheet.col_widths.get(&col).copied().unwrap_or(80.0);
                    let cell_value = if raw.is_empty() && !display_val.is_empty() {
                        display_val.clone()
                    } else {
                        raw
                    };
                    loom_clipboard::SheetsCellData {
                        value: cell_value,
                        formatted_text: display_val,
                        background_color: if style.fill != loom_sheets_core::style::FillColor::None
                        {
                            Some(style.fill.hex().to_string())
                        } else {
                            None
                        },
                        bold: style.bold,
                        italic: style.italic,
                        text_color: style
                            .text_color
                            .map(|rgb| format!("#{:06X}", rgb & 0x00FF_FFFF)),
                        alignment: match alignment {
                            loom_sheets_core::style::CellAlignment::Left => {
                                Some(loom_objects::table::HorizontalAlign::Left)
                            }
                            loom_sheets_core::style::CellAlignment::Center => {
                                Some(loom_objects::table::HorizontalAlign::Center)
                            }
                            loom_sheets_core::style::CellAlignment::Right => {
                                Some(loom_objects::table::HorizontalAlign::Right)
                            }
                            loom_sheets_core::style::CellAlignment::General => None,
                        },
                        width_pt: width,
                    }
                })
                .collect()
        })
        .collect()
}

/// Copy the selection privately and onto the system clipboard.
fn copy_to_clipboards(state: &GuiState, sel: GridSelection) -> usize {
    let values = evaluate_current(state);
    let data = copy_selection_with_values(&state.current.borrow(), Some(&values), sel);
    let count = data.iter().map(Vec::len).sum();
    let text = encode_tsv(&external_rows(&state.current.borrow(), &values, sel));
    let cell_data = external_cells_data(&state.current.borrow(), &values, sel);
    let table = loom_clipboard::sheets_range_to_table(&cell_data);
    let mut clip_data = loom_clipboard::ClipboardData::from_table(&table);
    clip_data.tsv = Some(text.clone());
    clip_data.plain_text = Some(text.clone());

    let origin = sel.range().start;
    let formats = paste_formats::capture(&state.current.borrow(), sel);
    let written = crate::system_clipboard::set_clipboard_data(&clip_data);
    LAST_COPY.with(|last| {
        *last.borrow_mut() = Some(LastCopy {
            origin,
            formats,
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
fn clipboard_cells(state: &GuiState) -> Option<Pasteable> {
    let system_clip = crate::system_clipboard::get_clipboard_data();
    let system_text = system_clip
        .as_ref()
        .and_then(|c| c.tsv.as_deref().or(c.plain_text.as_deref()));
    let internal = state.clipboard.borrow().clone();
    let from_us = LAST_COPY.with(|last| {
        let last = last.borrow();
        last.as_ref()
            .filter(|last| match (&last.external_text, system_text) {
                // Our write failed, so the system clipboard cannot hold this copy.
                (None, _) => true,
                // Nothing readable (no clipboard, an empty string, or a failed
                // read): the private copy is all there is.
                (Some(_), None | Some("")) => true,
                // Another program has not replaced what Copy wrote.
                (Some(written), Some(text)) => text == written,
            })
            .map(|last| (last.origin, last.formats.clone()))
    });
    if let (Some((origin, formats)), Some(raw)) = (from_us, internal) {
        return Some(Pasteable {
            cells: raw,
            origin: Some(origin),
            formats: Some(formats),
        });
    }

    let clip = system_clip?;

    // 1. Private loom objects JSON
    if let Some(json) = &clip.loom_objects_json {
        if let Ok(table) = loom_objects::Table::from_json(json) {
            let grid = loom_clipboard::table_to_sheets_range(&table);
            let formats = paste_formats::from_grid(&grid);
            let rows: Vec<Vec<String>> = grid
                .into_iter()
                .map(|row| row.into_iter().map(|c| c.value).collect())
                .collect();
            if !rows.is_empty() && rows.iter().any(|r| !r.is_empty()) {
                return Some(Pasteable {
                    cells: rows,
                    origin: None,
                    formats: Some(formats),
                });
            }
        }
    }

    // 2. HTML table
    if let Some(html) = &clip.html {
        if html.contains("<table") || html.contains("<TABLE") {
            if let Some(table) = loom_clipboard::html_to_table(html) {
                let grid = loom_clipboard::table_to_sheets_range(&table);
                let formats = paste_formats::from_grid(&grid);
                let rows: Vec<Vec<String>> = grid
                    .into_iter()
                    .map(|row| row.into_iter().map(|c| c.value).collect())
                    .collect();
                if !rows.is_empty() && rows.iter().any(|r| !r.is_empty()) {
                    return Some(Pasteable {
                        cells: rows,
                        origin: None,
                        formats: Some(formats),
                    });
                }
            }
        }
    }

    // 3. TSV / plain text
    let text = clip
        .tsv
        .or(clip.plain_text)
        .filter(|text| !text.is_empty())?;
    let rows = parse_tsv(&text);
    (!rows.is_empty() && rows.iter().any(|row| !row.is_empty())).then_some(Pasteable {
        cells: rows,
        origin: None,
        formats: None,
    })
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
    formats: Option<Vec<Vec<CellFormat>>>,
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
    let before = formats
        .is_some()
        .then(|| Box::new(state.current.borrow().clone()));
    let pasted = paste_selection(
        &mut state.current.borrow_mut(),
        &mut state.undo_stack.borrow_mut(),
        &mut state.redo_stack.borrow_mut(),
        target,
        &data,
    );
    if let (Some(formats), Some(before), true) = (formats, before, pasted > 0) {
        // The formats join the values in one undo step: the paste's own step is
        // replaced by a snapshot of the sheet before and after both.
        let fill_over = fill.then_some((range.start, range.end));
        let mut sheet = state.current.borrow_mut();
        paste_formats::apply(&mut sheet, anchor, &formats, fill_over);
        let after = Box::new(sheet.clone());
        let mut undo = state.undo_stack.borrow_mut();
        undo.pop();
        crate::push_history(
            &mut undo,
            crate::SheetTransaction::Snapshot { before, after },
        );
    }
    pasted
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
                let Some(Pasteable {
                    cells,
                    origin,
                    formats,
                }) = clipboard_cells(&state)
                else {
                    app.set_status_left("Nothing to paste".into());
                    return;
                };
                let from_us = origin.is_some();
                let pasted = paste_cells(&state, sel, cells, origin, formats);
                if pasted > 0 {
                    apply_sheet(&app, &state);
                    sync_menu_state(&menu_service, &app, &state);
                    let mut status =
                        format!("Pasted {} at {}", cells_label(pasted), sel.focus.to_a1());
                    if !from_us {
                        if let Some(note) = crate::system_clipboard::get_clipboard_data()
                            .and_then(|clip| clip.truncation_note())
                        {
                            status.push_str(&format!(" - {note}"));
                        }
                    }
                    app.set_status_left(SharedString::from(status));
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
