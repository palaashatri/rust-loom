//! Cell-reference text handling for Loom Sheets formulas.
//!
//! Covers shifting references during fill/copy, quoting sheet names, and
//! rewriting sheet qualifiers on rename. All operations are pure string
//! transforms over formula text; evaluation lives in the formula engine.

/// Whether a sheet name needs `'...'` quoting in a cross-sheet reference:
/// anything but letters, digits, underscore, and dot.
pub fn sheet_name_needs_quotes(name: &str) -> bool {
    name.is_empty()
        || name
            .chars()
            .any(|c| !(c.is_ascii_alphanumeric() || c == '_' || c == '.'))
}

/// Quote a sheet name for use as a cross-sheet qualifier (`'My Sheet'!`),
/// doubling embedded quotes. Names that need no quoting pass through.
pub fn quote_sheet_name(name: &str) -> String {
    if !sheet_name_needs_quotes(name) {
        return name.to_string();
    }
    format!("'{}'", name.replace('\'', "''"))
}

/// Normalize a sheet qualifier for comparison: unquote and lowercase.
pub fn normalize_sheet_name(qualifier: &str) -> String {
    let unquoted = qualifier.strip_prefix('\'').and_then(|tail| {
        tail.strip_suffix('\'')
            .map(|inner| inner.replace("''", "'"))
    });
    unquoted
        .as_deref()
        .unwrap_or(qualifier)
        .to_ascii_lowercase()
}

/// Shift the cell references in a formula by a column/row delta, honoring
/// `$` absolute markers and leaving `Sheet!` / `'Sheet Name'!` qualifiers
/// (and string literals) untouched apart from their cell part.
pub fn shift_formula_references(formula: &str, delta_cols: i32, delta_rows: i32) -> String {
    if !formula.starts_with('=') {
        return formula.to_string();
    }

    let bytes = formula.as_bytes();
    let mut result = String::with_capacity(formula.len() + 4);
    let mut flushed = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        // String literals are opaque text, never references.
        if bytes[i] == b'"' {
            i += 1;
            while i < bytes.len() {
                if bytes[i] == b'"' {
                    if i + 1 < bytes.len() && bytes[i + 1] == b'"' {
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            continue;
        }

        // Quoted sheet qualifier: 'Name'! (with '' escapes).
        if bytes[i] == b'\'' {
            if let Some(after) = scan_quoted_name_end(bytes, i) {
                if bytes.get(after) == Some(&b'!') {
                    result.push_str(&formula[flushed..=after]);
                    i = after + 1;
                    flushed = i;
                    i = shift_ref_or_range_bytes(
                        formula,
                        bytes,
                        i,
                        delta_cols,
                        delta_rows,
                        &mut result,
                        &mut flushed,
                    );
                    continue;
                }
            }
            i += 1;
            continue;
        }

        // Unquoted sheet qualifier: Name! (letters/digits/underscore/dot).
        if bytes[i].is_ascii_alphabetic() {
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.')
            {
                j += 1;
            }
            if j > i && bytes.get(j) == Some(&b'!') {
                result.push_str(&formula[flushed..=j]);
                i = j + 1;
                flushed = i;
                i = shift_ref_or_range_bytes(
                    formula,
                    bytes,
                    i,
                    delta_cols,
                    delta_rows,
                    &mut result,
                    &mut flushed,
                );
                continue;
            }
        }

        // A word directly followed by `(` is a function name (`LOG10(`,
        // `STDEV.S(`), never a cell reference, whatever its letters and digits.
        if bytes[i].is_ascii_alphabetic() && !(i > 0 && bytes[i - 1].is_ascii_alphanumeric()) {
            let mut j = i;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'_' || bytes[j] == b'.')
            {
                j += 1;
            }
            if bytes.get(j) == Some(&b'(') {
                i = j;
                continue;
            }
        }

        // Check for start of cell reference (optional '$' followed by letters then optional '$' then digits)
        let is_ref_start = if bytes[i] == b'$' {
            i + 1 < bytes.len() && bytes[i + 1].is_ascii_alphabetic()
        } else if bytes[i].is_ascii_alphabetic() {
            // Must not be preceded by alphanumeric or underscore (which would make it a function name like SUM)
            !(i > 0 && (bytes[i - 1].is_ascii_alphanumeric() || bytes[i - 1] == b'_'))
        } else {
            false
        };

        if is_ref_start {
            let start = i;
            let mut col_abs = false;
            if bytes[i] == b'$' {
                col_abs = true;
                i += 1;
            }

            let col_start = i;
            while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
                i += 1;
            }
            let col_bytes = &bytes[col_start..i];

            let mut row_abs = false;
            if i < bytes.len() && bytes[i] == b'$' {
                row_abs = true;
                i += 1;
            }

            let row_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            let row_bytes = &bytes[row_start..i];

            // Check if this formed a valid cell reference (e.g., has digits for row)
            if !col_bytes.is_empty() && !row_bytes.is_empty() {
                result.push_str(&formula[flushed..start]);
                write_shifted_cell_text(
                    col_bytes,
                    row_bytes,
                    col_abs,
                    row_abs,
                    delta_cols,
                    delta_rows,
                    &mut result,
                );
                flushed = i;
            }
        } else {
            i += 1;
        }
    }

    result.push_str(&formula[flushed..]);
    result
}

