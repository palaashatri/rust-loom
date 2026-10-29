//! HTML table serialization and parsing with inline CSS styles.

use loom_objects::styles::{TableStyleCatalog, TableStyleOptions};
use loom_objects::table::{CellStyleOverride, HorizontalAlign, Table, TextRun};

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
                HorizontalAlign::Justify => css.push_str(" text-align: justify;"),
                HorizontalAlign::Left => css.push_str(" text-align: left;"),
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
const MAX_TABLE_COLUMN_WIDTH_PT: f32 = 100_000.0;
const MAX_INLINE_STYLE_DEPTH: usize = 64;

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
    let column_widths = parse_column_widths(table_slice, &start_time, &mut report);

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

            let runs = parse_inline_runs(
                inner_html,
                is_th,
                style_attr.as_deref(),
                &start_time,
                &mut report,
            );
            cells.push(RawCell {
                runs,
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
    for (column, width) in table.columns.iter_mut().zip(column_widths) {
        if let Some(width) = width {
            column.width_pt = width;
        }
    }

    for (r, row) in rows_raw.into_iter().enumerate() {
        let mut target_c = 0;
        for raw_cell in row {
            while target_c < num_cols && table.is_covered(r, target_c) {
                target_c += 1;
            }
            if target_c >= num_cols {
                break;
            }

            table.set_cell_rich_text(r, target_c, raw_cell.runs);

            let style_override = parse_cell_style_override(raw_cell.style_attr.as_deref());
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HtmlInlineTag {
    Span,
    Bold,
    Italic,
    Underline,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct HtmlTextStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    color: Option<String>,
}

fn parse_column_widths(
    table_html: &str,
    start_time: &std::time::Instant,
    report: &mut HtmlParseReport,
) -> Vec<Option<f32>> {
    let mut widths = Vec::new();
    let mut search_pos = 0;

    while search_pos < table_html.len() && widths.len() < MAX_TABLE_COLS {
        if start_time.elapsed().as_millis() > MAX_PARSE_DURATION_MS {
            report.timed_out = true;
            break;
        }

        let Some(tag_start_rel) = find_case_insensitive(&table_html[search_pos..], "<col") else {
            break;
        };
        let tag_start = search_pos + tag_start_rel;
        let name_end = tag_start + "<col".len();
        let Some(delimiter) = table_html.as_bytes().get(name_end).copied() else {
            break;
        };
        if !(delimiter.is_ascii_whitespace() || delimiter == b'/' || delimiter == b'>') {
            search_pos = name_end;
            continue;
        }

        let Some(tag_end_rel) = table_html[tag_start..].find('>') else {
            break;
        };
        let tag_end = tag_start + tag_end_rel;
        let attrs = &table_html[name_end..tag_end];
        let width = parse_attr_str(attrs, "style")
            .and_then(|style| css_property_value(&style, "width").map(str::to_owned))
            .and_then(|value| parse_css_length_pt(&value));
        let span = parse_attr_u32(attrs, "span").unwrap_or(1).max(1) as usize;
        for _ in 0..span.min(MAX_TABLE_COLS - widths.len()) {
            widths.push(width);
        }
        search_pos = tag_end + 1;
    }

    widths
}

fn parse_css_length_pt(value: &str) -> Option<f32> {
    let value = value.trim().to_ascii_lowercase();
    let (number, scale) = if let Some(value) = value.strip_suffix("pt") {
        (value.trim(), 1.0)
    } else if let Some(value) = value.strip_suffix("px") {
        (value.trim(), 0.75)
    } else if let Some(value) = value.strip_suffix("in") {
        (value.trim(), 72.0)
    } else if let Some(value) = value.strip_suffix("cm") {
        (value.trim(), 72.0 / 2.54)
    } else if let Some(value) = value.strip_suffix("mm") {
        (value.trim(), 72.0 / 25.4)
    } else {
        return None;
    };

    let width = number.parse::<f32>().ok()? * scale;
    (width.is_finite() && width > 0.0 && width <= MAX_TABLE_COLUMN_WIDTH_PT).then_some(width)
}

fn css_property_value<'a>(style: &'a str, property: &str) -> Option<&'a str> {
    style.split(';').find_map(|declaration| {
        let (name, value) = declaration.split_once(':')?;
        name.trim()
            .eq_ignore_ascii_case(property)
            .then_some(value.trim())
    })
}

fn parse_css_color(value: &str) -> Option<String> {
    let hex = value.trim().strip_prefix('#')?;
    if !matches!(hex.len(), 3 | 4 | 6 | 8)
        || !hex.is_ascii()
        || !hex.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return None;
    }

    match hex.len() {
        3 | 4 => {
            let mut expanded = String::with_capacity(1 + hex.len() * 2);
            expanded.push('#');
            for nibble in hex.bytes() {
                expanded.push(nibble as char);
                expanded.push(nibble as char);
            }
            Some(expanded)
        }
        6 | 8 => Some(format!("#{hex}")),
        _ => None,
    }
}

fn parse_font_weight(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "bold" | "bolder" => Some(true),
        "normal" | "lighter" => Some(false),
        numeric => numeric.parse::<u16>().ok().map(|weight| weight >= 600),
    }
}

