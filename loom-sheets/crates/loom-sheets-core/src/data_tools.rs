//! Data analysis tools for Loom Sheets: Find & Replace, Column AutoFilter,
//! Named Ranges, and Cell Data Validation.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::{CellRange, CellRef, DataValidationRule, Sheet, ValidationCriteria, Value};

/// Options controlling cell search and replacement.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FindOptions {
    pub query: String,
    pub match_case: bool,
    pub whole_cell: bool,
    pub workbook_scope: bool,
}

/// One matching cell found during a Find operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindMatch {
    pub sheet_index: usize,
    pub cell: CellRef,
    pub raw: String,
    pub display: String,
}

/// Non-destructive column AutoFilter state stored on a worksheet.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutoFilterState {
    pub enabled: bool,
    pub header_row: u32,
    pub column_filters: BTreeMap<u32, BTreeSet<String>>,
}

/// A data validation rule bound to a rectangular cell range.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetValidation {
    pub range: CellRange,
    pub rule: DataValidationRule,
}

fn text_matches(haystack: &str, query: &str, match_case: bool, whole_cell: bool) -> bool {
    if query.is_empty() {
        return false;
    }
    if match_case {
        if whole_cell {
            haystack == query
        } else {
            haystack.contains(query)
        }
    } else {
        let hay_lower = fold_lower(haystack);
        let query_lower = fold_lower(query);
        if whole_cell {
            hay_lower == query_lower
        } else {
            hay_lower.contains(&query_lower)
        }
    }
}

/// Case-folds `text` for case-insensitive comparison.
///
/// `str::to_lowercase` applies Unicode's contextual final-sigma rule, so it can
/// disagree with per-character `char::to_lowercase`. Find, whole-cell matching
/// and Replace must share one equivalence relation, so both sides are folded
/// per character and `Σ`/`ς` map to the same character.
fn fold_lower(text: &str) -> String {
    text.chars()
        .flat_map(char::to_lowercase)
        .map(|ch| if ch == 'ς' { 'σ' } else { ch })
        .collect()
}

/// Finds all matching cells in a single worksheet in deterministic row-major order.
pub fn find_matches_in_sheet(
    sheet: &Sheet,
    values: &HashMap<CellRef, Value>,
    sheet_index: usize,
    opts: &FindOptions,
) -> Vec<FindMatch> {
    if opts.query.is_empty() {
        return Vec::new();
    }
    let mut coords: Vec<CellRef> = sheet.cells.iter().map(|(cell, _)| *cell).collect();
    coords.sort_by_key(|c| (c.row, c.col));

    let mut matches = Vec::new();
    for cell in coords {
        let raw = sheet.raw(cell).unwrap_or("");
        if raw.is_empty() {
            continue;
        }
        let display = values
            .get(&cell)
            .filter(|v| !matches!(v, Value::Empty))
            .map(Value::display)
            .unwrap_or_else(|| raw.to_string());
        if text_matches(raw, &opts.query, opts.match_case, opts.whole_cell)
            || text_matches(&display, &opts.query, opts.match_case, opts.whole_cell)
        {
            matches.push(FindMatch {
                sheet_index,
                cell,
                raw: raw.to_string(),
                display,
            });
        }
    }
    matches
}

/// Finds all matching cells across the entire workbook (or active sheet if `workbook_scope` is false).
pub fn find_matches_in_workbook(
    sheets: &[Sheet],
    values_by_sheet: &[HashMap<CellRef, Value>],
    active_sheet: usize,
    opts: &FindOptions,
) -> Vec<FindMatch> {
    if opts.query.is_empty() || sheets.is_empty() {
        return Vec::new();
    }
    if !opts.workbook_scope {
        let sheet = &sheets[active_sheet];
        let empty = HashMap::new();
        let values = values_by_sheet.get(active_sheet).unwrap_or(&empty);
        return find_matches_in_sheet(sheet, values, active_sheet, opts);
    }
    let mut all = Vec::new();
    let empty = HashMap::new();
    for (idx, sheet) in sheets.iter().enumerate() {
        let values = values_by_sheet.get(idx).unwrap_or(&empty);
        all.extend(find_matches_in_sheet(sheet, values, idx, opts));
    }
    all
}

