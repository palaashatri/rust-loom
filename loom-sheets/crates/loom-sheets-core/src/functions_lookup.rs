//! Lookup functions: XLOOKUP, ROWS, COLUMNS.

use crate::functions::wildcard_match;
use crate::functions_logic::values_equal;
use crate::functions_util::to_number;
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

/// Dispatch lookup functions from the main evaluator.
pub(crate) fn eval_lookup_function(
    name: &str,
    raw_args: &[Expr],
    lookup: &dyn Fn(CellRef) -> Value,
) -> Option<Value> {
    match name {
        "XLOOKUP" => eval_xlookup(raw_args, lookup),
        "ROWS" => eval_rows(raw_args, lookup),
        "COLUMNS" => eval_columns(raw_args, lookup),
        _ => None,
    }
}

/// XLOOKUP: search for a value in an array and return a corresponding value.
/// Syntax: XLOOKUP(lookup_value, lookup_array, return_array, [if_not_found], [match_mode], [search_mode])
/// match_mode: 0=exact (default), -1=exact or next smaller, 1=exact or next larger, 2=wildcard
/// search_mode: 1=search first-to-last (default), -1=search last-to-first
fn eval_xlookup(raw_args: &[Expr], lookup: &dyn Fn(CellRef) -> Value) -> Option<Value> {
    if raw_args.len() < 3 || raw_args.len() > 6 {
        return Some(Value::Error(CalcError::Value));
    }

    let lookup_value = eval_expr(&raw_args[0], lookup);
    if let Value::Error(error) = lookup_value {
        return Some(Value::Error(error));
    }
    let lookup_array = collect_array_values(&raw_args[1], lookup);
    let return_array = collect_array_values(&raw_args[2], lookup);

    if lookup_array.is_empty() || return_array.is_empty() {
        return Some(Value::Error(CalcError::Value));
    }

    let if_not_found = if raw_args.len() >= 4 {
        Some(eval_expr(&raw_args[3], lookup))
    } else {
        None
    };

    let match_mode = if raw_args.len() >= 5 {
        match to_number(eval_expr(&raw_args[4], lookup)) {
            Ok(n) => n as i32,
            Err(error) => return Some(Value::Error(error)),
        }
    } else {
        0
    };

    let search_mode = if raw_args.len() >= 6 {
        match to_number(eval_expr(&raw_args[5], lookup)) {
            Ok(n) => n as i32,
            Err(error) => return Some(Value::Error(error)),
        }
    } else {
        1
    };

    // Find the matching index in lookup_array
    let index = find_lookup_index(&lookup_value, &lookup_array, match_mode, search_mode);

    match index {
        Some(idx) => {
            if idx < return_array.len() {
                Some(return_array[idx].clone())
            } else {
                Some(Value::Error(CalcError::Value))
            }
        }
        None => if_not_found.or(Some(Value::Error(CalcError::NA))),
    }
}

/// ROWS: count the number of rows in an array or range.
fn eval_rows(raw_args: &[Expr], lookup: &dyn Fn(CellRef) -> Value) -> Option<Value> {
    if raw_args.len() != 1 {
        return Some(Value::Error(CalcError::Value));
    }

    match &raw_args[0] {
        Expr::Range { start, end } => {
            let rows = (start.row.max(end.row) - start.row.min(end.row) + 1) as i32;
            Some(Value::Number(rows as f64))
        }
        Expr::Cell(_) => Some(Value::Number(1.0)),
        _ => {
            let val = eval_expr(&raw_args[0], lookup);
            match val {
                Value::Array(_, rows, _) => Some(Value::Number(rows as f64)),
                Value::Empty => Some(Value::Number(1.0)),
                _ => Some(Value::Number(1.0)),
            }
        }
    }
}

/// COLUMNS: count the number of columns in an array or range.
fn eval_columns(raw_args: &[Expr], lookup: &dyn Fn(CellRef) -> Value) -> Option<Value> {
    if raw_args.len() != 1 {
        return Some(Value::Error(CalcError::Value));
    }

    match &raw_args[0] {
        Expr::Range { start, end } => {
            let cols = (start.col.max(end.col) - start.col.min(end.col) + 1) as i32;
            Some(Value::Number(cols as f64))
        }
        Expr::Cell(_) => Some(Value::Number(1.0)),
        _ => {
            let val = eval_expr(&raw_args[0], lookup);
            match val {
                Value::Array(_, _, cols) => Some(Value::Number(cols as f64)),
                Value::Empty => Some(Value::Number(1.0)),
                _ => Some(Value::Number(1.0)),
            }
        }
    }
}

