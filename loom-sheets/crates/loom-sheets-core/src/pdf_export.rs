//! Pure-Rust paginated PDF export for Loom Sheets.
//!
//! Renders the active worksheet's evaluated cells, formatting, borders, and
//! gridlines into a deterministic PDF 1.4 document via `loom_pdf`.

use std::collections::HashMap;

use loom_pdf::{text_width_pt, PageIndex, PathStyle, PdfDocument, TextStyle};

use crate::style::{CellAlignment, FillColor};
use crate::{evaluate, CellRef, Sheet, Value};

const DEFAULT_COL_WIDTH_PT: f32 = 60.0;
const DEFAULT_ROW_HEIGHT_PT: f32 = 18.0;
const MIN_COL_WIDTH_PT: f32 = 36.0;
const MIN_ROW_HEIGHT_PT: f32 = 16.0;
const ROW_GUTTER_WIDTH_PT: f32 = 32.0;
const PAGE_HEADER_HEIGHT_PT: f32 = 24.0;
const COL_HEADER_HEIGHT_PT: f32 = 18.0;
const CELL_PAD_X_PT: f32 = 4.0;

/// Options controlling PDF page geometry and table rendering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PdfExportOptions {
    /// Page width in PDF points (default: 612.0 = US Letter).
    pub page_width_pt: f32,
    /// Page height in PDF points (default: 792.0 = US Letter).
    pub page_height_pt: f32,
    /// Page margin in PDF points on all four sides (default: 36.0 = 0.5 in).
    pub margin_pt: f32,
    /// Whether to draw light gridlines around cells.
    pub include_gridlines: bool,
    /// Whether to repeat `sheet.freeze_rows` at the top of continuation pages.
    pub repeat_frozen_rows: bool,
}

impl Default for PdfExportOptions {
    fn default() -> Self {
        Self {
            page_width_pt: 612.0,
            page_height_pt: 792.0,
            margin_pt: 36.0,
            include_gridlines: true,
            repeat_frozen_rows: true,
        }
    }
}

/// Export `sheet` as a paginated PDF document using default US Letter options.
pub fn export_sheet_pdf(sheet: &Sheet) -> Vec<u8> {
    export_sheet_pdf_with_options(sheet, PdfExportOptions::default())
}

/// Export `sheet` as a paginated PDF document with explicit page options.
pub fn export_sheet_pdf_with_options(sheet: &Sheet, options: PdfExportOptions) -> Vec<u8> {
    let page_w = options.page_width_pt.max(144.0);
    let page_h = options.page_height_pt.max(144.0);
    let margin = options
        .margin_pt
        .clamp(12.0, (page_w.min(page_h) / 4.0).max(12.0));
    let usable_w = (page_w - 2.0 * margin).max(72.0);
    let usable_h = (page_h - 2.0 * margin).max(72.0);

    let mut doc = PdfDocument::new();
    let evaluated = evaluate(sheet);

    let bounds = used_bounds(sheet);
    let Some((max_row, max_col)) = bounds else {
        let page = doc.add_page(page_w, page_h);
        draw_page_header(&mut doc, page, sheet, (page_w, page_h, margin), 1, 1);
        let empty_style = TextStyle {
            size_pt: 10.0,
            fill_rgb: (0.35, 0.37, 0.40),
            bold: false,
            italic: true,
        };
        let y = page_h - margin - PAGE_HEADER_HEIGHT_PT - 18.0;
        doc.draw_text(page, margin, y, "Empty worksheet", &empty_style);
        return doc.serialize();
    };

    let col_table_w = (usable_w - ROW_GUTTER_WIDTH_PT).max(MIN_COL_WIDTH_PT);
    let col_widths: Vec<f32> = (0..=max_col)
        .map(|col| column_width_pt(sheet, col, col_table_w))
        .collect();
    let row_heights: Vec<f32> = (0..=max_row)
        .map(|row| {
            row_height_pt(
                sheet,
                row,
                usable_h - PAGE_HEADER_HEIGHT_PT - COL_HEADER_HEIGHT_PT,
            )
        })
        .collect();

    let col_bands = partition_columns(&col_widths, col_table_w);
    let body_h = (usable_h - PAGE_HEADER_HEIGHT_PT - COL_HEADER_HEIGHT_PT).max(MIN_ROW_HEIGHT_PT);
    let frozen_rows = if options.repeat_frozen_rows {
        sheet.freeze_rows.min(max_row + 1)
    } else {
        0
    };
    let row_bands = partition_rows(&row_heights, body_h, frozen_rows);

    let total_pages = row_bands.len() * col_bands.len();
    let mut page_num = 0usize;

    for row_band in &row_bands {
        for &(col_start, col_end) in &col_bands {
            page_num += 1;
            let page = doc.add_page(page_w, page_h);
            draw_page_header(
                &mut doc,
                page,
                sheet,
                (page_w, page_h, margin),
                page_num,
                total_pages,
            );
            draw_table_band(
                &mut doc,
                page,
                sheet,
                &evaluated,
                page_h - margin - PAGE_HEADER_HEIGHT_PT,
                margin,
                col_start..col_end,
                row_band,
                &col_widths,
                &row_heights,
                options.include_gridlines,
            );
        }
    }

    doc.serialize()
}

