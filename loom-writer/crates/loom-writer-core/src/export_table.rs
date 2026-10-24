//! Table blocks in the PDF export. Each row's cells are wrapped to their
//! columns and drawn at fixed column positions, without Markdown pipes or the
//! separator row, so a table reads as a table rather than as Markdown source.
//! A row is as tall as its tallest wrapped cell.

use loom_objects::table::{HorizontalAlign, Table};
use loom_pdf::{PageIndex, PathStyle, PdfDocument, TextStyle};
use loom_text::{CharacterStyle, FontWeight, StyleRun};

use crate::export_flow::{self, line_width};
use crate::text_metrics::pdf_text_width;

/// Space between a cell's text and each of its column rules.
const CELL_PAD_X_PT: f32 = 5.0;
/// Space above a cell's first line and below its last line.
const CELL_PAD_Y_PT: f32 = 3.0;
/// Narrowest column, so an empty column of a new table is still drawn.
const MIN_COLUMN_PT: f32 = 48.0;
/// Colour of the cell rules.
const RULE_RGB: (f32, f32, f32) = (0.55, 0.55, 0.55);
/// Background of the header row.
const HEADER_FILL_RGB: (f32, f32, f32) = (0.90, 0.90, 0.90);

/// One cell's plain text with the bold and italic runs inside it, as byte
/// ranges of that text.
struct Cell {
    text: String,
    runs: Vec<StyleRun>,
}

impl Cell {
    /// The cell at row `row`, column `column`.
    fn of(table: &Table, row: usize, column: usize) -> Self {
        let mut text = String::new();
        let mut runs = Vec::new();
        for piece in table
            .cell(row, column)
            .map_or(&[][..], |cell| cell.content.runs.as_slice())
        {
            let start = text.len();
            text.push_str(&piece.text);
            if piece.bold || piece.italic || piece.underline {
                let style = CharacterStyle {
                    weight: if piece.bold {
                        FontWeight::Bold
                    } else {
                        FontWeight::Regular
                    },
                    italic: piece.italic,
                    underline: piece.underline,
                    ..CharacterStyle::default()
                };
                runs.push(StyleRun {
                    start,
                    end: text.len(),
                    style,
                });
            }
        }
        Self { text, runs }
    }

    /// The drawn width of the cell's longest whitespace-separated word.
    fn longest_word(&self, style: &TextStyle) -> f32 {
        let mut longest = 0.0f32;
        let mut word_start: Option<usize> = None;
        for (index, ch) in self.text.char_indices().chain([(self.text.len(), ' ')]) {
            match (ch.is_whitespace(), word_start) {
                (false, None) => word_start = Some(index),
                (true, Some(start)) => {
                    let word = &self.text[start..index];
                    longest = longest.max(line_width("", word, &self.runs, style, start));
                    word_start = None;
                }
                _ => {}
            }
        }
        longest
    }
}

/// One wrapped line of a cell: its text, the runs inside it (byte ranges of
/// that text) and its drawn width.
struct Line {
    text: String,
    runs: Vec<StyleRun>,
    width: f32,
}

impl Line {
    /// The part of `cell` from `start` to `end`, with its runs made relative to it.
    fn of(cell: &Cell, start: usize, end: usize, style: &TextStyle) -> Self {
        let text = cell.text[start..end].to_string();
        let runs: Vec<StyleRun> = cell
            .runs
            .iter()
            .filter_map(|run| {
                let (from, to) = (run.start.max(start), run.end.min(end));
                (from < to).then(|| StyleRun {
                    start: from - start,
                    end: to - start,
                    style: run.style.clone(),
                })
            })
            .collect();
        let width = line_width("", &text, &runs, style, 0);
        Self { text, runs, width }
    }
}

/// One row after wrapping: the lines of each cell and the row's height.
struct WrappedRow {
    cells: Vec<Vec<Line>>,
    height: f32,
    header: bool,
}

/// Column positions and wrapped rows of one table, shared by its rows.
pub(crate) struct TableLayout {
    starts: Vec<f32>,
    widths: Vec<f32>,
    alignments: Vec<HorizontalAlign>,
    /// The table's width, from its left edge to the end of its last column.
    pub(crate) width: f32,
    /// The body style the cells are set in.
    body: TextStyle,
    /// The distance between the baselines of a cell's lines.
    line_height: f32,
    rows: Vec<WrappedRow>,
}

