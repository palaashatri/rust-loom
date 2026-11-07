//! Table blocks in the PDF export. Each row's cells are wrapped to their
//! columns and drawn at fixed column positions, without Markdown pipes or the
//! separator row, so a table reads as a table rather than as Markdown source.
//! A row is as tall as its tallest wrapped cell.

use loom_objects::table::Table;
use loom_objects::{Border, BorderStyle, TableStyleCatalog, TableStyleOptions};
use loom_pdf::{PageIndex, PathStyle, PdfDocument};

use crate::export::shaped_export::{self, FontRegistry};
use crate::{
    measure_table_row_geometry_with_footnotes, shape_table_cell_geometry_with_footnotes,
    table_cell_rich_block, FootnoteReferencePlacement, RichBlock, TableTextMap, WriterDocument,
    WriterError, WriterShapedVisualLine, TABLE_CELL_BASE_FONT_SIZE_PT,
    TABLE_CELL_HORIZONTAL_INSET_PT, TABLE_CELL_VERTICAL_INSET_PT,
};

/// Space between a cell's text and each of its column rules.
const CELL_PAD_X_PT: f32 = TABLE_CELL_HORIZONTAL_INSET_PT;
/// Space above a cell's first line and below its last line.
const CELL_PAD_Y_PT: f32 = TABLE_CELL_VERTICAL_INSET_PT;
/// Colour of the cell rules: the table line the editor draws (the theme's
/// paper-muted token, #6E6E73 on the light theme).
const RULE_RGB: (f32, f32, f32) = (0.431, 0.431, 0.451);

/// One side of a cell as the table's style draws it: a colour and a width.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Edge {
    rgb: (f32, f32, f32),
    width: f32,
}

/// The edge a style border sets, or `None` when the style leaves that side
/// without a rule (no style, no width, or no readable colour).
fn edge(border: Option<&Border>) -> Option<Edge> {
    let border = border?;
    if matches!(border.style, BorderStyle::None) || border.width_pt <= 0.0 {
        return None;
    }
    Some(Edge {
        rgb: hex_rgb(&border.color)?,
        width: border.width_pt,
    })
}

/// How one cell is drawn, as the table's style resolves it: its background,
/// its text colour, whether its text is bold, and its style borders.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CellFormat {
    fill: Option<(f32, f32, f32)>,
    text: Option<(f32, f32, f32)>,
    bold: bool,
    top: Option<Edge>,
    bottom: Option<Edge>,
    left: Option<Edge>,
    right: Option<Edge>,
}

/// The format of the cell at `row`, `column`, resolved by the table's style the
/// same way the editor resolves it.
fn cell_format(table: &Table, row: usize, column: usize) -> CellFormat {
    let style =
        TableStyleCatalog::get(&table.style_id).unwrap_or_else(TableStyleCatalog::default_style);
    let options = TableStyleOptions {
        header_row: table.header_row,
        total_row: table.total_row,
        first_column: table.first_column,
        last_column: table.last_column,
        banded_rows: table.banded_rows,
        banded_columns: table.banded_columns,
    };
    let resolved = style.resolve_cell(
        row,
        column,
        table.row_count(),
        table.col_count(),
        &options,
        table.cell(row, column),
    );
    CellFormat {
        fill: resolved.background_color.as_deref().and_then(hex_rgb),
        text: resolved.text_color.as_deref().and_then(hex_rgb),
        bold: resolved.bold,
        top: edge(resolved.borders.top.as_ref()),
        bottom: edge(resolved.borders.bottom.as_ref()),
        left: edge(resolved.borders.left.as_ref()),
        right: edge(resolved.borders.right.as_ref()),
    }
}

/// A `#RRGGBB` colour as the 0..1 components the PDF draws with.
fn hex_rgb(hex: &str) -> Option<(f32, f32, f32)> {
    let digits = hex.strip_prefix('#')?;
    if digits.len() < 6 || !digits.is_char_boundary(6) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&digits[range], 16)
            .ok()
            .map(|value| f32::from(value) / 255.0)
    };
    Some((channel(0..2)?, channel(2..4)?, channel(4..6)?))
}

/// The column widths the editor draws a table with: its typed column widths
/// scaled to fill `total`, or equal columns when no width is set.
pub fn editor_column_widths(table: &Table, total: f32) -> Vec<f32> {
    let cols = table.col_count().max(1);
    let typed: Vec<f32> = (0..cols)
        .map(|column| {
            table
                .columns
                .get(column)
                .map_or(0.0, |col| col.width_pt.max(0.0))
        })
        .collect();
    let sum: f32 = typed.iter().sum();
    if sum <= 0.0 {
        return vec![total / cols as f32; cols];
    }
    typed.iter().map(|width| total * width / sum).collect()
}

