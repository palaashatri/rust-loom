//! Interactive Pivot Table Field Builder core engine.
//!
//! Supports header detection, unique value extraction, optional filtering,
//! 1D and 2D cross-tabulation with `SUM`, `AVERAGE`, `COUNT`, `MIN`, `MAX`,
//! grand totals, live cross-sheet/same-sheet formula generation, and target
//! `Sheet` construction.

use std::collections::{HashMap, HashSet};

use crate::refs::quote_sheet_name;
use crate::{evaluate, CellRange, CellRef, CellStyle, PivotAggregation, Sheet, Value};

/// Configuration for building an interactive pivot table from a source range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PivotFieldConfig {
    /// Inclusive source data range (first row is treated as headers).
    pub source_range: CellRange,
    /// Column offsets (0-based relative to `source_range.start.col`) used as row groupings.
    pub row_fields: Vec<usize>,
    /// Optional column offset (relative to `source_range.start.col`) for 2D cross-tabulation.
    pub col_field: Option<usize>,
    /// Value column offsets and their aggregation functions.
    pub value_fields: Vec<(usize, PivotAggregation)>,
    /// Optional `(field_offset, exact_value)` filter applied before grouping.
    pub filter_field: Option<(usize, String)>,
    /// Whether to place the pivot table on a newly created worksheet.
    pub destination_new_sheet: bool,
    /// Top-left anchor cell when placing on an existing sheet (defaults to `A1`).
    pub destination_cell: Option<CellRef>,
    /// Title / suggested sheet name for the pivot table.
    pub title: String,
}

impl Default for PivotFieldConfig {
    fn default() -> Self {
        Self {
            source_range: CellRange::new(CellRef { row: 0, col: 0 }, CellRef { row: 0, col: 0 }),
            row_fields: Vec::new(),
            col_field: None,
            value_fields: Vec::new(),
            filter_field: None,
            destination_new_sheet: true,
            destination_cell: None,
            title: "Pivot Table".to_string(),
        }
    }
}

/// A single cell in a computed pivot table.
#[derive(Debug, Clone, PartialEq)]
pub struct PivotCell {
    /// Formatted display text for preview and labels.
    pub display: String,
    /// Live spreadsheet formula when applicable (e.g. `=SUMIFS(...)`).
    pub formula: Option<String>,
    /// Computed numeric value when applicable.
    pub numeric: Option<f64>,
    /// Whether this cell belongs to the header row.
    pub is_header: bool,
    /// Whether this cell belongs to a grand total row or column.
    pub is_total: bool,
}

impl PivotCell {
    fn label(text: impl Into<String>, is_header: bool, is_total: bool) -> Self {
        Self {
            display: text.into(),
            formula: None,
            numeric: None,
            is_header,
            is_total,
        }
    }

    fn value(num: Option<f64>, formula: Option<String>, is_total: bool) -> Self {
        let display = num.map(format_pivot_number).unwrap_or_default();
        Self {
            display,
            formula,
            numeric: num,
            is_header: false,
            is_total,
        }
    }
}

/// Computed 2D grid of a pivot table, including headers and grand totals.
#[derive(Debug, Clone, PartialEq)]
pub struct PivotTableResult {
    /// Column header labels.
    pub headers: Vec<String>,
    /// All rows including the header row at index 0 and the grand total row at the end.
    pub rows: Vec<Vec<PivotCell>>,
    /// Total number of rows in `rows`.
    pub row_count: usize,
    /// Total number of columns in `headers`.
    pub col_count: usize,
}