impl TableLayout {
    /// Sizes each column to its widest cell, fitting the table into `max_width`
    /// (see `fit_columns`), and wraps each cell onto as many lines as its column
    /// needs. `line_height` is the distance between the lines of one cell.
    pub(crate) fn new(table: &Table, body: &TextStyle, max_width: f32, line_height: f32) -> Self {
        let columns = table.col_count();
        let header_row = table.header_row;
        let is_header = |index: usize| header_row && index == 0;
        // A column wants its widest whole cell, but it can always narrow to its
        // longest word, so words stay whole for as long as the table allows.
        let mut natural = vec![MIN_COLUMN_PT; columns];
        let mut shortest = vec![MIN_COLUMN_PT; columns];
        let cells: Vec<Vec<Cell>> = (0..table.row_count())
            .map(|row| {
                (0..columns)
                    .map(|column| Cell::of(table, row, column))
                    .collect()
            })
            .collect();
        for (index, row) in cells.iter().enumerate() {
            let style = cell_style(body, is_header(index));
            for (column, cell) in row.iter().enumerate() {
                let whole = line_width("", &cell.text, &cell.runs, &style, 0);
                natural[column] = natural[column].max(whole + 2.0 * CELL_PAD_X_PT);
                let longest = cell.longest_word(&style) + 2.0 * CELL_PAD_X_PT;
                shortest[column] = shortest[column].max(longest);
            }
        }
        for (short, full) in shortest.iter_mut().zip(&natural) {
            *short = short.min(*full);
        }
        let widths = fit_columns(&natural, &shortest, max_width);
        let mut starts = Vec::with_capacity(columns);
        let mut width = 0.0;
        for column in &widths {
            starts.push(width);
            width += column;
        }
        let rows = cells
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let header = is_header(index);
                let style = cell_style(body, header);
                let lines: Vec<Vec<Line>> = widths
                    .iter()
                    .enumerate()
                    .map(|(column, column_width)| {
                        let inner = (column_width - 2.0 * CELL_PAD_X_PT).max(1.0);
                        wrap_cell(&row[column], &style, inner)
                    })
                    .collect();
                let count = lines.iter().map(Vec::len).max().unwrap_or(1).max(1);
                WrappedRow {
                    cells: lines,
                    height: count as f32 * line_height + 2.0 * CELL_PAD_Y_PT,
                    header,
                }
            })
            .collect();
        let alignments = (0..columns)
            .map(|column| column_alignment(table, column))
            .collect();
        Self {
            starts,
            widths,
            alignments,
            width,
            body: body.clone(),
            line_height,
            rows,
        }
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

