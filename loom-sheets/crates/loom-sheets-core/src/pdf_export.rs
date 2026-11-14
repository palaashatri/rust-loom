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

/// Standard paper sizes in PDF points (72 pt = 1 inch).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PaperSize {
    /// US Letter (8.5 x 11 in = 612 x 792 pt).
    #[default]
    Letter,
    /// ISO A4 (210 x 297 mm = 595.28 x 841.89 pt).
    A4,
    /// US Legal (8.5 x 14 in = 612 x 1008 pt).
    Legal,
}

impl PaperSize {
    /// Portrait width and height in PDF points.
    pub fn dimensions_pt(self) -> (f32, f32) {
        match self {
            Self::Letter => (612.0, 792.0),
            Self::A4 => (595.28, 841.89),
            Self::Legal => (612.0, 1008.0),
        }
    }
}

/// Page orientation for PDF export and print preview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageOrientation {
    #[default]
    Portrait,
    Landscape,
}

/// Scaling mode applied to worksheet columns and rows when paginating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PrintScale {
    #[default]
    Actual100,
    Scale75,
    Scale50,
    FitOnePageWide,
}

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
    /// Standard paper size (default: Letter).
    pub paper_size: PaperSize,
    /// Page orientation (default: Portrait).
    pub orientation: PageOrientation,
    /// Scaling mode (default: Actual100).
    pub scale: PrintScale,
    /// Whether to include column (A, B, C...) and row (1, 2, 3...) headers.
    pub include_headers: bool,
}

impl Default for PdfExportOptions {
    fn default() -> Self {
        Self {
            page_width_pt: 612.0,
            page_height_pt: 792.0,
            margin_pt: 36.0,
            include_gridlines: true,
            repeat_frozen_rows: true,
            paper_size: PaperSize::Letter,
            orientation: PageOrientation::Portrait,
            scale: PrintScale::Actual100,
            include_headers: true,
        }
    }
}

impl PdfExportOptions {
    /// Resolve effective `(width_pt, height_pt)` accounting for `paper_size`,
    /// custom dimensions, and `orientation`.
    pub fn resolved_page_size_pt(&self) -> (f32, f32) {
        let (base_w, base_h) = if self.paper_size == PaperSize::Letter
            && ((self.page_width_pt - 612.0).abs() > 0.01
                || (self.page_height_pt - 792.0).abs() > 0.01)
        {
            (self.page_width_pt, self.page_height_pt)
        } else {
            self.paper_size.dimensions_pt()
        };
        let (w, h) = match self.orientation {
            PageOrientation::Portrait => (base_w.min(base_h), base_w.max(base_h)),
            PageOrientation::Landscape => (base_w.max(base_h), base_w.min(base_h)),
        };
        (w.max(144.0), h.max(144.0))
    }
}

/// A single rendered cell inside a [`SheetPagePreview`].
#[derive(Debug, Clone, PartialEq)]
pub struct PagePreviewCell {
    pub row: u32,
    pub col: u32,
    pub x_pt: f32,
    pub y_pt: f32,
    pub width_pt: f32,
    pub height_pt: f32,
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub align: CellAlignment,
    pub fill_rgb: Option<(f32, f32, f32)>,
    pub text_rgb: (f32, f32, f32),
    pub has_border: bool,
    pub font_size_pt: f32,
}

/// Paginated preview of one worksheet page, sharing exact layout with PDF export.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetPagePreview {
    pub page_number: usize,
    pub total_pages: usize,
    pub width_pt: f32,
    pub height_pt: f32,
    pub margin_pt: f32,
    pub sheet_title: String,
    pub col_headers: Vec<String>,
    pub col_widths_pt: Vec<f32>,
    pub row_headers: Vec<String>,
    pub row_heights_pt: Vec<f32>,
    pub row_gutter_pt: f32,
    pub col_header_height_pt: f32,
    pub page_header_height_pt: f32,
    pub include_headers: bool,
    pub include_gridlines: bool,
    pub is_empty: bool,
    pub cells: Vec<PagePreviewCell>,
}

