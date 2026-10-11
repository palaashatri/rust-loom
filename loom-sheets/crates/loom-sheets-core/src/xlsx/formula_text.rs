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
    "LAMBDA",
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
    "PERCENTRANK.INC",
    "PERCENTRANK.EXC",
    "FORECAST.LINEAR",
    "NORM.DIST",
    "NORM.S.DIST",
    "CEILING.MATH",
    "FLOOR.MATH",
    "NETWORKDAYS.INTL",
    "WORKDAY.INTL",
    "ISOWEEKNUM",
    // Excel 365 dynamic-array names.
    "TEXTBEFORE",
    "TEXTAFTER",
    "TAKE",
    "DROP",
    "VSTACK",
    "HSTACK",
    "TOCOL",
    "TOROW",
    "WRAPROWS",
    "WRAPCOLS",
    "CHOOSECOLS",
    "CHOOSEROWS",
    "EXPAND",
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

#[derive(Debug, Clone)]
struct ParamScope {
    start: usize,
    end: usize,
    params: Vec<String>,
}

fn split_top_level_args(text: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        match quote {
            Some(q) => {
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => quote = Some(c),
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    args.push(&text[start..i]);
                    start = i + 1;
                }
                _ => {}
            },
        }
    }
    if start < text.len() {
        args.push(&text[start..]);
    }
    args
}

fn is_valid_ident(s: &str) -> bool {
    let mut chars = s.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
}

fn extract_param_scopes(formula: &str) -> Vec<ParamScope> {
    let mut scopes = Vec::new();
    let chars: Vec<(usize, char)> = formula.char_indices().collect();
    let len = chars.len();
    for i in 0..len {
        let (_, c) = chars[i];
        if c.is_ascii_alphabetic() || c == '_' {
            let mut j = i;
            while j < len
                && (chars[j].1.is_ascii_alphanumeric() || chars[j].1 == '_' || chars[j].1 == '.')
            {
                j += 1;
            }
            if j < len && chars[j].1 == '(' {
                let name: String = chars[i..j].iter().map(|(_, ch)| *ch).collect();
                let upper = name.to_ascii_uppercase();
                if upper == "LET" || upper == "LAMBDA" {
                    let open_paren = chars[j].0;
                    let mut depth = 0i32;
                    let mut close_paren = None;
                    let mut quote: Option<char> = None;
                    for &(k_byte, k_char) in &chars[j..len] {
                        match quote {
                            Some(q) => {
                                if k_char == q {
                                    quote = None;
                                }
                            }
                            None => match k_char {
                                '"' | '\'' => quote = Some(k_char),
                                '(' => depth += 1,
                                ')' => {
                                    depth -= 1;
                                    if depth == 0 {
                                        close_paren = Some(k_byte);
                                        break;
                                    }
                                }
                                _ => {}
                            },
                        }
                    }
                    if let Some(close_byte) = close_paren {
                        let inner = &formula[open_paren + 1..close_byte];
                        let args = split_top_level_args(inner);
                        let mut params = Vec::new();
                        if upper == "LET" && args.len() >= 3 {
                            let end_param_idx = if args.len() % 2 == 1 {
                                args.len() - 1
                            } else {
                                args.len()
                            };
                            for idx in (0..end_param_idx).step_by(2) {
                                let arg_trimmed = args[idx].trim();
                                if is_valid_ident(arg_trimmed) {
                                    params.push(arg_trimmed.to_ascii_uppercase());
                                }
                            }
                        } else if upper == "LAMBDA" && args.len() >= 2 {
                            for arg in &args[..args.len() - 1] {
                                let arg_trimmed = arg.trim();
                                if is_valid_ident(arg_trimmed) {
                                    params.push(arg_trimmed.to_ascii_uppercase());
                                }
                            }
                        }
                        if !params.is_empty() {
                            scopes.push(ParamScope {
                                start: open_paren,
                                end: close_byte,
                                params,
                            });
                        }
                    }
                }
            }
        }
    }
    scopes
}

fn prefix_future_functions(formula: &str) -> String {
    let param_scopes = extract_param_scopes(formula);
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
        if character.is_ascii_alphabetic() || character == '_' {
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
            } else if !is_call {
                let byte_pos = formula
                    .char_indices()
                    .nth(start)
                    .map(|(b, _)| b)
                    .unwrap_or(start);
                let is_param = param_scopes.iter().any(|scope| {
                    byte_pos > scope.start && byte_pos < scope.end && scope.params.contains(&upper)
                });
                if is_param {
                    out.push_str("_xlpm.");
                }
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
    use crate::xlsx::formula_import::{loom_formula, DefinedNames};

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
            "XMATCH(A1,A1:A3)",
            "LET(X,1,X+1)",
            "TEXTBEFORE(A1,\"-\")",
            "TEXTAFTER(A1,\"-\")",
            "TEXTSPLIT(A1,\",\")",
            "TAKE(A1:A3,2)",
            "DROP(A1:A3,1)",
            "VSTACK(A1:A3,B1:B3)",
            "HSTACK(A1:A3,B1:B3)",
            "TOCOL(A1:A3)",
            "TOROW(A1:A3)",
            "WRAPROWS(A1:A3,2)",
            "WRAPCOLS(A1:A3,2)",
            "CHOOSECOLS(A1:B3,2)",
            "CHOOSEROWS(A1:B3,2)",
            "EXPAND(A1:A3,4)",
            "SORTBY(A1:A3,B1:B3)",
            "RANDARRAY(2)",
            "NETWORKDAYS.INTL(A1,B1,11)",
            "WORKDAY.INTL(A1,2,11)",
            "ISOWEEKNUM(A1)",
            "PERCENTRANK.INC(A1:A3,A1)",
            "PERCENTRANK.EXC(A1:A3,A1)",
            "PERCENTILE.EXC(A1:A3,0.5)",
            "QUARTILE.INC(A1:A3,1)",
            "QUARTILE.EXC(A1:A3,1)",
            "RANK.AVG(A1,A1:A3)",
            "FORECAST.LINEAR(A1,A1:A3,B1:B3)",
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

    #[test]
    fn let_and_lambda_parameters_are_prefixed_with_xlpm() {
        assert_eq!(
            excel_formula("=LET(x,5,x*2)", &[]),
            "_xlfn.LET(_xlpm.x,5,_xlpm.x*2)"
        );
        assert_eq!(
            excel_formula("=LET(x,1,y,x+1,x+y)", &[]),
            "_xlfn.LET(_xlpm.x,1,_xlpm.y,_xlpm.x+1,_xlpm.x+_xlpm.y)"
        );
        assert_eq!(
            excel_formula("=LAMBDA(x,y,x+y)", &[]),
            "_xlfn.LAMBDA(_xlpm.x,_xlpm.y,_xlpm.x+_xlpm.y)"
        );
        let names = DefinedNames::default();
        assert_eq!(
            loom_formula("_xlfn.LET(_xlpm.x,5,_xlpm.x*2)", &names),
            "LET(x,5,x*2)"
        );
        assert_eq!(
            loom_formula("_xlfn.LAMBDA(_xlpm.x,_xlpm.y,_xlpm.x+_xlpm.y)", &names),
            "LAMBDA(x,y,x+y)"
        );
    }
}