/// Scan a `'...'` quoted name starting at byte `i` (which must be `b'\''`),
/// returning the byte index right past its closing quote.
fn scan_quoted_name_end(bytes: &[u8], i: usize) -> Option<usize> {
    let mut j = i + 1;
    while j < bytes.len() {
        if bytes[j] == b'\'' {
            if j + 1 < bytes.len() && bytes[j + 1] == b'\'' {
                j += 2;
            } else {
                return Some(j + 1);
            }
        } else {
            j += 1;
        }
    }
    None
}

/// Scan a `'...'` quoted name starting at `i` (which must be `'`),
/// returning the qualifier text and the index past its closing quote.
fn scan_quoted_name(chars: &[char], i: usize) -> Option<(String, usize)> {
    let mut j = i + 1;
    let mut name = String::new();
    while j < chars.len() {
        if chars[j] == '\'' {
            if j + 1 < chars.len() && chars[j + 1] == '\'' {
                name.push('\'');
                j += 2;
            } else {
                return Some((name, j + 1));
            }
        } else {
            name.push(chars[j]);
            j += 1;
        }
    }
    None
}

/// Shift one `A1`-style reference (with `$` markers) at `i`, or a `A1:B2`
/// range when a colon follows. Returns the new byte index and advances `flushed`.
fn shift_ref_or_range_bytes(
    formula: &str,
    bytes: &[u8],
    i: usize,
    delta_cols: i32,
    delta_rows: i32,
    out: &mut String,
    flushed: &mut usize,
) -> usize {
    let start = i;
    let mut pos = i;
    let mut col_abs = false;
    if bytes.get(pos) == Some(&b'$') {
        col_abs = true;
        pos += 1;
    }
    let col_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_alphabetic() {
        pos += 1;
    }
    let col_bytes = &bytes[col_start..pos];
    let mut row_abs = false;
    if bytes.get(pos) == Some(&b'$') {
        row_abs = true;
        pos += 1;
    }
    let row_start = pos;
    while pos < bytes.len() && bytes[pos].is_ascii_digit() {
        pos += 1;
    }
    let row_bytes = &bytes[row_start..pos];
    if col_bytes.is_empty() || row_bytes.is_empty() {
        return start;
    }
    out.push_str(&formula[*flushed..start]);
    write_shifted_cell_text(
        col_bytes, row_bytes, col_abs, row_abs, delta_cols, delta_rows, out,
    );
    *flushed = pos;
    // Range end after a colon.
    if bytes.get(pos) == Some(&b':') {
        let mut j = pos + 1;
        let mut end_col_abs = false;
        if bytes.get(j) == Some(&b'$') {
            end_col_abs = true;
            j += 1;
        }
        let end_col_start = j;
        while j < bytes.len() && bytes[j].is_ascii_alphabetic() {
            j += 1;
        }
        let end_col = &bytes[end_col_start..j];
        let mut end_row_abs = false;
        if bytes.get(j) == Some(&b'$') {
            end_row_abs = true;
            j += 1;
        }
        let end_row_start = j;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        let end_row = &bytes[end_row_start..j];
        if !end_col.is_empty() && !end_row.is_empty() {
            out.push(':');
            write_shifted_cell_text(
                end_col,
                end_row,
                end_col_abs,
                end_row_abs,
                delta_cols,
                delta_rows,
                out,
            );
            *flushed = j;
            return j;
        }
    }
    pos
}