/// One cell line's already-shaped geometry and reserved height.
struct Line {
    shaped: Option<WriterShapedVisualLine>,
    height: f32,
}

struct CellLines {
    block: RichBlock,
    lines: Vec<Line>,
}

/// One row after wrapping: the lines of each cell, each cell's format and the
/// row's height.
struct WrappedRow {
    cells: Vec<CellLines>,
    formats: Vec<CellFormat>,
    height: f32,
}

/// Column positions and wrapped rows of one table, shared by its rows.
pub(crate) struct TableLayout {
    starts: Vec<f32>,
    widths: Vec<f32>,
    /// The table's width, from its left edge to the end of its last column.
    pub(crate) width: f32,
    rows: Vec<WrappedRow>,
}

impl TableLayout {
    /// Sizes the columns as the editor does (see [`editor_column_widths`]), wraps
    /// each cell onto as many lines as its column needs, and resolves each cell's
    /// format from the table's style. Text uses the same shaped runs as the page.
    pub(crate) fn new(
        doc: &WriterDocument,
        block_id: u64,
        table: &Table,
        max_width: f32,
    ) -> Result<Self, WriterError> {
        let columns = table.col_count();
        let header_row = table.header_row;
        let is_header = |index: usize| header_row && index == 0;
        let widths = editor_column_widths(table, max_width);
        let row_geometry = measure_table_row_geometry_with_footnotes(
            doc,
            block_id,
            table,
            max_width,
            TABLE_CELL_BASE_FONT_SIZE_PT,
        )
        .map_err(|error| WriterError::Invalid(format!("PDF export table geometry: {error}")))?;
        let mut starts = Vec::with_capacity(columns);
        let mut width = 0.0;
        for column in &widths {
            starts.push(width);
            width += column;
        }
        let rows: Vec<WrappedRow> = (0..table.row_count())
            .map(|row| -> Result<WrappedRow, WriterError> {
                let header = is_header(row);
                let formats: Vec<CellFormat> = (0..columns)
                    .map(|column| cell_format(table, row, column))
                    .collect();
                let cells: Vec<CellLines> = widths
                    .iter()
                    .enumerate()
                    .map(|(column, column_width)| -> Result<CellLines, WriterError> {
                        let base_color = formats[column].text.map(rgb_hex);
                        let block = table_cell_rich_block(
                            0,
                            table,
                            row,
                            column,
                            TABLE_CELL_BASE_FONT_SIZE_PT,
                            header || formats[column].bold,
                            base_color.as_deref(),
                        )
                        .unwrap_or_else(|| RichBlock::new(0, "paragraph", ""));
                        let block = shaped_export::with_automatic_ink(&block);
                        let inner = (column_width - 2.0 * CELL_PAD_X_PT).max(1.0);
                        let geometry = shape_table_cell_geometry_with_footnotes(
                            doc,
                            block_id,
                            table,
                            (row, column),
                            &block,
                            TABLE_CELL_BASE_FONT_SIZE_PT,
                            inner,
                        )
                        .map_err(|error| {
                            WriterError::Invalid(format!(
                                "PDF export table cell {row},{column}: {error}"
                            ))
                        })?;
                        let lines = geometry
                            .lines()
                            .iter()
                            .map(|line| Line {
                                shaped: line.shaped().cloned(),
                                height: line.pitch_pt(),
                            })
                            .collect();
                        Ok(CellLines { block, lines })
                    })
                    .collect::<Result<_, _>>()?;
                let height = row_geometry.row_height_pt(row).ok_or_else(|| {
                    WriterError::Invalid(format!("PDF export table geometry omitted row {row}"))
                })?;
                Ok(WrappedRow {
                    cells,
                    formats,
                    height,
                })
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            starts,
            widths,
            width,
            rows,
        })
    }

    /// How many rows the table has, header included.
    pub(crate) fn row_count(&self) -> usize {
        self.rows.len()
    }

    /// The height of row `index`.
    pub(crate) fn row_height(&self, index: usize) -> f32 {
        self.rows[index].height
    }
}