/// Draws row `index` of `layout` with its top-left corner at `origin`: the
/// header fill, the wrapped text, then the rules. Every row gets its box rules,
/// so an empty table still shows its grid; the first row also gets a top rule.
pub(crate) fn draw_row(
    pdf: &mut PdfDocument,
    page: PageIndex,
    layout: &TableLayout,
    index: usize,
    origin: (f32, f32),
) {
    let row = &layout.rows[index];
    let (left, top) = origin;
    let bottom = top - row.height;
    let right = left + layout.width;
    if row.header {
        let fill = PathStyle::filled(HEADER_FILL_RGB);
        pdf.draw_rect(page, left, bottom, layout.width, row.height, fill);
    }
    let style = cell_style(&layout.body, row.header);
    let size = layout.body.size_pt;
    for (column, lines) in row.cells.iter().enumerate() {
        let inner = (layout.widths[column] - 2.0 * CELL_PAD_X_PT).max(1.0);
        let x = left + layout.starts[column] + CELL_PAD_X_PT;
        for (index, line) in lines.iter().enumerate() {
            if line.text.is_empty() {
                continue;
            }
            // The first baseline sits one em below the top padding; each later
            // line is one line height lower.
            let baseline = top - CELL_PAD_Y_PT - size - index as f32 * layout.line_height;
            let shift = align_shift(layout.alignments[column], inner, line.width);
            draw_line(pdf, page, (x + shift, baseline), line, &style);
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
}

/// Column widths that add up to `max_width` when the table is wider than that.
/// Each column keeps its longest word when the table can; the room left over is
/// shared out in proportion to how much more each column wants. When even the
/// longest words do not fit, the columns shrink together and their words break
/// between characters.
fn fit_columns(natural: &[f32], shortest: &[f32], max_width: f32) -> Vec<f32> {
    let total: f32 = natural.iter().sum();
    if total <= max_width {
        return natural.to_vec();
    }
    let shortest_total: f32 = shortest.iter().sum();
    if shortest_total >= max_width {
        let scale = max_width / shortest_total;
        return shortest.iter().map(|width| width * scale).collect();
    }
    let spare = max_width - shortest_total;
    let wanted: f32 = natural
        .iter()
        .zip(shortest)
        .map(|(full, short)| full - short)
        .sum();
    let share = if wanted > 0.0 { spare / wanted } else { 0.0 };
    natural
        .iter()
        .zip(shortest)
        .map(|(full, short)| short + (full - short) * share)
        .collect()
}

/// Slack for float rounding, so a word that exactly fills its column stays whole.
const WRAP_SLACK_PT: f32 = 0.05;

/// The lines a cell's text takes in `inner` points, wrapped as the paragraphs of
/// the PDF wrap: words stay whole where they fit, a word wider than the column
/// breaks between characters, every character is kept, and an empty cell still
/// takes one line. The runs are measured in their faces.
fn wrap_cell(cell: &Cell, style: &TextStyle, inner: f32) -> Vec<Line> {
    let width = inner + WRAP_SLACK_PT;
    export_flow::wrap(&cell.text, &cell.runs, style, width, width)
        .into_iter()
        .map(|(start, end)| Line::of(cell, start, end, style))
        .collect()
}

/// How far a line of `width` moves right inside a cell's `inner` width for the
/// column's alignment. Left-aligned lines stay at the padding.
fn align_shift(alignment: HorizontalAlign, inner: f32, width: f32) -> f32 {
    match alignment {
        HorizontalAlign::Center => (inner - width) / 2.0,
        HorizontalAlign::Right => inner - width,
        _ => 0.0,
    }
    .max(0.0)
}

/// Draws one line as pieces, one per stretch of one style, so bold and italic
/// runs are set in their own faces. `origin` is the line's left edge and baseline.
fn draw_line(
    pdf: &mut PdfDocument,
    page: PageIndex,
    origin: (f32, f32),
    line: &Line,
    base: &TextStyle,
) {
    let (mut x, baseline) = origin;
    let mut cuts = vec![0, line.text.len()];
    for run in &line.runs {
        cuts.extend([run.start, run.end]);
    }
    cuts.sort_unstable();
    cuts.dedup();
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        let mut style = base.clone();
        let mut underline = false;
        if let Some(run) = line
            .runs
            .iter()
            .find(|run| run.start <= from && from < run.end)
        {
            style.bold |= run.style.weight.numeric() >= FontWeight::Bold.numeric();
            style.italic |= run.style.italic;
            underline = run.style.underline;
        }
        let piece = &line.text[from..to];
        pdf.draw_text(page, x, baseline, piece, &style);
        let width = pdf_text_width(piece, &style);
        // An underlined run gets the same rule the paragraphs draw under it.
        if underline && !piece.trim().is_empty() {
            let rule_y = baseline - style.size_pt * 0.12;
            pdf.draw_line(
                page,
                x,
                rule_y,
                x + width,
                rule_y,
                PathStyle::stroked(style.fill_rgb, (style.size_pt / 24.0).max(0.5)),
            );
        }
        x += width;
    }
}

/// The body style, in bold for a header cell.
/// The alignment of a column: the first explicit alignment set on its cells.
fn column_alignment(table: &Table, column: usize) -> HorizontalAlign {
    (0..table.row_count())
        .find_map(|row| {
            table
                .cell(row, column)
                .and_then(|cell| cell.style_override.as_ref())
                .and_then(|style| style.horizontal_align)
        })
        .unwrap_or_default()
}

fn cell_style(body: &TextStyle, header: bool) -> TextStyle {
    TextStyle {
        bold: body.bold || header,
        ..body.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::CELL_PAD_X_PT;
    use crate::text_metrics::pdf_text_width;
    use crate::{export_pdf, WriterDocument, WriterTable};
    use loom_pdf::TextStyle;

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

    /// Each drawn text run as (x, baseline y, text, bold), from `BT x y Td (text) Tj ET`.
    fn text_runs(pdf: &[u8]) -> Vec<(f32, f32, String, bool)> {
        content(pdf)
            .lines()
            .filter_map(|line| {
                let at = line.find(" Td (")?;
                let mut numbers = line[..at].rsplit(' ');
                let y: f32 = numbers.next()?.parse().ok()?;
                let x: f32 = numbers.next()?.parse().ok()?;
                let text = &line[at + " Td (".len()..line.rfind(") Tj")?];
                Some((x, y, text.to_string(), line.contains("/F2 ")))
            })
            .collect()
    }

    /// Each stroked rule as (x1, y1, x2, y2), from `r g b RG w w x1 y1 m x2 y2 l S`.
    fn segments(pdf: &[u8]) -> Vec<(f32, f32, f32, f32)> {
        content(pdf)
            .lines()
            .filter_map(|line| {
                let t: Vec<&str> = line.split_whitespace().collect();
                if t.len() != 13 || t[3] != "RG" || t[8] != "m" || t[11] != "l" {
                    return None;
                }
                Some((
                    t[6].parse().ok()?,
                    t[7].parse().ok()?,
                    t[9].parse().ok()?,
                    t[10].parse().ok()?,
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
    fn narrowing_a_table_gives_each_column_its_longest_word_first() {
        // Natural widths 120 and 900; longest words 120 and 55. In 468 the
        // first column must not be squeezed below its longest word.
        let widths = super::fit_columns(&[120.0, 900.0], &[120.0, 55.0], 468.0);
        assert!(
            (widths.iter().sum::<f32>() - 468.0).abs() < 0.01,
            "{widths:?}"
        );
        assert!(widths[0] >= 120.0 - 0.01, "{widths:?}");
        assert!(widths[1] >= 55.0, "{widths:?}");
        // When even the longest words do not fit, the columns shrink together.
        let squeezed = super::fit_columns(&[300.0, 300.0], &[250.0, 250.0], 400.0);
        assert!((squeezed.iter().sum::<f32>() - 400.0).abs() < 0.01);
        assert!((squeezed[0] - squeezed[1]).abs() < 0.01);
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
        assert!(
            printed
                .lines()
                .any(|line| line.contains("/F3 ") && line.contains("(Note) Tj")),
            "an italic cell run is drawn in the oblique face: {printed}"
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
        assert!(rule_under("2"), "the underlined cell run has a rule under it");
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
