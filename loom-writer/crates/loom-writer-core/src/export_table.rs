//! Table blocks in the PDF export. Each row's cells are drawn at fixed column
//! positions, without Markdown pipes or the separator row, so a table reads as
//! a table rather than as Markdown source.

use loom_pdf::{text_width_pt, PageIndex, PathStyle, PdfDocument, TextStyle};

use crate::parse_table_markdown;

/// Space kept between a cell and the next column.
const GUTTER_PT: f32 = 12.0;
/// Narrowest column, so an empty column of a new table is still drawn.
const MIN_COLUMN_PT: f32 = 48.0;
/// Colour of the cell rules.
const RULE_RGB: (f32, f32, f32) = (0.55, 0.55, 0.55);
/// Marks a cell that was shortened to fit its column.
const ELLIPSIS: char = '\u{2026}';

/// Column positions shared by every row of one table.
pub(crate) struct TableLayout {
    header_row: bool,
    starts: Vec<f32>,
    widths: Vec<f32>,
    /// The table's width, from its left edge to the end of its last column.
    pub(crate) width: f32,
    /// The body style the cells are set in.
    body: TextStyle,
}

impl TableLayout {
    /// Sizes each column to its widest cell. When the table is wider than
    /// `max_width` the columns shrink together, and their cells are shortened
    /// to fit when drawn.
    pub(crate) fn new(markdown: &str, body: &TextStyle, max_width: f32) -> Self {
        let table = parse_table_markdown(markdown);
        let mut natural = vec![MIN_COLUMN_PT; table.columns()];
        for (index, row) in table.rows.iter().enumerate() {
            let style = cell_style(body, table.header_row && index == 0);
            for (column, cell) in row.iter().enumerate() {
                natural[column] = natural[column].max(text_width_pt(cell, &style) + GUTTER_PT);
            }
        }
        let total: f32 = natural.iter().sum();
        let scale = if total > max_width && total > 0.0 {
            max_width / total
        } else {
            1.0
        };
        let widths: Vec<f32> = natural.iter().map(|width| width * scale).collect();
        let mut starts = Vec::with_capacity(widths.len());
        let mut width = 0.0;
        for column in &widths {
            starts.push(width);
            width += column;
        }
        Self {
            header_row: table.header_row,
            starts,
            widths,
            width,
            body: body.clone(),
        }
    }
}

/// Draws the table row that begins at byte `start` of the block's `text`, with
/// the table's top-left corner at `origin` and the row box `row_height` tall.
/// Every row gets its cell rules, so an empty table still shows its columns;
/// the first row also gets a top rule. A line that begins inside another line
/// draws nothing.
pub(crate) fn draw_line(
    pdf: &mut PdfDocument,
    page: PageIndex,
    text: &str,
    start: usize,
    layout: &TableLayout,
    origin: (f32, f32),
    row_height: f32,
) {
    let Some(line) = line_starting_at(text, start) else {
        return;
    };
    let Some(cells) = parse_table_markdown(line).rows.into_iter().next() else {
        return;
    };
    draw_rules(pdf, page, layout, origin, row_height, start == 0);
    let style = cell_style(&layout.body, layout.header_row && start == 0);
    for (column, cell) in cells.iter().enumerate().take(layout.starts.len()) {
        let fitted = fit(cell, &style, layout.widths[column] - GUTTER_PT);
        let x = origin.0 + layout.starts[column];
        pdf.draw_text(page, x, origin.1, &fitted, &style);
    }
}

/// Draws one row's box: a rule under it, a vertical rule at every column edge,
/// and a rule above the table when `first`.
fn draw_rules(
    pdf: &mut PdfDocument,
    page: PageIndex,
    layout: &TableLayout,
    origin: (f32, f32),
    row_height: f32,
    first: bool,
) {
    let rule = PathStyle::stroked(RULE_RGB, 0.5);
    // The text sits on its baseline, so the box runs from one em above it to
    // the bottom of the row.
    let size = layout.body.size_pt;
    let top = origin.1 + size;
    let bottom = origin.1 - (row_height - size).max(0.0);
    let right = origin.0 + layout.width;
    for start in &layout.starts {
        let x = origin.0 + start;
        pdf.draw_line(page, x, top, x, bottom, rule);
    }
    pdf.draw_line(page, right, top, right, bottom, rule);
    pdf.draw_line(page, origin.0, bottom, right, bottom, rule);
    if first {
        pdf.draw_line(page, origin.0, top, right, top, rule);
    }
}

/// The whole Markdown line that begins at byte `start`, or `None` when `start`
/// lies inside a line.
fn line_starting_at(text: &str, start: usize) -> Option<&str> {
    let before = text.get(..start)?;
    if !before.is_empty() && !before.ends_with('\n') {
        return None;
    }
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |offset| start + offset);
    Some(&text[start..end])
}

/// The body style, in bold for a header cell.
fn cell_style(body: &TextStyle, header: bool) -> TextStyle {
    TextStyle {
        bold: body.bold || header,
        ..body.clone()
    }
}

/// The cell text, shortened with an ellipsis until it fits `max_width`.
fn fit(text: &str, style: &TextStyle, max_width: f32) -> String {
    if text_width_pt(text, style) <= max_width {
        return text.to_string();
    }
    let mut kept = String::new();
    for ch in text.chars() {
        kept.push(ch);
        if text_width_pt(&format!("{kept}{ELLIPSIS}"), style) > max_width {
            kept.pop();
            break;
        }
    }
    format!("{kept}{ELLIPSIS}")
}
