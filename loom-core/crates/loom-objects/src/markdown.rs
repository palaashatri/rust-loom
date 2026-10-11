//! Markdown table migration and serialization for Loom tables.

use crate::table::{
    CellContent, CellStyleOverride, HorizontalAlign, Table, TableCell, TableColumn, TableRow,
    TextRun, DEFAULT_ROW_HEIGHT_PT,
};

/// Converts a Markdown pipe table into a structured `Table` object.
pub fn markdown_to_table(markdown: &str) -> Table {
    let mut raw_rows: Vec<Vec<String>> = Vec::new();
    let mut alignments: Vec<HorizontalAlign> = Vec::new();
    let mut header_row_present = false;

    for line in markdown.lines() {
        let trimmed = line.trim();
        let Some(first_pipe) = trimmed.find('|') else {
            if !trimmed.is_empty() {
                raw_rows.push(vec![trimmed.to_string()]);
            }
            continue;
        };
        let Some(last_pipe) = trimmed.rfind('|') else {
            continue;
        };
        if first_pipe >= last_pipe {
            if !trimmed.is_empty() {
                raw_rows.push(vec![trimmed.to_string()]);
            }
            continue;
        }

        let prefix = trimmed[..first_pipe].trim();
        let suffix = trimmed[last_pipe + 1..].trim();
        let inner = &trimmed[first_pipe + 1..last_pipe];

        let mut cells: Vec<String> = split_cells(inner)
            .into_iter()
            .map(|cell| cell.trim().replace("\\|", "|"))
            .collect();

        if is_delimiter_row(&cells) {
            alignments = parse_alignments(&cells);
            header_row_present = true;
            continue;
        }

        if !prefix.is_empty() {
            if let Some(first_cell) = cells.first_mut() {
                if first_cell.is_empty() {
                    *first_cell = prefix.to_string();
                } else {
                    *first_cell = format!("{prefix} {first_cell}");
                }
            }
        }
        if !suffix.is_empty() {
            if let Some(last_cell) = cells.last_mut() {
                if last_cell.is_empty() {
                    *last_cell = suffix.to_string();
                } else {
                    *last_cell = format!("{last_cell} {suffix}");
                }
            }
        }

        raw_rows.push(cells);
    }

    if raw_rows.is_empty() {
        return Table::new(1, 1);
    }

    let max_cols = raw_rows.iter().map(|r| r.len()).max().unwrap_or(1).max(1);

    // Compute column widths based on maximum character count (approx 7.0 pt per char, min 60.0 pt)
    let columns = (0..max_cols)
        .map(|c| {
            let max_len = raw_rows
                .iter()
                .filter_map(|r| r.get(c))
                .map(|text| text.len())
                .max()
                .unwrap_or(4);
            let width_pt = (max_len as f32 * 7.5 + 24.0).clamp(60.0, 300.0);
            TableColumn { width_pt }
        })
        .collect();

    let rows = raw_rows
        .into_iter()
        .map(|raw_cells| {
            let mut cells = Vec::with_capacity(max_cols);
            for c in 0..max_cols {
                let cell_text = raw_cells.get(c).cloned().unwrap_or_default();
                let runs = parse_inline_markdown(&cell_text);
                let align = alignments.get(c).copied();

                let style_override = align.map(|a| CellStyleOverride {
                    horizontal_align: Some(a),
                    ..Default::default()
                });

                cells.push(TableCell {
                    content: CellContent { runs },
                    row_span: 1,
                    col_span: 1,
                    is_covered: false,
                    style_override,
                });
            }
            TableRow {
                height_pt: Some(DEFAULT_ROW_HEIGHT_PT),
                cells,
            }
        })
        .collect();

    let mut table = Table::new(1, 1);
    table.columns = columns;
    table.rows = rows;
    table.header_row = header_row_present;
    table
}

