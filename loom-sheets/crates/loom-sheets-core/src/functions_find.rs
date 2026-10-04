//! VLOOKUP, HLOOKUP, INDEX and MATCH with Excel's typed comparison rules:
//! exact matches are type-strict (the number 12 never equals the text "12"),
//! text matches ignore case and honour `?` `*` `~` wildcards, blanks never
//! match, and approximate matches scan sorted data for the last value that
//! does not exceed the lookup value.

use std::cmp::Ordering;

use crate::functions::wildcard_match;
use crate::functions_util::{clean, grid, to_bool, to_number};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;

/// Dispatch the classic lookup functions.
pub(crate) fn eval_find_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result = match name {
        "VLOOKUP" => vlookup(args, lookup, true),
        "HLOOKUP" => vlookup(args, lookup, false),
        "INDEX" => index(args, lookup),
        "MATCH" => match_function(args, lookup),
        _ => return None,
    };
    Some(result.unwrap_or_else(Value::Error))
}

/// The value searched for: blanks look up as 0, arrays as their first element.
fn needle(expr: &Expr, lookup: Lookup) -> Result<Value, CalcError> {
    match eval_expr(expr, lookup) {
        Value::Error(error) => Err(error),
        Value::Empty => Ok(Value::Number(0.0)),
        Value::Array(items, ..) => Ok(items.into_iter().next().unwrap_or(Value::Number(0.0))),
        other => Ok(other),
    }
}

/// Ordering of two values of the same kind; `None` for different kinds.
fn same_kind_order(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => clean(*a).partial_cmp(&clean(*b)),
        (Value::Text(a), Value::Text(b)) => Some(a.to_lowercase().cmp(&b.to_lowercase())),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

fn exact_equal(needle: &Value, candidate: &Value) -> bool {
    match (needle, candidate) {
        (Value::Text(pattern), Value::Text(text)) => {
            if pattern.contains(['*', '?', '~']) {
                wildcard_match(&pattern.to_lowercase(), &text.to_lowercase())
            } else {
                pattern.to_lowercase() == text.to_lowercase()
            }
        }
        _ => same_kind_order(needle, candidate) == Some(Ordering::Equal),
    }
}

/// Position of an exact match in `items`.
fn find_exact(needle: &Value, items: &[Value]) -> Option<usize> {
    items.iter().position(|item| exact_equal(needle, item))
}

/// Position of the last item not exceeding `needle` (`ascending`) or the last
/// item not below it (descending data), stopping at the first item past it.
fn find_approximate(needle: &Value, items: &[Value], ascending: bool) -> Option<usize> {
    let mut found = None;
    for (position, item) in items.iter().enumerate() {
        let Some(order) = same_kind_order(item, needle) else {
            continue;
        };
        let within = if ascending {
            order != Ordering::Greater
        } else {
            order != Ordering::Less
        };
        if !within {
            break;
        }
        found = Some(position);
    }
    found
}

fn range_lookup_flag(args: &[Expr], index: usize, lookup: Lookup) -> Result<bool, CalcError> {
    if index >= args.len() {
        return Ok(true);
    }
    to_bool(eval_expr(&args[index], lookup))
}

fn whole_number(args: &[Expr], index: usize, lookup: Lookup) -> Result<i64, CalcError> {
    Ok(to_number(eval_expr(&args[index], lookup))?.trunc() as i64)
}

/// VLOOKUP (`vertical`) and HLOOKUP: search the first column or row.
fn vlookup(args: &[Expr], lookup: Lookup, vertical: bool) -> Result<Value, CalcError> {
    if !(3..=4).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let value = needle(&args[0], lookup)?;
    let (cells, rows, cols) = grid(&args[1], lookup);
    let index = whole_number(args, 2, lookup)?;
    let exact = !range_lookup_flag(args, 3, lookup)?;
    let (lines, depth) = if vertical { (rows, cols) } else { (cols, rows) };
    if index < 1 {
        return Err(CalcError::Value);
    }
    if index as usize > depth {
        return Err(CalcError::Ref);
    }
    let at = |line: usize, offset: usize| {
        if vertical {
            cells[line * cols + offset].clone()
        } else {
            cells[offset * cols + line].clone()
        }
    };
    let keys: Vec<Value> = (0..lines).map(|line| at(line, 0)).collect();
    let hit = if exact {
        find_exact(&value, &keys)
    } else {
        find_approximate(&value, &keys, true)
    };
    hit.map(|line| at(line, index as usize - 1))
        .ok_or(CalcError::NA)
}

/// INDEX(array, row, [column]) including row/column 0 for a whole line.
fn index(args: &[Expr], lookup: Lookup) -> Result<Value, CalcError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let (cells, rows, cols) = grid(&args[0], lookup);
    let first = whole_number(args, 1, lookup)?;
    let (row, col) = if args.len() == 3 {
        (first, whole_number(args, 2, lookup)?)
    } else if rows == 1 {
        (1, first)
    } else if cols == 1 {
        (first, 1)
    } else {
        return Err(CalcError::Ref);
    };
    if row < 0 || col < 0 {
        return Err(CalcError::Value);
    }
    if row as usize > rows || col as usize > cols {
        return Err(CalcError::Ref);
    }
    let rows_wanted: Vec<usize> = if row == 0 {
        (0..rows).collect()
    } else {
        vec![row as usize - 1]
    };
    let cols_wanted: Vec<usize> = if col == 0 {
        (0..cols).collect()
    } else {
        vec![col as usize - 1]
    };
    let mut picked = Vec::with_capacity(rows_wanted.len() * cols_wanted.len());
    for r in &rows_wanted {
        for c in &cols_wanted {
            picked.push(cells[r * cols + c].clone());
        }
    }
    if picked.len() == 1 {
        Ok(picked.remove(0))
    } else {
        Ok(Value::Array(picked, rows_wanted.len(), cols_wanted.len()))
    }
}

/// MATCH(value, array, [type]): 1 ascending (default), 0 exact, -1 descending.
fn match_function(args: &[Expr], lookup: Lookup) -> Result<Value, CalcError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let value = needle(&args[0], lookup)?;
    let (items, rows, cols) = grid(&args[1], lookup);
    let kind = if args.len() == 3 {
        to_number(eval_expr(&args[2], lookup))?.trunc()
    } else {
        1.0
    };
    if rows != 1 && cols != 1 {
        return Err(CalcError::NA);
    }
    let hit = if kind == 0.0 {
        find_exact(&value, &items)
    } else {
        find_approximate(&value, &items, kind > 0.0)
    };
    hit.map(|position| Value::Number(position as f64 + 1.0))
        .ok_or(CalcError::NA)
}
