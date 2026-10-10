//! XMATCH (spike C): the one-based position of a value in a one-dimensional
//! array. It shares XLOOKUP's match and search modes, so the two agree.

use crate::functions_lookup::find_lookup_index;
use crate::functions_util::{grid, opt_number};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;

/// Dispatch the lookup functions added in spike C.
pub(crate) fn eval_lookup_more_function(
    name: &str,
    args: &[Expr],
    lookup: Lookup,
) -> Option<Value> {
    match name {
        "XMATCH" => Some(match xmatch(args, lookup) {
            Ok(position) => Value::Number(position as f64),
            Err(error) => Value::Error(error),
        }),
        _ => None,
    }
}

/// XMATCH(lookup_value, lookup_array, [match_mode], [search_mode]).
/// match_mode: 0 exact (default), -1 exact or next smaller, 1 exact or next
/// larger, 2 wildcard. search_mode: 1 first to last (default), -1 last to first.
fn xmatch(args: &[Expr], lookup: Lookup) -> Result<usize, CalcError> {
    if !(2..=4).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let value = eval_expr(&args[0], lookup);
    if let Value::Error(error) = value {
        return Err(error);
    }
    let (cells, rows, cols) = grid(&args[1], lookup);
    if cells.is_empty() || (rows != 1 && cols != 1) {
        return Err(CalcError::Value);
    }
    let match_mode = opt_number(args, 2, 0.0, lookup)?.trunc() as i32;
    let search_mode = opt_number(args, 3, 1.0, lookup)?.trunc() as i32;
    if !(-1..=2).contains(&match_mode) || !matches!(search_mode, -2 | -1 | 1 | 2) {
        return Err(CalcError::Value);
    }
    find_lookup_index(&value, &cells, match_mode, search_mode)
        .map(|index| index + 1)
        .ok_or(CalcError::NA)
}
