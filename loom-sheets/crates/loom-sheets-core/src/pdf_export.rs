//! Pure-Rust paginated PDF export for Loom Sheets.
//!
//! Renders the active worksheet's evaluated cells, formatting, borders, and
//! gridlines into a deterministic PDF 1.4 document via `loom_pdf`.

use std::collections::HashMap;

use loom_pdf::{text_width_pt, PageIndex, PathStyle, PdfDocument, TextStyle};

use crate::style::{CellAlignment, CellBorders, FillColor};
use crate::workbook::evaluate_workbook;
use crate::{CellRange, CellRef, Sheet, Value};

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

/// Scale at which every column (and the row gutter) fits `usable_w`, honoring
/// the 20% floor and the 10 pt minimum rendered column width.
fn fit_scale_for_width(raw_col_widths: &[f32], base_gutter: f32, usable_w: f32) -> f32 {
    let rendered = |scale: f32| -> f32 {
        base_gutter * scale
            + raw_col_widths
                .iter()
                .map(|w| (w * scale).max(10.0))
                .sum::<f32>()
    };
    if rendered(1.0) <= usable_w {
        return 1.0;
    }
    let (mut lo, mut hi) = (0.20f32, 1.0f32);
    if rendered(lo) > usable_w {
        return lo;
    }
    for _ in 0..24 {
        let mid = (lo + hi) * 0.5;
        if rendered(mid) <= usable_w {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn build_page_plan(
    sheets: &[Sheet],
    sheet_index: usize,
    options: PdfExportOptions,
) -> (Sheet, PageLayoutPlan) {
    let sheet = sheets
        .get(sheet_index)
        .cloned()
        .unwrap_or_else(|| Sheet::new("Sheet"));
    let (page_w, page_h) = options.resolved_page_size_pt();
    let margin = options
        .margin_pt
        .clamp(12.0, (page_w.min(page_h) / 4.0).max(12.0));
    let usable_w = (page_w - 2.0 * margin).max(72.0);
    let usable_h = (page_h - 2.0 * margin).max(72.0);
    let evaluated = if sheets.is_empty() || sheet_index >= sheets.len() {
        HashMap::new()
    } else {
        evaluate_workbook(sheets)
            .into_iter()
            .nth(sheet_index)
            .unwrap_or_default()
    };
    let sheet_title = if sheet.name.trim().is_empty() {
        "Sheet".to_string()
    } else {
        sheet.name.clone()
    };

    let Some((max_row, max_col)) = used_bounds(&sheet, &evaluated) else {
        return (
            sheet,
            PageLayoutPlan {
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
            },
        );
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
        .map(|col| column_width_pt(&sheet, col, raw_col_table_w))
        .collect();
    let raw_body_h = (usable_h - PAGE_HEADER_HEIGHT_PT - base_col_hdr).max(MIN_ROW_HEIGHT_PT);
    let raw_row_heights: Vec<f32> = (0..=max_row)
        .map(|row| row_height_pt(&sheet, row, raw_body_h))
        .collect();

    let scale_factor = match options.scale {
        PrintScale::Actual100 => 1.0,
        PrintScale::Scale75 => 0.75,
        PrintScale::Scale50 => 0.50,
        PrintScale::FitOnePageWide => fit_scale_for_width(&raw_col_widths, base_gutter, usable_w),
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

    // FitOnePageWide only keeps a single band when the scaled columns really
    // fit; otherwise (minimum widths/scale floor) fall back to column bands
    // so no column is silently dropped off the page.
    let col_bands = partition_columns(&col_widths, col_table_w);
    let frozen_rows = if options.repeat_frozen_rows {
        sheet.freeze_rows.min(max_row + 1)
    } else {
        0
    };
    let row_bands = partition_rows(&row_heights, body_h, frozen_rows);

    (
        sheet,
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
        },
    )
}

/// Export `sheet` as a paginated PDF document using default US Letter options.
pub fn export_sheet_pdf(sheet: &Sheet) -> Vec<u8> {
    export_sheet_pdf_with_options(sheet, PdfExportOptions::default())
}

/// Export `sheet` as a paginated PDF document with explicit page options.
pub fn export_sheet_pdf_with_options(sheet: &Sheet, options: PdfExportOptions) -> Vec<u8> {
    export_workbook_sheet_pdf_with_options(std::slice::from_ref(sheet), 0, options)
}

/// Export `sheets[sheet_index]` with full workbook evaluation context using default options.
pub fn export_workbook_sheet_pdf(sheets: &[Sheet], sheet_index: usize) -> Vec<u8> {
    export_workbook_sheet_pdf_with_options(sheets, sheet_index, PdfExportOptions::default())
}

/// Export `sheets[sheet_index]` with full workbook evaluation context and explicit options.
pub fn export_workbook_sheet_pdf_with_options(
    sheets: &[Sheet],
    sheet_index: usize,
    options: PdfExportOptions,
) -> Vec<u8> {
    let (sheet, plan) = build_page_plan(sheets, sheet_index, options);
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
                &sheet,
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
    preview_workbook_sheet_pages(std::slice::from_ref(sheet), 0, options)
}

/// Compute paginated page previews for `sheets[sheet_index]` with full workbook
/// evaluation context, sharing exact layout with [`export_workbook_sheet_pdf_with_options`].
pub fn preview_workbook_sheet_pages(
    sheets: &[Sheet],
    sheet_index: usize,
    options: PdfExportOptions,
) -> Vec<SheetPagePreview> {
    let (sheet, plan) = build_page_plan(sheets, sheet_index, options);
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
    (0..total_pages)
        .map(|idx| build_preview_page(&sheet, &plan, idx))
        .collect()
}

/// Number of printed pages for `sheets[sheet_index]` and the page it renders.
/// Only the cells of the requested 1-based `page` are materialized, so UI
/// navigation does not allocate cells for every page.
pub fn preview_workbook_sheet_page(
    sheets: &[Sheet],
    sheet_index: usize,
    options: PdfExportOptions,
    page: usize,
) -> (usize, SheetPagePreview) {
    let (sheet, plan) = build_page_plan(sheets, sheet_index, options);
    if plan.is_empty {
        let mut pages = preview_workbook_sheet_pages(sheets, sheet_index, options);
        return (1, pages.remove(0));
    }
    let total = plan.row_bands.len() * plan.col_bands.len();
    let idx = page.clamp(1, total.max(1)) - 1;
    (total, build_preview_page(&sheet, &plan, idx))
}

fn build_preview_page(sheet: &Sheet, plan: &PageLayoutPlan, page_idx: usize) -> SheetPagePreview {
    let total_pages = plan.row_bands.len() * plan.col_bands.len();
    let page_num = page_idx + 1;
    let row_band = &plan.row_bands[page_idx / plan.col_bands.len()];
    let (col_start, col_end) = plan.col_bands[page_idx % plan.col_bands.len()];
    {
        {
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
            for (row_idx, &row) in row_band.iter().enumerate() {
                let h = plan.row_heights[row as usize];
                let mut x_left = plan.margin + plan.row_gutter_w;
                for col in col_start..col_end {
                    let w = plan.col_widths[col as usize];
                    if let Some(span) =
                        band_cell_span(sheet, plan, row_band, row_idx, col_start..col_end, col)
                    {
                        let raw_value = if span.render_text {
                            plan.evaluated
                                .get(&span.anchor)
                                .map(Value::display)
                                .or_else(|| sheet.cells.get(&span.anchor).map(|c| c.raw.clone()))
                                .unwrap_or_default()
                        } else {
                            String::new()
                        };
                        let style = sheet.effective_style(span.anchor, &raw_value);
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
                        let max_text_w =
                            (span.width_pt - 2.0 * CELL_PAD_X_PT * plan.scale_factor).max(4.0);
                        let clipped = if display.is_empty() {
                            String::new()
                        } else if style.wrap {
                            let pad_y = 2.0 * plan.scale_factor;
                            let line_h = size_pt * 1.25;
                            let max_lines =
                                ((span.height_pt - 2.0 * pad_y) / line_h).floor().max(1.0) as usize;
                            wrap_text_lines(&display, &text_style, max_text_w, max_lines).join("\n")
                        } else {
                            fit_text(&display, &text_style, max_text_w)
                        };
                        cells.push(PagePreviewCell {
                            row,
                            col,
                            x_pt: x_left,
                            y_pt: y_top,
                            width_pt: span.width_pt,
                            height_pt: span.height_pt,
                            text: clipped,
                            bold: style.bold,
                            italic: style.italic,
                            align: resolved_alignment(sheet, plan, span.anchor),
                            fill_rgb: fill_color_rgb(style.fill),
                            text_rgb: text_style.fill_rgb,
                            has_border: style.borders != CellBorders::NONE,
                            font_size_pt: size_pt,
                        });
                    }
                    x_left += w;
                }
                y_top += h;
            }

            SheetPagePreview {
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
            }
        }
    }
}

/// Alignment with `General` resolved from the evaluated value (numbers go
/// right), shared by the PDF renderer and the on-screen preview.
fn resolved_alignment(sheet: &Sheet, plan: &PageLayoutPlan, cell: CellRef) -> CellAlignment {
    let alignment = sheet.cell_alignment(cell);
    let is_numeric = plan
        .evaluated
        .get(&cell)
        .is_some_and(|v| matches!(v, Value::Number(_)));
    if alignment == CellAlignment::General && is_numeric {
        CellAlignment::Right
    } else {
        alignment
    }
}

fn used_bounds(sheet: &Sheet, evaluated: &HashMap<CellRef, Value>) -> Option<(u32, u32)> {
    let mut max_row = 0u32;
    let mut max_col = 0u32;
    let mut any = false;
    let styled_refs = sheet
        .styles
        .iter()
        .filter(|(_, style)| !style.is_default())
        .map(|(cell_ref, _)| cell_ref);
    for cell_ref in sheet
        .cells
        .keys()
        .chain(evaluated.keys())
        .chain(styled_refs)
    {
        any = true;
        max_row = max_row.max(cell_ref.row);
        max_col = max_col.max(cell_ref.col);
    }
    for merge in &sheet.merged_ranges {
        any = true;
        max_row = max_row.max(merge.start.row).max(merge.end.row);
        max_col = max_col.max(merge.start.col).max(merge.end.col);
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
        if use_frozen
            && cursor >= frozen_count
            && used + frozen_h + row_heights[cursor] <= body_h + 0.01
        {
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
    for (row_idx, &row) in rows.iter().enumerate() {
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
            if let Some(span) = band_cell_span(sheet, plan, rows, row_idx, cols.clone(), col) {
                let span_w = span.width_pt;
                let span_h = span.height_pt;
                let span_y_bot = y_top - span_h;
                let raw_value = if span.render_text {
                    plan.evaluated
                        .get(&span.anchor)
                        .map(Value::display)
                        .or_else(|| sheet.cells.get(&span.anchor).map(|c| c.raw.clone()))
                        .unwrap_or_default()
                } else {
                    String::new()
                };
                let style = sheet.effective_style(span.anchor, &raw_value);

                if let Some(fill_rgb) = fill_color_rgb(style.fill) {
                    doc.draw_rect(
                        page,
                        cell_x,
                        span_y_bot,
                        span_w,
                        span_h,
                        PathStyle::filled(fill_rgb),
                    );
                }
                if include_gridlines {
                    doc.draw_rect(page, cell_x, span_y_bot, span_w, span_h, grid_stroke);
                }
                let borders = span_borders(sheet, &span);
                if borders != CellBorders::NONE {
                    draw_borders(
                        doc,
                        page,
                        cell_x,
                        span_y_bot,
                        span_w,
                        span_h,
                        borders,
                        border_stroke,
                    );
                }
                let display = style.format_value(&raw_value);
                if !display.is_empty() {
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
                    let pad_x = CELL_PAD_X_PT * plan.scale_factor;
                    let max_text_w = (span_w - 2.0 * pad_x).max(4.0);
                    let alignment = sheet.cell_alignment(span.anchor);
                    let align_right =
                        resolved_alignment(sheet, plan, span.anchor) == CellAlignment::Right;
                    let aligned_x = |line: &str| {
                        let line_w = text_width_pt(line, &text_style);
                        if alignment == CellAlignment::Center {
                            cell_x + ((span_w - line_w) * 0.5).max(pad_x)
                        } else if align_right {
                            cell_x + (span_w - pad_x - line_w).max(pad_x)
                        } else {
                            cell_x + pad_x
                        }
                    };

                    if style.wrap {
                        let pad_y = 2.0 * plan.scale_factor;
                        let line_h = size_pt * 1.25;
                        let max_lines = ((span_h - 2.0 * pad_y) / line_h).floor().max(1.0) as usize;
                        let lines = wrap_text_lines(&display, &text_style, max_text_w, max_lines);
                        let total_h = lines.len() as f32 * line_h;
                        let block_top = span_y_bot + span_h - ((span_h - total_h) * 0.5).max(pad_y);
                        for (idx, line) in lines.iter().enumerate() {
                            if line.is_empty() {
                                continue;
                            }
                            let line_y = block_top - (idx as f32 + 1.0) * line_h
                                + (line_h - size_pt) * 0.5
                                + 1.0;
                            doc.draw_text(page, aligned_x(line), line_y, line, &text_style);
                        }
                    } else {
                        let clipped = fit_text(&display, &text_style, max_text_w);
                        let text_y = span_y_bot + ((span_h - size_pt) * 0.5).max(2.0) + 1.0;
                        doc.draw_text(page, aligned_x(&clipped), text_y, &clipped, &text_style);
                    }
                }
            }

            cell_x += w;
        }
        y_top = y_bot;
    }
}

struct BandCellSpan {
    anchor: CellRef,
    /// Drawn extent of the span after clamping to this page band.
    covered: CellRange,
    width_pt: f32,
    height_pt: f32,
    render_text: bool,
}

fn band_cell_span(
    sheet: &Sheet,
    plan: &PageLayoutPlan,
    rows: &[u32],
    row_idx: usize,
    cols: std::ops::Range<u32>,
    col: u32,
) -> Option<BandCellSpan> {
    let row = rows[row_idx];
    for merge in &sheet.merged_ranges {
        let r0 = merge.start.row.min(merge.end.row);
        let r1 = merge.start.row.max(merge.end.row);
        let c0 = merge.start.col.min(merge.end.col);
        let c1 = merge.start.col.max(merge.end.col);
        if row < r0 || row > r1 || col < c0 || col > c1 {
            continue;
        }
        let first_col = c0.max(cols.start);
        if col != first_col {
            return None;
        }
        if row_idx > 0
            && rows[row_idx - 1] >= r0
            && rows[row_idx - 1] <= r1
            && rows[row_idx - 1] + 1 == row
        {
            return None;
        }
        let last_col = c1.min(cols.end.saturating_sub(1));
        let width_pt: f32 = (col..=last_col).map(|c| plan.col_widths[c as usize]).sum();
        let mut height_pt = 0.0f32;
        let mut last_row = row;
        for k in row_idx..rows.len() {
            let r = rows[k];
            if r > r1 || (k > row_idx && r != rows[k - 1] + 1) {
                break;
            }
            height_pt += plan.row_heights[r as usize];
            last_row = r;
        }
        return Some(BandCellSpan {
            anchor: CellRef { row: r0, col: c0 },
            // Only this page band can be drawn. Starting `covered` at the
            // merge's original anchor would make border lookup rescan rows that
            // were printed on earlier pages (quadratic for a tall merge).
            covered: CellRange::new(
                CellRef {
                    row,
                    col: first_col,
                },
                CellRef {
                    row: last_row,
                    col: last_col,
                },
            ),
            width_pt,
            height_pt,
            render_text: row == r0 && col == c0,
        });
    }
    Some(BandCellSpan {
        anchor: CellRef { row, col },
        covered: CellRange::new(CellRef { row, col }, CellRef { row, col }),
        width_pt: plan.col_widths[col as usize],
        height_pt: plan.row_heights[row as usize],
        render_text: true,
    })
}

/// Border edges for a possibly merged block.
///
/// `BorderPreset::Outer` stores each outer edge on the perimeter cell that owns
/// it, and merged spans skip every covered cell, so reading only the anchor
/// would lose the right edge of a horizontal merge (for example the right edge
/// of `A1:B1`) and the bottom edge of a vertical one.
fn span_borders(sheet: &Sheet, span: &BandCellSpan) -> CellBorders {
    let (r0, r1) = (span.covered.start.row, span.covered.end.row);
    let (c0, c1) = (span.covered.start.col, span.covered.end.col);
    if (r0, c0) == (r1, c1) {
        // Use the covered cell, not the merge anchor: on a continuation page
        // they differ, and only the drawn piece owns the edges here.
        return sheet.cell_style(span.covered.start).borders;
    }
    let mut borders = CellBorders::NONE;
    // Only perimeter cells can contribute an edge. Reading interior styles
    // cannot change the result.
    for col in c0..=c1 {
        borders.top |= sheet.cell_style(CellRef { row: r0, col }).borders.top;
        borders.bottom |= sheet.cell_style(CellRef { row: r1, col }).borders.bottom;
    }
    for row in r0..=r1 {
        borders.left |= sheet.cell_style(CellRef { row, col: c0 }).borders.left;
        borders.right |= sheet.cell_style(CellRef { row, col: c1 }).borders.right;
    }
    borders
}

#[allow(clippy::too_many_arguments)]
fn draw_borders(
    doc: &mut PdfDocument,
    page: PageIndex,
    x: f32,
    y_bot: f32,
    w: f32,
    h: f32,
    borders: CellBorders,
    stroke: PathStyle,
) {
    if borders.top {
        doc.draw_line(page, x, y_bot + h, x + w, y_bot + h, stroke);
    }
    if borders.right {
        doc.draw_line(page, x + w, y_bot, x + w, y_bot + h, stroke);
    }
    if borders.bottom {
        doc.draw_line(page, x, y_bot, x + w, y_bot, stroke);
    }
    if borders.left {
        doc.draw_line(page, x, y_bot, x, y_bot + h, stroke);
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

fn wrap_text_lines(
    text: &str,
    style: &TextStyle,
    max_width_pt: f32,
    max_lines: usize,
) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let para = para.strip_suffix('\r').unwrap_or(para);
        if para.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut current = String::new();
        for word in para.split(' ') {
            if word.is_empty() {
                continue;
            }
            if current.is_empty() {
                if text_width_pt(word, style) <= max_width_pt {
                    current.push_str(word);
                } else {
                    for ch in word.chars() {
                        current.push(ch);
                        if text_width_pt(&current, style) > max_width_pt {
                            current.pop();
                            if !current.is_empty() {
                                lines.push(std::mem::take(&mut current));
                            }
                            current.push(ch);
                        }
                    }
                }
            } else {
                let candidate = format!("{current} {word}");
                if text_width_pt(&candidate, style) <= max_width_pt {
                    current = candidate;
                } else {
                    lines.push(std::mem::take(&mut current));
                    if text_width_pt(word, style) <= max_width_pt {
                        current.push_str(word);
                    } else {
                        for ch in word.chars() {
                            current.push(ch);
                            if text_width_pt(&current, style) > max_width_pt {
                                current.pop();
                                if !current.is_empty() {
                                    lines.push(std::mem::take(&mut current));
                                }
                                current.push(ch);
                            }
                        }
                    }
                }
            }
        }
        lines.push(current);
    }

    if lines.len() > max_lines && max_lines > 0 {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            let ellipsis = "...";
            let ellipsis_w = text_width_pt(ellipsis, style);
            while !last.is_empty() && text_width_pt(last, style) + ellipsis_w > max_width_pt {
                last.pop();
            }
            if ellipsis_w <= max_width_pt {
                last.push_str(ellipsis);
            }
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_general_numbers_right_align_and_conditional_style_applies() {
        let mut sheet = Sheet::new("CF");
        sheet.set_str("A1", "150");
        sheet.set_str("A2", "text");
        sheet.add_conditional_rule(crate::CellConditionalRule {
            range: crate::CellRange::parse("A1:A2").unwrap(),
            operator: crate::ConditionalOperator::GreaterThan,
            value: "100".to_string(),
            fill: crate::style::FillColor::Yellow,
            text_color: Some(0x00C6_2828),
        });
        let options = PdfExportOptions::default();
        let pages = preview_sheet_pages(&sheet, options);
        let a1 = pages[0]
            .cells
            .iter()
            .find(|c| (c.row, c.col) == (0, 0))
            .unwrap();
        let a2 = pages[0]
            .cells
            .iter()
            .find(|c| (c.row, c.col) == (1, 0))
            .unwrap();
        assert_eq!(a1.align, CellAlignment::Right);
        assert_eq!(a2.align, CellAlignment::General);
        assert!(a1.fill_rgb.is_some());
        assert!(a2.fill_rgb.is_none());
        assert_eq!(a1.text_rgb, rgb_from_u32(0x00C6_2828));
        // PDF path must carry the same fill (yellow rect operator count differs).
        let plain = {
            let mut s = sheet.clone();
            s.clear_conditional_rules(crate::CellRange::parse("A1:A2").unwrap());
            export_sheet_pdf_with_options(&s, options)
        };
        assert_ne!(export_sheet_pdf_with_options(&sheet, options), plain);
    }

    #[test]
    fn single_page_preview_builds_only_requested_page() {
        let mut sheet = Sheet::new("Long");
        for row in 1..=200 {
            sheet.set_str(&format!("A{row}"), &row.to_string());
        }
        let options = PdfExportOptions::default();
        let all = preview_sheet_pages(&sheet, options);
        assert!(all.len() > 1);
        let (total, page2) =
            preview_workbook_sheet_page(std::slice::from_ref(&sheet), 0, options, 2);
        assert_eq!(total, all.len());
        assert_eq!(page2.page_number, 2);
        assert_eq!(page2.cells.len(), all[1].cells.len());
        let (_, clamped) =
            preview_workbook_sheet_page(std::slice::from_ref(&sheet), 0, options, 999);
        assert_eq!(clamped.page_number, total);
    }

    #[test]
    fn fit_one_page_wide_never_drops_columns() {
        let mut sheet = Sheet::new("Wide");
        for col in 0..100u32 {
            sheet.set_str(&crate::CellRef { row: 0, col }.to_a1(), "x");
        }
        let options = PdfExportOptions {
            scale: PrintScale::FitOnePageWide,
            ..PdfExportOptions::default()
        };
        let pages = preview_sheet_pages(&sheet, options);
        let mut seen = std::collections::BTreeSet::new();
        for page in &pages {
            let usable = page.width_pt - 2.0 * page.margin_pt;
            let used: f32 = page.row_gutter_pt + page.col_widths_pt.iter().sum::<f32>();
            assert!(used <= usable + 0.05, "band {used} exceeds {usable}");
            seen.extend(page.cells.iter().map(|c| c.col));
        }
        assert_eq!(seen.len(), 100);
    }

    #[test]
    fn merged_span_keeps_perimeter_borders_from_covered_cells() {
        // `BorderPreset::Outer` on A1:B1 stores the right edge on B1, and the
        // merged span skips B1, so the exported PDF would lose that edge.
        let mut sheet = Sheet::new("MergedBorder");
        sheet.set_str("A1", "Heading");
        let range = crate::CellRange::parse("A1:B1").expect("range");
        assert!(sheet.merge_cells(range));
        assert!(sheet.apply_border_preset(range, crate::BorderPreset::Outer));
        let options = PdfExportOptions {
            include_gridlines: false,
            ..PdfExportOptions::default()
        };
        let (plan_sheet, plan) = build_page_plan(std::slice::from_ref(&sheet), 0, options);
        let rows = plan.row_bands.first().expect("row band");
        let (first_col, last_col) = plan.col_bands[0];
        let span = band_cell_span(&plan_sheet, &plan, rows, 0, first_col..last_col, first_col)
            .expect("merged span");
        assert_eq!(span.covered.end.col, 1, "span covers A1:B1");
        let borders = span_borders(&plan_sheet, &span);
        assert!(borders.top && borders.bottom && borders.left);
        assert!(borders.right, "right edge stored on the covered cell B1");

        // A vertical merge keeps its bottom edge the same way.
        let mut sheet = Sheet::new("MergedVertical");
        sheet.set_str("A1", "Tall");
        let range = crate::CellRange::parse("A1:A3").expect("range");
        sheet.merge_cells(range);
        sheet.apply_border_preset(range, crate::BorderPreset::Outer);
        let (plan_sheet, plan) = build_page_plan(std::slice::from_ref(&sheet), 0, options);
        let rows = plan.row_bands[0].clone();
        let (first_col, last_col) = plan.col_bands[0];
        let span = band_cell_span(&plan_sheet, &plan, &rows, 0, first_col..last_col, first_col)
            .expect("merged span");
        assert_eq!(span.covered.end.row, 2, "span covers A1:A3");
        let borders = span_borders(&plan_sheet, &span);
        assert!(borders.bottom, "bottom edge stored on the covered cell A3");
        assert!(borders.top && borders.left && borders.right);
    }

    #[test]
    fn continuation_page_clips_the_merged_span_to_its_own_band() {
        // A merged range taller than a page is drawn in pieces. The piece on
        // page 2 must not rescan rows already printed on page 1: the merge's
        // top border belongs to page 1 only, and starting `covered` at the
        // original anchor is quadratic for a very tall merge.
        let mut sheet = Sheet::new("TallMerge");
        sheet.set_str("A1", "Tall");
        let range = crate::CellRange::parse("A1:A80").expect("range");
        assert!(sheet.merge_cells(range));
        assert!(sheet.apply_border_preset(range, crate::BorderPreset::Outer));
        let options = PdfExportOptions {
            include_gridlines: false,
            include_headers: false,
            ..PdfExportOptions::default()
        };
        let (plan_sheet, plan) = build_page_plan(std::slice::from_ref(&sheet), 0, options);
        assert!(
            plan.row_bands.len() > 1,
            "the merge must cross a page break"
        );
        let rows = plan.row_bands[1].clone();
        let first_page_row = rows[0];
        assert!(first_page_row > 0, "page 2 starts below the merge anchor");
        let (first_col, last_col) = plan.col_bands[0];
        let span = band_cell_span(&plan_sheet, &plan, &rows, 0, first_col..last_col, first_col)
            .expect("merged span on page 2");
        assert_eq!(span.anchor.row, 0, "the merge still starts at A1");
        assert_eq!(
            span.covered.start.row, first_page_row,
            "covered starts at the first row drawn on this page"
        );
        assert!(!span.render_text, "only the first piece draws the text");
        let borders = span_borders(&plan_sheet, &span);
        assert!(!borders.top, "the top edge stays on page 1");
        assert!(
            borders.left && borders.right,
            "side edges run down the merge"
        );
    }

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

    #[test]
    fn workbook_export_resolves_cross_sheet_formulas() {
        let mut s1 = Sheet::new("Summary");
        s1.set_str("A1", "=Data!A1*2");
        let mut s2 = Sheet::new("Data");
        s2.set_str("A1", "21");

        let pdf_bytes = export_workbook_sheet_pdf(&[s1, s2], 0);
        loom_pdf::inspect::verify(&pdf_bytes).expect("valid PDF");
        let pages = loom_pdf::inspect::page_text(&pdf_bytes).expect("page text");
        assert!(pages[0].contains("42"));
        assert!(!pages[0].contains("#REF!"));
    }

    #[test]
    fn styled_blank_cells_and_spilled_results_expand_used_bounds() {
        let mut styled_only = Sheet::new("StyledBlank");
        let style = crate::style::CellStyle {
            fill: FillColor::Yellow,
            ..Default::default()
        };
        styled_only.set_cell_style(CellRef::parse("C4").unwrap(), style);
        let previews = preview_sheet_pages(&styled_only, PdfExportOptions::default());
        assert!(!previews[0].is_empty);
        assert_eq!(previews[0].col_headers, vec!["A", "B", "C"]);
        assert_eq!(previews[0].row_headers, vec!["1", "2", "3", "4"]);

        let mut spilled = Sheet::new("Spill");
        spilled.set_str("A1", "=SEQUENCE(2,2)");
        let spill_previews = preview_sheet_pages(&spilled, PdfExportOptions::default());
        assert_eq!(spill_previews[0].col_headers, vec!["A", "B"]);
        assert_eq!(spill_previews[0].row_headers, vec!["1", "2"]);
        let pdf_bytes = export_sheet_pdf(&spilled);
        let pages = loom_pdf::inspect::page_text(&pdf_bytes).expect("page text");
        assert!(pages[0].contains("4"));
    }

    #[test]
    fn partition_rows_omits_frozen_header_when_first_body_row_does_not_fit() {
        let bands = partition_rows(&[300.0, 678.0], 678.0, 1);
        assert_eq!(bands, vec![vec![0], vec![1]]);
        for band in &bands {
            let total_h: f32 = band.iter().map(|&r| [300.0, 678.0][r as usize]).sum();
            assert!(total_h <= 678.01, "band height {total_h} exceeded body_h");
        }
    }

    #[test]
    fn merged_cells_span_columns_and_skip_covered_cells() {
        let mut sheet = Sheet::new("Merges");
        sheet.set_str("A1", "Wide Title");
        sheet.merged_ranges.push(crate::CellRange::new(
            CellRef::parse("A1").unwrap(),
            CellRef::parse("B1").unwrap(),
        ));
        let previews = preview_sheet_pages(&sheet, PdfExportOptions::default());
        assert_eq!(previews[0].cells.len(), 1);
        assert_eq!(previews[0].cells[0].row, 0);
        assert_eq!(previews[0].cells[0].col, 0);
        assert_eq!(
            previews[0].cells[0].width_pt,
            previews[0].col_widths_pt[0] + previews[0].col_widths_pt[1]
        );
    }

    #[test]
    fn single_edge_border_draws_only_enabled_edge() {
        let mut sheet = Sheet::new("Borders");
        sheet.set_str("A1", "Underlined");
        let style = crate::style::CellStyle {
            borders: CellBorders {
                bottom: true,
                ..CellBorders::NONE
            },
            ..Default::default()
        };
        sheet.set_cell_style(CellRef::parse("A1").unwrap(), style);
        let opts = PdfExportOptions {
            include_gridlines: false,
            include_headers: false,
            ..Default::default()
        };
        let pdf_bytes = export_sheet_pdf_with_options(&sheet, opts);
        loom_pdf::inspect::verify(&pdf_bytes).expect("valid PDF");
        let raw = loom_pdf::inspect::readable_content(&pdf_bytes).expect("readable content");
        // Border stroke width is 0.90; only 1 horizontal bottom border line (plus 1 header separator line at 0.75).
        let border_strokes = raw.matches("0.90 w").count();
        assert_eq!(border_strokes, 1, "expected 1 border stroke, got:\n{raw}");
    }

    #[test]
    fn wrapped_text_renders_multiple_lines_in_tall_row() {
        let mut sheet = Sheet::new("Wrapped");
        sheet.set_str("A1", "Alpha Bravo Charlie Delta");
        sheet.set_col_width(0, 65.0);
        sheet.set_row_height(0, 72.0);
        let style = crate::style::CellStyle {
            wrap: true,
            ..Default::default()
        };
        sheet.set_cell_style(CellRef::parse("A1").unwrap(), style);

        let previews = preview_sheet_pages(&sheet, PdfExportOptions::default());
        assert!(
            previews[0].cells[0].text.contains('\n'),
            "expected multi-line wrapped preview text, got {:?}",
            previews[0].cells[0].text
        );
        let pdf_bytes = export_sheet_pdf(&sheet);
        loom_pdf::inspect::verify(&pdf_bytes).expect("valid PDF");
        let pages = loom_pdf::inspect::page_text(&pdf_bytes).expect("page text");
        assert!(pages[0].contains("Alpha"));
        assert!(pages[0].contains("Delta"));
    }
}