fn apply_css_run_style(style: &mut HtmlTextStyle, declarations: &str) {
    if let Some(value) = css_property_value(declarations, "font-weight").and_then(parse_font_weight)
    {
        style.bold = value;
    }
    if let Some(value) = css_property_value(declarations, "font-style") {
        match value.to_ascii_lowercase().trim() {
            "italic" | "oblique" => style.italic = true,
            "normal" => style.italic = false,
            _ => {}
        }
    }
    if let Some(value) = css_property_value(declarations, "text-decoration") {
        let value = value.to_ascii_lowercase();
        if value
            .split_ascii_whitespace()
            .any(|token| token == "underline")
        {
            style.underline = true;
        } else if value.trim() == "none" {
            style.underline = false;
        }
    }
    if let Some(color) = css_property_value(declarations, "color").and_then(parse_css_color) {
        style.color = Some(color);
    }
}

fn parse_cell_style_override(style: Option<&str>) -> CellStyleOverride {
    let mut cell_style = CellStyleOverride::default();
    let Some(style) = style else {
        return cell_style;
    };

    if let Some(value) = css_property_value(style, "text-align") {
        cell_style.horizontal_align = match value.to_ascii_lowercase().trim() {
            "left" => Some(HorizontalAlign::Left),
            "center" => Some(HorizontalAlign::Center),
            "right" => Some(HorizontalAlign::Right),
            "justify" => Some(HorizontalAlign::Justify),
            _ => None,
        };
    }
    cell_style.background_color =
        css_property_value(style, "background-color").and_then(parse_css_color);

    cell_style
}

fn parse_inline_tag(tag_text: &str) -> Option<(bool, bool, HtmlInlineTag, Option<String>)> {
    let tag_text = tag_text.trim();
    let closing = tag_text.starts_with('/');
    let contents = if closing {
        tag_text[1..].trim_start()
    } else {
        tag_text
    };
    let self_closing = !closing && contents.trim_end().ends_with('/');
    let name_end = contents
        .find(|ch: char| ch.is_ascii_whitespace() || ch == '/')
        .unwrap_or(contents.len());
    let tag = match &contents[..name_end] {
        name if name.eq_ignore_ascii_case("span") => HtmlInlineTag::Span,
        name if name.eq_ignore_ascii_case("b") || name.eq_ignore_ascii_case("strong") => {
            HtmlInlineTag::Bold
        }
        name if name.eq_ignore_ascii_case("i") || name.eq_ignore_ascii_case("em") => {
            HtmlInlineTag::Italic
        }
        name if name.eq_ignore_ascii_case("u") => HtmlInlineTag::Underline,
        _ => return None,
    };
    let declarations = if tag == HtmlInlineTag::Span && !closing {
        parse_attr_str(contents, "style")
    } else {
        None
    };
    Some((closing, self_closing, tag, declarations))
}