/// Draws row `index` of `layout` with its top-left corner at `origin`: the cell
/// backgrounds, the wrapped text, then the rules. Every row gets its box rules,
/// so an empty table still shows its grid; the first row also gets a top rule.
pub(crate) fn draw_row(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    page: PageIndex,
    layout: &TableLayout,
    index: usize,
    origin: (f32, f32),
) -> Result<(), WriterError> {
    let row = &layout.rows[index];
    let (left, top) = origin;
    let bottom = top - row.height;
    let right = left + layout.width;
    // One rectangle when the whole row shares a background, otherwise one per cell.
    let shared = row
        .formats
        .windows(2)
        .all(|pair| pair[0].fill == pair[1].fill);
    if shared {
        if let Some(fill) = row.formats.first().and_then(|format| format.fill) {
            pdf.draw_rect(
                page,
                left,
                bottom,
                layout.width,
                row.height,
                PathStyle::filled(fill),
            );
        }
    } else {
        for (column, format) in row.formats.iter().enumerate() {
            if let Some(fill) = format.fill {
                let x = left + layout.starts[column];
                let width = layout.widths[column];
                pdf.draw_rect(page, x, bottom, width, row.height, PathStyle::filled(fill));
            }
        }
    }
    for (column, cell) in row.cells.iter().enumerate() {
        let x = left + layout.starts[column] + CELL_PAD_X_PT;
        let mut line_top = top - CELL_PAD_Y_PT;
        for line in &cell.lines {
            if let Some(shaped) = &line.shaped {
                let context = format!("table cell {index},{column}");
                shaped_export::draw_line(
                    pdf,
                    fonts,
                    page,
                    &context,
                    shaped_export::PdfLine {
                        text: cell.block.text.as_str(),
                        line_top_pdf: line_top,
                        text_origin_x: x,
                        shaped_line: shaped,
                    },
                )?;
            }
            line_top -= line.height;
        }
    }
    let rule = PathStyle::stroked(RULE_RGB, 0.5);
    for start in &layout.starts {
        let x = left + start;
        pdf.draw_line(page, x, top, x, bottom, rule);
    }
    pdf.draw_line(page, right, top, right, bottom, rule);
    pdf.draw_line(page, left, bottom, right, bottom, rule);
    if index == 0 {
        pdf.draw_line(page, left, top, right, top, rule);
    }
    // The table style's own borders are drawn over the grid, cell by cell.
    for (column, format) in row.formats.iter().enumerate() {
        let x0 = left + layout.starts[column];
        let x1 = x0 + layout.widths[column];
        for (edge, from, to) in [
            (format.top, (x0, top), (x1, top)),
            (format.bottom, (x0, bottom), (x1, bottom)),
            (format.left, (x0, top), (x0, bottom)),
            (format.right, (x1, top), (x1, bottom)),
        ] {
            if let Some(edge) = edge {
                let style = PathStyle::stroked(edge.rgb, edge.width);
                pdf.draw_line(page, from.0, from.1, to.0, to.1, style);
            }
        }
    }
    Ok(())
}

/// One table row placed on a PDF page, with the typed table it came from.
pub(crate) struct RowDraw<'a> {
    pub layout: &'a TableLayout,
    pub table: &'a Table,
    pub index: usize,
    pub origin: (f32, f32),
}

/// Draw a table row and its inline footnote references at shaped cell carets.
pub(crate) fn draw_row_with_footnotes(
    pdf: &mut PdfDocument,
    fonts: &mut FontRegistry,
    page: PageIndex,
    row_draw: RowDraw<'_>,
    references: &[&FootnoteReferencePlacement],
) -> Result<(), WriterError> {
    let RowDraw {
        layout,
        table,
        index,
        origin,
    } = row_draw;
    draw_row(pdf, fonts, page, layout, index, origin)?;
    let map = TableTextMap::new(table);
    let row = &layout.rows[index];
    let (left, top) = origin;
    for reference in references {
        let (row_index, column) = map.cell_at(reference.offset).ok_or_else(|| {
            WriterError::Invalid(format!(
                "PDF export could not map footnote {} to a table cell",
                reference.note_id
            ))
        })?;
        if row_index != index {
            continue;
        }
        if table
            .effective_cell(row_index, column)
            .is_some_and(|(r, c, _)| r != row_index || c != column)
        {
            return Err(WriterError::Invalid(format!(
                "PDF export cannot place footnote {} in a covered table cell",
                reference.note_id
            )));
        }
        let cell_range = map.cell_range(row_index, column).ok_or_else(|| {
            WriterError::Invalid(format!(
                "PDF export has no text range for footnote {} table cell",
                reference.note_id
            ))
        })?;
        let cell_offset = reference.offset.saturating_sub(cell_range.start);
        let cell = &row.cells[column];
        let mut x = left + layout.starts[column] + CELL_PAD_X_PT;
        let mut line_top = top - CELL_PAD_Y_PT;
        let mut found = false;
        for line in &cell.lines {
            if let Some(shaped) = &line.shaped {
                let range = shaped.source_range();
                if cell_offset > range.start && cell_offset <= range.end
                    || cell_offset == range.start && range.start == 0
                {
                    x += crate::export::footnote_export::caret_x(shaped, cell_offset);
                    found = true;
                    break;
                }
            } else if cell.block.text.is_empty() && cell_offset == 0 {
                found = true;
                break;
            }
            line_top -= line.height;
        }
        if !found {
            return Err(WriterError::Invalid(format!(
                "PDF export could not map footnote {} to a shaped table-cell line",
                reference.note_id
            )));
        }
        crate::export::footnote_export::draw_reference_marker(
            pdf,
            fonts,
            crate::export::footnote_export::ReferenceMarker {
                page,
                number: reference.number,
                x,
                line_top,
                body_size_pt: TABLE_CELL_BASE_FONT_SIZE_PT,
            },
            &format!("footnote {} table reference", reference.note_id),
        )?;
    }
    Ok(())
}

