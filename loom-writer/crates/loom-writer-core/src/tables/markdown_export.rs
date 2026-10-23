//! GitHub-flavoured Markdown for a typed table: one line per row, a divider row
//! after the first row that carries each column's alignment, and each cell's
//! bold and italic written as emphasis. A literal asterisk is written `\*` and a
//! literal pipe `\|`.

use loom_objects::table::{HorizontalAlign, Table, TextRun};

/// The table as Markdown, ending with a newline. An empty table is empty text.
pub(crate) fn table_markdown(table: &Table) -> String {
    let columns = table.col_count();
    if table.row_count() == 0 || columns == 0 {
        return String::new();
    }
    let mut out = String::new();
    for row in 0..table.row_count() {
        out.push('|');
        for column in 0..columns {
            out.push(' ');
            let cell = table
                .cell(row, column)
                .map_or_else(String::new, |cell| cell_markup(&cell.content.runs));
            out.push_str(&cell.replace('|', "\\|"));
            out.push_str(" |");
        }
        out.push('\n');
        if row == 0 {
            out.push('|');
            for column in 0..columns {
                out.push_str(match column_alignment(table, column) {
                    HorizontalAlign::Center => " :---: |",
                    HorizontalAlign::Right => " ---: |",
                    _ => " --- |",
                });
            }
            out.push('\n');
        }
    }
    out
}

/// The table as an HTML `<table>`, one `<tr>` per row. The header row's cells
/// are `<th>`; text is escaped.
pub(crate) fn table_html(table: &Table) -> String {
    let mut out = String::from("<table>\n");
    for row in 0..table.row_count() {
        let tag = if table.header_row && row == 0 {
            "th"
        } else {
            "td"
        };
        out.push_str("<tr>");
        for column in 0..table.col_count() {
            let text = table.cell_text(row, column).unwrap_or_default();
            let escaped = text
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            out.push_str(&format!("<{tag}>{escaped}</{tag}>"));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

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

/// Writes a cell's runs as Markdown: a toggle of bold (`**`), italic (`*`) or both
/// (`***`) where the style changes, and a literal asterisk as `\*`. Line breaks
/// become spaces, since a cell is one line.
fn cell_markup(runs: &[TextRun]) -> String {
    let (mut bold, mut italic) = (false, false);
    let mut out = String::new();
    for run in runs.iter().filter(|run| !run.text.is_empty()) {
        match (run.bold != bold, run.italic != italic) {
            (true, true) => out.push_str("***"),
            (true, false) => out.push_str("**"),
            (false, true) => out.push('*'),
            (false, false) => {}
        }
        bold = run.bold;
        italic = run.italic;
        for ch in run.text.chars() {
            match ch {
                '*' => out.push_str("\\*"),
                '\n' | '\r' => out.push(' '),
                other => out.push(other),
            }
        }
    }
    match (bold, italic) {
        (true, true) => out.push_str("***"),
        (true, false) => out.push_str("**"),
        (false, true) => out.push('*'),
        (false, false) => {}
    }
    out
}