struct PageLayoutPlan {
    page_w: f32,
    page_h: f32,
    margin: f32,
    scale_factor: f32,
    row_gutter_w: f32,
    col_header_h: f32,
    include_headers: bool,
    include_gridlines: bool,
    sheet_title: String,
    evaluated: HashMap<CellRef, Value>,
    col_widths: Vec<f32>,
    row_heights: Vec<f32>,
    col_bands: Vec<(u32, u32)>,
    row_bands: Vec<Vec<u32>>,
    is_empty: bool,
}

fn build_page_plan(sheet: &Sheet, options: PdfExportOptions) -> PageLayoutPlan {
    let (page_w, page_h) = options.resolved_page_size_pt();
    let margin = options
        .margin_pt
        .clamp(12.0, (page_w.min(page_h) / 4.0).max(12.0));
    let usable_w = (page_w - 2.0 * margin).max(72.0);
    let usable_h = (page_h - 2.0 * margin).max(72.0);
    let evaluated = evaluate(sheet);
    let sheet_title = if sheet.name.trim().is_empty() {
        "Sheet".to_string()
    } else {
        sheet.name.clone()
    };

    let Some((max_row, max_col)) = used_bounds(sheet) else {
        return PageLayoutPlan {
            page_w,
            page_h,
            margin,
            scale_factor: 1.0,
            row_gutter_w: 0.0,
            col_header_h: 0.0,
            include_headers: options.include_headers,
            include_gridlines: options.include_gridlines,
            sheet_title,
            evaluated,
            col_widths: Vec::new(),
            row_heights: Vec::new(),
            col_bands: Vec::new(),
            row_bands: Vec::new(),
            is_empty: true,
        };
    };

    let base_gutter = if options.include_headers {
        ROW_GUTTER_WIDTH_PT
    } else {
        0.0
    };
    let base_col_hdr = if options.include_headers {
        COL_HEADER_HEIGHT_PT
    } else {
        0.0
    };
    let raw_col_table_w = (usable_w - base_gutter).max(MIN_COL_WIDTH_PT);
    let raw_col_widths: Vec<f32> = (0..=max_col)
        .map(|col| column_width_pt(sheet, col, raw_col_table_w))
        .collect();
    let raw_body_h = (usable_h - PAGE_HEADER_HEIGHT_PT - base_col_hdr).max(MIN_ROW_HEIGHT_PT);
    let raw_row_heights: Vec<f32> = (0..=max_row)
        .map(|row| row_height_pt(sheet, row, raw_body_h))
        .collect();

    let scale_factor = match options.scale {
        PrintScale::Actual100 => 1.0,
        PrintScale::Scale75 => 0.75,
        PrintScale::Scale50 => 0.50,
        PrintScale::FitOnePageWide => {
            let total_w: f32 = raw_col_widths.iter().sum();
            if total_w > raw_col_table_w && total_w > 0.0 {
                (raw_col_table_w / total_w).clamp(0.20, 1.0)
            } else {
                1.0
            }
        }
    };

    let row_gutter_w = base_gutter * scale_factor;
    let col_header_h = base_col_hdr * scale_factor;
    let col_table_w = (usable_w - row_gutter_w).max(24.0);
    let body_h = (usable_h - PAGE_HEADER_HEIGHT_PT - col_header_h).max(16.0);

    let col_widths: Vec<f32> = raw_col_widths
        .into_iter()
        .map(|w| (w * scale_factor).max(10.0))
        .collect();
    let row_heights: Vec<f32> = raw_row_heights
        .into_iter()
        .map(|h| (h * scale_factor).max(10.0))
        .collect();

    let col_bands = if options.scale == PrintScale::FitOnePageWide {
        vec![(0, max_col + 1)]
    } else {
        partition_columns(&col_widths, col_table_w)
    };
    let frozen_rows = if options.repeat_frozen_rows {
        sheet.freeze_rows.min(max_row + 1)
    } else {
        0
    };
    let row_bands = partition_rows(&row_heights, body_h, frozen_rows);

    PageLayoutPlan {
        page_w,
        page_h,
        margin,
        scale_factor,
        row_gutter_w,
        col_header_h,
        include_headers: options.include_headers,
        include_gridlines: options.include_gridlines,
        sheet_title,
        evaluated,
        col_widths,
        row_heights,
        col_bands,
        row_bands,
        is_empty: false,
    }
}