fn rgb_hex(color: (f32, f32, f32)) -> String {
    let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!(
        "#{:02X}{:02X}{:02X}",
        channel(color.0),
        channel(color.1),
        channel(color.2)
    )
}

#[cfg(test)]
mod tests {
    use super::CELL_PAD_X_PT;
    use crate::text_metrics::pdf_text_width;
    use crate::{export_pdf as export_pdf_result, WriterDocument, WriterTable};
    use loom_pdf::TextStyle;

    fn export_pdf(document: &WriterDocument) -> Vec<u8> {
        export_pdf_result(document).expect("valid PDF fixture")
    }

    const HEADER: &[&str] = &["Quarter", "Notes", "Owner"];
    const LONG: &[&str] = &[
        "Q1",
        "Quarterly revenue figures for the northern region were revised upward after the final audit closed",
        "Ana",
    ];
    const SHORT: &[&str] = &["Q2", "Costs", "Ben"];

    /// The page content as text, one character per byte as the PDF writes it.
    fn content(pdf: &[u8]) -> String {
        loom_pdf::inspect::readable_content(pdf).expect("readable PDF")
    }

    /// Each drawn text run as (x, baseline y, text, bold), from the shaped-PDF semantic reader.
    fn text_runs(pdf: &[u8]) -> Vec<(f32, f32, String, bool)> {
        crate::export::test_support::drawn_text_runs(pdf)
            .expect("read positioned text runs")
            .into_iter()
            .map(|run| (run.x, run.y, run.text, run.bold.unwrap_or(false)))
            .collect()
    }

    /// Each stroked rule as (x1, y1, x2, y2), from `r g b RG w w x1 y1 m x2 y2 l S`.
    fn segments(pdf: &[u8]) -> Vec<(f32, f32, f32, f32)> {
        content(pdf)
            .lines()
            .filter_map(|line| {
                let t: Vec<&str> = line.split_whitespace().collect();
                let stroke = t.iter().position(|token| *token == "RG")?;
                let move_to = t.iter().position(|token| *token == "m")?;
                let line_to = t.iter().position(|token| *token == "l")?;
                if t.get(line_to + 1) != Some(&"S") || move_to < stroke + 4 || line_to < move_to + 3
                {
                    return None;
                }
                Some((
                    t.get(move_to - 2)?.parse().ok()?,
                    t.get(move_to - 1)?.parse().ok()?,
                    t.get(line_to - 2)?.parse().ok()?,
                    t.get(line_to - 1)?.parse().ok()?,
                ))
            })
            .collect()
    }

    /// Each filled rectangle as (x, y, width, height), from `r g b rg x y w h re f`.
    fn fills(pdf: &[u8]) -> Vec<(f32, f32, f32, f32)> {
        content(pdf)
            .lines()
            .filter_map(|line| {
                let t: Vec<&str> = line.split_whitespace().collect();
                if t.len() != 10 || t[3] != "rg" || t[8] != "re" || t[9] != "f" {
                    return None;
                }
                Some((
                    t[4].parse().ok()?,
                    t[5].parse().ok()?,
                    t[6].parse().ok()?,
                    t[7].parse().ok()?,
                ))
            })
            .collect()
    }

    /// Distinct values in ascending order, with values within 0.01 merged.
    fn distinct(mut values: Vec<f32>) -> Vec<f32> {
        values.sort_by(f32::total_cmp);
        values.dedup_by(|a, b| (*a - *b).abs() < 0.01);
        values
    }

    /// The table's horizontal rule heights, top rule first.
    fn row_edges(pdf: &[u8]) -> Vec<f32> {
        let mut ys = distinct(
            segments(pdf)
                .iter()
                .filter(|s| (s.1 - s.3).abs() < 0.01)
                .map(|s| s.1)
                .collect(),
        );
        ys.reverse();
        ys
    }

    /// The table's column edges from left to right.
    fn column_edges(pdf: &[u8]) -> Vec<f32> {
        distinct(
            segments(pdf)
                .iter()
                .filter(|s| (s.0 - s.2).abs() < 0.01)
                .map(|s| s.0)
                .collect(),
        )
    }

