//! OOXML import parsing for workbook definedNames and worksheet features
//! (mergeCells, autoFilter, conditionalFormatting + dxfs, dataValidations, tables).

use std::collections::{BTreeMap, BTreeSet};

use loom_package::zip::PackageArchive;

use crate::conditional_format::{CellConditionalRule, ConditionalOperator};
use crate::data_tools::{AutoFilterState, SheetValidation};
use crate::sheet_tables::{SheetTable, TableStylePreset};
use crate::style::FillColor;
use crate::{CellRange, DataValidationRule, Sheet, ValidationCriteria};

use super::cell_refs::parse_cell_ref;
use super::package_parts::{parent_path, relationship_map, relationship_part_path, resolve_target};
use super::styles::fill_from_rgb;
use super::xml::{attr, elements, xml_unescape};

/// Parse `A1:B2` or `A1` into a `CellRange`.
pub(super) fn parse_range_ref(raw: &str) -> Option<CellRange> {
    let trimmed = raw.trim().replace('$', "");
    if let Some((start, end)) = trimmed.split_once(':') {
        let a = parse_cell_ref(start)?;
        let b = parse_cell_ref(end)?;
        Some(CellRange::new(a, b))
    } else {
        let a = parse_cell_ref(&trimmed)?;
        Some(CellRange::new(a, a))
    }
}

/// Parse `<dxfs>` from `xl/styles.xml` into `(FillColor, Option<u32>)` pairs.
pub(super) fn parse_dxf_styles(styles_xml: &str) -> Vec<(FillColor, Option<u32>)> {
    let Some(dxfs_body) = elements(styles_xml, "dxfs").next().map(|t| t.body) else {
        return Vec::new();
    };
    elements(dxfs_body, "dxf")
        .map(|dxf| {
            let fill = elements(dxf.body, "bgColor")
                .chain(elements(dxf.body, "fgColor"))
                .find_map(|c| attr(c.attrs, "rgb"))
                .map(|rgb| fill_from_rgb(&rgb))
                .unwrap_or(FillColor::None);
            let text_color = elements(dxf.body, "font")
                .next()
                .and_then(|f| elements(f.body, "color").next())
                .and_then(|c| attr(c.attrs, "rgb"))
                .and_then(|rgb| {
                    let hex = rgb.trim().trim_start_matches('#');
                    let hex = if hex.len() == 8 { &hex[2..] } else { hex };
                    u32::from_str_radix(hex, 16).ok()
                });
            (fill, text_color)
        })
        .collect()
}

/// Populate `sheet.named_ranges` from `<definedNames>` in `xl/workbook.xml`.
pub(super) fn parse_defined_names_into_sheets(
    workbook_xml: &str,
    sheets: &mut [Sheet],
    sheet_names: &[String],
) {
    for tag in elements(workbook_xml, "definedName") {
        let Some(name) = attr(tag.attrs, "name").map(|s| xml_unescape(&s)) else {
            continue;
        };
        if name.starts_with("_xlnm.") {
            continue;
        }
        let refers_to = xml_unescape(tag.body.trim());
        let Some((sheet_part, range_part)) = refers_to.split_once('!') else {
            continue;
        };
        let Some(range) = parse_range_ref(range_part) else {
            continue;
        };
        let target_idx = if let Some(local_id) =
            attr(tag.attrs, "localSheetId").and_then(|s| s.parse::<usize>().ok())
        {
            Some(local_id)
        } else {
            let unquoted = sheet_part
                .trim()
                .trim_start_matches('\'')
                .trim_end_matches('\'')
                .replace("''", "'");
            sheet_names.iter().position(|n| n == &unquoted)
        };
        if let Some(idx) = target_idx {
            if let Some(sheet) = sheets.get_mut(idx) {
                sheet.named_ranges.insert(name.to_ascii_uppercase(), range);
            }
        }
    }
}

fn decode_cf_formula_operand(raw: &str) -> String {
    let unescaped = xml_unescape(raw.trim());
    if unescaped.len() >= 2 && unescaped.starts_with('"') && unescaped.ends_with('"') {
        unescaped[1..unescaped.len() - 1].replace("\"\"", "\"")
    } else {
        unescaped
    }
}