fn used_bounds(sheet: &Sheet) -> Option<(u32, u32)> {
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    let mut any = false;
    for cell_ref in sheet.cells.keys() {
        any = true;
        max_row = max_row.max(cell_ref.row);
        max_col = max_col.max(cell_ref.col);
    }
    if any {
        Some((max_row, max_col))
    } else {
        None
    }
}

fn column_width_pt(sheet: &Sheet, col: u32, max_w: f32) -> f32 {
    let raw_px = sheet.col_widths.get(&col).copied().unwrap_or(80.0);
    let pt = if raw_px.is_finite() && raw_px > 0.0 {
        raw_px * 0.75
    } else {
        DEFAULT_COL_WIDTH_PT
    };
    pt.clamp(MIN_COL_WIDTH_PT, max_w.max(MIN_COL_WIDTH_PT))
}

fn row_height_pt(sheet: &Sheet, row: u32, max_h: f32) -> f32 {
    let raw_px = sheet.row_heights.get(&row).copied().unwrap_or(24.0);
    let pt = if raw_px.is_finite() && raw_px > 0.0 {
        raw_px * 0.75
    } else {
        DEFAULT_ROW_HEIGHT_PT
    };
    pt.clamp(MIN_ROW_HEIGHT_PT, max_h.max(MIN_ROW_HEIGHT_PT))
}

fn partition_columns(col_widths: &[f32], max_w: f32) -> Vec<(u32, u32)> {
    let mut bands = Vec::new();
    let mut start = 0usize;
    while start < col_widths.len() {
        let mut used = 0.0f32;
        let mut end = start;
        while end < col_widths.len() {
            let w = col_widths[end];
            if end > start && used + w > max_w + 0.01 {
                break;
            }
            used += w;
            end += 1;
        }
        bands.push((start as u32, end as u32));
        start = end;
    }
    bands
}

fn partition_rows(row_heights: &[f32], body_h: f32, frozen_rows: u32) -> Vec<Vec<u32>> {
    let frozen_count = (frozen_rows as usize).min(row_heights.len());
    let frozen_h: f32 = row_heights[..frozen_count].iter().sum();
    let use_frozen = frozen_count > 0 && frozen_h < body_h * 0.5;

    let mut bands = Vec::new();
    let mut cursor = 0usize;
    while cursor < row_heights.len() {
        let mut rows = Vec::new();
        let mut used = 0.0f32;
        if use_frozen && cursor >= frozen_count {
            for (r, &h) in row_heights.iter().enumerate().take(frozen_count) {
                rows.push(r as u32);
                used += h;
            }
        }
        let band_start = cursor;
        while cursor < row_heights.len() {
            let h = row_heights[cursor];
            if cursor > band_start && used + h > body_h + 0.01 {
                break;
            }
            rows.push(cursor as u32);
            used += h;
            cursor += 1;
        }
        bands.push(rows);
    }
    bands
}