    /// A document holding one Markdown table; the first row is its header.
    fn table_document(rows: &[&[&str]]) -> WriterDocument {
        let mut table = WriterTable::new("table", rows.len(), rows[0].len());
        for (row_index, row) in rows.iter().enumerate() {
            for (column, cell) in row.iter().enumerate() {
                table.set(row_index, column, *cell);
            }
        }
        let mut document = WriterDocument::new("table", "Table");
        let id = document.next_id();
        document.push(crate::tables::markdown_table_block(
            id,
            &table.to_markdown(),
        ));
        document
    }

    #[test]
    fn a_wide_cell_wraps_and_keeps_every_word_without_an_ellipsis() {
        let pdf = export_pdf(&table_document(&[HEADER, LONG, SHORT]));
        assert!(
            !content(&pdf).contains('\u{85}'),
            "no cell is cut with an ellipsis"
        );
        let printed: Vec<String> = text_runs(&pdf).into_iter().map(|run| run.2).collect();
        let words: Vec<&str> = printed
            .iter()
            .flat_map(|line| line.split_whitespace())
            .collect();
        for word in LONG[1].split_whitespace() {
            assert!(words.contains(&word), "{word:?} is printed: {printed:?}");
        }
    }

    #[test]
    fn a_row_whose_cell_wraps_is_taller_than_a_one_line_row() {
        let pdf = export_pdf(&table_document(&[HEADER, LONG, SHORT]));
        let edges = row_edges(&pdf);
        assert_eq!(
            edges.len(),
            4,
            "a top rule and a rule under each row: {edges:?}"
        );
        let heights: Vec<f32> = edges.windows(2).map(|pair| pair[0] - pair[1]).collect();
        assert!(
            heights[1] > heights[0] * 1.5,
            "the wrapped row is taller than the one-line header: {heights:?}"
        );
    }

    /// Every colour set with `RG`, as (r, g, b), in the order the page draws them.
    fn stroke_colours(pdf: &[u8]) -> Vec<(f32, f32, f32)> {
        let text = content(pdf);
        let tokens: Vec<&str> = text.split_whitespace().collect();
        tokens
            .windows(4)
            .filter(|window| window[3] == "RG")
            .filter_map(|window| {
                Some((
                    window[0].parse().ok()?,
                    window[1].parse().ok()?,
                    window[2].parse().ok()?,
                ))
            })
            .collect()
    }

    #[test]
    fn the_cell_rules_are_drawn_in_the_editors_line_colour() {
        // The editor draws table lines in the theme's paper-muted token, #6E6E73
        // on the light theme (the page's own table-line rule in writer_components).
        let pdf = export_pdf(&table_document(&[HEADER, SHORT]));
        let line = [
            0x6E as f32 / 255.0,
            0x6E as f32 / 255.0,
            0x73 as f32 / 255.0,
        ];
        let colours = stroke_colours(&pdf);
        assert!(
            colours.iter().any(|c| {
                (c.0 - line[0]).abs() < 0.01
                    && (c.1 - line[1]).abs() < 0.01
                    && (c.2 - line[2]).abs() < 0.01
            }),
            "the rules are the editor's #6E6E73 table line: {colours:?}"
        );
    }

    /// Every colour set with `rg`, as (r, g, b), in the order the page draws them.
    fn fill_colours(pdf: &[u8]) -> Vec<(f32, f32, f32)> {
        content(pdf)
            .lines()
            .filter_map(|line| {
                let t: Vec<&str> = line.split_whitespace().collect();
                if t.len() < 4 || t[3] != "rg" {
                    return None;
                }
                Some((t[0].parse().ok()?, t[1].parse().ok()?, t[2].parse().ok()?))
            })
            .collect()
    }

    #[test]
    fn cells_take_the_fill_and_text_colour_of_the_table_style() {
        let pdf = export_pdf(&table_document(&[HEADER, SHORT]));
        let style = loom_objects::TableStyleCatalog::get("table-style-medium-1")
            .expect("the default style is in the catalogue");
        let fill = super::hex_rgb(&style.header_row.background_color.expect("a filled header"))
            .expect("a #RRGGBB header fill");
        let colours = fill_colours(&pdf);
        assert!(
            colours.iter().any(|c| (c.0 - fill.0).abs() < 0.01
                && (c.1 - fill.1).abs() < 0.01
                && (c.2 - fill.2).abs() < 0.01),
            "the header is filled with the style's colour {fill:?}: {colours:?}"
        );
    }