fn parse_sqref_ranges(sqref: &str) -> Vec<CellRange> {
    sqref
        .split_ascii_whitespace()
        .filter_map(parse_range_ref)
        .collect()
}
pub(super) fn parse_worksheet_features(
    sheet_xml: &str,
    dxfs: &[(FillColor, Option<u32>)],
    sheet: &mut Sheet,
) {
    // 1. merged_ranges
    for tag in elements(sheet_xml, "mergeCell") {
        if let Some(ref_str) = attr(tag.attrs, "ref") {
            if let Some(range) = parse_range_ref(&ref_str) {
                if !sheet.merged_ranges.contains(&range) {
                    sheet.merged_ranges.push(range);
                }
            }
        }
    }

    // 2. auto_filter
    if let Some(af_tag) = elements(sheet_xml, "autoFilter").next() {
        let header_row = attr(af_tag.attrs, "ref")
            .and_then(|r| parse_range_ref(&r))
            .map(|r| r.start.row)
            .unwrap_or(0);
        let min_col = attr(af_tag.attrs, "ref")
            .and_then(|r| parse_range_ref(&r))
            .map(|r| r.start.col)
            .unwrap_or(0);
        let mut column_filters = BTreeMap::new();
        for fc in elements(af_tag.body, "filterColumn") {
            if let Some(col_id) = attr(fc.attrs, "colId").and_then(|s| s.parse::<u32>().ok()) {
                let vals: BTreeSet<String> = elements(fc.body, "filter")
                    .filter_map(|f| attr(f.attrs, "val").map(|v| xml_unescape(&v)))
                    .collect();
                if !vals.is_empty() {
                    column_filters.insert(min_col + col_id, vals);
                }
            }
        }
        sheet.auto_filter = AutoFilterState {
            enabled: true,
            header_row,
            column_filters,
        };
    }

    // 3. conditional_rules
    const MAX_IMPORTED_CONDITIONAL_RULES: usize = 4096;
    let mut imported_cf_rules: Vec<(u32, usize, CellConditionalRule)> = Vec::new();
    let mut cf_seq = 0usize;
    'cf_outer: for cf_tag in elements(sheet_xml, "conditionalFormatting") {
        let Some(sqref) = attr(cf_tag.attrs, "sqref") else {
            continue;
        };
        let ranges = parse_sqref_ranges(&sqref);
        if ranges.is_empty() {
            continue;
        }
        for rule_tag in elements(cf_tag.body, "cfRule") {
            let rule_type = attr(rule_tag.attrs, "type").unwrap_or_default();
            let op_attr = attr(rule_tag.attrs, "operator").unwrap_or_default();
            let priority = attr(rule_tag.attrs, "priority")
                .and_then(|s| s.parse::<u32>().ok())
                .unwrap_or(u32::MAX);
            let dxf_id = attr(rule_tag.attrs, "dxfId").and_then(|s| s.parse::<usize>().ok());
            let (fill, text_color) = dxf_id
                .and_then(|id| dxfs.get(id).copied())
                .unwrap_or((FillColor::None, None));

            let (operator, value) = if rule_type == "containsText" || op_attr == "containsText" {
                let text = attr(rule_tag.attrs, "text")
                    .map(|t| xml_unescape(&t))
                    .unwrap_or_default();
                (ConditionalOperator::Contains, text)
            } else {
                let op = match op_attr.as_str() {
                    "greaterThan" => ConditionalOperator::GreaterThan,
                    "lessThan" => ConditionalOperator::LessThan,
                    "equal" => ConditionalOperator::EqualTo,
                    _ => continue,
                };
                let formula = elements(rule_tag.body, "formula")
                    .next()
                    .map(|f| decode_cf_formula_operand(f.body))
                    .unwrap_or_default();
                (op, formula)
            };
            for &range in &ranges {
                if imported_cf_rules.len() >= MAX_IMPORTED_CONDITIONAL_RULES {
                    break 'cf_outer;
                }
                imported_cf_rules.push((
                    priority,
                    cf_seq,
                    CellConditionalRule {
                        range,
                        operator,
                        value: value.clone(),
                        fill,
                        text_color,
                    },
                ));
                cf_seq += 1;
            }
        }
    }
    imported_cf_rules
        .sort_by_key(|(priority, seq, _)| (std::cmp::Reverse(*priority), std::cmp::Reverse(*seq)));
    sheet
        .conditional_rules
        .extend(imported_cf_rules.into_iter().map(|(_, _, rule)| rule));

    // 4. validations
    for dv_tag in elements(sheet_xml, "dataValidation") {
        let Some(sqref) = attr(dv_tag.attrs, "sqref") else {
            continue;
        };
        let ranges = parse_sqref_ranges(&sqref);
        if ranges.is_empty() {
            continue;
        }
        let dv_type = attr(dv_tag.attrs, "type").unwrap_or_default();
        let dv_operator = attr(dv_tag.attrs, "operator").unwrap_or_else(|| "between".to_string());
        let allow_blank = matches!(
            attr(dv_tag.attrs, "allowBlank").as_deref(),
            Some("1") | Some("true")
        );
        let error_message = attr(dv_tag.attrs, "error")
            .map(|e| xml_unescape(&e))
            .unwrap_or_default();
        let f1 = elements(dv_tag.body, "formula1")
            .next()
            .map(|f| xml_unescape(f.body.trim()))
            .unwrap_or_default();
        let f2 = elements(dv_tag.body, "formula2")
            .next()
            .map(|f| xml_unescape(f.body.trim()))
            .unwrap_or_default();

        let criteria = match dv_type.as_str() {
            "list" => {
                if f1.starts_with('"') && f1.ends_with('"') && f1.len() >= 2 {
                    let inner = &f1[1..f1.len() - 1];
                    ValidationCriteria::List(
                        inner.split(',').map(|s| s.trim().to_string()).collect(),
                    )
                } else if let Some(r) = parse_range_ref(&f1) {
                    ValidationCriteria::RangeList(r)
                } else {
                    continue;
                }
            }
            "whole" if dv_operator == "between" => {
                let (Ok(min), Ok(max)) = (f1.parse::<i64>(), f2.parse::<i64>()) else {
                    continue;
                };
                ValidationCriteria::WholeNumberBetween(min, max)
            }
            "decimal" if dv_operator == "between" => {
                let (Ok(min), Ok(max)) = (f1.parse::<f64>(), f2.parse::<f64>()) else {
                    continue;
                };
                ValidationCriteria::DecimalBetween(min, max)
            }
            "textLength" if dv_operator == "between" => {
                let (Ok(min), Ok(max)) = (f1.parse::<usize>(), f2.parse::<usize>()) else {
                    continue;
                };
                ValidationCriteria::TextLengthBetween(min, max)
            }
            _ => continue,
        };
        for range in ranges {
            sheet.validations.push(SheetValidation {
                range,
                rule: DataValidationRule {
                    criteria: criteria.clone(),
                    allow_blank,
                    error_message: error_message.clone(),
                },
            });
        }
    }
}