fn parse_inline_runs(
    html: &str,
    is_th: bool,
    cell_style: Option<&str>,
    start_time: &std::time::Instant,
    report: &mut HtmlParseReport,
) -> Vec<TextRun> {
    let mut base_style = HtmlTextStyle {
        bold: is_th,
        ..HtmlTextStyle::default()
    };
    if let Some(cell_style) = cell_style {
        apply_css_run_style(&mut base_style, cell_style);
    }

    let mut style_stack: Vec<(HtmlInlineTag, HtmlTextStyle)> = Vec::new();
    let mut overflow_depth = 0;
    let mut runs = Vec::new();
    let mut text_buffer = String::new();
    let mut extracted_len = 0;
    let mut pos = 0;
    let mut steps = 0;

    while pos < html.len() {
        steps += 1;
        if steps % 256 == 0 && start_time.elapsed().as_millis() > MAX_PARSE_DURATION_MS {
            report.timed_out = true;
            break;
        }

        if html.as_bytes()[pos] == b'<' {
            let Some(tag_end_rel) = html[pos..].find('>') else {
                break;
            };
            let tag_end = pos + tag_end_rel;
            if let Some((closing, self_closing, tag, declarations)) =
                parse_inline_tag(&html[pos + 1..tag_end])
            {
                let active_style = style_stack
                    .last()
                    .map(|(_, style)| style)
                    .unwrap_or(&base_style);
                push_inline_run(&mut runs, &mut text_buffer, active_style);

                if !self_closing {
                    if closing {
                        if overflow_depth > 0 {
                            overflow_depth -= 1;
                        } else if let Some(index) = style_stack
                            .iter()
                            .rposition(|(open_tag, _)| *open_tag == tag)
                        {
                            style_stack.truncate(index);
                        }
                    } else if overflow_depth > 0 || style_stack.len() >= MAX_INLINE_STYLE_DEPTH {
                        overflow_depth += 1;
                    } else {
                        let mut style = active_style.clone();
                        match tag {
                            HtmlInlineTag::Span => {
                                if let Some(declarations) = declarations.as_deref() {
                                    apply_css_run_style(&mut style, declarations);
                                }
                            }
                            HtmlInlineTag::Bold => style.bold = true,
                            HtmlInlineTag::Italic => style.italic = true,
                            HtmlInlineTag::Underline => style.underline = true,
                        }
                        style_stack.push((tag, style));
                    }
                }
            }
            pos = tag_end + 1;
        } else {
            let next_tag = html[pos..]
                .find('<')
                .map(|offset| pos + offset)
                .unwrap_or(html.len());
            let remaining = MAX_CELL_TEXT_LEN.saturating_sub(extracted_len);
            if remaining == 0 {
                break;
            }
            extracted_len += append_decoded_text(&html[pos..next_tag], &mut text_buffer, remaining);
            pos = next_tag;
            if extracted_len >= MAX_CELL_TEXT_LEN {
                break;
            }
        }
    }

    let active_style = style_stack
        .last()
        .map(|(_, style)| style)
        .unwrap_or(&base_style);
    push_inline_run(&mut runs, &mut text_buffer, active_style);
    trim_inline_runs(&mut runs);
    runs
}

fn append_decoded_text(input: &str, output: &mut String, max_bytes: usize) -> usize {
    let initial_len = output.len();
    let mut pos = 0;

    while pos < input.len() && output.len() - initial_len < max_bytes {
        let remaining = max_bytes - (output.len() - initial_len);
        let rest = &input[pos..];
        let entity = [
            ("&nbsp;", " "),
            ("&amp;", "&"),
            ("&lt;", "<"),
            ("&gt;", ">"),
            ("&quot;", "\""),
        ]
        .into_iter()
        .find(|(encoded, _)| rest.starts_with(encoded));

        if let Some((encoded, decoded)) = entity {
            if decoded.len() > remaining {
                break;
            }
            output.push_str(decoded);
            pos += encoded.len();
        } else if let Some(character) = rest.chars().next() {
            if character.len_utf8() > remaining {
                break;
            }
            output.push(character);
            pos += character.len_utf8();
        } else {
            break;
        }
    }

    output.len() - initial_len
}

fn push_inline_run(runs: &mut Vec<TextRun>, text: &mut String, style: &HtmlTextStyle) {
    if text.is_empty() {
        return;
    }

    let mut run = TextRun::plain(std::mem::take(text));
    run.bold = style.bold;
    run.italic = style.italic;
    run.underline = style.underline;
    run.color = style.color.clone();

    if let Some(previous) = runs.last_mut() {
        if previous.bold == run.bold
            && previous.italic == run.italic
            && previous.underline == run.underline
            && previous.color == run.color
        {
            previous.text.push_str(&run.text);
            return;
        }
    }
    runs.push(run);
}