    #[test]
    fn a_soft_break_inside_a_table_cell_starts_a_new_line_in_the_pdf() {
        let mut document = WriterDocument::new("cell-break", "Cell break");
        let id = document.next_id();
        let mut table = loom_objects::Table::new(1, 1);
        table.set_cell_text(0, 0, "ab\u{b}cd");
        document.push(crate::RichBlock::table_block(id, table));
        let pdf = export_pdf(&document);
        let runs = text_runs(&pdf);
        let baseline = |word: &str| {
            runs.iter()
                .find(|run| run.2 == word)
                .map(|run| run.1)
                .unwrap_or_else(|| panic!("{word} is drawn: {runs:?}"))
        };
        assert!(
            (baseline("ab") - baseline("cd")).abs() > 1.0,
            "the second half sits on its own line below the first: {runs:?}"
        );
    }

    #[test]
    fn the_style_draws_the_header_bottom_border() {
        // Medium 1, the default style, sets a 1.5 pt navy rule under its header.
        let pdf = export_pdf(&table_document(&[HEADER, SHORT]));
        let navy = [
            0x1B as f32 / 255.0,
            0x36 as f32 / 255.0,
            0x5D as f32 / 255.0,
        ];
        assert!(
            stroke_colours(&pdf).iter().any(|c| {
                (c.0 - navy[0]).abs() < 0.01
                    && (c.1 - navy[1]).abs() < 0.01
                    && (c.2 - navy[2]).abs() < 0.01
            }),
            "the header's style border is drawn: {:?}",
            stroke_colours(&pdf)
        );
    }

    #[test]
    fn the_table_spans_the_text_column_like_the_editor() {
        let pdf = export_pdf(&table_document(&[HEADER, SHORT]));
        let page = crate::PageStyle::default();
        let text_column = page.width_pt - page.margin_left_pt - page.margin_right_pt;
        let columns = column_edges(&pdf);
        let width = columns[columns.len() - 1] - columns[0];
        assert!(
            (width - text_column).abs() < 1.0,
            "the table fills the {text_column} pt text column as the editor does, not {width} pt"
        );
    }

    #[test]
    fn the_header_row_has_a_filled_background_under_its_rules() {
        let pdf = export_pdf(&table_document(&[HEADER, SHORT]));
        let top = row_edges(&pdf)[0];
        let columns = column_edges(&pdf);
        let width = columns[columns.len() - 1] - columns[0];
        let found = fills(&pdf).iter().any(|&(x, y, w, h)| {
            (y + h - top).abs() < 0.01 && (x - columns[0]).abs() < 0.01 && (w - width).abs() < 0.01
        });
        assert!(
            found,
            "the header row is filled across the table: {:?}",
            fills(&pdf)
        );
    }

    #[test]
    fn an_empty_table_is_drawn_as_a_visible_grid() {
        let mut document = WriterDocument::new("empty", "Empty");
        let id = document.next_id();
        let markdown = WriterTable::new("empty", 3, 3).to_markdown();
        document.push(crate::tables::markdown_table_block(id, &markdown));
        let pdf = export_pdf(&document);

        let edges = row_edges(&pdf);
        assert_eq!(
            edges.len(),
            4,
            "a top rule and one under each row: {edges:?}"
        );
        let columns = column_edges(&pdf);
        assert_eq!(
            columns.len(),
            4,
            "three empty columns have four edges: {columns:?}"
        );
        let drawn = segments(&pdf);
        for &y in &edges {
            assert!(
                drawn.iter().any(|s| {
                    (s.1 - y).abs() < 0.01
                        && (s.3 - y).abs() < 0.01
                        && (s.0 - columns[0]).abs() < 0.01
                        && (s.2 - columns[3]).abs() < 0.01
                }),
                "the rule at {y} spans the whole table"
            );
        }
    }

    #[test]
    fn printed_text_stays_inside_its_column() {
        let pdf = export_pdf(&table_document(&[HEADER, LONG, SHORT]));
        let columns = column_edges(&pdf);
        for (x, _, text, bold) in text_runs(&pdf) {
            let index = columns
                .windows(2)
                .position(|pair| x >= pair[0] - 0.01 && x < pair[1])
                .unwrap_or_else(|| panic!("{text:?} starts at {x}, outside every column"));
            let style = TextStyle {
                size_pt: crate::PageStyle::default().body_font_size_pt,
                bold,
                ..TextStyle::default()
            };
            let end = x + pdf_text_width(&text, &style);
            assert!(
                end <= columns[index + 1] + 0.01,
                "{text:?} runs to {end}, past its column edge {}",
                columns[index + 1]
            );
        }
    }

