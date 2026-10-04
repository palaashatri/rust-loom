//! Logic functions: IFS, SWITCH, XOR, IFNA, ISBLANK, ISNUMBER, ISTEXT,
//! ISERROR, ISNA, ISLOGICAL, CHOOSE, NA.

use crate::functions_util::{arg_number, flatten, grid, to_bool};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;

/// Dispatch logic functions from the main evaluator.
pub(crate) fn eval_logic_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    Some(match name {
        "IF" => if_function(args, lookup),
        "AND" => all_or_any(args, lookup, true),
        "OR" => all_or_any(args, lookup, false),
        "NOT" if args.len() == 1 => match to_bool(eval_expr(&args[0], lookup)) {
            Ok(b) => Value::Bool(!b),
            Err(failure) => error(failure),
        },
        "TRUE" if args.is_empty() => Value::Bool(true),
        "FALSE" if args.is_empty() => Value::Bool(false),
        "IFS" => ifs(args, lookup),
        "SWITCH" => switch(args, lookup),
        "XOR" => xor(args, lookup),
        "IFNA" => ifna(args, lookup),
        "CHOOSE" => choose(args, lookup),
        "NA" if args.is_empty() => error(CalcError::NA),
        "ISBLANK" => is_kind(args, lookup, |v| matches!(v, Value::Empty)),
        "ISNUMBER" => is_kind(args, lookup, |v| matches!(v, Value::Number(_))),
        "ISTEXT" => is_kind(args, lookup, |v| matches!(v, Value::Text(_))),
        "ISERROR" => is_kind(args, lookup, |v| matches!(v, Value::Error(_))),
        "ISNA" => is_kind(args, lookup, |v| matches!(v, Value::Error(CalcError::NA))),
        "ISLOGICAL" => is_kind(args, lookup, |v| matches!(v, Value::Bool(_))),
        _ => return None,
    })
}

fn error(error: CalcError) -> Value {
    Value::Error(error)
}

/// IF(condition, then, [else]): only the taken branch is evaluated; a text
/// condition must read `TRUE` or `FALSE`, anything else is `#VALUE!`.
fn if_function(args: &[Expr], lookup: Lookup) -> Value {
    if !(2..=3).contains(&args.len()) {
        return error(CalcError::Value);
    }
    let (conditions, rows, cols) = grid(&args[0], lookup);
    if conditions.len() > 1 {
        return if_over_array(args, lookup, conditions, rows, cols);
    }
    match to_bool(conditions.into_iter().next().unwrap_or(Value::Empty)) {
        Ok(true) => eval_expr(&args[1], lookup),
        Ok(false) if args.len() == 3 => eval_expr(&args[2], lookup),
        Ok(false) => Value::Bool(false),
        Err(failure) => error(failure),
    }
}

/// IF over an array of conditions: each element picks from the matching
/// element of the branches (a one-cell branch repeats).
fn if_over_array(
    args: &[Expr],
    lookup: Lookup,
    conditions: Vec<Value>,
    rows: usize,
    cols: usize,
) -> Value {
    let branch = |index: usize| match args.get(index) {
        Some(expr) => grid(expr, lookup),
        None => (vec![Value::Bool(false)], 1, 1),
    };
    let (then_items, then_rows, then_cols) = branch(1);
    let (else_items, else_rows, else_cols) = branch(2);
    let pick = |items: &[Value], item_rows: usize, item_cols: usize, at: usize| {
        let (r, c) = (at / cols, at % cols);
        let (r, c) = (
            if item_rows == 1 { 0 } else { r },
            if item_cols == 1 { 0 } else { c },
        );
        if r < item_rows && c < item_cols {
            items[r * item_cols + c].clone()
        } else {
            Value::Error(CalcError::NA)
        }
    };
    let out = conditions
        .into_iter()
        .enumerate()
        .map(|(at, condition)| match to_bool(condition) {
            Ok(true) => pick(&then_items, then_rows, then_cols, at),
            Ok(false) => pick(&else_items, else_rows, else_cols, at),
            Err(failure) => error(failure),
        })
        .collect();
    Value::Array(out, rows, cols)
}