/// Export `sheet` as a paginated PDF document using default US Letter options.
pub fn export_sheet_pdf(sheet: &Sheet) -> Vec<u8> {
    export_sheet_pdf_with_options(sheet, PdfExportOptions::default())
}

/// Export `sheet` as a paginated PDF document with explicit page options.
pub fn export_sheet_pdf_with_options(sheet: &Sheet, options: PdfExportOptions) -> Vec<u8> {
    let plan = build_page_plan(sheet, options);
    let mut doc = PdfDocument::new();

    if plan.is_empty {
        let page = doc.add_page(plan.page_w, plan.page_h);
        draw_page_header(
            &mut doc,
            page,
            &plan.sheet_title,
            (plan.page_w, plan.page_h, plan.margin),
            1,
            1,
        );
        let empty_style = TextStyle {
            size_pt: 10.0,
            fill_rgb: (0.35, 0.37, 0.40),
            bold: false,
            italic: true,
        };
        let y = plan.page_h - plan.margin - PAGE_HEADER_HEIGHT_PT - 18.0;
        doc.draw_text(page, plan.margin, y, "Empty worksheet", &empty_style);
        return doc.serialize();
    }

    let total_pages = plan.row_bands.len() * plan.col_bands.len();
    let mut page_num = 0usize;

    for row_band in &plan.row_bands {
        for &(col_start, col_end) in &plan.col_bands {
            page_num += 1;
            let page = doc.add_page(plan.page_w, plan.page_h);
            draw_page_header(
                &mut doc,
                page,
                &plan.sheet_title,
                (plan.page_w, plan.page_h, plan.margin),
                page_num,
                total_pages,
            );
            draw_table_band(
                &mut doc,
                page,
                sheet,
                &plan,
                plan.page_h - plan.margin - PAGE_HEADER_HEIGHT_PT,
                col_start..col_end,
                row_band,
            );
        }
    }

    doc.serialize()
}