/// Collect values from a range or array expression into a flat vector.
fn collect_array_values(expr: &Expr, lookup: &dyn Fn(CellRef) -> Value) -> Vec<Value> {
    let mut values = Vec::new();
    match expr {
        Expr::Range { start, end } => {
            for row in start.row.min(end.row)..=start.row.max(end.row) {
                for col in start.col.min(end.col)..=start.col.max(end.col) {
                    let cell = CellRef { row, col };
                    values.push(lookup(cell));
                }
            }
        }
        _ => {
            let val = eval_expr(expr, lookup);
            push_flattened(val, &mut values);
        }
    }
    values
}

/// Flatten array values recursively.
fn push_flattened(value: Value, out: &mut Vec<Value>) {
    match value {
        Value::Array(items, _, _) => {
            for item in items {
                push_flattened(item, out);
            }
        }
        scalar => out.push(scalar),
    }
}

/// Find the index of a lookup value in an array according to match and search modes.
fn find_lookup_index(
    lookup_value: &Value,
    array: &[Value],
    match_mode: i32,
    search_mode: i32,
) -> Option<usize> {
    match match_mode {
        0 => find_exact_match(lookup_value, array, search_mode),
        -1 => find_next_smaller(lookup_value, array, search_mode),
        1 => find_next_larger(lookup_value, array, search_mode),
        2 => find_wildcard_match(lookup_value, array, search_mode),
        _ => None,
    }
}

/// Find exact match (match_mode = 0).
fn find_exact_match(lookup_value: &Value, array: &[Value], search_mode: i32) -> Option<usize> {
    if search_mode < 0 {
        // Search last to first
        for (i, val) in array.iter().enumerate().rev() {
            if values_equal(lookup_value, val) {
                return Some(i);
            }
        }
    } else {
        // Search first to last
        for (i, val) in array.iter().enumerate() {
            if values_equal(lookup_value, val) {
                return Some(i);
            }
        }
    }
    None
}

/// Find exact match or next smaller (match_mode = -1).
fn find_next_smaller(lookup_value: &Value, array: &[Value], search_mode: i32) -> Option<usize> {
    let num_val = match lookup_value {
        Value::Number(n) => *n,
        _ => return None,
    };

    let mut best_idx = None;
    let mut best_val = f64::NEG_INFINITY;

    let iter: Box<dyn Iterator<Item = (usize, &Value)>> = if search_mode < 0 {
        Box::new(array.iter().enumerate().rev())
    } else {
        Box::new(array.iter().enumerate())
    };

    for (i, val) in iter {
        if let Value::Number(n) = val {
            if *n <= num_val && *n > best_val {
                best_val = *n;
                best_idx = Some(i);
                if (*n - num_val).abs() < 1e-10 {
                    return best_idx;
                }
            }
        }
    }
    best_idx
}

/// Find exact match or next larger (match_mode = 1).
fn find_next_larger(lookup_value: &Value, array: &[Value], search_mode: i32) -> Option<usize> {
    let num_val = match lookup_value {
        Value::Number(n) => *n,
        _ => return None,
    };

    let mut best_idx = None;
    let mut best_val = f64::INFINITY;

    let iter: Box<dyn Iterator<Item = (usize, &Value)>> = if search_mode < 0 {
        Box::new(array.iter().enumerate().rev())
    } else {
        Box::new(array.iter().enumerate())
    };

    for (i, val) in iter {
        if let Value::Number(n) = val {
            if *n >= num_val && *n < best_val {
                best_val = *n;
                best_idx = Some(i);
                if (*n - num_val).abs() < 1e-10 {
                    return best_idx;
                }
            }
        }
    }
    best_idx
}