/// AND (`all`) and OR over every logical value in the arguments. Text and
/// blanks are ignored (`TRUE`/`FALSE` text counts), every argument is
/// evaluated so an error anywhere wins, and no logical value is `#VALUE!`.
fn all_or_any(args: &[Expr], lookup: Lookup, all: bool) -> Value {
    if args.is_empty() {
        return error(CalcError::Value);
    }
    let mut result = all;
    let mut counted = false;
    for arg in args {
        for value in flatten(arg, lookup) {
            let truth = match value {
                Value::Bool(b) => b,
                Value::Number(n) => n != 0.0,
                Value::Error(failure) => return error(failure),
                Value::Text(text) => match to_bool(Value::Text(text)) {
                    Ok(b) => b,
                    Err(_) => continue,
                },
                _ => continue,
            };
            counted = true;
            result = if all {
                result && truth
            } else {
                result || truth
            };
        }
    }
    if counted {
        Value::Bool(result)
    } else {
        error(CalcError::Value)
    }
}

/// IFS(condition1, value1, ...): the value after the first true condition;
/// later pairs are not evaluated. No true condition is `#N/A`.
fn ifs(args: &[Expr], lookup: Lookup) -> Value {
    if args.is_empty() || args.len() % 2 != 0 {
        return error(CalcError::Value);
    }
    for pair in args.chunks(2) {
        match to_bool(eval_expr(&pair[0], lookup)) {
            Ok(true) => return eval_expr(&pair[1], lookup),
            Ok(false) => {}
            Err(failure) => return error(failure),
        }
    }
    error(CalcError::NA)
}

/// SWITCH(expression, value1, result1, ..., [default]): text compares
/// without regard to case; only the chosen result is evaluated.
fn switch(args: &[Expr], lookup: Lookup) -> Value {
    if args.len() < 3 {
        return error(CalcError::Value);
    }
    let subject = eval_expr(&args[0], lookup);
    if let Value::Error(failure) = subject {
        return error(failure);
    }
    let rest = &args[1..];
    for pair in rest.chunks_exact(2) {
        let candidate = eval_expr(&pair[0], lookup);
        if let Value::Error(failure) = candidate {
            return error(failure);
        }
        if values_equal(&subject, &candidate) {
            return eval_expr(&pair[1], lookup);
        }
    }
    match rest.chunks_exact(2).remainder() {
        [default] => eval_expr(default, lookup),
        _ => error(CalcError::NA),
    }
}

/// XOR over every logical value in its arguments, ranges included; text and
/// blanks inside ranges are ignored.
fn xor(args: &[Expr], lookup: Lookup) -> Value {
    if args.is_empty() {
        return error(CalcError::Value);
    }
    let mut odd = false;
    let mut counted = false;
    for arg in args {
        let in_range = matches!(arg, Expr::Range { .. } | Expr::Cell(_));
        for value in flatten(arg, lookup) {
            let truth = match value {
                Value::Bool(b) => b,
                Value::Number(n) => n != 0.0,
                Value::Error(failure) => return error(failure),
                other if in_range => {
                    let _ = other;
                    continue;
                }
                other => match to_bool(other) {
                    Ok(b) => b,
                    Err(failure) => return error(failure),
                },
            };
            counted = true;
            odd ^= truth;
        }
    }
    if counted {
        Value::Bool(odd)
    } else {
        error(CalcError::Value)
    }
}

/// IFNA(value, if_na): the alternative is evaluated only for `#N/A`.
fn ifna(args: &[Expr], lookup: Lookup) -> Value {
    if args.len() != 2 {
        return error(CalcError::Value);
    }
    match eval_expr(&args[0], lookup) {
        Value::Error(CalcError::NA) => eval_expr(&args[1], lookup),
        other => other,
    }
}

