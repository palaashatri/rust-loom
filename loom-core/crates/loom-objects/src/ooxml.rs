//! OOXML serialization mapping for WordprocessingML `w:tbl` and DrawingML `a:tbl`.

use crate::styles::{TableStyleCatalog, TableStyleOptions};
use crate::table::{HorizontalAlign, Table};

/// Converts points to twips / dxa (1 pt = 20 twips).
pub fn pt_to_dxa(pt: f32) -> u32 {
    (pt * 20.0).round().max(0.0) as u32
}

/// Converts points to EMUs (1 pt = 12,700 EMUs).
pub fn pt_to_emu(pt: f32) -> u64 {
    (pt * 12_700.0).round().max(0.0) as u64
}

/// Cleans a hex color string (e.g. `"#2B579A"` -> `"2B579A"`).
fn clean_hex_color(color: &str) -> String {
    color.trim_start_matches('#').to_uppercase()
}

/// Serializes a `Table` to WordprocessingML `<w:tbl>`.
pub fn table_to_docx_wtbl(table: &Table) -> String {
    let mut out = String::new();
    out.push_str("<w:tbl>\n");

    // Table properties
    out.push_str("  <w:tblPr>\n");
    let total_dxa: u32 = table.columns.iter().map(|c| pt_to_dxa(c.width_pt)).sum();
    out.push_str(&format!(
        "    <w:tblW w:w=\"{}\" w:type=\"dxa\"/>\n",
        total_dxa
    ));
    out.push_str("    <w:tblBorders>\n");
    out.push_str("      <w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\n");
    out.push_str("      <w:bottom w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/>\n");
    out.push_str(
        "      <w:insideH w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"E0E0E0\"/>\n",
    );
    out.push_str("    </w:tblBorders>\n");
    out.push_str("  </w:tblPr>\n");

    // Table grid
    out.push_str("  <w:tblGrid>\n");
    for col in &table.columns {
        out.push_str(&format!(
            "    <w:gridCol w:w=\"{}\"/>\n",
            pt_to_dxa(col.width_pt)
        ));
    }
    out.push_str("  </w:tblGrid>\n");

    // Resolve style
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

    // Rows
    for (r, row) in table.rows.iter().enumerate() {
        out.push_str("  <w:tr>\n");
        if r == 0 && table.header_row {
            out.push_str("    <w:trPr><w:tblHeader/></w:trPr>\n");
        }

        for (c, cell) in row.cells.iter().enumerate() {
            if let Some((_, orig_c, orig_cell)) = table.effective_cell(r, c) {
                if (orig_cell.col_span > 1 || orig_cell.row_span > 1) && c > orig_c {
                    // Horizontally covered cell: skip emitting w:tc
                    continue;
                }
            }

            let resolved = style.resolve_cell(
                r,
                c,
                table.row_count(),
                table.col_count(),
                &options,
                Some(cell),
            );

            let span_cols = if let Some((_, orig_c, orig_cell)) = table.effective_cell(r, c) {
                if orig_cell.col_span > 1 && c == orig_c {
                    orig_cell.col_span
                } else {
                    cell.col_span
                }
            } else {
                cell.col_span
            };
            let col_width: f32 = (c..c + span_cols)
                .filter_map(|col_idx| table.columns.get(col_idx))
                .map(|col| col.width_pt)
                .sum();
            let cell_dxa = pt_to_dxa(col_width);

            out.push_str("    <w:tc>\n");
            out.push_str("      <w:tcPr>\n");
            out.push_str(&format!(
                "        <w:tcW w:w=\"{}\" w:type=\"dxa\"/>\n",
                cell_dxa
            ));

            if let Some((orig_r, orig_c, orig_cell)) = table.effective_cell(r, c) {
                if orig_cell.col_span > 1 && c == orig_c {
                    out.push_str(&format!(
                        "        <w:gridSpan w:val=\"{}\"/>\n",
                        orig_cell.col_span
                    ));
                }
                if orig_cell.row_span > 1 && c == orig_c {
                    if r == orig_r {
                        out.push_str("        <w:vMerge w:val=\"restart\"/>\n");
                    } else {
                        out.push_str("        <w:vMerge/>\n");
                    }
                }
            } else {
                if cell.col_span > 1 {
                    out.push_str(&format!(
                        "        <w:gridSpan w:val=\"{}\"/>\n",
                        cell.col_span
                    ));
                }
                if cell.row_span > 1 {
                    out.push_str("        <w:vMerge w:val=\"restart\"/>\n");
                } else if cell.is_covered {
                    out.push_str("        <w:vMerge/>\n");
                }
            }

            if let Some(bg) = &resolved.background_color {
                out.push_str(&format!(
                    "        <w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{}\"/>\n",
                    clean_hex_color(bg)
                ));
            }

            out.push_str("      </w:tcPr>\n");

            // Cell paragraph and runs
            out.push_str("      <w:p>\n");
            if resolved.horizontal_align != HorizontalAlign::Left {
                let jc = match resolved.horizontal_align {
                    HorizontalAlign::Center => "center",
                    HorizontalAlign::Right => "right",
                    HorizontalAlign::Justify => "both",
                    _ => "left",
                };
                out.push_str(&format!(
                    "        <w:pPr><w:jc w:val=\"{}\"/></w:pPr>\n",
                    jc
                ));
            }

            if cell.content.runs.is_empty() {
                out.push_str("        <w:r><w:t></w:t></w:r>\n");
            } else {
                for run in &cell.content.runs {
                    out.push_str("        <w:r>\n");
                    let is_bold = run.bold || resolved.bold;
                    if is_bold || run.italic || run.underline || resolved.text_color.is_some() {
                        out.push_str("          <w:rPr>\n");
                        if is_bold {
                            out.push_str("            <w:b/>\n");
                        }
                        if run.italic {
                            out.push_str("            <w:i/>\n");
                        }
                        if run.underline {
                            out.push_str("            <w:u w:val=\"single\"/>\n");
                        }
                        if let Some(fg) = run.color.as_ref().or(resolved.text_color.as_ref()) {
                            out.push_str(&format!(
                                "            <w:color w:val=\"{}\"/>\n",
                                clean_hex_color(fg)
                            ));
                        }
                        out.push_str("          </w:rPr>\n");
                    }
                    let escaped_text = quick_xml_escape(&run.text);
                    out.push_str(&format!(
                        "          <w:t xml:space=\"preserve\">{}</w:t>\n",
                        escaped_text
                    ));
                    out.push_str("        </w:r>\n");
                }
            }

            out.push_str("      </w:p>\n");
            out.push_str("    </w:tc>\n");
        }
        out.push_str("  </w:tr>\n");
    }

    out.push_str("</w:tbl>\n");
    out
}

