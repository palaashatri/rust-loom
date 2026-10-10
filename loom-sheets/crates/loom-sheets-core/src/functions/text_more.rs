//! Text splitting (spike C): TEXTBEFORE, TEXTAFTER and TEXTSPLIT. Delimiters
//! match left to right without overlap; a negative instance counts from the
//! end; `match_mode` 1 ignores case.

use crate::functions_util::{arg_text, opt_number, spill, to_bool};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;

/// Dispatch the text-splitting functions from the main evaluator.
pub(crate) fn eval_text_more_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result = match name {
        "TEXTBEFORE" => text_around(args, lookup, true),
        "TEXTAFTER" => text_around(args, lookup, false),
        "TEXTSPLIT" => text_split(args, lookup),
        _ => return None,
    };
    Some(result.unwrap_or_else(Value::Error))
}

fn chars_of(args: &[Expr], index: usize, lookup: Lookup) -> Result<Vec<char>, CalcError> {
    Ok(arg_text(args, index, lookup)?.chars().collect())
}

/// Lower-case form of one character, used when matching ignores case.
fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn same_run(a: &[char], b: &[char], ignore_case: bool) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            if ignore_case {
                fold(*x) == fold(*y)
            } else {
                x == y
            }
        })
}

/// Non-overlapping occurrences of `needle` in `text` as (start, end) offsets.
fn occurrences(text: &[char], needle: &[char], ignore_case: bool) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut at = 0;
    while at + needle.len() <= text.len() {
        if same_run(&text[at..at + needle.len()], needle, ignore_case) {
            found.push((at, at + needle.len()));
            at += needle.len();
        } else {
            at += 1;
        }
    }
    found
}

/// The pieces of `text` between occurrences of `delimiter`. An empty
/// delimiter leaves the text whole.
fn pieces<'a>(text: &'a [char], delimiter: &[char], ignore_case: bool) -> Vec<&'a [char]> {
    if delimiter.is_empty() {
        return vec![text];
    }
    let mut out = Vec::new();
    let mut from = 0;
    for (start, end) in occurrences(text, delimiter, ignore_case) {
        out.push(&text[from..start]);
        from = end;
    }
    out.push(&text[from..]);
    out
}

/// TEXTBEFORE / TEXTAFTER(text, delimiter, [instance_num], [match_mode],
/// [match_end], [if_not_found]). A missing instance is `#N/A` unless
/// `if_not_found` is given. `match_end` treats the end of the text as a delimiter.
fn text_around(args: &[Expr], lookup: Lookup, before: bool) -> Result<Value, CalcError> {
    if !(2..=6).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let text = chars_of(args, 0, lookup)?;
    let needle = chars_of(args, 1, lookup)?;
    let instance = opt_number(args, 2, 1.0, lookup)?.trunc() as i64;
    let mode = opt_number(args, 3, 0.0, lookup)?.trunc() as i64;
    let match_end = match args.get(4) {
        Some(expr) => to_bool(eval_expr(expr, lookup))?,
        None => false,
    };
    if instance == 0 || !(0..=1).contains(&mode) {
        return Err(CalcError::Value);
    }
    let mut found = if needle.is_empty() {
        vec![(0, 0)]
    } else {
        occurrences(&text, &needle, mode == 1)
    };
    if match_end {
        found.push((text.len(), text.len()));
    }
    let count = found.len() as i64;
    let index = if instance > 0 {
        instance - 1
    } else {
        count + instance
    };
    match usize::try_from(index)
        .ok()
        .and_then(|i| found.get(i).copied())
    {
        Some((start, end)) => {
            let piece = if before { &text[..start] } else { &text[end..] };
            Ok(Value::Text(piece.iter().collect()))
        }
        None => match args.get(5) {
            Some(expr) => Ok(eval_expr(expr, lookup)),
            None => Err(CalcError::NA),
        },
    }
}

/// TEXTSPLIT(text, col_delimiter, [row_delimiter], [ignore_empty],
/// [match_mode], [pad_with]). Rows split first; a short row is padded with
/// `#N/A` or `pad_with`.
fn text_split(args: &[Expr], lookup: Lookup) -> Result<Value, CalcError> {
    if !(2..=6).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let text = chars_of(args, 0, lookup)?;
    let column_delimiter = chars_of(args, 1, lookup)?;
    let row_delimiter = match args.get(2) {
        Some(_) => chars_of(args, 2, lookup)?,
        None => Vec::new(),
    };
    let ignore_empty = match args.get(3) {
        Some(expr) => to_bool(eval_expr(expr, lookup))?,
        None => false,
    };
    let mode = opt_number(args, 4, 0.0, lookup)?.trunc() as i64;
    if !(0..=1).contains(&mode) {
        return Err(CalcError::Value);
    }
    let pad = match args.get(5) {
        Some(expr) => eval_expr(expr, lookup),
        None => Value::Error(CalcError::NA),
    };
    let ignore_case = mode == 1;

    let mut rows: Vec<Vec<String>> = Vec::new();
    for row_text in pieces(&text, &row_delimiter, ignore_case) {
        let mut cells: Vec<String> = pieces(row_text, &column_delimiter, ignore_case)
            .into_iter()
            .map(|piece| piece.iter().collect::<String>())
            .collect();
        if ignore_empty {
            cells.retain(|cell| !cell.is_empty());
            if cells.is_empty() {
                continue;
            }
        }
        rows.push(cells);
    }
    if rows.is_empty() {
        return Err(CalcError::Calc);
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let height = rows.len();
    let mut cells = Vec::with_capacity(width * height);
    for row in &rows {
        for col in 0..width {
            cells.push(match row.get(col) {
                Some(cell) => Value::Text(cell.clone()),
                None => pad.clone(),
            });
        }
    }
    Ok(spill(cells, height, width))
}