/// Shift one parsed ASCII column/row byte slice pair and write directly to `out`.
fn write_shifted_cell_text(
    col_bytes: &[u8],
    row_bytes: &[u8],
    col_abs: bool,
    row_abs: bool,
    delta_cols: i32,
    delta_rows: i32,
    out: &mut String,
) {
    use std::fmt::Write as _;

    let mut col_idx: u32 = 0;
    for &b in col_bytes {
        col_idx = col_idx
            .saturating_mul(26)
            .saturating_add(u32::from(b.to_ascii_uppercase() - b'A' + 1));
    }
    let mut col_num = (col_idx.saturating_sub(1)) as i32;
    let mut row_num = std::str::from_utf8(row_bytes)
        .ok()
        .and_then(|s| s.parse::<i32>().ok())
        .unwrap_or(1)
        - 1;

    if !col_abs {
        col_num = (col_num + delta_cols).max(0);
    }
    if !row_abs {
        row_num = (row_num + delta_rows).max(0);
    }

    if col_abs {
        out.push('$');
    }
    let mut buf = [0u8; 8];
    let mut pos = buf.len();
    let mut cn = col_num + 1;
    while cn > 0 && pos > 0 {
        pos -= 1;
        buf[pos] = b'A' + ((cn - 1) % 26) as u8;
        cn = (cn - 1) / 26;
    }
    if let Ok(col_str) = std::str::from_utf8(&buf[pos..]) {
        out.push_str(col_str);
    }
    if row_abs {
        out.push('$');
    }
    let _ = write!(out, "{}", row_num + 1);
}

/// Validate a candidate sheet name against its future siblings (which must
/// exclude the sheet being renamed). Rejects empty names, `!` (which would
/// break qualifier syntax), cell-reference lookalikes such as `A1` (which
/// would not parse as qualifiers), and case-insensitive duplicates (which
/// would make reference resolution ambiguous).
pub fn validate_sheet_name(name: &str, sibling_names: &[&str]) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err("Sheet name must not be empty".to_string());
    }
    if trimmed.contains('!') {
        return Err("Sheet name must not contain '!'".to_string());
    }
    if crate::CellRef::parse(trimmed).is_some() {
        return Err(format!(
            "Sheet name {trimmed:?} looks like a cell reference"
        ));
    }
    if sibling_names
        .iter()
        .any(|sibling| sibling.eq_ignore_ascii_case(trimmed))
    {
        return Err(format!("A sheet named {trimmed:?} already exists"));
    }
    Ok(())
}

/// Rewrite the sheet qualifier of cross-sheet references when a tab is
/// renamed: `Old!A1` and `'Old'!A1` become the new qualifier (re-quoted only
/// if needed). Matching is case-insensitive; string literals, function
/// names, and other sheets pass through untouched.
pub fn rename_sheet_in_formula(formula: &str, old_name: &str, new_name: &str) -> String {
    remap_sheet_references_in_formula(formula, &[(old_name.to_string(), new_name.to_string())])
}

