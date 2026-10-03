//! Text functions: SUBSTITUTE, REPLACE, FIND, SEARCH, PROPER, REPT, EXACT,
//! CHAR, CODE, VALUE, TEXT, CLEAN. Positions are counted in characters.

use crate::functions_date::{parse_date_text, parse_time_text};
use crate::functions_format::{format_number, format_text};
use crate::functions_util::{arg_number, arg_text, opt_number, to_number};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;
type Text = Result<String, CalcError>;

/// Excel's longest allowed cell text.
const MAX_TEXT_CHARS: usize = 32_767;

/// Dispatch text functions from the main evaluator.
pub(crate) fn eval_text_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result: Result<Value, CalcError> = match name {
        "SUBSTITUTE" => substitute(args, lookup).map(Value::Text),
        "REPLACE" => replace(args, lookup).map(Value::Text),
        "FIND" => find(args, lookup, false).map(Value::Number),
        "SEARCH" => find(args, lookup, true).map(Value::Number),
        "PROPER" => one_text(args, lookup, proper).map(Value::Text),
        "REPT" => rept(args, lookup).map(Value::Text),
        "EXACT" => exact(args, lookup).map(Value::Bool),
        "CHAR" => char_of(args, lookup).map(Value::Text),
        "CODE" => code_of(args, lookup).map(Value::Number),
        "VALUE" => value_of(args, lookup).map(Value::Number),
        "TEXT" => text(args, lookup).map(Value::Text),
        "CLEAN" => one_text(args, lookup, |t| {
            t.chars().filter(|c| u32::from(*c) >= 32).collect()
        })
        .map(Value::Text),
        _ => return None,
    };
    Some(result.unwrap_or_else(Value::Error))
}

fn one_text(args: &[Expr], lookup: Lookup, f: impl Fn(&str) -> String) -> Text {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    Ok(f(&arg_text(args, 0, lookup)?))
}

/// SUBSTITUTE(text, old, new, [instance]): case-sensitive.
fn substitute(args: &[Expr], lookup: Lookup) -> Text {
    if !(3..=4).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let text = arg_text(args, 0, lookup)?;
    let old = arg_text(args, 1, lookup)?;
    let new = arg_text(args, 2, lookup)?;
    let instance = if args.len() == 4 {
        let n = arg_number(args, 3, lookup)?.trunc();
        if n < 1.0 {
            return Err(CalcError::Value);
        }
        Some(n as usize)
    } else {
        None
    };
    if old.is_empty() {
        return Ok(text);
    }
    let Some(wanted) = instance else {
        return Ok(text.replace(&old, &new));
    };
    let mut out = String::new();
    let mut last = 0;
    for (count, (at, _)) in text.match_indices(&old).enumerate() {
        if count + 1 == wanted {
            out.push_str(&text[last..at]);
            out.push_str(&new);
            last = at + old.len();
            break;
        }
    }
    out.push_str(&text[last..]);
    Ok(out)
}

/// REPLACE(old_text, start, count, new_text).
fn replace(args: &[Expr], lookup: Lookup) -> Text {
    if args.len() != 4 {
        return Err(CalcError::Value);
    }
    let text: Vec<char> = arg_text(args, 0, lookup)?.chars().collect();
    let start = arg_number(args, 1, lookup)?.trunc();
    let count = arg_number(args, 2, lookup)?.trunc();
    let new = arg_text(args, 3, lookup)?;
    if start < 1.0 || count < 0.0 {
        return Err(CalcError::Value);
    }
    let start = (start as usize - 1).min(text.len());
    let end = (start + count as usize).min(text.len());
    let mut out: String = text[..start].iter().collect();
    out.push_str(&new);
    out.extend(&text[end..]);
    Ok(out)
}

/// FIND (exact) and SEARCH (case-insensitive, `?` `*` `~` wildcards): the
/// 1-based character position of the first match at or after `start`.
fn find(args: &[Expr], lookup: Lookup, search: bool) -> Result<f64, CalcError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let needle = arg_text(args, 0, lookup)?;
    let haystack: Vec<char> = arg_text(args, 1, lookup)?.chars().collect();
    let start = opt_number(args, 2, 1.0, lookup)?.trunc();
    if start < 1.0 || start as usize > haystack.len().max(1) {
        return Err(CalcError::Value);
    }
    let from = start as usize - 1;
    let pattern = if search {
        pattern_of(&needle)
    } else {
        needle.chars().map(Piece::Literal).collect()
    };
    (from..=haystack.len())
        .find(|at| matches_prefix(&pattern, &haystack[*at..], search))
        .map(|at| at as f64 + 1.0)
        .ok_or(CalcError::Value)
}