/// Selects the next (or previous) match relative to `(current_sheet, current_cell)` with wrap-around.
pub fn find_next_match(
    matches: &[FindMatch],
    current_sheet: usize,
    current_cell: CellRef,
    forward: bool,
) -> Option<(usize, &FindMatch)> {
    if matches.is_empty() {
        return None;
    }
    let key = (current_sheet, current_cell.row, current_cell.col);
    if forward {
        if let Some((idx, m)) = matches
            .iter()
            .enumerate()
            .find(|(_, m)| (m.sheet_index, m.cell.row, m.cell.col) > key)
        {
            return Some((idx, m));
        }
        Some((0, &matches[0]))
    } else {
        if let Some((idx, m)) = matches
            .iter()
            .enumerate()
            .rev()
            .find(|(_, m)| (m.sheet_index, m.cell.row, m.cell.col) < key)
        {
            return Some((idx, m));
        }
        let last = matches.len() - 1;
        Some((last, &matches[last]))
    }
}

/// Replaces occurrences of `query` in `raw` according to `match_case` and `whole_cell`.
/// Returns `Some(updated)` if `raw` changed, or `None` if no replacement occurred.
pub fn replace_cell_raw(
    raw: &str,
    query: &str,
    replacement: &str,
    match_case: bool,
    whole_cell: bool,
) -> Option<String> {
    if query.is_empty() {
        return None;
    }
    if whole_cell {
        return text_matches(raw, query, match_case, true).then(|| replacement.to_string());
    }
    if match_case {
        if !raw.contains(query) {
            return None;
        }
        return Some(raw.replace(query, replacement));
    }
    let query_lower = fold_lower(query);
    // Case-insensitive replacement preserving unmatched slices of `raw`.
    // Map folded byte offsets back to original-string byte offsets, and
    // replace only when both ends of a match fall on original char boundaries.
    let mut lower = String::with_capacity(raw.len());
    let mut starts: Vec<(usize, usize)> = Vec::new();
    for (raw_off, ch) in raw.char_indices() {
        starts.push((lower.len(), raw_off));
        for folded in ch.to_lowercase() {
            lower.push(if folded == 'ς' { 'σ' } else { folded });
        }
    }
    starts.push((lower.len(), raw.len()));
    if !lower.contains(&query_lower) {
        return None;
    }
    let to_raw = |lo: usize| {
        starts
            .binary_search_by_key(&lo, |&(l, _)| l)
            .ok()
            .map(|i| starts[i].1)
    };
    let mut out = String::with_capacity(raw.len());
    let (mut lo_pos, mut raw_pos) = (0usize, 0usize);
    while let Some(rel) = lower[lo_pos..].find(&query_lower) {
        let ls = lo_pos + rel;
        let le = ls + query_lower.len();
        match (to_raw(ls), to_raw(le)) {
            (Some(rs), Some(re)) => {
                out.push_str(&raw[raw_pos..rs]);
                out.push_str(replacement);
                raw_pos = re;
                lo_pos = le;
            }
            _ => {
                lo_pos = ls + lower[ls..].chars().next().map_or(1, char::len_utf8);
            }
        }
    }
    out.push_str(&raw[raw_pos..]);
    if out == raw {
        None
    } else {
        Some(out)
    }
}

/// Collects `(cell, old_raw, new_raw)` for all cells in `sheet` that would change under Replace All.
pub fn collect_sheet_replacements(
    sheet: &Sheet,
    query: &str,
    replacement: &str,
    match_case: bool,
    whole_cell: bool,
) -> Vec<(CellRef, Option<String>, String)> {
    let mut coords: Vec<CellRef> = sheet.cells.iter().map(|(cell, _)| *cell).collect();
    coords.sort_by_key(|c| (c.row, c.col));
    let mut edits = Vec::new();
    for cell in coords {
        if let Some(raw) = sheet.raw(cell) {
            if let Some(updated) = replace_cell_raw(raw, query, replacement, match_case, whole_cell)
            {
                edits.push((cell, Some(raw.to_string()), updated));
            }
        }
    }
    edits
}