    #[test]
    fn a_long_word_stays_whole_next_to_a_very_wide_cell_of_short_words() {
        let word = "Internationalization";
        let sentence = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu \
                        xi omicron pi rho sigma tau upsilon phi chi psi omega "
            .repeat(6);
        let sentence = sentence.trim();
        let pdf = export_pdf(&table_document(&[&["Term", "Notes"], &[word, sentence]]));
        let printed: Vec<String> = text_runs(&pdf).into_iter().map(|run| run.2).collect();
        assert!(
            printed.iter().any(|line| line == word),
            "the long word sits on one line: {printed:?}"
        );
        let words: Vec<&str> = printed
            .iter()
            .flat_map(|line| line.split_whitespace())
            .collect();
        // Header (2) + the long word + every word of the sentence, none cut.
        assert_eq!(words.len(), 2 + 1 + sentence.split_whitespace().count());
        for expected in sentence.split_whitespace() {
            assert!(words.contains(&expected), "{expected:?} was broken");
        }
    }

    #[test]
    fn columns_are_the_typed_widths_scaled_to_the_text_column() {
        // The editor's rule: the typed proportions are kept and the table fills
        // the text column, so a PDF table is the width the page shows.
        let mut table = loom_objects::Table::new(2, 2);
        table.columns[0].width_pt = 120.0;
        table.columns[1].width_pt = 900.0;
        let widths = super::editor_column_widths(&table, 468.0);
        assert!(
            (widths.iter().sum::<f32>() - 468.0).abs() < 0.01,
            "{widths:?}"
        );
        assert!(
            (widths[1] / widths[0] - 7.5).abs() < 0.01,
            "the typed proportions are kept: {widths:?}"
        );
        table.columns[0].width_pt = 0.0;
        table.columns[1].width_pt = 0.0;
        assert_eq!(
            super::editor_column_widths(&table, 468.0),
            vec![234.0, 234.0],
            "unset widths split the column equally"
        );
    }

    /// A document holding one stored table, given as its Markdown source.
    fn markdown_document(markdown: &str) -> WriterDocument {
        let mut document = WriterDocument::new("table", "Table");
        let id = document.next_id();
        document.push(crate::tables::markdown_table_block(id, markdown));
        document
    }

    #[test]
    fn bold_and_italic_cell_runs_use_their_faces_and_centred_columns_are_centred() {
        let markdown = "| **Item** | Qty |\n| --- | :---: |\n| **Apples** | 50 |\n| *Note* | 7 |";
        let pdf = export_pdf(&markdown_document(markdown));
        let runs = text_runs(&pdf);
        let drawn = |text: &str| {
            runs.iter()
                .find(|run| run.2 == text)
                .unwrap_or_else(|| panic!("{text:?} is drawn: {runs:?}"))
        };
        assert!(
            drawn("Apples").3,
            "a bold cell run is drawn in the bold face"
        );
        let printed = content(&pdf);
        let semantic_runs =
            crate::export::test_support::drawn_text_runs(&pdf).expect("read semantic text runs");
        assert!(
            semantic_runs
                .iter()
                .any(|run| run.text == "Note" && run.font == "/F3"),
            "an italic cell run is drawn in the oblique face: {semantic_runs:?}; {printed}"
        );
        assert!(
            !printed.contains('*') && !printed.contains('|'),
            "no markup is printed"
        );
        let columns = column_edges(&pdf);
        let fifty = drawn("50");
        assert!(
            fifty.0 > columns[1] + CELL_PAD_X_PT + 1.0,
            "50 is centred in its column: x {} in {columns:?}",
            fifty.0
        );
    }

    /// A typed 2x2 table after a paragraph: `Name`/`Qty` over `Tea`/`2`.
    fn typed_table_document() -> (WriterDocument, u64) {
        use loom_objects::table::Table;
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Name");
        table.set_cell_text(0, 1, "Qty");
        table.set_cell_text(1, 0, "Tea");
        table.set_cell_text(1, 1, "2");
        let mut document = WriterDocument::new("table", "Table");
        document
            .blocks
            .push(crate::RichBlock::table_block(9, table));
        (document, 9)
    }

    /// The `w:tc` element of the DOCX table whose text is `text`.
    fn docx_cell_with_text<'a, 'i>(
        document: &'a roxmltree::Document<'i>,
        text: &str,
    ) -> roxmltree::Node<'a, 'i> {
        document
            .descendants()
            .filter(|node| node.tag_name().name() == "tc")
            .find(|cell| {
                cell.descendants()
                    .filter(|node| node.tag_name().name() == "t")
                    .filter_map(|node| node.text())
                    .collect::<String>()
                    == text
            })
            .unwrap_or_else(|| panic!("a cell reading {text:?}"))
    }