fn trim_inline_runs(runs: &mut Vec<TextRun>) {
    let mut leading_trimmed = false;
    for run in runs.iter_mut() {
        if !leading_trimmed {
            run.text = run.text.trim_start().to_owned();
            leading_trimmed = !run.text.is_empty();
        }
    }

    let mut trailing_trimmed = false;
    for run in runs.iter_mut().rev() {
        if !trailing_trimmed {
            run.text = run.text.trim_end().to_owned();
            trailing_trimmed = !run.text.is_empty();
        }
    }

    runs.retain(|run| !run.text.is_empty());
}

struct RawCell {
    runs: Vec<TextRun>,
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
    let key = format!("{name}=");
    let mut search_pos = 0;

    while search_pos < attrs.len() {
        let key_start = search_pos + find_case_insensitive(&attrs[search_pos..], &key)?;
        if !attrs.is_char_boundary(key_start) {
            return None;
        }
        if key_start > 0 && !attrs.as_bytes()[key_start - 1].is_ascii_whitespace() {
            search_pos = key_start + key.len();
            continue;
        }

        let value_start = key_start + key.len();
        let quote = *attrs.as_bytes().get(value_start)?;
        if quote != b'"' && quote != b'\'' {
            search_pos = value_start + 1;
            continue;
        }
        let content_start = value_start + 1;
        let end = attrs[content_start..].find(char::from(quote))? + content_start;
        let value = &attrs[content_start..end];
        let mut safe_end = value.len().min(1024);
        while !value.is_char_boundary(safe_end) {
            safe_end -= 1;
        }
        return Some(value[..safe_end].to_owned());
    }

    None
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
    fn html_round_trips_column_widths_cell_styles_and_inline_runs() {
        let mut source = Table::new(2, 2);
        source.header_row = false;
        source.set_cell_text(0, 0, "Name");
        source.set_cell_text(0, 1, "Score");
        source.cell_mut(0, 1).expect("cell").style_override = Some(CellStyleOverride {
            horizontal_align: Some(HorizontalAlign::Center),
            ..Default::default()
        });

        let mut styled_run = loom_objects::table::TextRun::plain("Styled");
        styled_run.bold = true;
        styled_run.italic = true;
        styled_run.underline = true;
        styled_run.color = Some("#123456FF".to_owned());
        source.set_cell_rich_text(1, 0, vec![styled_run]);
        source.set_cell_text(1, 1, "100");
        source.columns[0].width_pt = 210.0;
        source.columns[1].width_pt = 91.0;
        source.cell_mut(1, 1).expect("cell").style_override = Some(CellStyleOverride {
            background_color: Some("#E6F4EA".to_owned()),
            horizontal_align: Some(HorizontalAlign::Justify),
            ..Default::default()
        });

        let html = table_to_html(&source);
        let parsed = html_to_table(&html).expect("parsed table");

        assert_eq!(
            parsed
                .columns
                .iter()
                .map(|column| column.width_pt)
                .collect::<Vec<_>>(),
            vec![210.0, 91.0]
        );
        let styled_cell = parsed.cell(1, 0).expect("styled cell");
        let styled_run = styled_cell
            .content
            .runs
            .iter()
            .find(|run| run.text == "Styled")
            .expect("styled text run");
        assert!(styled_run.bold && styled_run.italic && styled_run.underline);
        assert_eq!(styled_run.color.as_deref(), Some("#123456FF"));

        let cell_style = parsed
            .cell(1, 1)
            .and_then(|cell| cell.style_override.as_ref())
            .expect("cell style override");
        assert_eq!(cell_style.horizontal_align, Some(HorizontalAlign::Justify));
        assert_eq!(cell_style.background_color.as_deref(), Some("#E6F4EA"));
        assert_eq!(
            parsed
                .cell(0, 1)
                .and_then(|cell| cell.style_override.as_ref())
                .and_then(|style| style.horizontal_align),
            Some(HorizontalAlign::Center)
        );
    }

    #[test]
    fn invalid_column_widths_fall_back_to_finite_defaults() {
        let html = concat!(
            "<table><col style=\"width: NaNpt;\">",
            "<col style=\"width: -4pt;\">",
            "<col style=\"width: 1e999pt;\">",
            "<tr><td>a</td><td>b</td><td>c</td></tr></table>"
        );
        let parsed = html_to_table(html).expect("table with invalid widths");
        let defaults = Table::new(1, 3);

        for (actual, default) in parsed.columns.iter().zip(defaults.columns.iter()) {
            assert!(actual.width_pt.is_finite() && actual.width_pt > 0.0);
            assert_eq!(actual.width_pt, default.width_pt);
        }
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