/// Returns sorted unique display values in `col` for data rows strictly below `header_row`.
pub fn unique_column_values(
    sheet: &Sheet,
    values: &HashMap<CellRef, Value>,
    header_row: u32,
    col: u32,
) -> Vec<String> {
    let Some((_, _, _, max_row)) = sheet.used_range() else {
        return Vec::new();
    };
    let mut unique = BTreeSet::new();
    for row in header_row.saturating_add(1)..=max_row {
        let cell = CellRef { row, col };
        let val = display_for_filter(sheet, values, cell);
        unique.insert(val);
    }
    unique.into_iter().collect()
}

fn display_for_filter(sheet: &Sheet, values: &HashMap<CellRef, Value>, cell: CellRef) -> String {
    match values.get(&cell) {
        Some(v) if !matches!(v, Value::Empty) => v.display(),
        _ => sheet.raw(cell).unwrap_or("").trim().to_string(),
    }
}

/// Recomputes `sheet.hidden_rows` from `sheet.auto_filter` and evaluated cell values.
pub fn recompute_hidden_rows(sheet: &mut Sheet, values: &HashMap<CellRef, Value>) {
    sheet.hidden_rows.clear();
    if !sheet.auto_filter.enabled || sheet.auto_filter.column_filters.is_empty() {
        return;
    }
    let Some((_, _, _, max_row)) = sheet.used_range() else {
        return;
    };
    let header_row = sheet.auto_filter.header_row;
    for row in header_row.saturating_add(1)..=max_row {
        let visible = sheet
            .auto_filter
            .column_filters
            .iter()
            .all(|(&col, allowed)| {
                let val = display_for_filter(sheet, values, CellRef { row, col });
                allowed.contains(&val)
            });
        if !visible {
            sheet.hidden_rows.insert(row);
        }
    }
}

/// Validates a named range identifier (ASCII alpha/`_` start, alphanumeric/`_` body, not an A1 cell ref).
pub fn valid_named_range_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
        && CellRef::parse(name).is_none()
}

/// Expands named range identifiers in a formula body (excluding string literals and function calls).
pub fn expand_named_ranges_in_formula(
    formula: &str,
    ranges: &BTreeMap<String, CellRange>,
) -> String {
    if ranges.is_empty() {
        return formula.to_string();
    }
    let mut output = String::with_capacity(formula.len());
    let bytes = formula.as_bytes();
    let mut index = 0;
    let mut quoted = false;
    while index < bytes.len() {
        let character = bytes[index] as char;
        if character == '"' {
            quoted = !quoted;
            output.push(character);
            index += 1;
            continue;
        }
        if !quoted && (character.is_ascii_alphabetic() || character == '_') {
            let start = index;
            index += 1;
            while index < bytes.len() {
                let next = bytes[index] as char;
                if next.is_ascii_alphanumeric() || next == '_' {
                    index += 1;
                } else {
                    break;
                }
            }
            let ident = &formula[start..index];
            let next_char = bytes.get(index).map(|byte| *byte as char);
            let prev_char = start.checked_sub(1).map(|i| bytes[i] as char);
            if next_char != Some('(') && prev_char != Some('!') {
                if let Some(range) = ranges.get(&ident.to_ascii_uppercase()) {
                    output.push_str(&range.to_a1());
                    continue;
                }
            }
            output.push_str(ident);
            continue;
        }
        let ch = formula[index..]
            .chars()
            .next()
            .expect("index stays on a char boundary");
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

/// Validates candidate cell input against a `DataValidationRule`, resolving `RangeList` from `sheet` if needed.
pub fn validate_input_against_rule(
    rule: &DataValidationRule,
    sheet: &Sheet,
    values: &HashMap<CellRef, Value>,
    input: &str,
) -> Result<(), String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return if rule.allow_blank {
            Ok(())
        } else {
            Err(rule.error_message.clone())
        };
    }
    match &rule.criteria {
        ValidationCriteria::RangeList(range) => {
            let mut items = Vec::new();
            for row in range.start.row..=range.end.row {
                for col in range.start.col..=range.end.col {
                    let cell = CellRef { row, col };
                    let text = values
                        .get(&cell)
                        .filter(|v| !matches!(v, Value::Empty))
                        .map(Value::display)
                        .or_else(|| sheet.raw(cell).map(str::to_string))
                        .unwrap_or_default();
                    let t = text.trim();
                    if !t.is_empty() {
                        items.push(t.to_string());
                    }
                }
            }
            if items.iter().any(|item| item.eq_ignore_ascii_case(trimmed)) {
                Ok(())
            } else {
                Err(rule.error_message.clone())
            }
        }
        _ => rule.validate(input),
    }
}

