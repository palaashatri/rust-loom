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
const MAX_CLIPBOARD_HTML_LEN: usize = 16 * 1024 * 1024;
/// Maximum table dimensions constructed from untrusted HTML.
const MAX_TABLE_ROWS: usize = 10_000;
const MAX_TABLE_COLS: usize = 1_000;

/// Parses an HTML table into a `Table` object.
pub fn html_to_table(html: &str) -> Option<Table> {
    if html.len() > MAX_CLIPBOARD_HTML_LEN {
        return None;
    }

    let table_lower = html.to_lowercase();
    let table_start = table_lower.find("<table")?;
    let table_end = match table_lower[table_start..].find("</table>") {
        Some(idx) => table_start + idx + 8,
        None => html.len(),
    };
    let table_slice = &html[table_start..table_end];

    let mut rows_raw: Vec<Vec<RawCell>> = Vec::new();
    let mut search_pos = 0;

    while search_pos < table_slice.len() && rows_raw.len() < MAX_TABLE_ROWS {
        let Some(tr_start_rel) = table_slice[search_pos..].to_lowercase().find("<tr") else {
            break;
        };
        let tr_start = search_pos + tr_start_rel;
        let Some(tag_offset) = table_slice[tr_start..].find('>') else {
            break;
        };
        let tr_close_tag_end = tr_start + tag_offset + 1;
        let tr_end = match table_slice[tr_close_tag_end..].to_lowercase().find("</tr>") {
            Some(end_rel) => tr_close_tag_end + end_rel,
            None => table_slice.len(),
        };

        let row_content = &table_slice[tr_close_tag_end..tr_end];
        let mut cells = Vec::new();
        let mut cell_pos = 0;

        while cell_pos < row_content.len() && cells.len() < MAX_TABLE_COLS {
            let lower_row = row_content[cell_pos..].to_lowercase();
            let td_idx = lower_row.find("<td");
            let th_idx = lower_row.find("<th");

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
            let (cell_end, next_pos) = match row_content[tag_close_end..]
                .to_lowercase()
                .find(close_tag_name)
            {
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

        if !cells.is_empty() {
            rows_raw.push(cells);
        }
        search_pos = (tr_end + 5).max(search_pos + 1).min(table_slice.len());
    }

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

    Some(table)
}

struct RawCell {
    text: String,
    col_span: usize,
    row_span: usize,
    is_th: bool,
    style_attr: Option<String>,
}

fn parse_attr_u32(attrs: &str, name: &str) -> Option<u32> {
    let lower = attrs.to_lowercase();
    let key = format!("{}=\"", name);
    let start = lower.find(&key)? + key.len();
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
    let lower = attrs.to_lowercase();
    let key = format!("{}=\"", name);
    let start = lower.find(&key)? + key.len();
    if !attrs.is_char_boundary(start) {
        return None;
    }
    let end = attrs[start..].find('"')? + start;
    if !attrs.is_char_boundary(end) {
        return None;
    }
    Some(attrs[start..end].to_string())
}

fn strip_html_tags(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for ch in html.chars() {
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
}