/// Formats a numeric value cleanly for pivot preview and display.
pub fn format_pivot_number(value: f64) -> String {
    if !value.is_finite() {
        return String::new();
    }
    if (value - value.round()).abs() < 1e-9 && value.abs() < 1e15 {
        format!("{}", value.round() as i64)
    } else {
        let s = format!("{value:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn col_letter(col: u32) -> String {
    CellRef { row: 0, col }
        .to_a1()
        .trim_end_matches('1')
        .to_string()
}

fn display_cell(sheet: &Sheet, vals: &HashMap<CellRef, Value>, row: u32, col: u32) -> String {
    let cell = CellRef { row, col };
    if let Some(val) = vals.get(&cell) {
        match val {
            Value::Empty => String::new(),
            Value::Number(n) => format_pivot_number(*n),
            Value::Text(s) => s.clone(),
            Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
            Value::Error(e) => format!("{e:?}"),
            Value::Array(_, _, _) => sheet.raw(cell).unwrap_or_default().to_string(),
        }
    } else {
        sheet.raw(cell).unwrap_or_default().to_string()
    }
}

fn parse_numeric(sheet: &Sheet, vals: &HashMap<CellRef, Value>, row: u32, col: u32) -> Option<f64> {
    let cell = CellRef { row, col };
    if let Some(Value::Number(n)) = vals.get(&cell) {
        return n.is_finite().then_some(*n);
    }
    let raw = display_cell(sheet, vals, row, col);
    let clean = raw
        .trim()
        .trim_start_matches('$')
        .trim_end_matches('%')
        .replace(',', "");
    clean.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// Detects column header labels from the first row of `range`.
///
/// Blank header cells fall back to `"Column <Letter>"`.
pub fn detect_headers(sheet: &Sheet, range: CellRange) -> Vec<String> {
    let vals = evaluate(sheet);
    let mut headers = Vec::new();
    for col in range.start.col..=range.end.col {
        let text = display_cell(sheet, &vals, range.start.row, col);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            headers.push(format!("Column {}", col_letter(col)));
        } else {
            headers.push(trimmed.to_string());
        }
    }
    headers
}

/// Extracts distinct non-empty display values for `field_idx` (0-based offset
/// from `range.start.col`) across all data rows below the header row.
pub fn extract_unique_values(sheet: &Sheet, range: CellRange, field_idx: usize) -> Vec<String> {
    let col = range.start.col + field_idx as u32;
    if col > range.end.col || range.end.row <= range.start.row {
        return Vec::new();
    }
    let vals = evaluate(sheet);
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for row in (range.start.row + 1)..=range.end.row {
        let val = display_cell(sheet, &vals, row, col).trim().to_string();
        if !val.is_empty() && seen.insert(val.clone()) {
            out.push(val);
        }
    }
    out
}

fn agg_name(agg: PivotAggregation) -> &'static str {
    match agg {
        PivotAggregation::Sum => "Sum",
        PivotAggregation::Count => "Count",
        PivotAggregation::Average => "Average",
        PivotAggregation::Min => "Min",
        PivotAggregation::Max => "Max",
    }
}

fn aggregate_slice(values: &[f64], count_non_empty: usize, agg: PivotAggregation) -> Option<f64> {
    match agg {
        PivotAggregation::Count => Some(count_non_empty as f64),
        PivotAggregation::Sum => (!values.is_empty()).then(|| values.iter().sum()),
        PivotAggregation::Average => {
            (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
        }
        PivotAggregation::Min => values.iter().copied().reduce(f64::min),
        PivotAggregation::Max => values.iter().copied().reduce(f64::max),
    }
}

fn abs_col_range(qualifier: &str, col: u32, first_row_1: u32, last_row_1: u32) -> String {
    let letter = col_letter(col);
    if qualifier.is_empty() {
        format!("${letter}${first_row_1}:${letter}${last_row_1}")
    } else {
        format!("{qualifier}!${letter}${first_row_1}:${letter}${last_row_1}")
    }
}

fn escape_criteria(val: &str) -> String {
    val.replace('"', "\"\"")
}

fn build_ifs_formula(
    agg: PivotAggregation,
    val_range: &str,
    criteria: &[(String, String)],
) -> String {
    let mut args = Vec::new();
    if agg != PivotAggregation::Count {
        args.push(val_range.to_string());
    }
    for (crit_range, crit_val) in criteria {
        args.push(crit_range.clone());
        args.push(format!("\"{}\"", escape_criteria(crit_val)));
    }
    if agg == PivotAggregation::Count && args.is_empty() {
        return format!("=COUNTA({val_range})");
    }
    let fn_name = match agg {
        PivotAggregation::Sum => "SUMIFS",
        PivotAggregation::Count => "COUNTIFS",
        PivotAggregation::Average => "AVERAGEIFS",
        PivotAggregation::Min => "MINIFS",
        PivotAggregation::Max => "MAXIFS",
    };
    format!("={fn_name}({})", args.join(","))
}

fn validate_config(config: &PivotFieldConfig, num_cols: usize) -> Result<(), String> {
    if config.source_range.end.row <= config.source_range.start.row {
        return Err("Pivot source range needs a header row and at least one data row".to_string());
    }
    if config.row_fields.is_empty() {
        return Err("Select at least one Row field".to_string());
    }
    if config.value_fields.is_empty() {
        return Err("Select at least one Value field".to_string());
    }
    for &rf in &config.row_fields {
        if rf >= num_cols {
            return Err(format!("Row field index {rf} is out of range"));
        }
    }
    if let Some(cf) = config.col_field {
        if cf >= num_cols {
            return Err(format!("Column field index {cf} is out of range"));
        }
    }
    for &(vf, _) in &config.value_fields {
        if vf >= num_cols {
            return Err(format!("Value field index {vf} is out of range"));
        }
    }
    if let Some((ff, _)) = config.filter_field {
        if ff >= num_cols {
            return Err(format!("Filter field index {ff} is out of range"));
        }
    }
    Ok(())
}

/// Computes a full 1D or 2D pivot table with grand totals and live formulas.
pub fn compute_pivot_table(
    sheet: &Sheet,
    config: &PivotFieldConfig,
) -> Result<PivotTableResult, String> {
    compute_pivot_table_with_qualifier(sheet, config, true, CellRef { row: 0, col: 0 })
}

fn compute_pivot_table_with_qualifier(
    sheet: &Sheet,
    config: &PivotFieldConfig,
    qualify_source: bool,
    anchor: CellRef,
) -> Result<PivotTableResult, String> {
    let range = config.source_range;
    let num_cols = (range.end.col - range.start.col + 1) as usize;
    validate_config(config, num_cols)?;

    let headers = detect_headers(sheet, range);
    let vals = evaluate(sheet);
    let qualifier = if qualify_source {
        quote_sheet_name(&sheet.name)
    } else {
        String::new()
    };
    let first_data_1 = range.start.row + 2;
    let last_data_1 = range.end.row + 1;

    // Collect matching data rows after applying optional filter.
    let mut matching_rows: Vec<u32> = Vec::new();
    for row in (range.start.row + 1)..=range.end.row {
        if let Some((f_idx, ref filter_val)) = config.filter_field {
            let trimmed = filter_val.trim();
            if !trimmed.is_empty() && trimmed != "(All)" {
                let cell_val = display_cell(sheet, &vals, row, range.start.col + f_idx as u32);
                if cell_val.trim() != trimmed {
                    continue;
                }
            }
        }
        matching_rows.push(row);
    }

    // Extract unique row keys and optional column keys in first-appearance order.
    let mut row_keys: Vec<Vec<String>> = Vec::new();
    let mut row_key_set: HashSet<Vec<String>> = HashSet::new();
    let mut col_keys: Vec<String> = Vec::new();
    let mut col_key_set: HashSet<String> = HashSet::new();

    for &row in &matching_rows {
        let rk: Vec<String> = config
            .row_fields
            .iter()
            .map(|&rf| {
                display_cell(sheet, &vals, row, range.start.col + rf as u32)
                    .trim()
                    .to_string()
            })
            .collect();
        if row_key_set.insert(rk.clone()) {
            row_keys.push(rk);
        }
        if let Some(cf) = config.col_field {
            let ck = display_cell(sheet, &vals, row, range.start.col + cf as u32)
                .trim()
                .to_string();
            if col_key_set.insert(ck.clone()) {
                col_keys.push(ck);
            }
        }
    }

    // Build column headers.
    let mut out_headers: Vec<String> = config
        .row_fields
        .iter()
        .map(|&rf| headers[rf].clone())
        .collect();

    if config.col_field.is_some() {
        for ck in &col_keys {
            for &(vf, agg) in &config.value_fields {
                if config.value_fields.len() == 1 {
                    out_headers.push(ck.clone());
                } else {
                    out_headers.push(format!("{ck} — {} of {}", agg_name(agg), headers[vf]));
                }
            }
        }
        for &(vf, agg) in &config.value_fields {
            if config.value_fields.len() == 1 {
                out_headers.push("Grand Total".to_string());
            } else {
                out_headers.push(format!(
                    "Grand Total — {} of {}",
                    agg_name(agg),
                    headers[vf]
                ));
            }
        }
    } else {
        for &(vf, agg) in &config.value_fields {
            out_headers.push(format!("{} of {}", agg_name(agg), headers[vf]));
        }
    }

    let mut rows: Vec<Vec<PivotCell>> = Vec::new();
    rows.push(
        out_headers
            .iter()
            .map(|h| PivotCell::label(h.clone(), true, false))
            .collect(),
    );

    let filter_crit: Option<(String, String)> =
        config
            .filter_field
            .as_ref()
            .and_then(|(f_idx, filter_val)| {
                let trimmed = filter_val.trim();
                if trimmed.is_empty() || trimmed == "(All)" {
                    None
                } else {
                    Some((
                        abs_col_range(
                            &qualifier,
                            range.start.col + *f_idx as u32,
                            first_data_1,
                            last_data_1,
                        ),
                        trimmed.to_string(),
                    ))
                }
            });

    // Helper to collect numbers & count for a bucket of matching rows.
    let bucket_stats = |rows_in_bucket: &[u32], val_col_idx: usize| -> (Vec<f64>, usize) {
        let abs_col = range.start.col + val_col_idx as u32;
        let mut nums = Vec::new();
        let mut non_empty = 0usize;
        for &r in rows_in_bucket {
            if !display_cell(sheet, &vals, r, abs_col).trim().is_empty() {
                non_empty += 1;
            }
            if let Some(n) = parse_numeric(sheet, &vals, r, abs_col) {
                nums.push(n);
            }
        }
        (nums, non_empty)
    };

    // Data rows.
    for (r_idx, rk) in row_keys.iter().enumerate() {
        let mut row_cells: Vec<PivotCell> = rk
            .iter()
            .map(|part| PivotCell::label(part.clone(), false, false))
            .collect();

        let row_matches: Vec<u32> = matching_rows
            .iter()
            .copied()
            .filter(|&r| {
                config.row_fields.iter().enumerate().all(|(i, &rf)| {
                    display_cell(sheet, &vals, r, range.start.col + rf as u32).trim() == rk[i]
                })
            })
            .collect();

        let mut base_criteria: Vec<(String, String)> = config
            .row_fields
            .iter()
            .enumerate()
            .map(|(i, &rf)| {
                (
                    abs_col_range(
                        &qualifier,
                        range.start.col + rf as u32,
                        first_data_1,
                        last_data_1,
                    ),
                    rk[i].clone(),
                )
            })
            .collect();
        if let Some(ref fc) = filter_crit {
            base_criteria.push(fc.clone());
        }

        if let Some(cf) = config.col_field {
            let cf_range = abs_col_range(
                &qualifier,
                range.start.col + cf as u32,
                first_data_1,
                last_data_1,
            );
            for ck in &col_keys {
                let cell_rows: Vec<u32> = row_matches
                    .iter()
                    .copied()
                    .filter(|&r| {
                        display_cell(sheet, &vals, r, range.start.col + cf as u32).trim() == ck
                    })
                    .collect();
                let mut cell_crit = base_criteria.clone();
                cell_crit.push((cf_range.clone(), ck.clone()));
                for &(vf, agg) in &config.value_fields {
                    let (nums, non_empty) = bucket_stats(&cell_rows, vf);
                    let agg_val = aggregate_slice(&nums, non_empty, agg);
                    let formula = if agg_val.is_some() {
                        let val_range = abs_col_range(
                            &qualifier,
                            range.start.col + vf as u32,
                            first_data_1,
                            last_data_1,
                        );
                        Some(build_ifs_formula(agg, &val_range, &cell_crit))
                    } else {
                        None
                    };
                    row_cells.push(PivotCell::value(agg_val, formula, false));
                }
            }
            // Row Grand Total column(s).
            let sheet_row = anchor.row + 1 + r_idx as u32;
            let num_val_fields = config.value_fields.len();
            for (v_idx, &(vf, agg)) in config.value_fields.iter().enumerate() {
                let (nums, non_empty) = bucket_stats(&row_matches, vf);
                let agg_val = aggregate_slice(&nums, non_empty, agg);
                let formula = if agg_val.is_some() && !col_keys.is_empty() {
                    if num_val_fields == 1 {
                        let start_c = CellRef {
                            row: sheet_row,
                            col: anchor.col + config.row_fields.len() as u32,
                        }
                        .to_a1();
                        let end_c = CellRef {
                            row: sheet_row,
                            col: anchor.col
                                + config.row_fields.len() as u32
                                + col_keys.len() as u32
                                - 1,
                        }
                        .to_a1();
                        let fn_name = match agg {
                            PivotAggregation::Sum | PivotAggregation::Count => "SUM",
                            PivotAggregation::Average => "AVERAGE",
                            PivotAggregation::Min => "MIN",
                            PivotAggregation::Max => "MAX",
                        };
                        Some(format!("={fn_name}({start_c}:{end_c})"))
                    } else {
                        let val_range = abs_col_range(
                            &qualifier,
                            range.start.col + vf as u32,
                            first_data_1,
                            last_data_1,
                        );
                        let _ = v_idx;
                        Some(build_ifs_formula(agg, &val_range, &base_criteria))
                    }
                } else {
                    None
                };
                row_cells.push(PivotCell::value(agg_val, formula, true));
            }
        } else {
            for &(vf, agg) in &config.value_fields {
                let (nums, non_empty) = bucket_stats(&row_matches, vf);
                let agg_val = aggregate_slice(&nums, non_empty, agg);
                let formula = if agg_val.is_some() {
                    let val_range = abs_col_range(
                        &qualifier,
                        range.start.col + vf as u32,
                        first_data_1,
                        last_data_1,
                    );
                    Some(build_ifs_formula(agg, &val_range, &base_criteria))
                } else {
                    None
                };
                row_cells.push(PivotCell::value(agg_val, formula, false));
            }
        }

        rows.push(row_cells);
    }

    // Grand Total row at the bottom.
    let mut total_row: Vec<PivotCell> = Vec::with_capacity(out_headers.len());
    for i in 0..config.row_fields.len() {
        total_row.push(PivotCell::label(
            if i == 0 { "Grand Total" } else { "" },
            false,
            true,
        ));
    }
    let first_pivot_data_row = anchor.row + 1;
    let last_pivot_data_row = anchor.row + row_keys.len() as u32;
    let total_data_cols = out_headers.len() - config.row_fields.len();

    for offset in 0..total_data_cols {
        let col_idx = config.row_fields.len() + offset;
        let abs_pivot_col = anchor.col + col_idx as u32;
        let agg = if config.col_field.is_some() {
            config.value_fields[offset % config.value_fields.len()].1
        } else {
            config.value_fields[offset].1
        };

        // Compute exact grand total value from matching rows.
        let mut col_nums = Vec::new();
        let mut col_non_empty = 0usize;
        let is_grand_total_col =
            config.col_field.is_some() && offset >= col_keys.len() * config.value_fields.len();
        let vf = if config.col_field.is_some() {
            config.value_fields[offset % config.value_fields.len()].0
        } else {
            config.value_fields[offset].0
        };
        if is_grand_total_col || config.col_field.is_none() {
            let (nums, non_empty) = bucket_stats(&matching_rows, vf);
            col_nums = nums;
            col_non_empty = non_empty;
        } else if let Some(cf) = config.col_field {
            let ck = &col_keys[offset / config.value_fields.len()];
            let col_matches: Vec<u32> = matching_rows
                .iter()
                .copied()
                .filter(|&r| {
                    display_cell(sheet, &vals, r, range.start.col + cf as u32).trim() == ck
                })
                .collect();
            let (nums, non_empty) = bucket_stats(&col_matches, vf);
            col_nums = nums;
            col_non_empty = non_empty;
        }

        let total_val = aggregate_slice(&col_nums, col_non_empty, agg);
        let formula = if total_val.is_some() && !row_keys.is_empty() {
            let start_c = CellRef {
                row: first_pivot_data_row,
                col: abs_pivot_col,
            }
            .to_a1();
            let end_c = CellRef {
                row: last_pivot_data_row,
                col: abs_pivot_col,
            }
            .to_a1();
            let fn_name = match agg {
                PivotAggregation::Sum | PivotAggregation::Count => "SUM",
                PivotAggregation::Average => "AVERAGE",
                PivotAggregation::Min => "MIN",
                PivotAggregation::Max => "MAX",
            };
            Some(format!("={fn_name}({start_c}:{end_c})"))
        } else {
            None
        };
        total_row.push(PivotCell::value(total_val, formula, true));
    }
    rows.push(total_row);

    let row_count = rows.len();
    let col_count = out_headers.len();
    Ok(PivotTableResult {
        headers: out_headers,
        rows,
        row_count,
        col_count,
    })
}

/// Plans cell updates `(CellRef, raw_or_formula, is_bold)` for writing a pivot
/// table at `anchor`.
pub fn plan_pivot_updates(
    source: &Sheet,
    config: &PivotFieldConfig,
    anchor: CellRef,
    qualify_source: bool,
) -> Result<Vec<(CellRef, String, bool)>, String> {
    let result = compute_pivot_table_with_qualifier(source, config, qualify_source, anchor)?;
    let mut updates = Vec::with_capacity(result.row_count * result.col_count);
    for (r_idx, row) in result.rows.iter().enumerate() {
        for (c_idx, cell) in row.iter().enumerate() {
            let target = CellRef {
                row: anchor.row + r_idx as u32,
                col: anchor.col + c_idx as u32,
            };
            let raw = cell.formula.clone().unwrap_or_else(|| cell.display.clone());
            let is_bold = cell.is_header || cell.is_total;
            updates.push((target, raw, is_bold));
        }
    }
    Ok(updates)
}

/// Builds a new `Sheet` populated with the configured pivot table and live formulas.
pub fn build_pivot_sheet(source: &Sheet, config: &PivotFieldConfig) -> Result<Sheet, String> {
    let anchor = CellRef { row: 0, col: 0 };
    let updates = plan_pivot_updates(source, config, anchor, true)?;
    let sheet_name = if config.title.trim().is_empty() {
        format!("Pivot of {}", source.name)
    } else {
        config.title.trim().to_string()
    };
    let mut pivot = Sheet::new(&sheet_name);
    let bold_style = CellStyle {
        bold: true,
        ..Default::default()
    };
    for (cell, raw, is_bold) in updates {
        if !raw.is_empty() {
            pivot.set_raw(cell, &raw);
        }
        if is_bold {
            pivot.set_cell_style(cell, bold_style);
        }
    }
    Ok(pivot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sales_sheet() -> Sheet {
        let mut s = Sheet::new("Sales");
        s.set_str("A1", "Region");
        s.set_str("B1", "Quarter");
        s.set_str("C1", "Category");
        s.set_str("D1", "Revenue");

        s.set_str("A2", "North");
        s.set_str("B2", "Q1");
        s.set_str("C2", "Software");
        s.set_str("D2", "100");

        s.set_str("A3", "North");
        s.set_str("B3", "Q2");
        s.set_str("C3", "Software");
        s.set_str("D3", "150");

        s.set_str("A4", "South");
        s.set_str("B4", "Q1");
        s.set_str("C4", "Hardware");
        s.set_str("D4", "200");

        s.set_str("A5", "South");
        s.set_str("B5", "Q2");
        s.set_str("C5", "Software");
        s.set_str("D5", "50");
        s
    }

    #[test]
    fn detects_headers_and_unique_values() {
        let s = sample_sales_sheet();
        let range = CellRange::parse("A1:D5").unwrap();
        assert_eq!(
            detect_headers(&s, range),
            vec!["Region", "Quarter", "Category", "Revenue"]
        );
        assert_eq!(extract_unique_values(&s, range, 0), vec!["North", "South"]);
        assert_eq!(extract_unique_values(&s, range, 1), vec!["Q1", "Q2"]);
    }

    #[test]
    fn computes_1d_and_2d_pivots_with_filter_and_grand_totals() {
        let s = sample_sales_sheet();
        let range = CellRange::parse("A1:D5").unwrap();

        // 1D Sum
        let cfg_1d = PivotFieldConfig {
            source_range: range,
            row_fields: vec![0],
            col_field: None,
            value_fields: vec![(3, PivotAggregation::Sum)],
            filter_field: None,
            destination_new_sheet: true,
            destination_cell: None,
            title: "1D Pivot".to_string(),
        };
        let res = compute_pivot_table(&s, &cfg_1d).unwrap();
        assert_eq!(res.headers, vec!["Region", "Sum of Revenue"]);
        assert_eq!(res.rows.len(), 4); // header + North + South + Grand Total
        assert_eq!(res.rows[1][0].display, "North");
        assert_eq!(res.rows[1][1].numeric, Some(250.0));
        assert_eq!(res.rows[2][0].display, "South");
        assert_eq!(res.rows[2][1].numeric, Some(250.0));
        assert_eq!(res.rows[3][0].display, "Grand Total");
        assert_eq!(res.rows[3][1].numeric, Some(500.0));

        // 2D Average with filter Category = Software
        let cfg_2d = PivotFieldConfig {
            source_range: range,
            row_fields: vec![0],
            col_field: Some(1),
            value_fields: vec![(3, PivotAggregation::Average)],
            filter_field: Some((2, "Software".to_string())),
            destination_new_sheet: true,
            destination_cell: None,
            title: "2D Pivot".to_string(),
        };
        let res2 = compute_pivot_table(&s, &cfg_2d).unwrap();
        assert_eq!(res2.headers, vec!["Region", "Q1", "Q2", "Grand Total"]);
        // North: Q1=100, Q2=150, Grand Total=125
        assert_eq!(res2.rows[1][1].numeric, Some(100.0));
        assert_eq!(res2.rows[1][2].numeric, Some(150.0));
        assert_eq!(res2.rows[1][3].numeric, Some(125.0));
        // South: Q1=None (Hardware filtered out), Q2=50, Grand Total=50
        assert_eq!(res2.rows[2][1].numeric, None);
        assert_eq!(res2.rows[2][2].numeric, Some(50.0));
        assert_eq!(res2.rows[2][3].numeric, Some(50.0));
        // Grand Total row: Q1=100, Q2=100, Grand Total=100
        assert_eq!(res2.rows[3][1].numeric, Some(100.0));
        assert_eq!(res2.rows[3][2].numeric, Some(100.0));
        assert_eq!(res2.rows[3][3].numeric, Some(100.0));
    }

    #[test]
    fn builds_pivot_sheet_with_live_formulas() {
        let s = sample_sales_sheet();
        let cfg = PivotFieldConfig {
            source_range: CellRange::parse("A1:D5").unwrap(),
            row_fields: vec![0],
            col_field: Some(1),
            value_fields: vec![(3, PivotAggregation::Sum)],
            filter_field: None,
            destination_new_sheet: true,
            destination_cell: None,
            title: "Pivot Summary".to_string(),
        };
        let pivot = build_pivot_sheet(&s, &cfg).unwrap();
        assert_eq!(pivot.name, "Pivot Summary");
        assert!(pivot
            .raw(CellRef::parse("B2").unwrap())
            .unwrap()
            .starts_with("=SUMIFS("));
        assert_eq!(
            pivot.raw(CellRef::parse("D4").unwrap()).as_deref(),
            Some("=SUM(D2:D3)")
        );
        assert!(pivot.cell_style(CellRef::parse("A1").unwrap()).bold);
        assert!(pivot.cell_style(CellRef::parse("A4").unwrap()).bold);
    }

    #[test]
    fn supports_count_min_max_aggregations() {
        let s = sample_sales_sheet();
        let range = CellRange::parse("A1:D5").unwrap();
        for (agg, expected_north) in [
            (PivotAggregation::Count, 2.0),
            (PivotAggregation::Min, 100.0),
            (PivotAggregation::Max, 150.0),
        ] {
            let cfg = PivotFieldConfig {
                source_range: range,
                row_fields: vec![0],
                col_field: None,
                value_fields: vec![(3, agg)],
                filter_field: None,
                destination_new_sheet: true,
                destination_cell: None,
                title: "Agg Test".to_string(),
            };
            let res = compute_pivot_table(&s, &cfg).unwrap();
            assert_eq!(res.rows[1][1].numeric, Some(expected_north));
        }
    }
}