/// Serializes a `Table` back to clean, aligned GFM Markdown.
pub fn table_to_markdown(table: &Table) -> String {
    if table.row_count() == 0 || table.col_count() == 0 {
        return String::new();
    }

    let col_count = table.col_count();
    let mut col_widths: Vec<usize> = vec![3; col_count];

    // Calculate minimum column character widths for alignment
    for row in &table.rows {
        for (c, cell) in row.cells.iter().enumerate() {
            if c < col_count {
                let text = cell.content.text().replace('|', "\\|");
                col_widths[c] = col_widths[c].max(text.chars().count());
            }
        }
    }

    let mut out = String::new();

    // Determine alignments per column from the first row or cell overrides
    let alignments: Vec<HorizontalAlign> = (0..col_count)
        .map(|c| {
            for row in &table.rows {
                if let Some(cell) = row.cells.get(c) {
                    if let Some(ov) = &cell.style_override {
                        if let Some(a) = ov.horizontal_align {
                            return a;
                        }
                    }
                }
            }
            HorizontalAlign::Left
        })
        .collect();

    for (r, row) in table.rows.iter().enumerate() {
        out.push('|');
        for (c, width) in col_widths.iter().enumerate() {
            out.push(' ');
            let raw_text = row
                .cells
                .get(c)
                .map(|cell| cell.content.text().replace('|', "\\|"))
                .unwrap_or_default();

            let pad = width.saturating_sub(raw_text.chars().count());
            match alignments.get(c).copied().unwrap_or(HorizontalAlign::Left) {
                HorizontalAlign::Right => {
                    out.push_str(&" ".repeat(pad));
                    out.push_str(&raw_text);
                }
                HorizontalAlign::Center => {
                    let left_pad = pad / 2;
                    let right_pad = pad - left_pad;
                    out.push_str(&" ".repeat(left_pad));
                    out.push_str(&raw_text);
                    out.push_str(&" ".repeat(right_pad));
                }
                _ => {
                    out.push_str(&raw_text);
                    out.push_str(&" ".repeat(pad));
                }
            }
            out.push_str(" |");
        }
        out.push('\n');

        // Write delimiter row after header row (row 0)
        if r == 0 && table.header_row {
            out.push('|');
            for (c, width) in col_widths.iter().enumerate() {
                out.push(' ');
                let d_len = (*width).max(3);
                match alignments.get(c).copied().unwrap_or(HorizontalAlign::Left) {
                    HorizontalAlign::Right => {
                        out.push_str(&"-".repeat(d_len - 1));
                        out.push(':');
                    }
                    HorizontalAlign::Center => {
                        out.push(':');
                        out.push_str(&"-".repeat(d_len.saturating_sub(2)));
                        out.push(':');
                    }
                    _ => {
                        out.push(':');
                        out.push_str(&"-".repeat(d_len - 1));
                    }
                }
                out.push_str(" |");
            }
            out.push('\n');
        }
    }

    out
}

fn split_cells(inner: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut current = String::new();
    let mut escaped = false;

    for ch in inner.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
        } else if ch == '\\' {
            current.push(ch);
            escaped = true;
        } else if ch == '|' {
            cells.push(current);
            current = String::new();
        } else {
            current.push(ch);
        }
    }
    cells.push(current);
    cells
}

fn is_delimiter_row(cells: &[String]) -> bool {
    if cells.is_empty() {
        return false;
    }
    cells.iter().all(|cell| {
        let trimmed = cell.trim();
        !trimmed.is_empty()
            && trimmed
                .chars()
                .all(|ch| ch == '-' || ch == ':' || ch == ' ')
            && trimmed.contains('-')
    })
}

fn parse_alignments(cells: &[String]) -> Vec<HorizontalAlign> {
    cells
        .iter()
        .map(|cell| {
            let trimmed = cell.trim();
            let starts_col = trimmed.starts_with(':');
            let ends_col = trimmed.ends_with(':');
            if starts_col && ends_col {
                HorizontalAlign::Center
            } else if ends_col {
                HorizontalAlign::Right
            } else {
                HorizontalAlign::Left
            }
        })
        .collect()
}