/// CHOOSE(index, value1, ...): only the chosen value is evaluated.
fn choose(args: &[Expr], lookup: Lookup) -> Value {
    if args.len() < 2 {
        return error(CalcError::Value);
    }
    let index = match arg_number(args, 0, lookup) {
        Ok(n) => n.trunc(),
        Err(failure) => return error(failure),
    };
    if index < 1.0 || index > (args.len() - 1) as f64 {
        return error(CalcError::Value);
    }
    let (mut items, rows, cols) = grid(&args[index as usize], lookup);
    if items.len() == 1 {
        items.remove(0)
    } else {
        Value::Array(items, rows, cols)
    }
}

/// IS* predicates take exactly one value and never propagate its error; over
/// a range or array they answer for every element.
fn is_kind(args: &[Expr], lookup: Lookup, test: impl Fn(&Value) -> bool) -> Value {
    if args.len() != 1 {
        return error(CalcError::Value);
    }
    let (items, rows, cols) = grid(&args[0], lookup);
    if items.len() == 1 {
        return Value::Bool(test(&items[0]));
    }
    Value::Array(
        items.iter().map(|v| Value::Bool(test(v))).collect(),
        rows,
        cols,
    )
}

/// Excel's `=` between two scalars: same type, text without regard to case.
pub(crate) fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(l), Value::Number(r)) => l == r,
        (Value::Text(l), Value::Text(r)) => l.to_lowercase() == r.to_lowercase(),
        (Value::Bool(l), Value::Bool(r)) => l == r,
        (Value::Empty, Value::Empty) => true,
        (Value::Empty, Value::Number(n)) | (Value::Number(n), Value::Empty) => *n == 0.0,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{evaluate, CellRef, Sheet};

    fn eval(formula: &str) -> Value {
        let mut sheet = Sheet::new("t");
        sheet.set_str("Z1", formula);
        evaluate(&sheet)
            .get(&CellRef::parse("Z1").unwrap())
            .cloned()
            .unwrap_or(Value::Empty)
    }

    fn bool_result(formula: &str) -> bool {
        match eval(formula) {
            Value::Bool(b) => b,
            other => panic!("{formula} -> {other:?}"),
        }
    }

    #[test]
    fn ifs_evaluates_first_true_condition() {
        match eval("=IFS(1, \"yes\", 0, \"no\")") {
            Value::Text(s) => assert_eq!(s, "yes"),
            other => panic!("Expected 'yes', got {other:?}"),
        }
        match eval("=IFS(0, \"yes\", 1, \"no\")") {
            Value::Text(s) => assert_eq!(s, "no"),
            other => panic!("Expected 'no', got {other:?}"),
        }
    }

    #[test]
    fn ifs_returns_na_if_no_condition_true() {
        match eval("=IFS(0, 1, 0, 2)") {
            Value::Error(CalcError::NA) => {}
            other => panic!("Expected #N/A, got {other:?}"),
        }
    }

    #[test]
    fn switch_matches_first_case() {
        match eval("=SWITCH(2, 1, \"one\", 2, \"two\", 3, \"three\")") {
            Value::Text(s) => assert_eq!(s, "two"),
            other => panic!("Expected 'two', got {other:?}"),
        }
    }

    #[test]
    fn switch_uses_default_if_no_match() {
        match eval("=SWITCH(5, 1, \"one\", 2, \"two\", \"default\")") {
            Value::Text(s) => assert_eq!(s, "default"),
            other => panic!("Expected 'default', got {other:?}"),
        }
    }

    #[test]
    fn xor_true_for_odd_true_count() {
        assert!(!bool_result("=XOR(1, 1)"));
        assert!(bool_result("=XOR(1, 0)"));
        assert!(bool_result("=XOR(1, 1, 1)"));
    }

    #[test]
    fn ifna_returns_alternative_only_for_na() {
        assert_eq!(eval("=IFNA(NA(),7)"), Value::Number(7.0));
        assert_eq!(eval("=IFNA(3,1/0)"), Value::Number(3.0));
        assert_eq!(eval("=IFNA(1/0,7)"), Value::Error(CalcError::DivZero));
    }

    #[test]
    fn lazy_and_error_rules_follow_excel() {
        // Later pairs and unchosen branches are never evaluated.
        assert_eq!(eval("=IFS(TRUE,1,1/0,2)"), Value::Number(1.0));
        assert_eq!(eval("=CHOOSE(1,5,1/0)"), Value::Number(5.0));
        // An error in a reached condition is the result.
        assert_eq!(eval("=IFS(1/0,1,TRUE,2)"), Value::Error(CalcError::DivZero));
        assert_eq!(eval("=IFS(\"abc\",1)"), Value::Error(CalcError::Value));
        assert_eq!(eval("=CHOOSE(3,1,2)"), Value::Error(CalcError::Value));
    }

    #[test]
    fn switch_ignores_text_case_and_reports_na() {
        assert_eq!(eval("=SWITCH(\"A\",\"a\",1,2)"), Value::Number(1.0));
        assert_eq!(eval("=SWITCH(9,1,1,2,2)"), Value::Error(CalcError::NA));
    }

    #[test]
    fn xor_reads_ranges() {
        let mut sheet = Sheet::new("t");
        sheet.set_str("A1", "1");
        sheet.set_str("A2", "0");
        sheet.set_str("A3", "1");
        sheet.set_str("A4", "text");
        sheet.set_str("B1", "=XOR(A1:A4)");
        sheet.set_str("B2", "=XOR(A1:A3,TRUE)");
        let values = evaluate(&sheet);
        let at = |a: &str| values.get(&CellRef::parse(a).unwrap()).cloned();
        assert_eq!(at("B1"), Some(Value::Bool(false)));
        assert_eq!(at("B2"), Some(Value::Bool(true)));
    }

    #[test]
    fn isblank_detects_empty() {
        assert!(!bool_result("=ISBLANK(\"\")"));
        let _sheet = Sheet::new("t");
        let result = eval("=ISBLANK(A1)");
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn isnumber_detects_numbers() {
        assert!(bool_result("=ISNUMBER(42)"));
        assert!(!bool_result("=ISNUMBER(\"42\")"));
    }

    #[test]
    fn istext_detects_text() {
        assert!(bool_result("=ISTEXT(\"hello\")"));
        assert!(!bool_result("=ISTEXT(42)"));
    }

    #[test]
    fn iserror_detects_errors() {
        match eval("=ISERROR(1/0)") {
            Value::Bool(true) => {}
            other => panic!("Expected true for div error, got {other:?}"),
        }
        assert!(!bool_result("=ISERROR(42)"));
    }

    #[test]
    fn isna_detects_na_error() {
        // Test with 1/0 which produces a #DIV/0 error (not #N/A)
        assert!(!bool_result("=ISNA(1/0)"));
        // Test with a normal value
        assert!(!bool_result("=ISNA(0)"));
        // Test with a text value
        assert!(!bool_result("=ISNA(\"text\")"));
    }

    #[test]
    fn islogical_detects_booleans() {
        match eval("=ISLOGICAL(1=1)") {
            Value::Bool(true) => {}
            other => panic!("Expected true, got {other:?}"),
        }
        assert!(!bool_result("=ISLOGICAL(1)"));
    }

    #[test]
    fn choose_selects_by_index() {
        match eval("=CHOOSE(2, \"a\", \"b\", \"c\")") {
            Value::Text(s) => assert_eq!(s, "b"),
            other => panic!("Expected 'b', got {other:?}"),
        }
    }

    #[test]
    fn choose_returns_error_for_invalid_index() {
        match eval("=CHOOSE(0, \"a\", \"b\")") {
            Value::Error(CalcError::Value) => {}
            other => panic!("Expected #VALUE!, got {other:?}"),
        }
    }
}