#[derive(Debug, Clone, Copy)]
enum Piece {
    Literal(char),
    AnyOne,
    AnyRun,
}

fn pattern_of(text: &str) -> Vec<Piece> {
    let mut pieces = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        pieces.push(match c {
            '?' => Piece::AnyOne,
            '*' => Piece::AnyRun,
            '~' => chars.next().map_or(Piece::Literal('~'), Piece::Literal),
            other => Piece::Literal(other),
        });
    }
    pieces
}

fn same_char(a: char, b: char, ignore_case: bool) -> bool {
    a == b || (ignore_case && a.to_lowercase().eq(b.to_lowercase()))
}

/// Whether `pattern` matches the start of `text` (not necessarily all of it).
fn matches_prefix(pattern: &[Piece], text: &[char], ignore_case: bool) -> bool {
    match pattern.split_first() {
        None => true,
        Some((Piece::AnyRun, rest)) => {
            (0..=text.len()).any(|skip| matches_prefix(rest, &text[skip..], ignore_case))
        }
        Some((Piece::AnyOne, rest)) => {
            !text.is_empty() && matches_prefix(rest, &text[1..], ignore_case)
        }
        Some((Piece::Literal(c), rest)) => {
            text.first().is_some_and(|t| same_char(*c, *t, ignore_case))
                && matches_prefix(rest, &text[1..], ignore_case)
        }
    }
}

/// PROPER: a letter is capitalised when the previous character is not a letter.
fn proper(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_letter = false;
    for c in text.chars() {
        if c.is_alphabetic() {
            if after_letter {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
            }
            after_letter = true;
        } else {
            out.push(c);
            after_letter = false;
        }
    }
    out
}

fn rept(args: &[Expr], lookup: Lookup) -> Text {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let text = arg_text(args, 0, lookup)?;
    let times = arg_number(args, 1, lookup)?.trunc();
    if times < 0.0 || text.chars().count() as f64 * times > MAX_TEXT_CHARS as f64 {
        return Err(CalcError::Value);
    }
    Ok(text.repeat(times as usize))
}

fn exact(args: &[Expr], lookup: Lookup) -> Result<bool, CalcError> {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    Ok(arg_text(args, 0, lookup)? == arg_text(args, 1, lookup)?)
}

/// Windows-1252 characters for codes 128..=159 (0 where undefined).
const CP1252_HIGH: [char; 32] = [
    '€', '\0', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\0', 'Ž', '\0', '\0', '‘',
    '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\0', 'ž', 'Ÿ',
];

/// CHAR(1..=255) in the Windows-1252 character set.
fn char_of(args: &[Expr], lookup: Lookup) -> Text {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    let code = arg_number(args, 0, lookup)?.trunc();
    if !(1.0..=255.0).contains(&code) {
        return Err(CalcError::Value);
    }
    let code = code as u32;
    let c = match code {
        128..=159 => CP1252_HIGH[code as usize - 128],
        other => char::from_u32(other).unwrap_or('\0'),
    };
    if c == '\0' {
        return Err(CalcError::Value);
    }
    Ok(c.to_string())
}

/// CODE: the Windows-1252 code of the first character (`?` = 63 otherwise).
fn code_of(args: &[Expr], lookup: Lookup) -> Result<f64, CalcError> {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    let first = arg_text(args, 0, lookup)?
        .chars()
        .next()
        .ok_or(CalcError::Value)?;
    let code = match CP1252_HIGH.iter().position(|c| *c == first && *c != '\0') {
        Some(offset) => 128 + offset as u32,
        None if u32::from(first) < 256 => u32::from(first),
        None => 63,
    };
    Ok(f64::from(code))
}

/// VALUE: numbers, numeric text with `$`, `,` grouping or `%`, and date/time text.
fn value_of(args: &[Expr], lookup: Lookup) -> Result<f64, CalcError> {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    match eval_expr(&args[0], lookup) {
        Value::Text(text) => parse_numeric_text(&text),
        Value::Bool(_) => Err(CalcError::Value),
        other => to_number(other),
    }
}