fn draw_page_header(
    doc: &mut PdfDocument,
    page: PageIndex,
    sheet: &Sheet,
    (page_w, page_h, margin): (f32, f32, f32),
    page_num: usize,
    total_pages: usize,
) {
    let top = page_h - margin;
    let title_style = TextStyle {
        size_pt: 11.0,
        fill_rgb: (0.10, 0.12, 0.15),
        bold: true,
        italic: false,
    };
    let page_style = TextStyle {
        size_pt: 9.0,
        fill_rgb: (0.38, 0.40, 0.44),
        bold: false,
        italic: false,
    };
    let title = if sheet.name.trim().is_empty() {
        "Sheet"
    } else {
        sheet.name.as_str()
    };
    let page_label = format!("Page {page_num} of {total_pages}");
    let page_label_w = text_width_pt(&page_label, &page_style);
    let max_title_w = (page_w - 2.0 * margin - page_label_w - 12.0).max(40.0);
    let clipped_title = fit_text(title, &title_style, max_title_w);

    doc.draw_text(page, margin, top - 13.0, &clipped_title, &title_style);
    doc.draw_text(
        page,
        page_w - margin - page_label_w,
        top - 13.0,
        &page_label,
        &page_style,
    );
    doc.draw_line(
        page,
        margin,
        top - PAGE_HEADER_HEIGHT_PT + 4.0,
        page_w - margin,
        top - PAGE_HEADER_HEIGHT_PT + 4.0,
        PathStyle::stroked((0.78, 0.80, 0.84), 0.75),
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_table_band(
    doc: &mut PdfDocument,
    page: PageIndex,
    sheet: &Sheet,
    evaluated: &HashMap<CellRef, Value>,
    table_top: f32,
    left_x: f32,
    cols: std::ops::Range<u32>,
    rows: &[u32],
    col_widths: &[f32],
    row_heights: &[f32],
    include_gridlines: bool,
) {
    let header_bg = PathStyle::filled((0.94, 0.95, 0.97));
    let grid_stroke = PathStyle::stroked((0.82, 0.84, 0.87), 0.5);
    let border_stroke = PathStyle::stroked((0.25, 0.27, 0.30), 0.9);
    let header_text = TextStyle {
        size_pt: 8.5,
        fill_rgb: (0.28, 0.30, 0.34),
        bold: true,
        italic: false,
    };

    // Corner cell
    let col_hdr_y = table_top - COL_HEADER_HEIGHT_PT;
    doc.draw_rect(
        page,
        left_x,
        col_hdr_y,
        ROW_GUTTER_WIDTH_PT,
        COL_HEADER_HEIGHT_PT,
        header_bg,
    );
    if include_gridlines {
        doc.draw_rect(
            page,
            left_x,
            col_hdr_y,
            ROW_GUTTER_WIDTH_PT,
            COL_HEADER_HEIGHT_PT,
            grid_stroke,
        );
    }

    // Column headers (A, B, C...)
    let mut x = left_x + ROW_GUTTER_WIDTH_PT;
    for col in cols.clone() {
        let w = col_widths[col as usize];
        doc.draw_rect(page, x, col_hdr_y, w, COL_HEADER_HEIGHT_PT, header_bg);
        if include_gridlines {
            doc.draw_rect(page, x, col_hdr_y, w, COL_HEADER_HEIGHT_PT, grid_stroke);
        }
        let label = column_letters(col);
        let label_w = text_width_pt(&label, &header_text);
        let text_x = x + ((w - label_w) * 0.5).max(2.0);
        doc.draw_text(page, text_x, col_hdr_y + 5.5, &label, &header_text);
        x += w;
    }

    // Data rows
    let mut y_top = col_hdr_y;
    for &row in rows {
        let h = row_heights[row as usize];
        let y_bot = y_top - h;

        // Row gutter
        doc.draw_rect(page, left_x, y_bot, ROW_GUTTER_WIDTH_PT, h, header_bg);
        if include_gridlines {
            doc.draw_rect(page, left_x, y_bot, ROW_GUTTER_WIDTH_PT, h, grid_stroke);
        }
        let row_label = (row + 1).to_string();
        let row_label_w = text_width_pt(&row_label, &header_text);
        let row_text_x = left_x + ((ROW_GUTTER_WIDTH_PT - row_label_w) * 0.5).max(2.0);
        let row_text_y = y_bot + ((h - header_text.size_pt) * 0.5).max(3.0) + 1.5;
        doc.draw_text(page, row_text_x, row_text_y, &row_label, &header_text);

        let mut cell_x = left_x + ROW_GUTTER_WIDTH_PT;
        for col in cols.clone() {
            let w = col_widths[col as usize];
            let cell_ref = CellRef { row, col };
            let style = sheet.cell_style(cell_ref);

            if let Some(fill_rgb) = fill_color_rgb(style.fill) {
                doc.draw_rect(page, cell_x, y_bot, w, h, PathStyle::filled(fill_rgb));
            }
            if style.border {
                doc.draw_rect(page, cell_x, y_bot, w, h, border_stroke);
            } else if include_gridlines {
                doc.draw_rect(page, cell_x, y_bot, w, h, grid_stroke);
            }

            let raw_value = evaluated
                .get(&cell_ref)
                .map(Value::display)
                .or_else(|| sheet.cells.get(&cell_ref).map(|c| c.raw.clone()))
                .unwrap_or_default();
            let display = style.format_value(&raw_value);
            if !display.is_empty() {
                let size_pt = (style.font_size.unwrap_or(13) as f32 * 0.72).clamp(7.0, 16.0);
                let text_style = TextStyle {
                    size_pt,
                    fill_rgb: style
                        .text_color
                        .map(rgb_from_u32)
                        .unwrap_or((0.10, 0.11, 0.12)),
                    bold: style.bold,
                    italic: style.italic,
                };
                let max_text_w = (w - 2.0 * CELL_PAD_X_PT).max(4.0);
                let clipped = fit_text(&display, &text_style, max_text_w);
                let text_w = text_width_pt(&clipped, &text_style);
                let alignment = sheet.cell_alignment(cell_ref);
                let is_numeric = evaluated
                    .get(&cell_ref)
                    .is_some_and(|v| matches!(v, Value::Number(_)));
                let align_right = alignment == CellAlignment::Right
                    || (alignment == CellAlignment::General && is_numeric);
                let text_x = if alignment == CellAlignment::Center {
                    cell_x + ((w - text_w) * 0.5).max(CELL_PAD_X_PT)
                } else if align_right {
                    cell_x + (w - CELL_PAD_X_PT - text_w).max(CELL_PAD_X_PT)
                } else {
                    cell_x + CELL_PAD_X_PT
                };
                let text_y = y_bot + ((h - size_pt) * 0.5).max(2.5) + 1.5;
                doc.draw_text(page, text_x, text_y, &clipped, &text_style);
            }

            cell_x += w;
        }
        y_top = y_bot;
    }
}

fn column_letters(col: u32) -> String {
    CellRef { row: 0, col }
        .to_a1()
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .to_string()
}

fn fill_color_rgb(fill: FillColor) -> Option<(f32, f32, f32)> {
    match fill {
        FillColor::None => None,
        FillColor::Red => Some((0.996, 0.792, 0.792)),
        FillColor::Orange => Some((0.996, 0.843, 0.667)),
        FillColor::Yellow => Some((0.996, 0.941, 0.541)),
        FillColor::Green => Some((0.733, 0.969, 0.816)),
        FillColor::Blue => Some((0.749, 0.859, 0.996)),
        FillColor::Purple => Some((0.867, 0.839, 0.996)),
        FillColor::Gray => Some((0.898, 0.906, 0.922)),
    }
}

fn rgb_from_u32(rgb: u32) -> (f32, f32, f32) {
    let r = ((rgb >> 16) & 0xFF) as f32 / 255.0;
    let g = ((rgb >> 8) & 0xFF) as f32 / 255.0;
    let b = (rgb & 0xFF) as f32 / 255.0;
    (r, g, b)
}

fn fit_text(text: &str, style: &TextStyle, max_width_pt: f32) -> String {
    if text_width_pt(text, style) <= max_width_pt {
        return text.to_string();
    }
    let ellipsis = "...";
    let ellipsis_w = text_width_pt(ellipsis, style);
    if ellipsis_w >= max_width_pt {
        return String::new();
    }
    let mut out = String::new();
    for ch in text.chars() {
        out.push(ch);
        if text_width_pt(&out, style) + ellipsis_w > max_width_pt {
            out.pop();
            break;
        }
    }
    out.push_str(ellipsis);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sheet_produces_valid_single_page_pdf() {
        let sheet = Sheet::new("Empty");
        let bytes = export_sheet_pdf(&sheet);
        assert!(bytes.starts_with(b"%PDF-1.4"));
        loom_pdf::inspect::verify(&bytes).expect("valid PDF");
        let pages = loom_pdf::inspect::page_text(&bytes).expect("page text");
        assert_eq!(pages.len(), 1);
        assert!(pages[0].contains("Empty"));
        assert!(pages[0].contains("Empty worksheet"));
    }

    #[test]
    fn evaluates_formulas_and_renders_headers_and_values() {
        let mut sheet = Sheet::new("Budget");
        sheet.set_str("A1", "Item");
        sheet.set_str("B1", "Cost");
        sheet.set_str("A2", "Rent");
        sheet.set_str("B2", "1200");
        sheet.set_str("A3", "Total");
        sheet.set_str("B3", "=SUM(B2:B2)");
        let bytes = export_sheet_pdf(&sheet);
        loom_pdf::inspect::verify(&bytes).expect("valid PDF");
        let pages = loom_pdf::inspect::page_text(&bytes).expect("page text");
        assert_eq!(pages.len(), 1);
        assert!(pages[0].contains("Budget"));
        assert!(pages[0].contains("Item"));
        assert!(pages[0].contains("1200"));
        assert!(pages[0].contains("Page 1 of 1"));
    }

    #[test]
    fn paginates_tall_and_wide_sheets_with_frozen_header_repetition() {
        let mut sheet = Sheet::new("Large");
        sheet.freeze_rows = 1;
        for col in 0..14u32 {
            let cell = CellRef { row: 0, col }.to_a1();
            sheet.set_str(&cell, &format!("H{col}"));
        }
        for row in 1..60u32 {
            for col in 0..14u32 {
                let cell = CellRef { row, col }.to_a1();
                sheet.set_str(&cell, &format!("R{row}C{col}"));
            }
        }
        let bytes = export_sheet_pdf(&sheet);
        loom_pdf::inspect::verify(&bytes).expect("valid PDF");
        let pages = loom_pdf::inspect::page_text(&bytes).expect("page text");
        assert!(
            pages.len() >= 4,
            "expected multi-page vertical+horizontal pagination, got {}",
            pages.len()
        );
        let all_text = loom_pdf::inspect::readable_content(&bytes).expect("readable content");
        assert!(all_text.contains("H0"));
        assert!(all_text.contains("R59C13"));
    }
}
