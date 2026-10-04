//! Shared argument handling for the function modules: Excel's coercion rules
//! and range flattening, so each function module does not re-derive them.

use crate::{eval_expr, CalcError, CellRef, Expr, Value};

/// Coerce a value to a number the way Excel does for a direct argument:
/// numbers, booleans, empty (0) and numeric text; errors propagate unchanged.
pub(crate) fn to_number(value: Value) -> Result<f64, CalcError> {
    match value {
        Value::Number(n) => Ok(n),
        Value::Bool(b) => Ok(f64::from(u8::from(b))),
        Value::Empty => Ok(0.0),
        Value::Text(text) => crate::functions_text::parse_numeric_text(&text),
        Value::Error(error) => Err(error),
        Value::Array(items, _, _) => match items.into_iter().next() {
            Some(first) => to_number(first),
            None => Err(CalcError::Value),
        },
    }
}

/// Coerce a value to text: numbers use their General display, booleans are
/// `TRUE`/`FALSE`, empty is `""`; errors propagate unchanged.
pub(crate) fn to_text(value: Value) -> Result<String, CalcError> {
    match value {
        Value::Text(text) => Ok(text),
        Value::Number(n) => Ok(Value::Number(n).display()),
        Value::Bool(b) => Ok(if b { "TRUE" } else { "FALSE" }.to_string()),
        Value::Empty => Ok(String::new()),
        Value::Error(error) => Err(error),
        Value::Array(items, _, _) => match items.into_iter().next() {
            Some(first) => to_text(first),
            None => Ok(String::new()),
        },
    }
}

/// Excel's truthiness for a condition: booleans, numbers, empty (false) and
/// the text `TRUE`/`FALSE`; other text is `#VALUE!`, errors propagate.
pub(crate) fn to_bool(value: Value) -> Result<bool, CalcError> {
    match value {
        Value::Bool(b) => Ok(b),
        Value::Number(n) => Ok(n != 0.0),
        Value::Empty => Ok(false),
        Value::Text(text) => match text.trim().to_ascii_uppercase().as_str() {
            "TRUE" => Ok(true),
            "FALSE" => Ok(false),
            _ => Err(CalcError::Value),
        },
        Value::Error(error) => Err(error),
        Value::Array(items, _, _) => match items.into_iter().next() {
            Some(first) => to_bool(first),
            None => Err(CalcError::Value),
        },
    }
}

/// Round to 15 significant digits, removing binary representation noise.
pub(crate) fn clean(value: f64) -> f64 {
    if value == 0.0 || !value.is_finite() {
        return value;
    }
    format!("{value:.14e}").parse().unwrap_or(value)
}

/// Evaluate argument `index` as a number.
pub(crate) fn arg_number(
    args: &[Expr],
    index: usize,
    lookup: &dyn Fn(CellRef) -> Value,
) -> Result<f64, CalcError> {
    to_number(eval_expr(&args[index], lookup))
}

/// Evaluate argument `index` as text.
pub(crate) fn arg_text(
    args: &[Expr],
    index: usize,
    lookup: &dyn Fn(CellRef) -> Value,
) -> Result<String, CalcError> {
    to_text(eval_expr(&args[index], lookup))
}

/// Optional numeric argument: `default` when absent or an empty argument.
pub(crate) fn opt_number(
    args: &[Expr],
    index: usize,
    default: f64,
    lookup: &dyn Fn(CellRef) -> Value,
) -> Result<f64, CalcError> {
    if index < args.len() {
        arg_number(args, index, lookup)
    } else {
        Ok(default)
    }
}

/// Whether the argument is a reference or array rather than a scalar.
fn is_reference(expr: &Expr) -> bool {
    matches!(expr, Expr::Range { .. } | Expr::Cell(_))
}

/// Every value an argument denotes, row-major: a range expands to its cells,
/// an array result to its elements, anything else to one value.
pub(crate) fn flatten(expr: &Expr, lookup: &dyn Fn(CellRef) -> Value) -> Vec<Value> {
    match expr {
        Expr::Range { start, end } => {
            let mut values = Vec::new();
            for row in start.row.min(end.row)..=start.row.max(end.row) {
                for col in start.col.min(end.col)..=start.col.max(end.col) {
                    values.push(lookup(CellRef { row, col }));
                }
            }
            values
        }
        other => {
            let mut values = Vec::new();
            push_flat(eval_expr(other, lookup), &mut values);
            values
        }
    }
}

fn push_flat(value: Value, out: &mut Vec<Value>) {
    match value {
        Value::Array(items, _, _) => items.into_iter().for_each(|item| push_flat(item, out)),
        scalar => out.push(scalar),
    }
}

/// The numbers an aggregate sees (Excel rules): inside a range or array only
/// numbers count and text/booleans/blanks are skipped; a direct scalar
/// argument is coerced, so non-numeric text is `#VALUE!`. Errors propagate.
pub(crate) fn numbers(
    args: &[Expr],
    lookup: &dyn Fn(CellRef) -> Value,
) -> Result<Vec<f64>, CalcError> {
    let mut out = Vec::new();
    for arg in args {
        let values = if is_reference(arg) {
            flatten(arg, lookup)
        } else {
            match eval_expr(arg, lookup) {
                array @ Value::Array(..) => {
                    let mut flat = Vec::new();
                    push_flat(array, &mut flat);
                    flat
                }
                scalar => {
                    out.push(to_number(scalar)?);
                    continue;
                }
            }
        };
        for value in values {
            match value {
                Value::Number(n) => out.push(n),
                Value::Error(error) => return Err(error),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// Dimensions-aware access to a reference or array argument.
pub(crate) fn grid(expr: &Expr, lookup: &dyn Fn(CellRef) -> Value) -> (Vec<Value>, usize, usize) {
    match expr {
        Expr::Range { start, end } => {
            let rows = start.row.max(end.row) - start.row.min(end.row) + 1;
            let cols = start.col.max(end.col) - start.col.min(end.col) + 1;
            (flatten(expr, lookup), rows as usize, cols as usize)
        }
        other => match eval_expr(other, lookup) {
            Value::Array(items, rows, cols) => (items, rows, cols),
            scalar => (vec![scalar], 1, 1),
        },
    }
}

/// `base ^ exponent` with Excel's edge cases: `0^0` is `#NUM!`, a zero base
/// with a negative exponent is `#DIV/0!`, a negative base needs an integer
/// exponent or the reciprocal of an odd integer (a real odd root), and
/// overflow is `#NUM!`.
pub(crate) fn excel_pow(base: f64, exponent: f64) -> Result<f64, CalcError> {
    if base == 0.0 {
        return match exponent.partial_cmp(&0.0) {
            Some(std::cmp::Ordering::Equal) => Err(CalcError::Num),
            Some(std::cmp::Ordering::Less) => Err(CalcError::DivZero),
            _ => Ok(0.0),
        };
    }
    let result = if base < 0.0 && exponent.fract() != 0.0 {
        let root = 1.0 / exponent;
        if (root - root.round()).abs() < 1e-9 && root.round() % 2.0 != 0.0 {
            -(-base).powf(exponent)
        } else {
            return Err(CalcError::Num);
        }
    } else {
        base.powf(exponent)
    };
    if result.is_finite() {
        Ok(result)
    } else {
        Err(CalcError::Num)
    }
}

/// Upper-case text one character at a time, keeping characters (like `ß`)
/// whose upper case would need several characters, as Excel does.
pub(crate) fn excel_upper(text: &str) -> String {
    text.chars()
        .map(|c| {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(single), None) => single,
                _ => c,
            }
        })
        .collect()
}
