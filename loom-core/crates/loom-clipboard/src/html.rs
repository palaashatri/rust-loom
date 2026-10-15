//! HTML table serialization and parsing with inline CSS styles.

use loom_objects::styles::{TableStyleCatalog, TableStyleOptions};
use loom_objects::table::{CellStyleOverride, HorizontalAlign, Table};

/// Converts a `Table` to an HTML table with inline CSS styling.
pub fn table_to_html(table: &Table) -> String {
    let mut out = String::new();
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

    let total_width: f32 = table.columns.iter().map(|c| c.width_pt).sum();
    out.push_str(&format!(
        "<table style=\"border-collapse: collapse; width: {:.1}pt;\">\n",
        total_width
    ));

    // Column group
    out.push_str("  <colgroup>\n");
    for col in &table.columns {
        out.push_str(&format!(
            "    <col style=\"width: {:.1}pt;\" />\n",
            col.width_pt
        ));
    }
    out.push_str("  </colgroup>\n");

    out.push_str("  <tbody>\n");
    for (r, row) in table.rows.iter().enumerate() {
        out.push_str("    <tr>\n");
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.is_covered {
                continue;
            }

            let resolved = style.resolve_cell(
                r,
                c,
                table.row_count(),
                table.col_count(),
                &options,
                Some(cell),
            );

            let mut td_attrs = String::new();
            if cell.col_span > 1 {
                td_attrs.push_str(&format!(" colspan=\"{}\"", cell.col_span));
            }
            if cell.row_span > 1 {
                td_attrs.push_str(&format!(" rowspan=\"{}\"", cell.row_span));
            }

            // Build inline CSS
            let mut css = "padding: 4pt 6pt; border: 0.5pt solid #D0D0D0;".to_string();
            if let Some(bg) = &resolved.background_color {
                css.push_str(&format!(" background-color: {};", bg));
            }
            if let Some(fg) = &resolved.text_color {
                css.push_str(&format!(" color: {};", fg));
            }
            if resolved.bold {
                css.push_str(" font-weight: bold;");
            }
            match resolved.horizontal_align {
                HorizontalAlign::Center => css.push_str(" text-align: center;"),
                HorizontalAlign::Right => css.push_str(" text-align: right;"),
                _ => css.push_str(" text-align: left;"),
            }

            out.push_str(&format!("      <td style=\"{}\"{}>\n", css, td_attrs));

            if cell.content.runs.is_empty() {
                out.push_str("&nbsp;");
            } else {
                for run in &cell.content.runs {
                    let mut run_css = String::new();
                    if run.bold {
                        run_css.push_str("font-weight: bold; ");
                    }
                    if run.italic {
                        run_css.push_str("font-style: italic; ");
                    }
                    if run.underline {
                        run_css.push_str("text-decoration: underline; ");
                    }
                    if let Some(color) = &run.color {
                        run_css.push_str(&format!("color: {}; ", color));
                    }

                    let escaped = quick_escape(&run.text);
                    if run_css.is_empty() {
                        out.push_str(&escaped);
                    } else {
                        out.push_str(&format!(
                            "<span style=\"{}\">{}</span>",
                            run_css.trim(),
                            escaped
                        ));
                    }
                }
            }

            out.push_str("\n      </td>\n");
        }
        out.push_str("    </tr>\n");
    }
    out.push_str("  </tbody>\n");
    out.push_str("</table>");

    out
}

/// Maximum HTML payload size accepted from clipboard to guard against OOM DOS.
const MAX_CLIPBOARD_HTML_LEN: usize = 64 * 1024 * 1024;
/// Maximum table dimensions constructed from untrusted HTML.
const MAX_TABLE_ROWS: usize = 10_000;
const MAX_TABLE_COLS: usize = 1_000;
/// Maximum time in milliseconds to spend parsing untrusted HTML before bailing/truncating.
const MAX_PARSE_DURATION_MS: u128 = 1800;
/// Maximum text length extracted per cell to guard against gigantic string allocations.
const MAX_CELL_TEXT_LEN: usize = 64 * 1024;