/// Parse `<tablePart r:id="..."/>` relationships into `sheet.tables`.
pub(super) fn parse_sheet_tables(
    archive: &PackageArchive,
    sheet_path: &str,
    sheet_xml: &str,
    sheet: &mut Sheet,
) {
    let rels_path = relationship_part_path(sheet_path);
    let Some(rels_xml) = archive
        .get(&rels_path)
        .and_then(|b| std::str::from_utf8(b).ok())
    else {
        return;
    };
    let rels = relationship_map(rels_xml);
    let base_dir = parent_path(sheet_path);

    for part_tag in elements(sheet_xml, "tablePart") {
        let Some(rid) = attr(part_tag.attrs, "r:id").or_else(|| attr(part_tag.attrs, "id")) else {
            continue;
        };
        let Some(target) = rels.get(&rid) else {
            continue;
        };
        let table_path = resolve_target(base_dir, target);
        let Some(table_xml) = archive
            .get(&table_path)
            .and_then(|b| std::str::from_utf8(b).ok())
        else {
            continue;
        };
        let Some(table_tag) = elements(table_xml, "table").next() else {
            continue;
        };
        let Some(range) = attr(table_tag.attrs, "ref").and_then(|r| parse_range_ref(&r)) else {
            continue;
        };
        let id = attr(table_tag.attrs, "id").unwrap_or_else(|| "1".to_string());
        let name = attr(table_tag.attrs, "displayName")
            .or_else(|| attr(table_tag.attrs, "name"))
            .map(|n| xml_unescape(&n))
            .unwrap_or_else(|| format!("Table{id}"));
        let header_rows = attr(table_tag.attrs, "headerRowCount")
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(1);
        let style_info = elements(table_tag.body, "tableStyleInfo").next();
        let alternating_rows = style_info
            .as_ref()
            .and_then(|s| attr(s.attrs, "showRowStripes"))
            .as_deref()
            != Some("0");
        let style = match style_info
            .as_ref()
            .and_then(|s| attr(s.attrs, "name"))
            .as_deref()
        {
            Some("TableStyleMedium1") => TableStylePreset::Charcoal,
            Some("TableStyleMedium4") => TableStylePreset::Coral,
            Some("TableStyleMedium6") => TableStylePreset::Ocean,
            Some("TableStyleMedium7") => TableStylePreset::Jade,
            _ => TableStylePreset::Classic,
        };
        sheet.tables.push(SheetTable {
            id: format!("table-{id}"),
            name,
            start_row: range.start.row,
            start_col: range.start.col,
            rows: range.end.row - range.start.row + 1,
            cols: range.end.col - range.start.col + 1,
            style,
            show_title: true,
            alternating_rows,
            header_rows,
            header_cols: 0,
        });
    }
}
