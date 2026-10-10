//! Table blocks in the PDF export. Each row's cells are wrapped to their
//! columns and drawn at fixed column positions, without Markdown pipes or the
//! separator row, so a table reads as a table rather than as Markdown source.
//! A row is as tall as its tallest wrapped cell.

use loom_pdf::{PageIndex, PathStyle, PdfDocument, TextStyle};

use crate::parse_table_markdown;
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

/// One row after wrapping: the wrapped lines of each cell and the row's height.
struct WrappedRow {
    cells: Vec<Vec<String>>,
    height: f32,
    header: bool,
}

/// Column positions and wrapped rows of one table, shared by its rows.
pub(crate) struct TableLayout {
    starts: Vec<f32>,
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
    pub(crate) fn new(markdown: &str, body: &TextStyle, max_width: f32, line_height: f32) -> Self {
        let table = parse_table_markdown(markdown);
        let columns = table.columns();
        // A column wants its widest whole cell, but it can always narrow to its
        // longest word, so words stay whole for as long as the table allows.
        let mut natural = vec![MIN_COLUMN_PT; columns];
        let mut shortest = vec![MIN_COLUMN_PT; columns];
        for (index, row) in table.rows.iter().enumerate() {
            let style = cell_style(body, table.header_row && index == 0);
            for (column, cell) in row.iter().enumerate() {
                let whole = pdf_text_width(cell, &style) + 2.0 * CELL_PAD_X_PT;
                natural[column] = natural[column].max(whole);
                let longest = cell
                    .split_whitespace()
                    .map(|word| pdf_text_width(word, &style))
                    .fold(0.0, f32::max);
                shortest[column] = shortest[column].max(longest + 2.0 * CELL_PAD_X_PT);
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
        let rows = table
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let header = table.header_row && index == 0;
                let style = cell_style(body, header);
                let cells: Vec<Vec<String>> = widths
                    .iter()
                    .enumerate()
                    .map(|(column, column_width)| {
                        let text = row.get(column).map_or("", String::as_str);
                        let inner = (column_width - 2.0 * CELL_PAD_X_PT).max(1.0);
                        wrap(text, &style, inner)
                    })
                    .collect();
                let lines = cells.iter().map(Vec::len).max().unwrap_or(1).max(1);
                WrappedRow {
                    cells,
                    height: lines as f32 * line_height + 2.0 * CELL_PAD_Y_PT,
                    header,
                }
            })
            .collect();
        Self {
            starts,
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
        let x = left + layout.starts[column] + CELL_PAD_X_PT;
        for (line, text) in lines.iter().enumerate() {
            if text.is_empty() {
                continue;
            }
            // The first baseline sits one em below the top padding; each later
            // line is one line height lower.
            let baseline = top - CELL_PAD_Y_PT - size - line as f32 * layout.line_height;
            pdf.draw_text(page, x, baseline, text, &style);
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

/// The lines a cell's text takes in `max_width`. Words stay whole where they
/// fit; a word wider than the column breaks between characters. Every character
/// is kept, and an empty cell still takes one line.
fn wrap(text: &str, style: &TextStyle, max_width: f32) -> Vec<String> {
    let limit = max_width + WRAP_SLACK_PT;
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let joined = if line.is_empty() {
            word.to_string()
        } else {
            format!("{line} {word}")
        };
        if pdf_text_width(&joined, style) <= limit {
            line = joined;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for ch in word.chars() {
            line.push(ch);
            if line.chars().count() > 1 && pdf_text_width(&line, style) > limit {
                let next = line.pop().unwrap_or(ch);
                lines.push(std::mem::take(&mut line));
                line.push(next);
            }
        }
    }
    lines.push(line);
    lines
}

/// The body style, in bold for a header cell.
fn cell_style(body: &TextStyle, header: bool) -> TextStyle {
    TextStyle {
        bold: body.bold || header,
        ..body.clone()
    }
}

#[cfg(test)]
mod tests {
    use crate::text_metrics::pdf_text_width;
    use crate::{export_pdf, RichBlock, WriterDocument, WriterTable, TABLE_BLOCK_KIND};
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
        document.push(RichBlock::new(id, TABLE_BLOCK_KIND, &table.to_markdown()));
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
        document.push(RichBlock::new(id, TABLE_BLOCK_KIND, &markdown));
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
}