/// Performs zero-allocation case-insensitive ASCII search.
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    let needle_bytes = needle.as_bytes();
    let haystack_bytes = haystack.as_bytes();
    let n_len = needle_bytes.len();
    if n_len > haystack_bytes.len() {
        return None;
    }
    let first_lower = needle_bytes[0].to_ascii_lowercase();
    let first_upper = needle_bytes[0].to_ascii_uppercase();

    let max_start = haystack_bytes.len() - n_len;
    let mut i = 0;
    while i <= max_start {
        let b = haystack_bytes[i];
        if (b == first_lower || b == first_upper)
            && haystack_bytes[i..i + n_len].eq_ignore_ascii_case(needle_bytes)
        {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// What an untrusted-HTML parse had to leave out. All false means every row
/// and column of the first table was kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HtmlParseReport {
    /// More rows existed than the row limit allows.
    pub rows_truncated: bool,
    /// At least one row had more cells than the column limit allows.
    pub columns_truncated: bool,
    /// Parsing stopped early because it ran past its time budget.
    pub timed_out: bool,
    /// Rows kept in the returned table.
    pub rows_kept: usize,
}

impl HtmlParseReport {
    /// True when any part of the pasted table was dropped.
    pub fn is_lossy(&self) -> bool {
        self.rows_truncated || self.columns_truncated || self.timed_out
    }

    /// User-facing sentence for a status line, or `None` when nothing was lost.
    pub fn message(&self) -> Option<String> {
        if !self.is_lossy() {
            return None;
        }
        let what = if self.rows_truncated || self.timed_out {
            format!(
                "only the first {} rows were pasted; the rest was too large to import",
                self.rows_kept
            )
        } else {
            format!(
                "some columns were cut off at {MAX_TABLE_COLS} columns; the rest was too large to import"
            )
        };
        Some(what)
    }
}

/// Parses an HTML table into a `Table` object.
pub fn html_to_table(html: &str) -> Option<Table> {
    html_to_table_with_report(html).map(|(table, _)| table)
}

/// Like [`html_to_table`], and also reports anything the safety limits dropped
/// so the caller can tell the user instead of losing data silently.
pub fn html_to_table_with_report(html: &str) -> Option<(Table, HtmlParseReport)> {
    if html.len() > MAX_CLIPBOARD_HTML_LEN {
        return None;
    }
    let mut report = HtmlParseReport::default();

    let start_time = std::time::Instant::now();

    let table_start = find_case_insensitive(html, "<table")?;
    let table_end = match find_case_insensitive(&html[table_start..], "</table>") {
        Some(idx) => table_start + idx + 8,
        None => html.len(),
    };
    let table_slice = &html[table_start..table_end];

    let mut rows_raw: Vec<Vec<RawCell>> = Vec::new();
    let mut search_pos = 0;

    while search_pos < table_slice.len() && rows_raw.len() < MAX_TABLE_ROWS {
        if start_time.elapsed().as_millis() > MAX_PARSE_DURATION_MS {
            report.timed_out = true;
            break;
        }

        let Some(tr_start_rel) = find_case_insensitive(&table_slice[search_pos..], "<tr") else {
            break;
        };
        let tr_start = search_pos + tr_start_rel;
        let Some(tag_offset) = table_slice[tr_start..].find('>') else {
            break;
        };
        let tr_close_tag_end = tr_start + tag_offset + 1;
        let tr_end = match find_case_insensitive(&table_slice[tr_close_tag_end..], "</tr>") {
            Some(end_rel) => tr_close_tag_end + end_rel,
            None => table_slice.len(),
        };

        let row_content = &table_slice[tr_close_tag_end..tr_end];
        let mut cells = Vec::new();
        let mut cell_pos = 0;

        while cell_pos < row_content.len() && cells.len() < MAX_TABLE_COLS {
            if start_time.elapsed().as_millis() > MAX_PARSE_DURATION_MS {
                report.timed_out = true;
                break;
            }

            let td_idx = find_case_insensitive(&row_content[cell_pos..], "<td");
            let th_idx = find_case_insensitive(&row_content[cell_pos..], "<th");

            let (tag_start_rel, is_th) = match (td_idx, th_idx) {
                (Some(d), Some(h)) if d < h => (d, false),
                (Some(_), Some(h)) => (h, true),
                (Some(d), None) => (d, false),
                (None, Some(h)) => (h, true),
                (None, None) => break,
            };

            let tag_start = cell_pos + tag_start_rel;
            let tag_close_end = match row_content[tag_start..].find('>') {
                Some(idx) => tag_start + idx + 1,
                None => break,
            };

            let close_tag_name = if is_th { "</th>" } else { "</td>" };
            let (cell_end, next_pos) =
                match find_case_insensitive(&row_content[tag_close_end..], close_tag_name) {
                    Some(idx) => (
                        tag_close_end + idx,
                        tag_close_end + idx + close_tag_name.len(),
                    ),
                    None => (row_content.len(), row_content.len()),
                };

            let attrs = &row_content[tag_start..tag_close_end];
            let inner_html = &row_content[tag_close_end..cell_end];

            let col_span = parse_attr_u32(attrs, "colspan").unwrap_or(1) as usize;
            let row_span = parse_attr_u32(attrs, "rowspan").unwrap_or(1) as usize;
            let style_attr = parse_attr_str(attrs, "style");

            let text = strip_html_tags(inner_html);
            cells.push(RawCell {
                text,
                col_span: col_span.min(100),
                row_span: row_span.min(100),
                is_th,
                style_attr,
            });

            cell_pos = next_pos.max(cell_pos + 1);
        }

        if cells.len() >= MAX_TABLE_COLS
            && cell_pos < row_content.len()
            && (find_case_insensitive(&row_content[cell_pos..], "<td").is_some()
                || find_case_insensitive(&row_content[cell_pos..], "<th").is_some())
        {
            report.columns_truncated = true;
        }
        if !cells.is_empty() {
            rows_raw.push(cells);
        }
        search_pos = (tr_end + 5).max(search_pos + 1).min(table_slice.len());
    }
    if rows_raw.len() >= MAX_TABLE_ROWS
        && search_pos < table_slice.len()
        && find_case_insensitive(&table_slice[search_pos..], "<tr").is_some()
    {
        report.rows_truncated = true;
    }
    report.rows_kept = rows_raw.len();

    if rows_raw.is_empty() {
        return None;
    }

    // Determine grid bounds considering spans
    let num_rows = rows_raw.len().min(MAX_TABLE_ROWS);
    let mut num_cols = 0;
    for row in &rows_raw {
        let span_sum: usize = row.iter().map(|c| c.col_span).sum();
        num_cols = num_cols.max(span_sum);
    }
    if num_cols == 0 {
        num_cols = 1;
    }

    let mut table = Table::new(num_rows, num_cols);
    table.header_row = rows_raw.first().is_some_and(|r| r.iter().any(|c| c.is_th));

    for (r, row) in rows_raw.iter().enumerate() {
        let mut target_c = 0;
        for raw_cell in row {
            while target_c < num_cols && table.is_covered(r, target_c) {
                target_c += 1;
            }
            if target_c >= num_cols {
                break;
            }

            table.set_cell_text(r, target_c, &raw_cell.text);

            let mut style_override = CellStyleOverride::default();
            if let Some(style_str) = &raw_cell.style_attr {
                let lower_style = style_str.to_lowercase();
                if lower_style.contains("font-weight: bold")
                    || lower_style.contains("font-weight:bold")
                    || raw_cell.is_th
                {
                    if let Some(cell) = table.cell_mut(r, target_c) {
                        for run in &mut cell.content.runs {
                            run.bold = true;
                        }
                    }
                }
                if lower_style.contains("text-align: right")
                    || lower_style.contains("text-align:right")
                {
                    style_override.horizontal_align = Some(HorizontalAlign::Right);
                } else if lower_style.contains("text-align: center")
                    || lower_style.contains("text-align:center")
                {
                    style_override.horizontal_align = Some(HorizontalAlign::Center);
                }

                if let Some(bg_start) = lower_style.find("background-color:") {
                    let val = &style_str[bg_start + 17..];
                    let end = val.find(';').unwrap_or(val.len());
                    style_override.background_color = Some(val[..end].trim().to_string());
                }
            }

            if style_override != CellStyleOverride::default() {
                if let Some(cell) = table.cell_mut(r, target_c) {
                    cell.style_override = Some(style_override);
                }
            }

            if raw_cell.row_span > 1 || raw_cell.col_span > 1 {
                let _ = table.merge_cells(r, target_c, raw_cell.row_span, raw_cell.col_span);
            }

            target_c += raw_cell.col_span;
        }
    }

    Some((table, report))
}

struct RawCell {
    text: String,
    col_span: usize,
    row_span: usize,
    is_th: bool,
    style_attr: Option<String>,
}

fn parse_attr_u32(attrs: &str, name: &str) -> Option<u32> {
    let key = format!("{}=\"", name);
    let start = find_case_insensitive(attrs, &key)? + key.len();
    if !attrs.is_char_boundary(start) {
        return None;
    }
    let end = attrs[start..].find('"')? + start;
    if !attrs.is_char_boundary(end) {
        return None;
    }
    attrs[start..end].parse().ok().map(|v: u32| v.min(100))
}

fn parse_attr_str(attrs: &str, name: &str) -> Option<String> {
    let key = format!("{}=\"", name);
    let start = find_case_insensitive(attrs, &key)? + key.len();
    if !attrs.is_char_boundary(start) {
        return None;
    }
    let end = attrs[start..].find('"')? + start;
    if !attrs.is_char_boundary(end) {
        return None;
    }
    let val = &attrs[start..end];
    if val.len() > 1024 {
        Some(val[..1024].to_string())
    } else {
        Some(val.to_string())
    }
}

fn strip_html_tags(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
        if text.len() >= MAX_CELL_TEXT_LEN {
            break;
        }
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            text.push(ch);
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .trim()
        .to_string()
}

fn quick_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_round_trip() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Name");
        table.set_cell_text(0, 1, "Score");
        table.set_cell_text(1, 0, "Alice");
        table.set_cell_text(1, 1, "100");

        let html = table_to_html(&table);
        assert!(html.contains("<table"));
        assert!(html.contains("Alice"));
        assert!(html.contains("100"));

        let parsed = html_to_table(&html).expect("parsed table");
        assert_eq!(parsed.row_count(), 2);
        assert_eq!(parsed.col_count(), 2);
        assert_eq!(parsed.cell_text(0, 0).as_deref(), Some("Name"));
        assert_eq!(parsed.cell_text(1, 0).as_deref(), Some("Alice"));
    }

    #[test]
    fn html_extracts_inline_styles() {
        let html = "<table><tr><th style=\"background-color: #2B579A; text-align: center;\">Head</th></tr><tr><td style=\"text-align: right;\">123</td></tr></table>";
        let table = html_to_table(html).expect("table");
        assert_eq!(table.row_count(), 2);
        assert!(table.header_row);

        let cell_1 = table.cell(1, 0).unwrap();
        assert_eq!(
            cell_1.style_override.as_ref().unwrap().horizontal_align,
            Some(HorizontalAlign::Right)
        );
    }

    #[test]
    fn hostile_input_does_not_hang_or_crash() {
        // 1. 50 MB input
        let large_junk = "a".repeat(50 * 1024 * 1024);
        assert_eq!(html_to_table(&large_junk), None);

        // 2. Deep nesting (1,000 nested divs around a table)
        let mut nested = String::new();
        for _ in 0..1000 {
            nested.push_str("<div><span>");
        }
        nested.push_str("<table><tr><td>Deep</td></tr></table>");
        for _ in 0..1000 {
            nested.push_str("</span></div>");
        }
        let parsed = html_to_table(&nested).expect("parses table inside deep nesting");
        assert_eq!(parsed.cell_text(0, 0).as_deref(), Some("Deep"));

        // 3. Malformed unclosed tags
        let malformed = "<table><tr><td>No Close<tr><td>Second";
        let parsed_malformed = html_to_table(malformed).expect("recovers from malformed table");
        assert!(parsed_malformed.row_count() >= 1);
    }

    #[test]
    fn a_table_over_the_row_limit_reports_what_was_dropped() {
        let mut html = String::from("<table>");
        for r in 0..(MAX_TABLE_ROWS + 25) {
            use std::fmt::Write;
            let _ = write!(html, "<tr><td>R{r}</td></tr>");
        }
        html.push_str("</table>");
        let (table, report) = html_to_table_with_report(&html).expect("table");
        assert_eq!(table.row_count(), MAX_TABLE_ROWS);
        assert!(report.rows_truncated && report.is_lossy());
        let message = report.message().expect("message");
        assert!(message.contains("10000"), "{message}");

        let (_, whole) =
            html_to_table_with_report("<table><tr><td>a</td></tr></table>").expect("small");
        assert!(!whole.is_lossy() && whole.message().is_none());
    }

    #[test]
    fn a_row_over_the_column_limit_reports_columns_truncated() {
        let mut html = String::from("<table><tr>");
        for c in 0..(MAX_TABLE_COLS + 5) {
            use std::fmt::Write;
            let _ = write!(html, "<td>{c}</td>");
        }
        html.push_str("</tr></table>");
        let (_, report) = html_to_table_with_report(&html).expect("table");
        assert!(report.columns_truncated && !report.rows_truncated);
        assert!(report.message().expect("message").contains("columns"));
    }

    #[test]
    fn test_hostile_paste_50mb_10k_rows_and_deep_nested_tables() {
        // 1. 10,000 rows HTML table
        let mut ten_k_rows = String::from("<table>");
        for r in 0..10_000 {
            use std::fmt::Write;
            let _ = write!(ten_k_rows, "<tr><td>Row{r}</td><td>Val{r}</td></tr>");
        }
        ten_k_rows.push_str("</table>");

        let start = std::time::Instant::now();
        let parsed = html_to_table(&ten_k_rows);
        let elapsed = start.elapsed();
        println!("10k rows parsed in {:?}", elapsed);
        assert!(
            elapsed.as_secs_f64() < 2.0,
            "Must parse 10k rows in < 2.0 s, took {:?}",
            elapsed
        );
        assert!(parsed.is_some(), "Must parse 10k rows table");
        let tbl = parsed.unwrap();
        assert_eq!(tbl.row_count(), 10_000);

        // 2. 50 MB HTML with padding and table
        let mut large_html = String::with_capacity(50 * 1024 * 1024 + 1000);
        large_html.push_str("<!-- ");
        large_html.push_str(&"x".repeat(25 * 1024 * 1024));
        large_html.push_str(" --><table><tr><td>Middle</td></tr></table><!-- ");
        large_html.push_str(&"y".repeat(25 * 1024 * 1024));
        large_html.push_str(" -->");

        let start_large = std::time::Instant::now();
        let parsed_large = html_to_table(&large_html);
        let elapsed_large = start_large.elapsed();
        println!("50MB HTML handled in {:?}", elapsed_large);
        assert!(
            elapsed_large.as_secs_f64() < 2.0,
            "Must handle 50MB HTML in < 2.0 s, took {:?}",
            elapsed_large
        );
        assert!(parsed_large.is_some());
        assert_eq!(
            parsed_large.unwrap().cell_text(0, 0).as_deref(),
            Some("Middle")
        );

        // 3. Deeply nested tables (500 levels)
        let mut deeply_nested = String::new();
        for _ in 0..500 {
            deeply_nested.push_str("<table><tr><td>");
        }
        deeply_nested.push_str("Innermost");
        for _ in 0..500 {
            deeply_nested.push_str("</td></tr></table>");
        }
        let start_deep = std::time::Instant::now();
        let parsed_deep = html_to_table(&deeply_nested);
        let elapsed_deep = start_deep.elapsed();
        println!("500 nested tables handled in {:?}", elapsed_deep);
        assert!(
            elapsed_deep.as_secs_f64() < 2.0,
            "Must handle 500 nested tables in < 2.0 s, took {:?}",
            elapsed_deep
        );
        assert!(parsed_deep.is_some());
    }
}