/// Find wildcard match (match_mode = 2).
fn find_wildcard_match(lookup_value: &Value, array: &[Value], search_mode: i32) -> Option<usize> {
    let pattern = match lookup_value {
        Value::Text(s) => s,
        _ => return None,
    };

    if search_mode < 0 {
        // Search last to first
        for (i, val) in array.iter().enumerate().rev() {
            if let Value::Text(s) = val {
                if wildcard_match(pattern, s) {
                    return Some(i);
                }
            }
        }
    } else {
        // Search first to last
        for (i, val) in array.iter().enumerate() {
            if let Value::Text(s) = val {
                if wildcard_match(pattern, s) {
                    return Some(i);
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{evaluate, Sheet};

    fn eval(formula: &str) -> Value {
        let mut sheet = Sheet::new("t");
        sheet.set_str("Z1", formula);
        evaluate(&sheet)
            .get(&CellRef::parse("Z1").unwrap())
            .cloned()
            .unwrap_or(Value::Empty)
    }

    fn num(formula: &str) -> f64 {
        match eval(formula) {
            Value::Number(n) => n,
            other => panic!("{formula} -> {other:?}"),
        }
    }

    #[test]
    fn rows_counts_range_height() {
        assert_eq!(num("=ROWS(A1:A5)"), 5.0);
        assert_eq!(num("=ROWS(A1:C3)"), 3.0);
        assert_eq!(num("=ROWS(A1)"), 1.0);
    }

    #[test]
    fn columns_counts_range_width() {
        assert_eq!(num("=COLUMNS(A1:A5)"), 1.0);
        assert_eq!(num("=COLUMNS(A1:C3)"), 3.0);
        assert_eq!(num("=COLUMNS(A1)"), 1.0);
    }

    #[test]
    fn xlookup_finds_exact_match() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "1");
        sheet.set_str("A2", "2");
        sheet.set_str("A3", "3");
        sheet.set_str("B1", "One");
        sheet.set_str("B2", "Two");
        sheet.set_str("B3", "Three");
        sheet.set_str("C1", "=XLOOKUP(2, A1:A3, B1:B3)");
        let result = evaluate(&sheet)
            .get(&CellRef::parse("C1").unwrap())
            .cloned();
        match result {
            Some(Value::Text(s)) => assert_eq!(s, "Two"),
            other => panic!("Expected 'Two', got {other:?}"),
        }
    }

    #[test]
    fn xlookup_returns_if_not_found_when_provided() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "1");
        sheet.set_str("A2", "2");
        sheet.set_str("B1", "One");
        sheet.set_str("B2", "Two");
        sheet.set_str("C1", "=XLOOKUP(3, A1:A2, B1:B2, \"Not Found\")");
        let result = evaluate(&sheet)
            .get(&CellRef::parse("C1").unwrap())
            .cloned();
        match result {
            Some(Value::Text(s)) => assert_eq!(s, "Not Found"),
            other => panic!("Expected 'Not Found', got {other:?}"),
        }
    }

    #[test]
    fn xlookup_returns_na_when_not_found() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "1");
        sheet.set_str("A2", "2");
        sheet.set_str("B1", "One");
        sheet.set_str("B2", "Two");
        sheet.set_str("C1", "=XLOOKUP(3, A1:A2, B1:B2)");
        let result = evaluate(&sheet)
            .get(&CellRef::parse("C1").unwrap())
            .cloned();
        match result {
            Some(Value::Error(CalcError::NA)) => {}
            other => panic!("Expected #N/A, got {other:?}"),
        }
    }

    #[test]
    fn rows_and_columns_of_a_spilled_array_are_not_swapped() {
        assert_eq!(num("=ROWS(SEQUENCE(3,2))"), 3.0);
        assert_eq!(num("=COLUMNS(SEQUENCE(3,2))"), 2.0);
    }

    #[test]
    fn xlookup_matches_text_without_case_and_with_wildcards() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "Apple");
        sheet.set_str("A2", "Banana");
        sheet.set_str("B1", "1");
        sheet.set_str("B2", "2");
        sheet.set_str("C1", "=XLOOKUP(\"banana\",A1:A2,B1:B2)");
        sheet.set_str("C2", "=XLOOKUP(\"b*\",A1:A2,B1:B2,0,2)");
        sheet.set_str("C3", "=XLOOKUP(1/0,A1:A2,B1:B2)");
        let values = evaluate(&sheet);
        let at = |a: &str| values.get(&CellRef::parse(a).unwrap()).cloned();
        assert_eq!(at("C1"), Some(Value::Number(2.0)));
        assert_eq!(at("C2"), Some(Value::Number(2.0)));
        assert_eq!(at("C3"), Some(Value::Error(CalcError::DivZero)));
    }
}