/// Compute paginated page previews for `sheet` using the exact same pagination
/// and cell formatting logic as [`export_sheet_pdf_with_options`].
pub fn preview_sheet_pages(sheet: &Sheet, options: PdfExportOptions) -> Vec<SheetPagePreview> {
    let plan = build_page_plan(sheet, options);
    if plan.is_empty {
        return vec![SheetPagePreview {
            page_number: 1,
            total_pages: 1,
            width_pt: plan.page_w,
            height_pt: plan.page_h,
            margin_pt: plan.margin,
            sheet_title: plan.sheet_title,
            col_headers: Vec::new(),
            col_widths_pt: Vec::new(),
            row_headers: Vec::new(),
            row_heights_pt: Vec::new(),
            row_gutter_pt: 0.0,
            col_header_height_pt: 0.0,
            page_header_height_pt: PAGE_HEADER_HEIGHT_PT,
            include_headers: plan.include_headers,
            include_gridlines: plan.include_gridlines,
            is_empty: true,
            cells: Vec::new(),
        }];
    }

    let total_pages = plan.row_bands.len() * plan.col_bands.len();
    let mut pages = Vec::with_capacity(total_pages);
    let mut page_num = 0usize;

    for row_band in &plan.row_bands {
        for &(col_start, col_end) in &plan.col_bands {
            page_num += 1;
            let col_headers = if plan.include_headers {
                (col_start..col_end).map(column_letters).collect()
            } else {
                Vec::new()
            };
            let col_widths_pt: Vec<f32> = (col_start..col_end)
                .map(|c| plan.col_widths[c as usize])
                .collect();
            let row_headers = if plan.include_headers {
                row_band.iter().map(|r| (r + 1).to_string()).collect()
            } else {
                Vec::new()
            };
            let row_heights_pt: Vec<f32> = row_band
                .iter()
                .map(|&r| plan.row_heights[r as usize])
                .collect();

            let mut cells = Vec::with_capacity(row_band.len() * (col_end - col_start) as usize);
            let mut y_top = plan.margin + PAGE_HEADER_HEIGHT_PT + plan.col_header_h;
            for &row in row_band {
                let h = plan.row_heights[row as usize];
                let mut x_left = plan.margin + plan.row_gutter_w;
                for col in col_start..col_end {
                    let w = plan.col_widths[col as usize];
                    let cell_ref = CellRef { row, col };
                    let style = sheet.cell_style(cell_ref);
                    let raw_value = plan
                        .evaluated
                        .get(&cell_ref)
                        .map(Value::display)
                        .or_else(|| sheet.cells.get(&cell_ref).map(|c| c.raw.clone()))
                        .unwrap_or_default();
                    let display = style.format_value(&raw_value);
                    let size_pt = ((style.font_size.unwrap_or(13) as f32 * 0.72)
                        * plan.scale_factor)
                        .clamp(5.5, 16.0);
                    let text_style = TextStyle {
                        size_pt,
                        fill_rgb: style
                            .text_color
                            .map(rgb_from_u32)
                            .unwrap_or((0.10, 0.11, 0.12)),
                        bold: style.bold,
                        italic: style.italic,
                    };
                    let max_text_w = (w - 2.0 * CELL_PAD_X_PT * plan.scale_factor).max(4.0);
                    let clipped = if display.is_empty() {
                        String::new()
                    } else {
                        fit_text(&display, &text_style, max_text_w)
                    };
                    cells.push(PagePreviewCell {
                        row,
                        col,
                        x_pt: x_left,
                        y_pt: y_top,
                        width_pt: w,
                        height_pt: h,
                        text: clipped,
                        bold: style.bold,
                        italic: style.italic,
                        align: sheet.cell_alignment(cell_ref),
                        fill_rgb: fill_color_rgb(style.fill),
                        text_rgb: text_style.fill_rgb,
                        has_border: style.borders != crate::style::CellBorders::NONE,
                        font_size_pt: size_pt,
                    });
                    x_left += w;
                }
                y_top += h;
            }

            pages.push(SheetPagePreview {
                page_number: page_num,
                total_pages,
                width_pt: plan.page_w,
                height_pt: plan.page_h,
                margin_pt: plan.margin,
                sheet_title: plan.sheet_title.clone(),
                col_headers,
                col_widths_pt,
                row_headers,
                row_heights_pt,
                row_gutter_pt: plan.row_gutter_w,
                col_header_height_pt: plan.col_header_h,
                page_header_height_pt: PAGE_HEADER_HEIGHT_PT,
                include_headers: plan.include_headers,
                include_gridlines: plan.include_gridlines,
                is_empty: false,
                cells,
            });
        }
    }

    pages
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
    title: &str,
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

fn draw_table_band(
    doc: &mut PdfDocument,
    page: PageIndex,
    sheet: &Sheet,
    plan: &PageLayoutPlan,
    table_top: f32,
    cols: std::ops::Range<u32>,
    rows: &[u32],
) {
    let left_x = plan.margin;
    let include_gridlines = plan.include_gridlines;
    let header_bg = PathStyle::filled((0.94, 0.95, 0.97));
    let grid_stroke = PathStyle::stroked((0.82, 0.84, 0.87), 0.5);
    let border_stroke = PathStyle::stroked((0.25, 0.27, 0.30), 0.9);
    let header_text = TextStyle {
        size_pt: (8.5 * plan.scale_factor).clamp(5.5, 10.0),
        fill_rgb: (0.28, 0.30, 0.34),
        bold: true,
        italic: false,
    };

    let col_hdr_y = table_top - plan.col_header_h;
    if plan.include_headers {
        // Corner cell
        doc.draw_rect(
            page,
            left_x,
            col_hdr_y,
            plan.row_gutter_w,
            plan.col_header_h,
            header_bg,
        );
        if include_gridlines {
            doc.draw_rect(
                page,
                left_x,
                col_hdr_y,
                plan.row_gutter_w,
                plan.col_header_h,
                grid_stroke,
            );
        }

        // Column headers (A, B, C...)
        let mut x = left_x + plan.row_gutter_w;
        for col in cols.clone() {
            let w = plan.col_widths[col as usize];
            doc.draw_rect(page, x, col_hdr_y, w, plan.col_header_h, header_bg);
            if include_gridlines {
                doc.draw_rect(page, x, col_hdr_y, w, plan.col_header_h, grid_stroke);
            }
            let label = column_letters(col);
            let label_w = text_width_pt(&label, &header_text);
            let text_x = x + ((w - label_w) * 0.5).max(2.0);
            let text_y = col_hdr_y + ((plan.col_header_h - header_text.size_pt) * 0.5).max(2.0);
            doc.draw_text(page, text_x, text_y, &label, &header_text);
            x += w;
        }
    }

    // Data rows
    let mut y_top = col_hdr_y;
    for &row in rows {
        let h = plan.row_heights[row as usize];
        let y_bot = y_top - h;

        if plan.include_headers {
            // Row gutter
            doc.draw_rect(page, left_x, y_bot, plan.row_gutter_w, h, header_bg);
            if include_gridlines {
                doc.draw_rect(page, left_x, y_bot, plan.row_gutter_w, h, grid_stroke);
            }
            let row_label = (row + 1).to_string();
            let row_label_w = text_width_pt(&row_label, &header_text);
            let row_text_x = left_x + ((plan.row_gutter_w - row_label_w) * 0.5).max(2.0);
            let row_text_y = y_bot + ((h - header_text.size_pt) * 0.5).max(2.0) + 1.0;
            doc.draw_text(page, row_text_x, row_text_y, &row_label, &header_text);
        }

        let mut cell_x = left_x + plan.row_gutter_w;
        for col in cols.clone() {
            let w = plan.col_widths[col as usize];
            let cell_ref = CellRef { row, col };
            let style = sheet.cell_style(cell_ref);

            if let Some(fill_rgb) = fill_color_rgb(style.fill) {
                doc.draw_rect(page, cell_x, y_bot, w, h, PathStyle::filled(fill_rgb));
            }
            if style.borders != crate::style::CellBorders::NONE {
                doc.draw_rect(page, cell_x, y_bot, w, h, border_stroke);
            } else if include_gridlines {
                doc.draw_rect(page, cell_x, y_bot, w, h, grid_stroke);
            }

            let raw_value = plan
                .evaluated
                .get(&cell_ref)
                .map(Value::display)
                .or_else(|| sheet.cells.get(&cell_ref).map(|c| c.raw.clone()))
                .unwrap_or_default();
            let display = style.format_value(&raw_value);
            if !display.is_empty() {
                let size_pt = ((style.font_size.unwrap_or(13) as f32 * 0.72) * plan.scale_factor)
                    .clamp(5.5, 16.0);
                let text_style = TextStyle {
                    size_pt,
                    fill_rgb: style
                        .text_color
                        .map(rgb_from_u32)
                        .unwrap_or((0.10, 0.11, 0.12)),
                    bold: style.bold,
                    italic: style.italic,
                };
                let pad_x = CELL_PAD_X_PT * plan.scale_factor;
                let max_text_w = (w - 2.0 * pad_x).max(4.0);
                let clipped = fit_text(&display, &text_style, max_text_w);
                let text_w = text_width_pt(&clipped, &text_style);
                let alignment = sheet.cell_alignment(cell_ref);
                let is_numeric = plan
                    .evaluated
                    .get(&cell_ref)
                    .is_some_and(|v| matches!(v, Value::Number(_)));
                let align_right = alignment == CellAlignment::Right
                    || (alignment == CellAlignment::General && is_numeric);
                let text_x = if alignment == CellAlignment::Center {
                    cell_x + ((w - text_w) * 0.5).max(pad_x)
                } else if align_right {
                    cell_x + (w - pad_x - text_w).max(pad_x)
                } else {
                    cell_x + pad_x
                };
                let text_y = y_bot + ((h - size_pt) * 0.5).max(2.0) + 1.0;
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

    #[test]
    fn paper_size_and_orientation_resolve_dimensions() {
        let mut opts = PdfExportOptions::default();
        assert_eq!(opts.resolved_page_size_pt(), (612.0, 792.0));

        opts.orientation = PageOrientation::Landscape;
        assert_eq!(opts.resolved_page_size_pt(), (792.0, 612.0));

        opts.paper_size = PaperSize::A4;
        opts.orientation = PageOrientation::Portrait;
        assert_eq!(opts.resolved_page_size_pt(), (595.28, 841.89));

        opts.paper_size = PaperSize::Legal;
        opts.orientation = PageOrientation::Landscape;
        assert_eq!(opts.resolved_page_size_pt(), (1008.0, 612.0));
    }

    #[test]
    fn preview_shares_exact_pagination_and_scale_with_pdf_export() {
        let mut sheet = Sheet::new("Forecast");
        sheet.freeze_rows = 1;
        for col in 0..14u32 {
            sheet.set_str(&CellRef { row: 0, col }.to_a1(), &format!("Col{col}"));
            sheet.set_str(&CellRef { row: 1, col }.to_a1(), &format!("Val{col}"));
        }

        let fit_opts = PdfExportOptions {
            scale: PrintScale::FitOnePageWide,
            orientation: PageOrientation::Landscape,
            ..Default::default()
        };
        let previews = preview_sheet_pages(&sheet, fit_opts);
        let pdf_bytes = export_sheet_pdf_with_options(&sheet, fit_opts);
        let pdf_pages = loom_pdf::inspect::page_text(&pdf_bytes).expect("page text");
        assert_eq!(previews.len(), pdf_pages.len());
        assert_eq!(previews.len(), 1);
        assert_eq!(previews[0].width_pt, 792.0);
        assert_eq!(previews[0].height_pt, 612.0);
        assert_eq!(previews[0].col_headers.len(), 14);
        assert!(previews[0].cells.iter().any(|c| c.text == "Col13"));
    }

    #[test]
    fn include_headers_false_omits_headers_in_preview_and_pdf() {
        let mut sheet = Sheet::new("Clean");
        sheet.set_str("A1", "Alpha");
        let opts = PdfExportOptions {
            include_headers: false,
            include_gridlines: false,
            ..Default::default()
        };
        let previews = preview_sheet_pages(&sheet, opts);
        assert_eq!(previews.len(), 1);
        assert!(previews[0].col_headers.is_empty());
        assert!(previews[0].row_headers.is_empty());
        assert_eq!(previews[0].row_gutter_pt, 0.0);
        assert_eq!(previews[0].col_header_height_pt, 0.0);

        let pdf_bytes = export_sheet_pdf_with_options(&sheet, opts);
        loom_pdf::inspect::verify(&pdf_bytes).expect("valid PDF");
        let pages = loom_pdf::inspect::page_text(&pdf_bytes).expect("page text");
        assert!(pages[0].contains("Alpha"));
    }

    #[test]
    fn empty_sheet_preview_returns_single_empty_page() {
        let sheet = Sheet::new("Blank");
        let previews = preview_sheet_pages(&sheet, PdfExportOptions::default());
        assert_eq!(previews.len(), 1);
        assert!(previews[0].is_empty);
        assert_eq!(previews[0].page_number, 1);
        assert_eq!(previews[0].total_pages, 1);
        assert!(previews[0].cells.is_empty());
    }
}