/// Parses simple inline markdown into rich text runs.
fn parse_inline_markdown(text: &str) -> Vec<TextRun> {
    if text.is_empty() {
        return Vec::new();
    }

    // Check for bold **text** or italic *text*
    let mut runs = Vec::new();
    let mut remainder = text;

    while !remainder.is_empty() {
        if let Some(bold_start) = remainder.find("**") {
            if bold_start > 0 {
                runs.push(TextRun::plain(&remainder[..bold_start]));
            }
            let after_start = &remainder[bold_start + 2..];
            if let Some(bold_end) = after_start.find("**") {
                runs.push(TextRun::bold(&after_start[..bold_end]));
                remainder = &after_start[bold_end + 2..];
            } else {
                runs.push(TextRun::plain(&remainder[bold_start..]));
                break;
            }
        } else if let Some(italic_start) = remainder.find('*') {
            if italic_start > 0 {
                runs.push(TextRun::plain(&remainder[..italic_start]));
            }
            let after_start = &remainder[italic_start + 1..];
            if let Some(italic_end) = after_start.find('*') {
                let mut italic_run = TextRun::plain(&after_start[..italic_end]);
                italic_run.italic = true;
                runs.push(italic_run);
                remainder = &after_start[italic_end + 1..];
            } else {
                runs.push(TextRun::plain(&remainder[italic_start..]));
                break;
            }
        } else {
            runs.push(TextRun::plain(remainder));
            break;
        }
    }

    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_round_trip() {
        let md = "| Name | Role | Score |\n| :--- | :--- | :--- |\n| Alice | Lead | 95 |\n| Bob | Coder | 88 |\n";
        let table = markdown_to_table(md);
        assert_eq!(table.row_count(), 3);
        assert_eq!(table.col_count(), 3);
        assert!(table.header_row);
        assert_eq!(table.cell_text(0, 0).as_deref(), Some("Name"));
        assert_eq!(table.cell_text(1, 0).as_deref(), Some("Alice"));

        let exported = table_to_markdown(&table);
        assert!(exported.contains("| Name"));
        assert!(exported.contains("| Alice"));
        assert!(exported.contains("| Lead"));
    }

    #[test]
    fn alignment_parsed_and_preserved() {
        let md = "| Left | Center | Right |\n| :--- | :---: | ---: |\n| A | B | C |\n";
        let table = markdown_to_table(md);
        let c0_align = table
            .cell(1, 0)
            .unwrap()
            .style_override
            .as_ref()
            .and_then(|s| s.horizontal_align);
        let c1_align = table
            .cell(1, 1)
            .unwrap()
            .style_override
            .as_ref()
            .and_then(|s| s.horizontal_align);
        let c2_align = table
            .cell(1, 2)
            .unwrap()
            .style_override
            .as_ref()
            .and_then(|s| s.horizontal_align);

        assert_eq!(c0_align, Some(HorizontalAlign::Left));
        assert_eq!(c1_align, Some(HorizontalAlign::Center));
        assert_eq!(c2_align, Some(HorizontalAlign::Right));

        let exported = table_to_markdown(&table);
        let round_trip_table = markdown_to_table(&exported);
        let rt_c0 = round_trip_table
            .cell(1, 0)
            .unwrap()
            .style_override
            .as_ref()
            .and_then(|s| s.horizontal_align);
        let rt_c1 = round_trip_table
            .cell(1, 1)
            .unwrap()
            .style_override
            .as_ref()
            .and_then(|s| s.horizontal_align);
        let rt_c2 = round_trip_table
            .cell(1, 2)
            .unwrap()
            .style_override
            .as_ref()
            .and_then(|s| s.horizontal_align);
        assert_eq!(rt_c0, Some(HorizontalAlign::Left));
        assert_eq!(rt_c1, Some(HorizontalAlign::Center));
        assert_eq!(rt_c2, Some(HorizontalAlign::Right));
    }

    #[test]
    fn escaped_pipes_handled() {
        let md = "| Key | Formula |\n| :--- | :--- |\n| Val | a \\| b |\n";
        let table = markdown_to_table(md);
        assert_eq!(table.cell_text(1, 1).as_deref(), Some("a | b"));

        let exported = table_to_markdown(&table);
        assert!(exported.contains("a \\| b"));
    }

    #[test]
    fn inline_formatting_parsed_into_runs() {
        let md = "| Item | Desc |\n| :--- | :--- |\n| Test | **Bold** and *Italic* |\n";
        let table = markdown_to_table(md);
        let cell = table.cell(1, 1).unwrap();
        assert_eq!(cell.content.runs.len(), 3);
        assert!(cell.content.runs[0].bold);
        assert_eq!(cell.content.runs[0].text, "Bold");
        assert_eq!(cell.content.runs[1].text, " and ");
        assert!(cell.content.runs[2].italic);
        assert_eq!(cell.content.runs[2].text, "Italic");
    }

    #[test]
    fn prefix_and_suffix_text_preserved() {
        let md = "Note: | Col1 | Col2 | (End)\n| :--- | :--- |\n| Val1 | Val2 |\n";
        let table = markdown_to_table(md);
        assert!(table.cell_text(0, 0).unwrap().contains("Note:"));
        assert!(table.cell_text(0, 1).unwrap().contains("(End)"));
    }
}
