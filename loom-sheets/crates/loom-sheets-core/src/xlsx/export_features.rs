//! OOXML export rendering for workbook definedNames, worksheet autoFilter,
//! mergeCells, conditionalFormatting (+ dxfs), dataValidations, and tables.

use std::collections::BTreeSet;

use crate::conditional_format::{CellConditionalRule, ConditionalOperator};
use crate::sheet_tables::{SheetTable, TableStylePreset};
use crate::style::FillColor;
use crate::{CellRange, Sheet, ValidationCriteria};

use super::cell_refs::column_letters;
use super::styles::rgb_for_fill;
use super::xml::{xml_escape_attr, xml_escape_text};
use super::{MAIN_NS, REL_NS};

const TABLE_REL_TYPE: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/table";

/// Format a `CellRange` as an A1-style range (e.g. `A1:C10`).
pub(super) fn format_range_ref(range: CellRange) -> String {
    let start_col = column_letters(range.start.col as usize);
    let end_col = column_letters(range.end.col as usize);
    format!(
        "{}{}:{}{}",
        start_col,
        range.start.row + 1,
        end_col,
        range.end.row + 1
    )
}

/// Format a `CellRange` as an absolute `$A$1:$C$10` reference.
fn format_abs_range_ref(range: CellRange) -> String {
    let start_col = column_letters(range.start.col as usize);
    let end_col = column_letters(range.end.col as usize);
    format!(
        "${}${}:${}${}",
        start_col,
        range.start.row + 1,
        end_col,
        range.end.row + 1
    )
}

/// Quote a sheet name for OOXML formulas/`definedNames` when it contains
/// spaces, punctuation, or starts with a digit.
pub(super) fn quote_sheet_name(name: &str) -> String {
    let needs_quotes = name.is_empty()
        || name.chars().next().is_some_and(|ch| ch.is_ascii_digit())
        || name
            .chars()
            .any(|ch| !ch.is_ascii_alphanumeric() && ch != '_');
    if needs_quotes {
        format!("'{}'", name.replace('\'', "''"))
    } else {
        name.to_string()
    }
}

/// Render `<definedNames>...</definedNames>` for `xl/workbook.xml`.
pub(super) fn render_defined_names(sheets: &[Sheet], exported_sheet_names: &[String]) -> String {
    let mut entries = String::new();
    for (index, sheet) in sheets.iter().enumerate() {
        let sheet_ref = quote_sheet_name(&exported_sheet_names[index]);
        for (name, range) in &sheet.named_ranges {
            let formula = format!("{sheet_ref}!{}", format_abs_range_ref(*range));
            entries.push_str(&format!(
                "<definedName name=\"{}\" localSheetId=\"{index}\">{}</definedName>",
                xml_escape_attr(name),
                xml_escape_text(&formula)
            ));
        }
    }
    if entries.is_empty() {
        String::new()
    } else {
        format!("<definedNames>{entries}</definedNames>")
    }
}

/// Render `<autoFilter .../>` for `xl/worksheets/sheetN.xml`.
pub(super) fn render_auto_filter(sheet: &Sheet) -> String {
    if !sheet.auto_filter.enabled {
        return String::new();
    }
    let (min_col, max_col, max_row) = sheet
        .used_range()
        .map(|(_, _, mc, mr)| (0u32, mc, mr.max(sheet.auto_filter.header_row)))
        .unwrap_or((0, 0, sheet.auto_filter.header_row));
    let range = CellRange {
        start: crate::CellRef {
            row: sheet.auto_filter.header_row,
            col: min_col,
        },
        end: crate::CellRef {
            row: max_row,
            col: max_col,
        },
    };
    let ref_str = format_range_ref(range);
    if sheet.auto_filter.column_filters.is_empty() {
        return format!("<autoFilter ref=\"{ref_str}\"/>");
    }
    let mut cols = String::new();
    for (&col, values) in &sheet.auto_filter.column_filters {
        let col_id = col.saturating_sub(min_col);
        let filters = values
            .iter()
            .map(|val| format!("<filter val=\"{}\"/>", xml_escape_attr(val)))
            .collect::<String>();
        cols.push_str(&format!(
            "<filterColumn colId=\"{col_id}\"><filters>{filters}</filters></filterColumn>"
        ));
    }
    format!("<autoFilter ref=\"{ref_str}\">{cols}</autoFilter>")
}

/// Render `<mergeCells ...>` for `xl/worksheets/sheetN.xml`.
pub(super) fn render_merge_cells(sheet: &Sheet) -> String {
    if sheet.merged_ranges.is_empty() {
        return String::new();
    }
    let items = sheet
        .merged_ranges
        .iter()
        .map(|range| format!("<mergeCell ref=\"{}\"/>", format_range_ref(*range)))
        .collect::<String>();
    format!(
        "<mergeCells count=\"{}\">{items}</mergeCells>",
        sheet.merged_ranges.len()
    )
}

fn format_cf_operand(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.parse::<f64>().is_ok_and(|v| v.is_finite()) {
        trimmed.to_string()
    } else {
        format!("\"{}\"", raw.replace('"', "\"\""))
    }
}

