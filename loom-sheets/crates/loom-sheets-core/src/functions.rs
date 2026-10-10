//! Extended spreadsheet formula functions: lookups, text, conditional
//! aggregations, financial functions, and date functions.

use std::time::{SystemTime, UNIX_EPOCH};

use crate::{eval_expr, CalcError, CellRef, Value};

mod date_more;
mod dynamic_arrays;
mod finance_more;
mod inventory;
mod let_binding;
mod lookup_more;
mod stats_more;
mod text_more;

/// Functions whose result can be an array that spills into the cells around
/// the formula. Workbook evaluation tracks spill dependencies for any formula
/// that calls one of them.
pub(crate) const ARRAY_RESULT_FUNCTIONS: &[&str] = &[
    "SEQUENCE",
    "TRANSPOSE",
    "SORT",
    "SORTBY",
    "UNIQUE",
    "FILTER",
    "XLOOKUP",
    "RANDARRAY",
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
    "TEXTSPLIT",
];

/// Evaluates extended built-in functions for the formula engine.
pub(crate) fn eval_extended_function(
    name: &str,
    raw_args: &[crate::Expr],
    lookup: &dyn Fn(CellRef) -> Value,
) -> Option<Value> {
    if let Some(v) = crate::functions_math::eval_math_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = crate::functions_stats::eval_stats_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = crate::functions_text::eval_text_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = crate::functions_logic::eval_logic_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = crate::functions_lookup::eval_lookup_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = crate::functions_finance::eval_finance_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = crate::functions_date::eval_date_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = date_more::eval_date_more_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = finance_more::eval_finance_more_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = stats_more::eval_stats_more_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = text_more::eval_text_more_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = lookup_more::eval_lookup_more_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = dynamic_arrays::eval_dynamic_array_function(name, raw_args, lookup) {
        return Some(v);
    }
    if let Some(v) = let_binding::eval_let_function(name, raw_args, lookup) {
        return Some(v);
    }
    match name {
        "LEFT" => {
            if raw_args.is_empty() || raw_args.len() > 2 {
                return Some(Value::Error(CalcError::Value));
            }
            let text = match crate::functions_util::arg_text(raw_args, 0, lookup) {
                Ok(text) => text,
                Err(error) => return Some(Value::Error(error)),
            };
            let n = if raw_args.len() == 2 {
                match eval_expr(&raw_args[1], lookup) {
                    Value::Number(x) if x >= 0.0 => x as usize,
                    _ => return Some(Value::Error(CalcError::Value)),
                }
            } else {
                1
            };
            let chars: String = text.chars().take(n).collect();
            Some(Value::Text(chars))
        }
        "RIGHT" => {
            if raw_args.is_empty() || raw_args.len() > 2 {
                return Some(Value::Error(CalcError::Value));
            }
            let text = match crate::functions_util::arg_text(raw_args, 0, lookup) {
                Ok(text) => text,
                Err(error) => return Some(Value::Error(error)),
            };
            let n = if raw_args.len() == 2 {
                match eval_expr(&raw_args[1], lookup) {
                    Value::Number(x) if x >= 0.0 => x as usize,
                    _ => return Some(Value::Error(CalcError::Value)),
                }
            } else {
                1
            };
            let total = text.chars().count();
            let skip = total.saturating_sub(n);
            let chars: String = text.chars().skip(skip).collect();
            Some(Value::Text(chars))
        }
        "MID" => {
            if raw_args.len() != 3 {
                return Some(Value::Error(CalcError::Value));
            }
            let text = match crate::functions_util::arg_text(raw_args, 0, lookup) {
                Ok(text) => text,
                Err(error) => return Some(Value::Error(error)),
            };
            let start = match eval_expr(&raw_args[1], lookup) {
                Value::Number(x) if x >= 1.0 => (x as usize) - 1,
                _ => return Some(Value::Error(CalcError::Value)),
            };
            let count = match eval_expr(&raw_args[2], lookup) {
                Value::Number(x) if x >= 0.0 => x as usize,
                _ => return Some(Value::Error(CalcError::Value)),
            };
            let chars: String = text.chars().skip(start).take(count).collect();
            Some(Value::Text(chars))
        }
        "LEN" => {
            if raw_args.len() != 1 {
                return Some(Value::Error(CalcError::Value));
            }
            let text = match crate::functions_util::arg_text(raw_args, 0, lookup) {
                Ok(text) => text,
                Err(error) => return Some(Value::Error(error)),
            };
            Some(Value::Number(text.chars().count() as f64))
        }
        "UPPER" => {
            if raw_args.len() != 1 {
                return Some(Value::Error(CalcError::Value));
            }
            Some(match crate::functions_util::arg_text(raw_args, 0, lookup) {
                Ok(text) => Value::Text(crate::functions_util::excel_upper(&text)),
                Err(error) => Value::Error(error),
            })
        }
        "LOWER" => {
            if raw_args.len() != 1 {
                return Some(Value::Error(CalcError::Value));
            }
            Some(match crate::functions_util::arg_text(raw_args, 0, lookup) {
                Ok(text) => Value::Text(text.to_lowercase()),
                Err(error) => Value::Error(error),
            })
        }
        "TRIM" => {
            if raw_args.len() != 1 {
                return Some(Value::Error(CalcError::Value));
            }
            Some(match crate::functions_util::arg_text(raw_args, 0, lookup) {
                Ok(text) => Value::Text(excel_trim(&text)),
                Err(error) => Value::Error(error),
            })
        }
        "IFERROR" => {
            if raw_args.len() != 2 {
                return Some(Value::Error(CalcError::Value));
            }
            let first = eval_expr(&raw_args[0], lookup);
            match first {
                Value::Error(_) => Some(eval_expr(&raw_args[1], lookup)),
                other => Some(other),
            }
        }
        "SUMIF" | "COUNTIF" | "AVERAGEIF" => {
            // SUMIF/COUNTIF/AVERAGEIF take (range, criteria[, sum_range]).
            if raw_args.len() < 2 || raw_args.len() > 3 {
                return Some(Value::Error(CalcError::Value));
            }
            let range_cells = match expand_cells(&raw_args[0]) {
                Some(cells) if !cells.is_empty() => cells,
                _ => return Some(Value::Error(CalcError::Value)),
            };
            let criteria = eval_expr(&raw_args[1], lookup);
            if let Value::Error(e) = criteria {
                return Some(Value::Error(e));
            }
            let criteria_text = criteria.display();
            let mask_cells = range_cells.clone();
            // The sum range starts at its own top-left cell but takes the
            // shape of the criteria range, as Excel resizes it.
            let sum_cells = if raw_args.len() == 3 {
                match (&raw_args[0], expand_cells(&raw_args[2])) {
                    (_, Some(cells)) if cells.len() == range_cells.len() => cells,
                    (crate::Expr::Range { start, end }, Some(cells)) if !cells.is_empty() => {
                        let top = cells
                            .iter()
                            .map(|c| (c.row, c.col))
                            .min()
                            .unwrap_or_default();
                        let rows = start.row.max(end.row) - start.row.min(end.row);
                        let cols = start.col.max(end.col) - start.col.min(end.col);
                        let mut resized = Vec::new();
                        for dr in 0..=rows {
                            for dc in 0..=cols {
                                resized.push(CellRef {
                                    row: top.0 + dr,
                                    col: top.1 + dc,
                                });
                            }
                        }
                        resized
                    }
                    _ => return Some(Value::Error(CalcError::Value)),
                }
            } else {
                range_cells.clone()
            };
            let mask: Vec<bool> = mask_cells
                .iter()
                .map(|cell| {
                    let v = lookup(*cell);
                    criteria_matches(&v, &criteria_text)
                })
                .collect();
            match name {
                "COUNTIF" => {
                    let count = mask.iter().filter(|&&m| m).count() as f64;
                    Some(Value::Number(count))
                }
                "SUMIF" => {
                    let mut total = 0.0;
                    for (matched, cell) in mask.iter().zip(sum_cells.iter()) {
                        if !matched {
                            continue;
                        }
                        match lookup(*cell) {
                            Value::Number(n) => total += n,
                            Value::Error(e) => return Some(Value::Error(e)),
                            _ => {}
                        }
                    }
                    Some(Value::Number(total))
                }
                "AVERAGEIF" => {
                    let mut total = 0.0;
                    let mut count = 0usize;
                    for (matched, cell) in mask.iter().zip(sum_cells.iter()) {
                        if !matched {
                            continue;
                        }
                        match lookup(*cell) {
                            Value::Number(n) => {
                                total += n;
                                count += 1;
                            }
                            Value::Error(e) => return Some(Value::Error(e)),
                            _ => {}
                        }
                    }
                    if count == 0 {
                        Some(Value::Error(CalcError::DivZero))
                    } else {
                        Some(Value::Number(total / count as f64))
                    }
                }
                _ => Some(Value::Error(CalcError::Name)),
            }
        }
        "TEXTJOIN" => {
            if raw_args.len() < 3 {
                return Some(Value::Error(CalcError::Value));
            }
            let delimiter = eval_expr(&raw_args[0], lookup).display();
            let ignore_empty = match eval_expr(&raw_args[1], lookup) {
                Value::Bool(b) => b,
                Value::Number(n) => n != 0.0,
                Value::Empty => false,
                _ => return Some(Value::Error(CalcError::Value)),
            };
            let mut parts = Vec::new();
            for arg in &raw_args[2..] {
                match arg {
                    crate::Expr::Range { start, end } => {
                        let min_row = start.row.min(end.row);
                        let max_row = start.row.max(end.row);
                        let min_col = start.col.min(end.col);
                        let max_col = start.col.max(end.col);
                        for row in min_row..=max_row {
                            for col in min_col..=max_col {
                                match lookup(CellRef { row, col }) {
                                    Value::Error(e) => {
                                        return Some(Value::Error(e));
                                    }
                                    v => {
                                        let text = v.display();
                                        if !ignore_empty || !text.is_empty() {
                                            parts.push(text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    _ => match eval_expr(arg, lookup) {
                        Value::Error(e) => return Some(Value::Error(e)),
                        Value::Array(flat, _, _) => {
                            for element in flat {
                                match element {
                                    Value::Error(e) => {
                                        return Some(Value::Error(e));
                                    }
                                    v => {
                                        let text = v.display();
                                        if !ignore_empty || !text.is_empty() {
                                            parts.push(text);
                                        }
                                    }
                                }
                            }
                        }
                        v => {
                            let text = v.display();
                            if !ignore_empty || !text.is_empty() {
                                parts.push(text);
                            }
                        }
                    },
                }
            }
            Some(Value::Text(parts.join(&delimiter)))
        }
        "PMT" | "FV" | "PV" => {
            if raw_args.len() < 3 || raw_args.len() > 5 {
                return Some(Value::Error(CalcError::Value));
            }
            let mut numbers = Vec::with_capacity(raw_args.len());
            for arg in raw_args {
                match eval_expr(arg, lookup) {
                    Value::Number(n) => numbers.push(n),
                    Value::Error(e) => return Some(Value::Error(e)),
                    _ => return Some(Value::Error(CalcError::Value)),
                }
            }
            let rate = numbers[0];
            let nper = numbers[1];
            let third = numbers[2];
            let fourth = numbers.get(3).copied().unwrap_or(0.0);
            let end_of_period = numbers.get(4).copied().unwrap_or(0.0) == 0.0;
            let result = match name {
                "PMT" => crate::pmt(rate, nper, third, fourth, end_of_period),
                "FV" => crate::fv(rate, nper, third, fourth, end_of_period),
                _ => crate::pv(rate, nper, third, fourth, end_of_period),
            };
            match result {
                Ok(n) => Some(Value::Number(n)),
                Err(_) => Some(Value::Error(CalcError::Num)),
            }
        }
        "SEQUENCE" => {
            if raw_args.is_empty() || raw_args.len() > 4 {
                return Some(Value::Error(CalcError::Value));
            }
            let mut dims = Vec::with_capacity(raw_args.len());
            for arg in raw_args {
                match eval_expr(arg, lookup) {
                    Value::Number(n) => dims.push(n),
                    Value::Error(e) => return Some(Value::Error(e)),
                    _ => return Some(Value::Error(CalcError::Value)),
                }
            }
            let rows = dims[0].floor() as i64;
            let cols = dims.get(1).copied().unwrap_or(1.0).floor() as i64;
            let start = dims.get(2).copied().unwrap_or(1.0);
            let step = dims.get(3).copied().unwrap_or(1.0);
            if rows == 0 || cols == 0 {
                return Some(Value::Error(CalcError::Calc));
            }
            if rows < 1 || cols < 1 || rows > 10_000 || cols > 10_000 || rows * cols > 100_000 {
                return Some(Value::Error(CalcError::Value));
            }
            let (rows, cols) = (rows as usize, cols as usize);
            let mut flat = Vec::with_capacity(rows * cols);
            for index in 0..rows * cols {
                flat.push(Value::Number(start + index as f64 * step));
            }
            Some(Value::Array(flat, rows, cols))
        }
        "TRANSPOSE" => {
            if raw_args.len() != 1 {
                return Some(Value::Error(CalcError::Value));
            }
            let (matrix, rows, cols) = to_matrix(&raw_args[0], lookup);
            let mut flat = Vec::with_capacity(rows * cols);
            if let Some(first_row) = matrix.first() {
                for (col, _) in first_row.iter().enumerate() {
                    for row in &matrix {
                        flat.push(row[col].clone());
                    }
                }
            }
            Some(Value::Array(flat, cols, rows))
        }
        "SORT" => {
            if raw_args.is_empty() || raw_args.len() > 4 {
                return Some(Value::Error(CalcError::Value));
            }
            let (mut matrix, rows, cols) = to_matrix(&raw_args[0], lookup);
            if rows == 0 || cols == 0 {
                return Some(Value::Error(CalcError::Value));
            }
            let index = match raw_args.get(1).map(|arg| eval_expr(arg, lookup)) {
                None => 1i64,
                Some(Value::Number(n)) => n.floor() as i64,
                Some(Value::Error(e)) => return Some(Value::Error(e)),
                Some(_) => return Some(Value::Error(CalcError::Value)),
            };
            let descending = match raw_args.get(2).map(|arg| eval_expr(arg, lookup)) {
                None => false,
                Some(Value::Number(1.0)) => false,
                Some(Value::Number(-1.0)) => true,
                Some(Value::Error(e)) => return Some(Value::Error(e)),
                Some(_) => return Some(Value::Error(CalcError::Value)),
            };
            let by_col = match raw_args.get(3).map(|arg| eval_expr(arg, lookup)) {
                None => false,
                Some(Value::Number(n)) => n != 0.0,
                Some(Value::Bool(b)) => b,
                Some(Value::Error(e)) => return Some(Value::Error(e)),
                Some(_) => return Some(Value::Error(CalcError::Value)),
            };
            if by_col {
                matrix = transpose_grid(matrix);
            }
            let key_position = (index - 1) as usize;
            if index < 1 || key_position >= matrix[0].len() {
                return Some(Value::Error(CalcError::Value));
            }
            for row in &matrix {
                if let Value::Error(e) = row[key_position] {
                    return Some(Value::Error(e));
                }
            }
            matrix.sort_by(|a, b| {
                let ordering = sort_key_order(&a[key_position], &b[key_position]);
                let blank = a[key_position] == Value::Empty || b[key_position] == Value::Empty;
                if descending && !blank {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
            if by_col {
                matrix = transpose_grid(matrix);
            }
            let (rows, cols) = (matrix.len(), matrix[0].len());
            let flat = matrix.into_iter().flatten().collect::<Vec<_>>();
            Some(Value::Array(flat, rows, cols))
        }
        "UNIQUE" => {
            if raw_args.is_empty() || raw_args.len() > 3 {
                return Some(Value::Error(CalcError::Value));
            }
            let (mut matrix, rows, _) = to_matrix(&raw_args[0], lookup);
            if rows == 0 {
                return Some(Value::Error(CalcError::Value));
            }
            let by_col = match raw_args.get(1).map(|arg| eval_expr(arg, lookup)) {
                None => false,
                Some(Value::Number(n)) => n != 0.0,
                Some(Value::Bool(b)) => b,
                Some(Value::Error(e)) => return Some(Value::Error(e)),
                Some(_) => return Some(Value::Error(CalcError::Value)),
            };
            let exactly_once = match raw_args.get(2).map(|arg| eval_expr(arg, lookup)) {
                None => false,
                Some(Value::Number(n)) => n != 0.0,
                Some(Value::Bool(b)) => b,
                Some(Value::Error(e)) => return Some(Value::Error(e)),
                Some(_) => return Some(Value::Error(CalcError::Value)),
            };
            if by_col {
                matrix = transpose_grid(matrix);
            }
            let mut counts: std::collections::HashMap<String, usize> =
                std::collections::HashMap::new();
            for row in &matrix {
                let key = row_key(row);
                // Errors propagate instead of grouping silently.
                if row.iter().any(|v| matches!(v, Value::Error(_))) {
                    let error = row
                        .iter()
                        .find_map(|v| match v {
                            Value::Error(e) => Some(*e),
                            _ => None,
                        })
                        .unwrap_or(CalcError::Value);
                    return Some(Value::Error(error));
                }
                *counts.entry(key).or_insert(0) += 1;
            }
            let mut kept: Vec<Vec<Value>> = Vec::new();
            for row in matrix {
                let count = counts.get(&row_key(&row)).copied().unwrap_or(0);
                if (!exactly_once && count >= 1) || (exactly_once && count == 1) {
                    // First-appearance order; drop later duplicates.
                    if kept
                        .iter()
                        .all(|kept_row| row_key(kept_row) != row_key(&row))
                    {
                        kept.push(row);
                    }
                }
            }
            if kept.is_empty() {
                return Some(Value::Error(CalcError::Value));
            }
            if by_col {
                kept = transpose_grid(kept);
            }
            let (rows, cols) = (kept.len(), kept[0].len());
            let flat = kept.into_iter().flatten().collect::<Vec<_>>();
            Some(Value::Array(flat, rows, cols))
        }
        "FILTER" => {
            if raw_args.len() < 2 || raw_args.len() > 3 {
                return Some(Value::Error(CalcError::Value));
            }
            let (matrix, rows, _) = to_matrix(&raw_args[0], lookup);
            let (include, include_rows, _) = to_matrix(&raw_args[1], lookup);
            if rows == 0 || include_rows != rows {
                return Some(Value::Error(CalcError::Value));
            }
            let mut kept = Vec::new();
            for (row, mask_row) in matrix.into_iter().zip(include) {
                let mut keep = false;
                for flag in &mask_row {
                    match flag {
                        Value::Bool(b) => keep = keep || *b,
                        Value::Number(n) => keep = keep || *n != 0.0,
                        Value::Empty => {}
                        Value::Error(e) => return Some(Value::Error(*e)),
                        _ => return Some(Value::Error(CalcError::Value)),
                    }
                }
                if keep {
                    kept.push(row);
                }
            }
            if kept.is_empty() {
                return match raw_args.get(2).map(|arg| eval_expr(arg, lookup)) {
                    None => Some(Value::Error(CalcError::Calc)),
                    Some(Value::Error(e)) => Some(Value::Error(e)),
                    Some(Value::Array(_, _, _)) => Some(Value::Error(CalcError::Value)),
                    Some(scalar) => Some(Value::Array(vec![scalar], 1, 1)),
                };
            }
            let (rows, cols) = (kept.len(), kept[0].len());
            let flat = kept.into_iter().flatten().collect::<Vec<_>>();
            Some(Value::Array(flat, rows, cols))
        }
        "TODAY" => {
            if !raw_args.is_empty() {
                return Some(Value::Error(CalcError::Value));
            }
            Some(Value::Number(today_serial()))
        }
        "NOW" => {
            if !raw_args.is_empty() {
                return Some(Value::Error(CalcError::Value));
            }
            match SystemTime::now().duration_since(UNIX_EPOCH) {
                Ok(elapsed) => Some(Value::Number(elapsed.as_secs_f64() / 86_400.0)),
                Err(_) => Some(Value::Error(CalcError::Value)),
            }
        }
        _ => None,
    }
}

/// Whole days since 1970-01-01 (UTC), matching the civil-date helpers.
fn today_serial() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(elapsed) => (elapsed.as_secs() / 86_400) as f64,
        Err(_) => 0.0,
    }
}

/// Cell coordinates of a Range or single-Cell argument, in row-major order.
pub(crate) fn expand_cells(expr: &crate::Expr) -> Option<Vec<CellRef>> {
    match expr {
        crate::Expr::Range { start, end } => {
            let mut cells = Vec::new();
            for row in start.row.min(end.row)..=start.row.max(end.row) {
                for col in start.col.min(end.col)..=start.col.max(end.col) {
                    cells.push(CellRef { row, col });
                }
            }
            Some(cells)
        }
        crate::Expr::Cell(cell) => Some(vec![*cell]),
        _ => None,
    }
}

/// Whether a looked-up cell value satisfies a SUMIF/COUNTIF-style criteria
/// string: optional `>=`, `<=`, `<>`, `>`, `<`, `=` operator prefix, then a
/// number, text, or `*`/`?` wildcard pattern (wildcards apply to `=`/`<>`).
pub(crate) fn criteria_matches(value: &Value, criteria: &str) -> bool {
    let (op, operand) = if let Some(rest) = criteria.strip_prefix(">=") {
        (">=", rest)
    } else if let Some(rest) = criteria.strip_prefix("<=") {
        ("<=", rest)
    } else if let Some(rest) = criteria.strip_prefix("<>") {
        ("<>", rest)
    } else if let Some(rest) = criteria.strip_prefix('>') {
        (">", rest)
    } else if let Some(rest) = criteria.strip_prefix('<') {
        ("<", rest)
    } else if let Some(rest) = criteria.strip_prefix('=') {
        ("=", rest)
    } else {
        ("=", criteria)
    };
    let cell_text = value.display();
    // A blank cell never satisfies an ordering test against text.
    if *value == Value::Empty
        && matches!(op, ">" | "<" | ">=" | "<=")
        && operand.parse::<f64>().is_err()
    {
        return false;
    }
    let cell_num = match value {
        Value::Number(n) => Some(*n),
        _ => None,
    };
    let ordering = matches!(op, ">" | "<" | ">=" | "<=");
    // Ordering against a number counts numbers only, never numeric text.
    let numeric_operand = operand.trim().parse::<f64>().ok();
    if ordering
        && (numeric_operand.is_some() != cell_num.is_some() || matches!(value, Value::Bool(_)))
    {
        return false;
    }
    // Numeric comparison when both sides are numbers.
    if let (Some(cell), Ok(target)) = (cell_num, operand.trim().parse::<f64>()) {
        return match op {
            "=" => cell == target,
            "<>" => cell != target,
            ">" => cell > target,
            "<" => cell < target,
            ">=" => cell >= target,
            "<=" => cell <= target,
            _ => false,
        };
    }
    // Text comparison (case-insensitive); wildcards only for equality.
    match op {
        "=" => {
            if operand.contains(['*', '?', '~']) {
                matches!(value, Value::Text(_)) && wildcard_match(operand, &cell_text)
            } else {
                cell_text.eq_ignore_ascii_case(operand)
            }
        }
        "<>" => {
            if operand.contains(['*', '?', '~']) {
                !(matches!(value, Value::Text(_)) && wildcard_match(operand, &cell_text))
            } else {
                !cell_text.eq_ignore_ascii_case(operand)
            }
        }
        ">" => cell_text.to_uppercase() > operand.to_uppercase(),
        "<" => cell_text.to_uppercase() < operand.to_uppercase(),
        ">=" => cell_text.to_uppercase() >= operand.to_uppercase(),
        "<=" => cell_text.to_uppercase() <= operand.to_uppercase(),
        _ => false,
    }
}

/// Case-insensitive wildcard match: `*` spans any run, `?` one character.
pub(crate) fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_uppercase().chars().collect();
    let text: Vec<char> = text.to_uppercase().chars().collect();
    fn go(pattern: &[char], text: &[char]) -> bool {
        if pattern.is_empty() {
            return text.is_empty();
        }
        if pattern[0] == '*' {
            return (0..=text.len()).any(|skip| go(&pattern[1..], &text[skip..]));
        }
        if text.is_empty() {
            return false;
        }
        // `~` makes the next wildcard character literal.
        if pattern[0] == '~' && pattern.len() > 1 {
            return pattern[1] == text[0] && go(&pattern[2..], &text[1..]);
        }
        if pattern[0] == '?' || pattern[0] == text[0] {
            go(&pattern[1..], &text[1..])
        } else {
            false
        }
    }
    go(&pattern, &text)
}

/// Read an argument as a value matrix for spill producers: ranges expand to
/// lookup grids, single cells and scalars become 1x1, and nested arrays pass
/// through with their dimensions.
fn to_matrix(
    arg: &crate::Expr,
    lookup: &dyn Fn(CellRef) -> Value,
) -> (Vec<Vec<Value>>, usize, usize) {
    match arg {
        crate::Expr::Range { start, end } => {
            let min_row = start.row.min(end.row);
            let max_row = start.row.max(end.row);
            let min_col = start.col.min(end.col);
            let max_col = start.col.max(end.col);
            let mut matrix = Vec::new();
            for row in min_row..=max_row {
                let mut line = Vec::new();
                for col in min_col..=max_col {
                    line.push(lookup(CellRef { row, col }));
                }
                matrix.push(line);
            }
            let rows = matrix.len();
            let cols = matrix[0].len();
            (matrix, rows, cols)
        }
        crate::Expr::Cell(cell) => (vec![vec![lookup(*cell)]], 1, 1),
        _ => match eval_expr(arg, lookup) {
            Value::Array(flat, rows, cols) => {
                let mut matrix = Vec::with_capacity(rows);
                let mut cells = flat.into_iter();
                for _ in 0..rows {
                    let mut line = Vec::with_capacity(cols);
                    for _ in 0..cols {
                        line.push(cells.next().unwrap_or(Value::Empty));
                    }
                    matrix.push(line);
                }
                let cols = matrix[0].len();
                (matrix, rows, cols)
            }
            scalar => (vec![vec![scalar]], 1, 1),
        },
    }
}

/// Transpose a value matrix.
fn transpose_grid(matrix: Vec<Vec<Value>>) -> Vec<Vec<Value>> {
    if matrix.is_empty() {
        return matrix;
    }
    let cols = matrix[0].len();
    let mut out: Vec<Vec<Value>> = (0..cols).map(|_| Vec::new()).collect();
    for row in matrix {
        for (col, value) in row.into_iter().enumerate() {
            if col < out.len() {
                out[col].push(value);
            }
        }
    }
    out
}

/// Join key for row identity in UNIQUE: display text per cell.
fn row_key(row: &[Value]) -> String {
    row.iter()
        .map(|value| value.display())
        .collect::<Vec<_>>()
        .join("\0")
}

/// Total ordering for SORT keys: numbers numerically, everything else by
/// display text, empty cells always last.
fn sort_key_order(left: &Value, right: &Value) -> std::cmp::Ordering {
    match (left, right) {
        (Value::Empty, Value::Empty) => std::cmp::Ordering::Equal,
        (Value::Empty, _) => std::cmp::Ordering::Greater,
        (_, Value::Empty) => std::cmp::Ordering::Less,
        (Value::Number(a), Value::Number(b)) => a.total_cmp(b),
        _ => left
            .display()
            .to_uppercase()
            .cmp(&right.display().to_uppercase()),
    }
}

/// Excel TRIM: strip leading/trailing spaces and collapse inner space runs.
fn excel_trim(text: &str) -> String {
    text.split(' ')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use crate::{evaluate, Sheet, Value};

    #[test]
    fn test_vlookup_formula() {
        let mut sheet = Sheet::new("lookup");
        sheet.set_str("A1", "Apple");
        sheet.set_str("B1", "1.50");
        sheet.set_str("A2", "Banana");
        sheet.set_str("B2", "0.75");
        sheet.set_str("C1", "=VLOOKUP(\"Banana\", A1:B2, 2, 1)");

        let vals = evaluate(&sheet);
        let cell = crate::CellRef::parse("C1").unwrap();
        assert_eq!(vals.get(&cell).unwrap().display(), "0.75");
    }

    #[test]
    fn test_text_formulas() {
        let mut sheet = Sheet::new("text");
        sheet.set_str("A1", "  Hello World  ");
        sheet.set_str("B1", "=TRIM(A1)");
        sheet.set_str("B2", "=UPPER(B1)");
        sheet.set_str("B3", "=LEFT(B2, 5)");
        sheet.set_str("B4", "=LEN(B1)");

        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&crate::CellRef::parse("B1").unwrap())
                .unwrap()
                .display(),
            "Hello World"
        );
        assert_eq!(
            vals.get(&crate::CellRef::parse("B2").unwrap())
                .unwrap()
                .display(),
            "HELLO WORLD"
        );
        assert_eq!(
            vals.get(&crate::CellRef::parse("B3").unwrap())
                .unwrap()
                .display(),
            "HELLO"
        );
        assert_eq!(
            vals.get(&crate::CellRef::parse("B4").unwrap())
                .unwrap()
                .display(),
            "11"
        );
    }

    #[test]
    fn test_iferror_formula() {
        let mut sheet = Sheet::new("err");
        sheet.set_str("A1", "=1/0");
        sheet.set_str("A2", "=IFERROR(A1, \"Fallback\")");

        let vals = evaluate(&sheet);
        assert_eq!(
            vals.get(&crate::CellRef::parse("A2").unwrap())
                .unwrap()
                .display(),
            "Fallback"
        );
    }

    #[test]
    fn test_sumif_countif_averageif_formulas() {
        let mut sheet = Sheet::new("cond");
        for (c, v) in [
            ("A1", "Food"),
            ("A2", "Rent"),
            ("A3", "Food"),
            ("B1", "10"),
            ("B2", "20"),
            ("B3", "30"),
        ] {
            sheet.set_str(c, v);
        }
        sheet.set_str("C1", "=SUMIF(A1:A3, \"Food\", B1:B3)");
        sheet.set_str("C2", "=COUNTIF(A1:A3, \"Food\")");
        sheet.set_str("C3", "=AVERAGEIF(B1:B3, \">15\")");
        sheet.set_str("C4", "=COUNTIF(B1:B3, \">=20\")");
        sheet.set_str("C5", "=SUMIF(A1:A3, \"F*\", B1:B3)");

        let vals = evaluate(&sheet);
        let get = |a1: &str| {
            vals.get(&crate::CellRef::parse(a1).unwrap())
                .unwrap()
                .display()
        };
        assert_eq!(get("C1"), "40");
        assert_eq!(get("C2"), "2");
        assert_eq!(get("C3"), "25");
        assert_eq!(get("C4"), "2");
        assert_eq!(get("C5"), "40");
    }

    #[test]
    fn test_minifs_maxifs_formulas() {
        let mut sheet = Sheet::new("extremes");
        for (c, v) in [
            ("A1", "Food"),
            ("A2", "Rent"),
            ("A3", "Food"),
            ("B1", "10"),
            ("B2", "20"),
            ("B3", "30"),
        ] {
            sheet.set_str(c, v);
        }
        sheet.set_str("C1", "=MINIFS(B1:B3, A1:A3, \"Food\")");
        sheet.set_str("C2", "=MAXIFS(B1:B3, A1:A3, \"Food\")");
        sheet.set_str("C3", "=MINIFS(B1:B3, A1:A3, \"Absent\")");

        let vals = evaluate(&sheet);
        let get = |a1: &str| {
            vals.get(&crate::CellRef::parse(a1).unwrap())
                .unwrap()
                .display()
        };
        assert_eq!(get("C1"), "10");
        assert_eq!(get("C2"), "30");
        assert_eq!(get("C3"), "0");
    }

    #[test]
    fn test_textjoin_and_concatenate_formulas() {
        let mut sheet = Sheet::new("join");
        sheet.set_str("A1", "a");
        sheet.set_str("A2", "");
        sheet.set_str("A3", "c");
        sheet.set_str("B1", "=TEXTJOIN(\", \", 1, A1:A3)");
        sheet.set_str("B2", "=TEXTJOIN(\"-\", 0, A1:A3)");
        sheet.set_str("B3", "=CONCATENATE(A1, \"!\", A3)");

        let vals = evaluate(&sheet);
        let get = |a1: &str| {
            vals.get(&crate::CellRef::parse(a1).unwrap())
                .unwrap()
                .display()
        };
        assert_eq!(get("B1"), "a, c");
        assert_eq!(get("B2"), "a--c");
        assert_eq!(get("B3"), "a!c");
    }

    #[test]
    fn test_financial_and_date_formulas() {
        let mut sheet = Sheet::new("fin");
        sheet.set_str("A1", "=PMT(0.05/12, 12, 1000)");
        sheet.set_str("A2", "=FV(0.05/12, 12, -100, -1000)");
        sheet.set_str("A3", "=PV(0.05, 10, -100)");
        sheet.set_str("A4", "=TODAY()");
        sheet.set_str("A5", "=NOW()");

        let vals = evaluate(&sheet);
        let get = |a1: &str| vals.get(&crate::CellRef::parse(a1).unwrap()).unwrap();
        match get("A1") {
            Value::Number(n) => assert!((*n + 85.61).abs() < 0.05, "PMT ≈ -85.61, got {n}"),
            other => panic!("PMT should be numeric, got {other:?}"),
        }
        assert!(matches!(get("A2"), Value::Number(_)));
        assert!(matches!(get("A3"), Value::Number(_)));
        // Serial days since 1970-01-01: must exceed 2020-01-01 (18262).
        match get("A4") {
            Value::Number(n) => assert!(*n > 18262.0, "TODAY serial {n} too small"),
            other => panic!("TODAY should be numeric, got {other:?}"),
        }
        match (get("A4"), get("A5")) {
            (Value::Number(today), Value::Number(now)) => {
                assert!(*now >= *today && *now < *today + 1.0)
            }
            other => panic!("TODAY/NOW should be numeric, got {other:?}"),
        }
    }
}