fn parse_numeric_text(text: &str) -> Result<f64, CalcError> {
    let trimmed = text.trim();
    let (negative, body) = match trimmed.strip_prefix('(').and_then(|t| t.strip_suffix(')')) {
        Some(inner) => (true, inner),
        None => (false, trimmed),
    };
    let (percent, body) = match body.strip_suffix('%') {
        Some(rest) => (true, rest),
        None => (false, body),
    };
    let unsigned = body.trim_start_matches(['$', '-', '+']);
    let sign_negative = body.starts_with('-') || negative;
    let digits: String = unsigned.replace(',', "");
    let grouped_ok = !unsigned.contains(',') || valid_grouping(unsigned);
    if grouped_ok && !digits.is_empty() {
        if let Ok(n) = digits.parse::<f64>() {
            if n.is_finite()
                && !digits
                    .chars()
                    .any(|c| c.is_alphabetic() && c != 'e' && c != 'E')
            {
                let n = if percent { n / 100.0 } else { n };
                return Ok(if sign_negative { -n } else { n });
            }
        }
    }
    if trimmed.is_empty() {
        return Ok(0.0);
    }
    match (parse_date_text(trimmed), parse_time_text(trimmed)) {
        (Some(date), _) => Ok(date),
        (None, Some(time)) => Ok(time),
        _ => Err(CalcError::Value),
    }
}

/// `1,234,567.5` style grouping: groups of three after the first.
fn valid_grouping(text: &str) -> bool {
    let integer = text.split('.').next().unwrap_or("");
    let mut groups = integer.split(',');
    let first = groups.next().unwrap_or("");
    (1..=3).contains(&first.len()) && groups.all(|g| g.len() == 3)
}

/// TEXT(value, format_text).
fn text(args: &[Expr], lookup: Lookup) -> Text {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let code = arg_text(args, 1, lookup)?;
    match eval_expr(&args[0], lookup) {
        Value::Error(error) => Err(error),
        Value::Bool(b) => Ok(if b { "TRUE" } else { "FALSE" }.to_string()),
        Value::Text(text) => match parse_numeric_text(&text) {
            Ok(n) if !text.trim().is_empty() => format_number(n, &code),
            _ => Ok(format_text(&text, &code)),
        },
        other => format_number(to_number(other)?, &code),
    }
}

#[cfg(test)]
mod tests {
    use crate::{evaluate, CalcError, CellRef, Sheet, Value};

    fn eval(formula: &str) -> Value {
        let mut sheet = Sheet::new("t");
        sheet.set_str("Z1", formula);
        evaluate(&sheet)
            .get(&CellRef::parse("Z1").unwrap())
            .cloned()
            .unwrap_or(Value::Empty)
    }

    fn text(formula: &str) -> String {
        match eval(formula) {
            Value::Text(s) => s,
            other => panic!("{formula} -> {other:?}"),
        }
    }

    fn num(formula: &str) -> f64 {
        match eval(formula) {
            Value::Number(n) => n,
            other => panic!("{formula} -> {other:?}"),
        }
    }

    #[test]
    fn substitute_replaces_all_or_one_instance() {
        assert_eq!(text("=SUBSTITUTE(\"a-b-c\",\"-\",\"+\")"), "a+b+c");
        assert_eq!(text("=SUBSTITUTE(\"a-b-c\",\"-\",\"+\",2)"), "a-b+c");
        assert_eq!(text("=SUBSTITUTE(\"a-b\",\"-\",\"+\",5)"), "a-b");
        assert_eq!(text("=SUBSTITUTE(\"Aa\",\"a\",\"x\")"), "Ax");
        assert_eq!(
            eval("=SUBSTITUTE(\"a\",\"a\",\"b\",0)"),
            Value::Error(CalcError::Value)
        );
    }

    #[test]
    fn replace_counts_characters_not_bytes() {
        assert_eq!(text("=REPLACE(\"abcdef\",2,3,\"X\")"), "aXef");
        assert_eq!(text("=REPLACE(\"héllo\",2,1,\"e\")"), "hello");
        assert_eq!(text("=REPLACE(\"abc\",10,2,\"Z\")"), "abcZ");
        assert_eq!(
            eval("=REPLACE(\"abc\",0,1,\"Z\")"),
            Value::Error(CalcError::Value)
        );
    }