/// Render `<conditionalFormatting ...>` blocks for one sheet and return the
/// corresponding `<dxf>` XML elements to append to `xl/styles.xml`.
pub(super) fn render_conditional_formatting(
    sheet: &Sheet,
    dxf_offset: usize,
) -> (String, Vec<String>) {
    if sheet.conditional_rules.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut xml = String::new();
    let mut dxfs = Vec::new();
    let total = sheet.conditional_rules.len();
    for (idx, rule) in sheet.conditional_rules.iter().enumerate() {
        let dxf_id = dxf_offset + idx;
        dxfs.push(render_dxf(rule));
        let sqref = format_range_ref(rule.range);
        let priority = total - idx;
        let top_left = format!(
            "{}{}",
            column_letters(rule.range.start.col as usize),
            rule.range.start.row + 1
        );
        let cf_rule = match rule.operator {
            ConditionalOperator::GreaterThan => format!(
                "<cfRule type=\"cellIs\" dxfId=\"{dxf_id}\" priority=\"{priority}\" operator=\"greaterThan\"><formula>{}</formula></cfRule>",
                xml_escape_text(&format_cf_operand(&rule.value))
            ),
            ConditionalOperator::LessThan => format!(
                "<cfRule type=\"cellIs\" dxfId=\"{dxf_id}\" priority=\"{priority}\" operator=\"lessThan\"><formula>{}</formula></cfRule>",
                xml_escape_text(&format_cf_operand(&rule.value))
            ),
            ConditionalOperator::EqualTo => format!(
                "<cfRule type=\"cellIs\" dxfId=\"{dxf_id}\" priority=\"{priority}\" operator=\"equal\"><formula>{}</formula></cfRule>",
                xml_escape_text(&format_cf_operand(&rule.value))
            ),
            ConditionalOperator::Contains => {
                let escaped = rule.value.replace('"', "\"\"");
                let formula = format!("NOT(ISERROR(SEARCH(\"{escaped}\",{top_left})))");
                format!(
                    "<cfRule type=\"containsText\" dxfId=\"{dxf_id}\" priority=\"{priority}\" operator=\"containsText\" text=\"{}\"><formula>{}</formula></cfRule>",
                    xml_escape_attr(&rule.value),
                    xml_escape_text(&formula)
                )
            }
        };
        xml.push_str(&format!(
            "<conditionalFormatting sqref=\"{sqref}\">{cf_rule}</conditionalFormatting>"
        ));
    }
    (xml, dxfs)
}

fn render_dxf(rule: &CellConditionalRule) -> String {
    let font = match rule.text_color {
        Some(rgb) => format!("<font><color rgb=\"FF{:06X}\"/></font>", rgb & 0x00FF_FFFF),
        None => String::new(),
    };
    let fill = if rule.fill == FillColor::None {
        String::new()
    } else {
        format!(
            "<fill><patternFill patternType=\"solid\"><bgColor rgb=\"FF{}\"/></patternFill></fill>",
            rgb_for_fill(rule.fill)
        )
    };
    format!("<dxf>{font}{fill}</dxf>")
}

/// Render `<dataValidations ...>` for `xl/worksheets/sheetN.xml`.
pub(super) fn render_data_validations(sheet: &Sheet) -> Result<String, String> {
    if sheet.validations.is_empty() {
        return Ok(String::new());
    }
    let mut items = String::new();
    for validation in &sheet.validations {
        let sqref = format_range_ref(validation.range);
        let allow_blank = if validation.rule.allow_blank {
            "1"
        } else {
            "0"
        };
        let error_attr = if validation.rule.error_message.is_empty() {
            String::new()
        } else {
            format!(
                " showErrorMessage=\"1\" error=\"{}\"",
                xml_escape_attr(&validation.rule.error_message)
            )
        };
        let (attrs, formulas) = match &validation.rule.criteria {
            ValidationCriteria::List(items_list) => {
                if items_list
                    .iter()
                    .any(|item| item.contains(',') || item.contains('"'))
                {
                    return Err(
                        "xlsx export cannot represent inline list validation items containing commas or quotes without a range".to_string(),
                    );
                }
                let joined = items_list.join(",");
                (
                    "type=\"list\"".to_string(),
                    format!("<formula1>\"{}\"</formula1>", xml_escape_text(&joined)),
                )
            }
            ValidationCriteria::RangeList(range) => (
                "type=\"list\"".to_string(),
                format!("<formula1>{}</formula1>", format_abs_range_ref(*range)),
            ),
            ValidationCriteria::WholeNumberBetween(min, max) => (
                "type=\"whole\" operator=\"between\"".to_string(),
                format!("<formula1>{min}</formula1><formula2>{max}</formula2>"),
            ),
            ValidationCriteria::DecimalBetween(min, max) => (
                "type=\"decimal\" operator=\"between\"".to_string(),
                format!("<formula1>{min}</formula1><formula2>{max}</formula2>"),
            ),
            ValidationCriteria::TextLengthBetween(min, max) => (
                "type=\"textLength\" operator=\"between\"".to_string(),
                format!("<formula1>{min}</formula1><formula2>{max}</formula2>"),
            ),
        };
        items.push_str(&format!(
            "<dataValidation {attrs} allowBlank=\"{allow_blank}\"{error_attr} sqref=\"{sqref}\">{formulas}</dataValidation>"
        ));
    }
    Ok(format!(
        "<dataValidations count=\"{}\">{items}</dataValidations>",
        sheet.validations.len()
    ))
}