    #[test]
    fn docx_cells_keep_the_editor_column_alignment_bold_and_underline() {
        use loom_objects::table::HorizontalAlign;
        let (mut document, id) = typed_table_document();
        document
            .set_table_column_alignment(id, 1..2, HorizontalAlign::Center)
            .expect("centre column Qty");
        document
            .set_table_cells_runs(id, 1..2, 1..2, |run| run.underline = true)
            .expect("underline cell 2");
        let export = crate::export_docx(&document).expect("docx");
        let archive =
            loom_package::zip::PackageArchive::from_bytes(&export.bytes).expect("zip reads back");
        let xml = String::from_utf8(
            archive
                .get("word/document.xml")
                .expect("document part")
                .to_vec(),
        )
        .expect("utf-8");
        let body = roxmltree::Document::parse(&xml).expect("well formed");
        let has = |cell: roxmltree::Node, name: &str| {
            cell.descendants()
                .any(|node| node.tag_name().name() == name)
        };
        let centred = |cell: roxmltree::Node| {
            cell.descendants().any(|node| {
                node.tag_name().name() == "jc"
                    && node
                        .attributes()
                        .any(|attr| attr.name() == "val" && attr.value() == "center")
            })
        };
        let two = docx_cell_with_text(&body, "2");
        assert!(centred(two), "the centred column is centred in Word too");
        assert!(has(two, "u"), "the underlined run is underlined in Word");
        let tea = docx_cell_with_text(&body, "Tea");
        assert!(!centred(tea), "column Name keeps its own alignment");
        assert!(!has(tea, "u"), "untouched cells are not underlined");
    }

    #[test]
    fn a_centred_typed_column_is_centred_in_the_pdf() {
        use loom_objects::table::HorizontalAlign;
        let (mut document, id) = typed_table_document();
        let left = text_runs(&export_pdf(&document));
        document
            .set_table_column_alignment(id, 1..2, HorizontalAlign::Center)
            .expect("centre column Qty");
        let centred = text_runs(&export_pdf(&document));
        let x_of = |runs: &[(f32, f32, String, bool)]| {
            runs.iter()
                .find(|run| run.2 == "2")
                .map(|run| run.0)
                .expect("2 is drawn")
        };
        assert!(
            x_of(&centred) > x_of(&left) + 1.0,
            "2 moves right when its column is centred: {} -> {}",
            x_of(&left),
            x_of(&centred)
        );
    }

    #[test]
    fn an_underlined_typed_cell_run_draws_a_rule_under_its_text_in_the_pdf() {
        let (mut document, id) = typed_table_document();
        document
            .set_table_cells_runs(id, 1..2, 1..2, |run| run.underline = true)
            .expect("underline cell 2");
        let pdf = export_pdf(&document);
        let runs = text_runs(&pdf);
        let rule_under = |text: &str| {
            let (x, baseline, _, _) = runs
                .iter()
                .find(|run| run.2 == text)
                .unwrap_or_else(|| panic!("{text:?} is drawn"))
                .clone();
            segments(&pdf).iter().any(|&(x1, y1, x2, y2)| {
                (y1 - y2).abs() < 0.01
                    && y1 < baseline
                    && y1 > baseline - 3.0
                    && x1 <= x + 0.5
                    && x2 >= x + 0.5
            })
        };
        assert!(
            rule_under("2"),
            "the underlined cell run has a rule under it"
        );
        assert!(!rule_under("Tea"), "an untouched cell has no rule");
    }

    #[test]
    fn typed_column_widths_are_the_split_word_draws() {
        let (mut document, id) = typed_table_document();
        let mut table = document.loom_table_from_block(id).expect("table");
        table.columns[0].width_pt *= 3.0;
        document
            .blocks
            .iter_mut()
            .find(|block| block.id == id)
            .expect("table block")
            .set_table(table);
        let export = crate::export_docx(&document).expect("docx");
        let archive =
            loom_package::zip::PackageArchive::from_bytes(&export.bytes).expect("zip reads back");
        let xml = String::from_utf8(
            archive
                .get("word/document.xml")
                .expect("document part")
                .to_vec(),
        )
        .expect("utf-8");
        let body = roxmltree::Document::parse(&xml).expect("well formed");
        let grid: Vec<f32> = body
            .descendants()
            .filter(|node| node.tag_name().name() == "gridCol")
            .filter_map(|node| {
                node.attributes()
                    .find(|attr| attr.name() == "w")
                    .and_then(|attr| attr.value().parse().ok())
            })
            .collect();
        assert_eq!(grid.len(), 2, "two grid columns");
        assert!(
            (grid[0] / grid[1] - 3.0).abs() < 0.01,
            "Word's first column is three times the second, as on the page: {grid:?}"
        );
    }
}
