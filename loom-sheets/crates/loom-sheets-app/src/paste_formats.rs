//! Cell formats that travel with a copied range.
//!
//! A copy inside Sheets keeps each cell's style and alignment, and a table
//! pasted from Writer, Present or a web page maps its bold, italic, colours
//! and alignment onto the nearest thing Sheets has. Plain tab-separated text
//! carries no format, so pasting it leaves the target's formatting alone.

use loom_clipboard::SheetsCellData;
use loom_objects::table::HorizontalAlign;
use loom_sheets_core::style::{CellAlignment, CellStyle, FillColor};
use loom_sheets_core::{CellRef, Sheet};

use crate::GridSelection;

/// One cell's look: its style and its alignment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CellFormat {
    pub style: CellStyle,
    pub alignment: CellAlignment,
}

/// The formats of every cell in the selection, row by row.
pub(crate) fn capture(sheet: &Sheet, sel: GridSelection) -> Vec<Vec<CellFormat>> {
    let range = sel.range();
    (range.start.row..=range.end.row)
        .map(|row| {
            (range.start.col..=range.end.col)
                .map(|col| {
                    let cell = CellRef { row, col };
                    CellFormat {
                        style: sheet.cell_style(cell),
                        alignment: sheet.cell_alignment(cell),
                    }
                })
                .collect()
        })
        .collect()
}

fn parse_hex(raw: &str) -> Option<u32> {
    let hex = raw.trim().trim_start_matches('#');
    (hex.len() == 6)
        .then(|| u32::from_str_radix(hex, 16).ok())
        .flatten()
}

fn channels(rgb: u32) -> [i32; 3] {
    [
        ((rgb >> 16) & 0xFF) as i32,
        ((rgb >> 8) & 0xFF) as i32,
        (rgb & 0xFF) as i32,
    ]
}

/// The nearest of Sheets' fill swatches, or none when nothing is close.
fn nearest_fill(hex: &str) -> FillColor {
    let Some(rgb) = parse_hex(hex) else {
        return FillColor::None;
    };
    let want = channels(rgb);
    [
        FillColor::Red,
        FillColor::Orange,
        FillColor::Yellow,
        FillColor::Green,
        FillColor::Blue,
        FillColor::Purple,
        FillColor::Gray,
    ]
    .into_iter()
    .map(|fill| {
        let have = parse_hex(fill.hex()).map_or([0; 3], channels);
        let distance: i32 = want.iter().zip(have).map(|(a, b)| (a - b) * (a - b)).sum();
        (distance, fill)
    })
    .min_by_key(|(distance, _)| *distance)
    .filter(|(distance, _)| *distance < 20_000)
    .map_or(FillColor::None, |(_, fill)| fill)
}

/// Formats for a table that came from outside Sheets.
pub(crate) fn from_grid(grid: &[Vec<SheetsCellData>]) -> Vec<Vec<CellFormat>> {
    grid.iter()
        .map(|row| {
            row.iter()
                .map(|data| {
                    let mut style = CellStyle::new();
                    style.bold = data.bold;
                    style.italic = data.italic;
                    style.text_color = data.text_color.as_deref().and_then(parse_hex);
                    if let Some(background) = data.background_color.as_deref() {
                        style.fill = nearest_fill(background);
                    }
                    let alignment = match data.alignment {
                        Some(HorizontalAlign::Left) => CellAlignment::Left,
                        Some(HorizontalAlign::Center) => CellAlignment::Center,
                        Some(HorizontalAlign::Right) => CellAlignment::Right,
                        _ => CellAlignment::General,
                    };
                    CellFormat { style, alignment }
                })
                .collect()
        })
        .collect()
}

/// Write `formats` onto the sheet. With `fill_over`, the first cell's format is
/// copied over that whole range (one cell pasted onto a selection); otherwise
/// the grid lands with its top-left cell at `anchor`.
pub(crate) fn apply(
    sheet: &mut Sheet,
    anchor: CellRef,
    formats: &[Vec<CellFormat>],
    fill_over: Option<(CellRef, CellRef)>,
) {
    let mut set = |cell: CellRef, format: CellFormat| {
        sheet.set_cell_style(cell, format.style);
        sheet.set_cell_alignment(cell, format.alignment);
    };
    match (fill_over, formats.first().and_then(|row| row.first())) {
        (Some((start, end)), Some(&first)) => {
            for row in start.row..=end.row {
                for col in start.col..=end.col {
                    set(CellRef { row, col }, first);
                }
            }
        }
        _ => {
            for (row_offset, row) in formats.iter().enumerate() {
                for (col_offset, &format) in row.iter().enumerate() {
                    let cell = CellRef {
                        row: anchor.row.saturating_add(row_offset as u32),
                        col: anchor.col.saturating_add(col_offset as u32),
                    };
                    set(cell, format);
                }
            }
        }
    }
}