fn allocate_unique_table_name(raw: &str, used_names: &mut BTreeSet<String>) -> String {
    let mut base: String = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if base.is_empty() || base.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        base.insert_str(0, "Table_");
    }
    let mut candidate = base.clone();
    let mut suffix = 2usize;
    while !used_names.insert(candidate.to_ascii_lowercase()) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    candidate
}

pub(super) type TablePartsResult = (String, Vec<(String, String)>, Vec<String>);

/// Render `<tableParts ...>`, `xl/tables/tableN.xml` files, and worksheet
/// relationship entries for `sheet.tables`.
pub(super) fn render_table_parts(
    sheet: &Sheet,
    start_rid: usize,
    table_start_id: usize,
    used_names: &mut BTreeSet<String>,
) -> Result<TablePartsResult, String> {
    if sheet.tables.is_empty() {
        return Ok((String::new(), Vec::new(), Vec::new()));
    }
    let mut parts_xml = String::new();
    let mut table_files = Vec::new();
    let mut rels = Vec::new();
    for (idx, table) in sheet.tables.iter().enumerate() {
        let rid = format!("rId{}", start_rid + idx);
        let table_id = table_start_id + idx;
        let table_name = allocate_unique_table_name(&table.name, used_names);
        parts_xml.push_str(&format!("<tablePart r:id=\"{rid}\"/>"));
        rels.push(format!(
            "<Relationship Id=\"{rid}\" Type=\"{TABLE_REL_TYPE}\" Target=\"../tables/table{table_id}.xml\"/>"
        ));
        table_files.push((
            format!("xl/tables/table{table_id}.xml"),
            render_table_xml(sheet, table, table_id, &table_name)?,
        ));
    }
    Ok((
        format!(
            "<tableParts count=\"{}\" xmlns:r=\"{REL_NS}\">{parts_xml}</tableParts>",
            sheet.tables.len()
        ),
        table_files,
        rels,
    ))
}

fn render_table_xml(
    sheet: &Sheet,
    table: &SheetTable,
    table_id: usize,
    table_name: &str,
) -> Result<String, String> {
    let end_row = table.start_row + table.rows.saturating_sub(1);
    let end_col = table.start_col + table.cols.saturating_sub(1);
    let ref_str = format!(
        "{}{}:{}{}",
        column_letters(table.start_col as usize),
        table.start_row + 1,
        column_letters(end_col as usize),
        end_row + 1
    );
    let header_row_count = if table.header_rows == 0 { 0 } else { 1 };
    let mut seen_captions: BTreeSet<String> = BTreeSet::new();
    let mut explicit_captions: Vec<Option<String>> = Vec::with_capacity(table.cols as usize);
    for c in 0..table.cols {
        let header_cell = crate::CellRef {
            row: table.start_row,
            col: table.start_col + c,
        };
        let explicit = sheet
            .raw(header_cell)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string());
        if let Some(ref cap) = explicit {
            if !seen_captions.insert(cap.to_ascii_lowercase()) {
                return Err(format!(
                    "xlsx export duplicate table column caption '{cap}' in table '{}'",
                    table.name
                ));
            }
        }
        explicit_captions.push(explicit);
    }
    let mut cols_xml = String::new();
    for (c, explicit) in explicit_captions.into_iter().enumerate() {
        let col_name = match explicit {
            Some(cap) => cap,
            None => {
                let mut idx = c + 1;
                loop {
                    let candidate = format!("Column{idx}");
                    if seen_captions.insert(candidate.to_ascii_lowercase()) {
                        break candidate;
                    }
                    idx += 1;
                }
            }
        };
        cols_xml.push_str(&format!(
            "<tableColumn id=\"{}\" name=\"{}\"/>",
            c + 1,
            xml_escape_attr(&col_name)
        ));
    }
    let style_name = match table.style {
        TableStylePreset::Classic => "TableStyleMedium2",
        TableStylePreset::Jade => "TableStyleMedium7",
        TableStylePreset::Ocean => "TableStyleMedium6",
        TableStylePreset::Coral => "TableStyleMedium4",
        TableStylePreset::Charcoal => "TableStyleMedium1",
    };
    let row_stripes = if table.alternating_rows { "1" } else { "0" };
    let safe_name = xml_escape_attr(table_name);
    Ok(format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><table xmlns=\"{MAIN_NS}\" id=\"{table_id}\" name=\"{safe_name}\" displayName=\"{safe_name}\" ref=\"{ref_str}\" headerRowCount=\"{header_row_count}\"><tableColumns count=\"{}\">{cols_xml}</tableColumns><tableStyleInfo name=\"{style_name}\" showFirstColumn=\"0\" showLastColumn=\"0\" showRowStripes=\"{row_stripes}\" showColumnStripes=\"0\"/></table>",
        table.cols
    ))
}