/// Removes duplicate rows in `range` based on `key_cols`, shifting remaining rows up.
/// Returns the number of duplicate rows removed.
pub fn remove_duplicates(
    sheet: &mut Sheet,
    range: CellRange,
    key_cols: &[u32],
    has_header: bool,
) -> usize {
    let start_row = if has_header {
        range.start.row + 1
    } else {
        range.start.row
    };
    if start_row > range.end.row || key_cols.is_empty() {
        return 0;
    }
    let mut seen = BTreeSet::<Vec<String>>::new();
    let mut kept_rows = Vec::<Vec<Option<String>>>::new();
    let mut removed = 0usize;
    for r in start_row..=range.end.row {
        let key: Vec<String> = key_cols
            .iter()
            .map(|&c| {
                sheet
                    .raw(CellRef { row: r, col: c })
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        if seen.insert(key) {
            let row_vals = (range.start.col..=range.end.col)
                .map(|c| sheet.raw(CellRef { row: r, col: c }).map(str::to_string))
                .collect();
            kept_rows.push(row_vals);
        } else {
            removed += 1;
        }
    }
    if removed == 0 {
        return 0;
    }
    for (idx, r) in (start_row..=range.end.row).enumerate() {
        for (c_off, c) in (range.start.col..=range.end.col).enumerate() {
            let cell = CellRef { row: r, col: c };
            if let Some(Some(val)) = kept_rows.get(idx).and_then(|row| row.get(c_off)) {
                sheet.set_raw(cell, val);
            } else {
                sheet.cells.remove(&cell);
            }
        }
    }
    removed
}

/// Sorts rows in `range` by multiple `(col, ascending)` keys.
pub fn multi_level_sort(
    sheet: &mut Sheet,
    range: CellRange,
    keys: &[(u32, bool)],
    has_header: bool,
) {
    let start_row = if has_header {
        range.start.row + 1
    } else {
        range.start.row
    };
    if start_row >= range.end.row || keys.is_empty() {
        return;
    }
    let mut rows: Vec<Vec<Option<String>>> = (start_row..=range.end.row)
        .map(|r| {
            (range.start.col..=range.end.col)
                .map(|c| sheet.raw(CellRef { row: r, col: c }).map(str::to_string))
                .collect()
        })
        .collect();
    rows.sort_by(|a, b| {
        for &(col, asc) in keys {
            let idx = (col.saturating_sub(range.start.col)) as usize;
            let sa = a.get(idx).and_then(|v| v.as_deref()).unwrap_or("");
            let sb = b.get(idx).and_then(|v| v.as_deref()).unwrap_or("");
            let ord = match (sa.parse::<f64>(), sb.parse::<f64>()) {
                (Ok(na), Ok(nb)) => na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal),
                _ => sa.cmp(sb),
            };
            if ord != std::cmp::Ordering::Equal {
                return if asc { ord } else { ord.reverse() };
            }
        }
        std::cmp::Ordering::Equal
    });
    for (r_off, r) in (start_row..=range.end.row).enumerate() {
        for (c_off, c) in (range.start.col..=range.end.col).enumerate() {
            let cell = CellRef { row: r, col: c };
            if let Some(Some(val)) = rows.get(r_off).and_then(|row| row.get(c_off)) {
                sheet.set_raw(cell, val);
            } else {
                sheet.cells.remove(&cell);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{sheet_from_json, sheet_to_json};
    use crate::{evaluate, CellRange, CellRef, DataValidationRule, Sheet};

    #[test]
    fn find_and_replace_respects_case_whole_cell_and_workbook_scope() {
        let mut s1 = Sheet::new("One");
        s1.set_str("A1", "Apple");
        s1.set_str("B1", "pineapple");
        s1.set_str("A2", "10");
        s1.set_str("B2", "=A2*2");

        let mut s2 = Sheet::new("Two");
        s2.set_str("C3", "Apple");

        let sheets = vec![s1, s2];
        let vals = crate::workbook::evaluate_workbook(&sheets);

        let m_case_insensitive = find_matches_in_workbook(
            &sheets,
            &vals,
            0,
            &FindOptions {
                query: "apple".into(),
                match_case: false,
                whole_cell: false,
                workbook_scope: false,
            },
        );
        assert_eq!(m_case_insensitive.len(), 2);

        let m_whole_case = find_matches_in_workbook(
            &sheets,
            &vals,
            0,
            &FindOptions {
                query: "Apple".into(),
                match_case: true,
                whole_cell: true,
                workbook_scope: true,
            },
        );
        assert_eq!(m_whole_case.len(), 2);
        assert_eq!(m_whole_case[0].sheet_index, 0);
        assert_eq!(m_whole_case[1].sheet_index, 1);

        let next = find_next_match(&m_whole_case, 0, CellRef::parse("A1").unwrap(), true).unwrap();
        assert_eq!(next.1.sheet_index, 1);
        let prev = find_next_match(&m_whole_case, 0, CellRef::parse("A1").unwrap(), false).unwrap();
        assert_eq!(prev.1.sheet_index, 1);

        assert_eq!(
            replace_cell_raw("pineapple", "APPLE", "pear", false, false),
            Some("pinepear".to_string())
        );
        assert_eq!(
            replace_cell_raw("pineapple", "Apple", "pear", true, false),
            None
        );
        assert_eq!(
            replace_cell_raw("Apple", "apple", "Orange", false, true),
            Some("Orange".to_string())
        );
    }

    #[test]
    fn autofilter_non_destructively_hides_and_restores_rows() {
        let mut sheet = Sheet::new("Filter");
        sheet.set_str("A1", "Region");
        sheet.set_str("B1", "Status");
        sheet.set_str("A2", "North");
        sheet.set_str("B2", "Open");
        sheet.set_str("A3", "South");
        sheet.set_str("B3", "Closed");
        sheet.set_str("A4", "North");
        sheet.set_str("B4", "Closed");

        let values = evaluate(&sheet);
        let unique_a = unique_column_values(&sheet, &values, 0, 0);
        assert_eq!(unique_a, vec!["North", "South"]);

        sheet.auto_filter.enabled = true;
        sheet.auto_filter.header_row = 0;
        sheet
            .auto_filter
            .column_filters
            .insert(0, BTreeSet::from(["North".to_string()]));
        recompute_hidden_rows(&mut sheet, &values);
        assert_eq!(sheet.hidden_rows, BTreeSet::from([2]));

        sheet
            .auto_filter
            .column_filters
            .insert(1, BTreeSet::from(["Open".to_string()]));
        recompute_hidden_rows(&mut sheet, &values);
        assert_eq!(sheet.hidden_rows, BTreeSet::from([2, 3]));
        // Underlying cells are untouched.
        assert_eq!(sheet.raw(CellRef::parse("A3").unwrap()), Some("South"));

        sheet.auto_filter.column_filters.clear();
        recompute_hidden_rows(&mut sheet, &values);
        assert!(sheet.hidden_rows.is_empty());
    }

    #[test]
    fn named_ranges_create_resolve_evaluate_and_persist() {
        let mut sheet = Sheet::new("Sales");
        sheet.set_str("A1", "10");
        sheet.set_str("A2", "20");
        sheet.set_str("A3", "30");
        sheet.set_str("B1", "=SUM(Q1_Sales)");

        assert!(sheet
            .set_named_range("A1", CellRange::parse("A1:A3").unwrap())
            .is_err());
        sheet
            .set_named_range("Q1_Sales", CellRange::parse("A1:A3").unwrap())
            .unwrap();
        assert_eq!(
            sheet.resolve_named_range("q1_sales"),
            Some(CellRange::parse("A1:A3").unwrap())
        );

        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(60.0))
        );

        let json = sheet_to_json(&sheet);
        let loaded = sheet_from_json(&json).unwrap();
        assert_eq!(
            loaded.resolve_named_range("Q1_Sales"),
            Some(CellRange::parse("A1:A3").unwrap())
        );
    }

    #[test]
    fn cell_data_validation_supports_list_range_and_number_rules() {
        let mut sheet = Sheet::new("Val");
        sheet.set_str("E1", "Alpha");
        sheet.set_str("E2", "Beta");
        let vals = evaluate(&sheet);

        let list_rule = DataValidationRule::new(
            ValidationCriteria::RangeList(CellRange::parse("E1:E2").unwrap()),
            "Must be Alpha or Beta",
        );
        sheet.set_validation(CellRange::parse("A1:A2").unwrap(), list_rule);
        assert!(sheet
            .validate_cell_input(CellRef::parse("A1").unwrap(), "alpha", &vals)
            .is_ok());
        assert_eq!(
            sheet.validate_cell_input(CellRef::parse("A1").unwrap(), "Gamma", &vals),
            Err("Must be Alpha or Beta".to_string())
        );

        let num_rule = DataValidationRule::new(
            ValidationCriteria::WholeNumberBetween(1, 100),
            "1 to 100 only",
        );
        sheet.set_validation(CellRange::parse("B1:B1").unwrap(), num_rule);
        assert!(sheet
            .validate_cell_input(CellRef::parse("B1").unwrap(), "50", &vals)
            .is_ok());
        assert!(sheet
            .validate_cell_input(CellRef::parse("B1").unwrap(), "150", &vals)
            .is_err());

        let json = sheet_to_json(&sheet);
        let loaded = sheet_from_json(&json).unwrap();
        assert_eq!(loaded.validations.len(), 2);
    }

    #[test]
    fn case_insensitive_replace_handles_multibyte_case_changes() {
        assert_eq!(
            replace_cell_raw("İstanbul", "stan", "X", false, false).as_deref(),
            Some("İXbul")
        );
        assert_eq!(
            replace_cell_raw("\u{212A}xab", "ab", "cd", false, false).as_deref(),
            Some("\u{212A}xcd")
        );
    }

    #[test]
    fn case_insensitive_replace_agrees_with_find_on_final_sigma() {
        // `str::to_lowercase` lowercases a word-final Σ to ς; per-character
        // lowering produces σ. Find and Replace must still agree.
        let raw = "ΟΣ";
        let query = "ος";
        assert!(text_matches(raw, query, false, false));
        assert_eq!(
            replace_cell_raw(raw, query, "x", false, false).as_deref(),
            Some("x")
        );
        // Whole-cell matching must use the same equivalence.
        assert!(text_matches(raw, query, false, true));
        // Non-final sigma keeps working in both directions.
        assert_eq!(
            replace_cell_raw("ΣΟΦΙΑ", "σο", "so", false, false).as_deref(),
            Some("soΦΙΑ")
        );
    }

    #[test]
    fn named_range_expansion_preserves_non_ascii_text() {
        let mut ranges = std::collections::BTreeMap::new();
        ranges.insert(
            "RATE".to_string(),
            CellRange::parse("A1:A1").expect("valid range"),
        );
        assert_eq!(
            expand_named_ranges_in_formula("=\"café\"&RATE+'Données'!B1", &ranges),
            "=\"café\"&A1+'Données'!B1"
        );
    }

    #[test]
    fn remove_duplicates_and_multi_level_sort() {
        let mut sheet = Sheet::new("S1");
        sheet.set_raw(CellRef { row: 0, col: 0 }, "Name");
        sheet.set_raw(CellRef { row: 0, col: 1 }, "Score");
        sheet.set_raw(CellRef { row: 1, col: 0 }, "Bob");
        sheet.set_raw(CellRef { row: 1, col: 1 }, "10");
        sheet.set_raw(CellRef { row: 2, col: 0 }, "Alice");
        sheet.set_raw(CellRef { row: 2, col: 1 }, "20");
        sheet.set_raw(CellRef { row: 3, col: 0 }, "Bob");
        sheet.set_raw(CellRef { row: 3, col: 1 }, "10");
        let range = CellRange::new(CellRef { row: 0, col: 0 }, CellRef { row: 3, col: 1 });
        let removed = remove_duplicates(&mut sheet, range, &[0, 1], true);
        assert_eq!(removed, 1);
        assert_eq!(sheet.raw(CellRef { row: 3, col: 0 }), None);

        let sort_range = CellRange::new(CellRef { row: 0, col: 0 }, CellRef { row: 2, col: 1 });
        multi_level_sort(&mut sheet, sort_range, &[(0, true)], true);
        assert_eq!(sheet.raw(CellRef { row: 1, col: 0 }), Some("Alice"));
        assert_eq!(sheet.raw(CellRef { row: 2, col: 0 }), Some("Bob"));
    }
}