/// Rewrite any matching sheet qualifiers in one pass. Using a complete name
/// map avoids accidentally rewriting a new name a second time when it also
/// happens to be another sheet's old name.
pub fn remap_sheet_references_in_formula(
    formula: &str,
    sheet_names: &[(String, String)],
) -> String {
    if !formula.starts_with('=') {
        return formula.to_string();
    }
    let mut result = String::with_capacity(formula.len());
    let chars: Vec<char> = formula.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        // Skip string literals verbatim.
        if chars[i] == '"' {
            let start = i;
            i += 1;
            while i < chars.len() {
                if chars[i] == '"' {
                    if i + 1 < chars.len() && chars[i + 1] == '"' {
                        i += 2;
                    } else {
                        i += 1;
                        break;
                    }
                } else {
                    i += 1;
                }
            }
            for ch in &chars[start..i.min(chars.len())] {
                result.push(*ch);
            }
            continue;
        }
        // Quoted qualifier candidate.
        if chars[i] == '\'' {
            if let Some((name, after)) = scan_quoted_name(&chars, i) {
                if chars.get(after) == Some(&'!') {
                    if let Some((_, new_name)) = sheet_names
                        .iter()
                        .find(|(old_name, _)| old_name.eq_ignore_ascii_case(&name))
                    {
                        result.push_str(&quote_sheet_name(new_name));
                        result.push('!');
                        i = after + 1;
                        continue;
                    }
                }
            }
            result.push(chars[i]);
            i += 1;
            continue;
        }
        // Unquoted qualifier candidate.
        if chars[i].is_ascii_alphabetic() || chars[i] == '_' || chars[i] == '.' {
            let mut j = i;
            while j < chars.len()
                && (chars[j].is_ascii_alphanumeric() || chars[j] == '_' || chars[j] == '.')
            {
                j += 1;
            }
            if j > i && chars.get(j) == Some(&'!') {
                let candidate: String = chars[i..j].iter().collect();
                if let Some((_, new_name)) = sheet_names
                    .iter()
                    .find(|(old_name, _)| old_name.eq_ignore_ascii_case(&candidate))
                {
                    result.push_str(&quote_sheet_name(new_name));
                    result.push('!');
                } else {
                    for ch in &chars[i..=j] {
                        result.push(*ch);
                    }
                }
                i = j + 1;
                continue;
            }
        }
        result.push(chars[i]);
        i += 1;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unqualified_shift_behavior_is_preserved() {
        assert_eq!(shift_formula_references("=A1", 1, 2), "=B3");
        assert_eq!(shift_formula_references("=$A$1", 3, 3), "=$A$1");
        assert_eq!(shift_formula_references("=$A1", 2, 5), "=$A6");
        assert_eq!(shift_formula_references("=A$1", 2, 5), "=C$1");
        assert_eq!(
            shift_formula_references("=SUM($A$1:B2)", 1, 1),
            "=SUM($A$1:C3)"
        );
        assert_eq!(
            shift_formula_references("=$A1+B$2+$C$3+D4", 1, 1),
            "=$A2+C$2+$C$3+E5"
        );
        assert_eq!(shift_formula_references("Hello World", 1, 1), "Hello World");
    }

    #[test]
    fn qualified_shift_moves_only_the_cell_part() {
        assert_eq!(shift_formula_references("=Data!A1", 1, 1), "=Data!B2");
        assert_eq!(shift_formula_references("=Data!$A$1", 2, 2), "=Data!$A$1");
        assert_eq!(
            shift_formula_references("='My Sheet'!A1:B2", 1, 1),
            "='My Sheet'!B2:C3"
        );
        assert_eq!(
            shift_formula_references("=SUM(Data!A1:A5)", 0, 1),
            "=SUM(Data!A2:A6)"
        );
        // String literals naming sheets are text, not references.
        assert_eq!(
            shift_formula_references("=IF(A1=\"Data!A1\",1,0)", 5, 5),
            "=IF(F6=\"Data!A1\",1,0)"
        );
        // Functions sharing a prefix with a sheet name still parse as calls.
        assert_eq!(
            shift_formula_references("=SUMMARY(A1)", 1, 0),
            "=SUMMARY(B1)"
        );
    }

    #[test]
    fn rename_rewrites_only_matching_qualifiers() {
        assert_eq!(
            rename_sheet_in_formula("=Data!A1+Other!B2", "Data", "Figures"),
            "=Figures!A1+Other!B2"
        );
        assert_eq!(
            rename_sheet_in_formula("='Data'!A1", "data", "Figures"),
            "=Figures!A1"
        );
        assert_eq!(
            rename_sheet_in_formula("=Data!A1", "Data", "My Figures"),
            "='My Figures'!A1"
        );
        assert_eq!(
            rename_sheet_in_formula("=IF(A1=\"Data!A1\",Data!A2,0)", "Data", "D2"),
            "=IF(A1=\"Data!A1\",D2!A2,0)"
        );
        assert_eq!(
            rename_sheet_in_formula("=SUMMARY(A1)+Data!A1", "Data", "D"),
            "=SUMMARY(A1)+D!A1"
        );
        assert_eq!(
            rename_sheet_in_formula("='O''Brien'!A1", "o'brien", "OB"),
            "=OB!A1"
        );
    }

    #[test]
    fn remap_rewrites_all_qualifiers_once_and_leaves_text_alone() {
        let names = vec![
            ("A/B".to_string(), "AB".to_string()),
            ("AB".to_string(), "AB (2)".to_string()),
            ("_Data".to_string(), "Data Export".to_string()),
        ];
        assert_eq!(
            remap_sheet_references_in_formula("=IF(A1=\"A/B!A1\",'A/B'!A1,AB!A1)+_Data!B2", &names,),
            "=IF(A1=\"A/B!A1\",AB!A1,'AB (2)'!A1)+'Data Export'!B2"
        );
    }

    #[test]
    fn sheet_name_quoting_rules() {
        assert!(!sheet_name_needs_quotes("Data2"));
        assert!(!sheet_name_needs_quotes("my_sheet.v1"));
        assert!(sheet_name_needs_quotes("My Sheet"));
        assert!(sheet_name_needs_quotes("A-B"));
        assert_eq!(quote_sheet_name("Data"), "Data");
        assert_eq!(quote_sheet_name("My Figures"), "'My Figures'");
        assert_eq!(quote_sheet_name("O'Brien"), "'O''Brien'");
        assert_eq!(normalize_sheet_name("'O''Brien'"), "o'brien");
        assert_eq!(normalize_sheet_name("DATA"), "data");
    }

    #[test]
    fn sheet_name_validation_rejects_ambiguous_names() {
        assert!(validate_sheet_name("Figures", &["Data"]).is_ok());
        assert!(validate_sheet_name("  ", &[]).is_err());
        assert!(validate_sheet_name("A!B", &[]).is_err());
        assert!(validate_sheet_name("A1", &[]).is_err());
        assert!(validate_sheet_name("AA10", &[]).is_err());
        assert!(validate_sheet_name("data", &["Data", "Other"]).is_err());
        assert!(validate_sheet_name("Figures", &["Data", "Other"]).is_ok());
    }
}