    #[test]
    fn find_is_exact_and_positions_are_characters() {
        assert_eq!(num("=FIND(\"l\",\"héllo\")"), 3.0);
        assert_eq!(num("=FIND(\"l\",\"héllo\",4)"), 4.0);
        assert_eq!(
            eval("=FIND(\"L\",\"hello\")"),
            Value::Error(CalcError::Value)
        );
        assert_eq!(num("=FIND(\"\",\"abc\")"), 1.0);
        assert_eq!(
            eval("=FIND(\"a\",\"abc\",9)"),
            Value::Error(CalcError::Value)
        );
    }

    #[test]
    fn search_ignores_case_and_supports_wildcards() {
        assert_eq!(num("=SEARCH(\"L\",\"héllo\")"), 3.0);
        assert_eq!(num("=SEARCH(\"h?l\",\"héllo\")"), 1.0);
        assert_eq!(num("=SEARCH(\"e*o\",\"hello\")"), 2.0);
        assert_eq!(num("=SEARCH(\"~?\",\"what?\")"), 5.0);
        assert_eq!(
            eval("=SEARCH(\"z\",\"hello\")"),
            Value::Error(CalcError::Value)
        );
    }

    #[test]
    fn proper_capitalises_after_any_non_letter() {
        assert_eq!(text("=PROPER(\"hello wORLD\")"), "Hello World");
        assert_eq!(text("=PROPER(\"a2b o'neil\")"), "A2B O'Neil");
    }

    #[test]
    fn rept_exact_char_code_and_clean() {
        assert_eq!(text("=REPT(\"ab\",3)"), "ababab");
        assert_eq!(eval("=REPT(\"a\",-1)"), Value::Error(CalcError::Value));
        assert_eq!(eval("=REPT(\"abc\",20000)"), Value::Error(CalcError::Value));
        assert_eq!(eval("=EXACT(\"a\",\"A\")"), Value::Bool(false));
        assert_eq!(eval("=EXACT(\"a\",\"a\")"), Value::Bool(true));
        assert_eq!(text("=CHAR(65)"), "A");
        assert_eq!(text("=CHAR(128)"), "€");
        assert_eq!(eval("=CHAR(0)"), Value::Error(CalcError::Value));
        assert_eq!(num("=CODE(\"A\")"), 65.0);
        assert_eq!(num("=CODE(\"€\")"), 128.0);
        assert_eq!(text("=CLEAN(\"a\"&CHAR(9)&\"b\")"), "ab");
    }

    #[test]
    fn value_reads_formatted_and_date_text() {
        assert_eq!(num("=VALUE(\"1,234.5\")"), 1234.5);
        assert_eq!(num("=VALUE(\"$1,000\")"), 1000.0);
        assert_eq!(num("=VALUE(\"50%\")"), 0.5);
        assert_eq!(num("=VALUE(\" 12 \")"), 12.0);
        assert_eq!(num("=VALUE(\"2024-01-01\")"), 45292.0);
        assert_eq!(num("=VALUE(\"12:00\")"), 0.5);
        assert_eq!(eval("=VALUE(\"abc\")"), Value::Error(CalcError::Value));
        assert_eq!(eval("=VALUE(\"inf\")"), Value::Error(CalcError::Value));
        assert_eq!(eval("=VALUE(\"1,23\")"), Value::Error(CalcError::Value));
    }

    #[test]
    fn text_formats_numbers_dates_and_passes_text_through() {
        assert_eq!(text("=TEXT(3.14159,\"0.00\")"), "3.14");
        assert_eq!(text("=TEXT(-1234.5,\"#,##0.00\")"), "-1,234.50");
        assert_eq!(text("=TEXT(0.256,\"0%\")"), "26%");
        assert_eq!(text("=TEXT(45292,\"yyyy-mm-dd\")"), "2024-01-01");
        assert_eq!(text("=TEXT(45292,\"dddd\")"), "Monday");
        assert_eq!(text("=TEXT(\"12\",\"0.0\")"), "12.0");
        assert_eq!(text("=TEXT(\"abc\",\"0.0\")"), "abc");
        assert_eq!(text("=TEXT(5,\"0\"&\" \"\"units\"\"\")"), "5 units");
        assert_eq!(eval("=TEXT(1/0,\"0\")"), Value::Error(CalcError::DivZero));
    }

    #[test]
    fn errors_propagate_through_text_functions() {
        assert_eq!(eval("=PROPER(1/0)"), Value::Error(CalcError::DivZero));
        assert_eq!(eval("=REPT(NA(),2)"), Value::Error(CalcError::NA));
    }
}