/// Serializes a `Table` to DrawingML `<a:tbl>` for PowerPoint presentations.
pub fn table_to_pptx_atbl(table: &Table) -> String {
    let mut out = String::new();
    out.push_str("<a:tbl>\n");
    out.push_str("  <a:tblPr rtl=\"0\">\n");
    out.push_str("    <a:tableStyleId>{5940675A-B579-460E-94D1-54222C63F5DA}</a:tableStyleId>\n");
    out.push_str("  </a:tblPr>\n");

    // Column grid in EMUs
    out.push_str("  <a:tblGrid>\n");
    for col in &table.columns {
        out.push_str(&format!(
            "    <a:gridCol w=\"{}\"/>\n",
            pt_to_emu(col.width_pt)
        ));
    }
    out.push_str("  </a:tblGrid>\n");

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

    // Rows
    for (r, row) in table.rows.iter().enumerate() {
        let row_height_pt = row.height_pt.unwrap_or(20.0);
        out.push_str(&format!("  <a:tr h=\"{}\">\n", pt_to_emu(row_height_pt)));

        for (c, cell) in row.cells.iter().enumerate() {
            let resolved = style.resolve_cell(
                r,
                c,
                table.row_count(),
                table.col_count(),
                &options,
                Some(cell),
            );

            let mut tc_tag = "    <a:tc".to_string();
            if cell.row_span > 1 {
                tc_tag.push_str(&format!(" rowSpan=\"{}\"", cell.row_span));
            }
            if cell.col_span > 1 {
                tc_tag.push_str(&format!(" gridSpan=\"{}\"", cell.col_span));
            }
            if cell.is_covered {
                tc_tag.push_str(" hMerge=\"1\"");
            }
            tc_tag.push_str(">\n");
            out.push_str(&tc_tag);

            // Text body
            out.push_str("      <a:txBody>\n");
            out.push_str("        <a:bodyPr/>\n");
            out.push_str("        <a:lstStyle/>\n");
            out.push_str("        <a:p>\n");

            let align_attr = match resolved.horizontal_align {
                HorizontalAlign::Center => " algn=\"ctr\"",
                HorizontalAlign::Right => " algn=\"r\"",
                HorizontalAlign::Justify => " algn=\"just\"",
                _ => " algn=\"l\"",
            };
            out.push_str(&format!("          <a:pPr{}/>\n", align_attr));

            if cell.content.runs.is_empty() {
                out.push_str("          <a:endParaRPr lang=\"en-US\"/>\n");
            } else {
                for run in &cell.content.runs {
                    out.push_str("          <a:r>\n");
                    let is_bold = run.bold || resolved.bold;
                    let mut rpr = format!(
                        "            <a:rPr lang=\"en-US\" b=\"{}\" i=\"{}\"",
                        if is_bold { 1 } else { 0 },
                        if run.italic { 1 } else { 0 }
                    );
                    if let Some(size) = run.font_size_pt {
                        rpr.push_str(&format!(" sz=\"{}\"", (size * 100.0).round() as u32));
                    }
                    rpr.push_str(">\n");
                    out.push_str(&rpr);

                    if let Some(fg) = run.color.as_ref().or(resolved.text_color.as_ref()) {
                        out.push_str(&format!(
                            "              <a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>\n",
                            clean_hex_color(fg)
                        ));
                    }
                    out.push_str("            </a:rPr>\n");
                    out.push_str(&format!(
                        "            <a:t>{}</a:t>\n",
                        quick_xml_escape(&run.text)
                    ));
                    out.push_str("          </a:r>\n");
                }
            }

            out.push_str("        </a:p>\n");
            out.push_str("      </a:txBody>\n");

            // Cell properties (fill, borders)
            out.push_str("      <a:tcPr>\n");
            if let Some(bg) = &resolved.background_color {
                out.push_str(&format!(
                    "        <a:solidFill><a:srgbClr val=\"{}\"/></a:solidFill>\n",
                    clean_hex_color(bg)
                ));
            }
            out.push_str("      </a:tcPr>\n");

            out.push_str("    </a:tc>\n");
        }
        out.push_str("  </a:tr>\n");
    }

    out.push_str("</a:tbl>\n");
    out
}

fn quick_xml_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_to_docx_wtbl_structure() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Header");
        table.set_cell_text(1, 0, "Cell 1");

        let xml = table_to_docx_wtbl(&table);
        assert!(xml.starts_with("<w:tbl>"));
        assert!(xml.contains("<w:tblGrid>"));
        assert!(xml.contains("<w:tblHeader/>"));
        assert!(xml.contains("Header"));
        assert!(xml.contains("Cell 1"));
        assert!(xml.ends_with("</w:tbl>\n"));
    }

    #[test]
    fn table_to_pptx_atbl_structure() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Slide Header");
        table.set_cell_text(1, 0, "Body Value");

        let xml = table_to_pptx_atbl(&table);
        assert!(xml.starts_with("<a:tbl>"));
        assert!(xml.contains("<a:tblGrid>"));
        assert!(xml.contains("Slide Header"));
        assert!(xml.contains("Body Value"));
        assert!(xml.ends_with("</a:tbl>\n"));
    }
}
