//! Turn a Loom formula into the text Excel stores in `<f>`.
//!
//! Loom and Excel share the grammar for references, operators and classic
//! functions. Functions Excel added after 2007 must be written with the
//! `_xlfn.` prefix in the file or Excel evaluates them as unknown names
//! (`#NAME?`), so the prefix is added here, outside string literals and
//! quoted sheet names.

use crate::refs::remap_sheet_references_in_formula;

/// Functions stored as `_xlfn.NAME`.
const FUTURE_FUNCTIONS: &[&str] = &[
    "CONCAT",
    "TEXTJOIN",
    "MINIFS",
    "MAXIFS",
    "IFS",
    "SWITCH",
    "XLOOKUP",
    "XMATCH",
    "SEQUENCE",
    "UNIQUE",
    "SORTBY",
    "RANDARRAY",
    "TEXTSPLIT",
    "LET",
    "IFNA",
    "XOR",
    "DAYS",
    "NUMBERVALUE",
    // Excel 2010 statistical and math names.
    "STDEV.S",
    "STDEV.P",
    "VAR.S",
    "VAR.P",
    "MODE.SNGL",
    "RANK.EQ",
    "RANK.AVG",
    "PERCENTILE.INC",
    "PERCENTILE.EXC",
    "QUARTILE.INC",
    "QUARTILE.EXC",
    "NORM.DIST",
    "NORM.S.DIST",
    "CEILING.MATH",
    "FLOOR.MATH",
];

/// Dynamic-array functions that also carry the `_xlws.` worksheet scope.
const WORKSHEET_FUNCTIONS: &[&str] = &["SORT", "FILTER"];

/// `raw` is the cell text including the leading `=`; the result has none.
pub(super) fn excel_formula(raw: &str, sheet_names: &[(String, String)]) -> String {
    let remapped = remap_sheet_references_in_formula(raw, sheet_names);
    let body = remapped.strip_prefix('=').unwrap_or(&remapped);
    prefix_future_functions(body)
}

/// True when `raw` (a cell starting with `=`) is plausibly a formula Excel can
/// parse. Loom's own grammar is smaller than Excel's (no `&`, no `STDEV.S`),
/// so this checks only structure: something follows the `=`, quotes and
/// parentheses balance, and the text does not end on an operator or opening
/// token. A formula that fails is kept as text instead of corrupting the sheet.
pub(super) fn is_plausible_formula(raw: &str) -> bool {
    let body = raw.strip_prefix('=').unwrap_or(raw).trim();
    if body.is_empty() {
        return false;
    }
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut last = ' ';
    for character in body.chars() {
        match quote {
            Some(open) => {
                if character == open {
                    // A doubled quote reopens on the next character.
                    quote = None;
                }
            }
            None => match character {
                '"' | '\'' => quote = Some(character),
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth < 0 {
                        return false;
                    }
                }
                _ => {}
            },
        }
        if !character.is_whitespace() {
            last = character;
        }
    }
    quote.is_none()
        && depth == 0
        && !matches!(
            last,
            '+' | '-' | '*' | '/' | '^' | '&' | '=' | '<' | '>' | ',' | ';' | ':' | '(' | '!'
        )
}

fn prefix_future_functions(formula: &str) -> String {
    let characters: Vec<char> = formula.chars().collect();
    let mut out = String::with_capacity(formula.len() + 8);
    let mut position = 0;
    while position < characters.len() {
        let character = characters[position];
        if character == '"' || character == '\'' {
            // A string literal or quoted sheet name; a doubled quote is an
            // escaped quote and does not end it.
            out.push(character);
            position += 1;
            while position < characters.len() {
                out.push(characters[position]);
                if characters[position] == character {
                    if characters.get(position + 1) == Some(&character) {
                        out.push(character);
                        position += 2;
                        continue;
                    }
                    position += 1;
                    break;
                }
                position += 1;
            }
            continue;
        }
        if character.is_ascii_alphabetic() {
            let start = position;
            while position < characters.len()
                && (characters[position].is_ascii_alphanumeric()
                    || characters[position] == '.'
                    || characters[position] == '_')
            {
                position += 1;
            }
            let word: String = characters[start..position].iter().collect();
            let is_call = characters.get(position) == Some(&'(');
            let upper = word.to_ascii_uppercase();
            if is_call && FUTURE_FUNCTIONS.contains(&upper.as_str()) {
                out.push_str("_xlfn.");
            } else if is_call && WORKSHEET_FUNCTIONS.contains(&upper.as_str()) {
                out.push_str("_xlfn._xlws.");
            }
            out.push_str(&word);
            continue;
        }
        out.push(character);
        position += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_functions_get_their_prefix_outside_text() {
        assert_eq!(
            excel_formula("=TEXTJOIN(\", \",TRUE,A1:A3)", &[]),
            "_xlfn.TEXTJOIN(\", \",TRUE,A1:A3)"
        );
        assert_eq!(
            excel_formula("=sort(unique(A1:A9))", &[]),
            "_xlfn._xlws.sort(_xlfn.unique(A1:A9))"
        );
        assert_eq!(
            excel_formula("=\"CONCAT(\"&A1&'CONCAT(x)'!B2", &[]),
            "\"CONCAT(\"&A1&'CONCAT(x)'!B2"
        );
        assert_eq!(
            excel_formula("=SUM(A1:A3)+TRANSPOSE(B1)", &[]),
            "SUM(A1:A3)+TRANSPOSE(B1)"
        );
    }

    #[test]
    fn every_post_2007_function_round_trips_through_the_file_prefix() {
        use super::super::formula_import::{loom_formula, DefinedNames};
        let calls = [
            "IFS(A1,1)",
            "SWITCH(A1,1,2)",
            "XLOOKUP(A1,B1:B2,C1:C2)",
            "XOR(A1,B1)",
            "IFNA(A1,0)",
            "STDEV.S(A1:A3)",
            "STDEV.P(A1:A3)",
            "VAR.S(A1:A3)",
            "VAR.P(A1:A3)",
            "MODE.SNGL(A1:A3)",
            "RANK.EQ(A1,A1:A3)",
            "CONCAT(A1,B1)",
            "TEXTJOIN(\",\",TRUE,A1:A3)",
            "MAXIFS(A1:A3,B1:B3,1)",
            "MINIFS(A1:A3,B1:B3,1)",
            "UNIQUE(A1:A3)",
            "SEQUENCE(3)",
            "DAYS(A1,B1)",
        ];
        for call in calls {
            let in_file = excel_formula(&format!("={call}"), &[]);
            assert!(in_file.starts_with("_xlfn."), "{call} -> {in_file}");
            assert_eq!(loom_formula(&in_file, &DefinedNames::default()), call);
        }
        for call in ["FILTER(A1:A3,B1:B3)", "SORT(A1:A3)"] {
            let in_file = excel_formula(&format!("={call}"), &[]);
            assert!(in_file.starts_with("_xlfn._xlws."), "{call} -> {in_file}");
            assert_eq!(loom_formula(&in_file, &DefinedNames::default()), call);
        }
    }
}
