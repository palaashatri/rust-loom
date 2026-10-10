//! Dynamic-array producers and reshapers (spike C): SORTBY, RANDARRAY, TAKE,
//! DROP, VSTACK, HSTACK, TOCOL, TOROW, WRAPROWS, WRAPCOLS, CHOOSECOLS,
//! CHOOSEROWS and EXPAND. Results are row-major blocks (see
//! `functions_util::spill`). An empty result is `#CALC!`; a malformed argument
//! or a block beyond the sheet is `#VALUE!`.

use std::cmp::Ordering;

use crate::functions_util::{arg_number, grid, opt_number, spill, to_bool, to_number};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;

/// Largest block these functions build, in cells.
const MAX_CELLS: usize = 1_000_000;

/// A row-major rectangle of values.
struct Block {
    cells: Vec<Value>,
    rows: usize,
    cols: usize,
}

impl Block {
    fn at(&self, row: usize, col: usize) -> &Value {
        &self.cells[row * self.cols + col]
    }
}

/// Evaluate an array argument. A scalar error passed directly is the result.
fn block(expr: &Expr, lookup: Lookup) -> Result<Block, CalcError> {
    let (cells, rows, cols) = grid(expr, lookup);
    let is_reference = matches!(expr, Expr::Range { .. } | Expr::Cell(_));
    if !is_reference {
        if let [Value::Error(error)] = cells.as_slice() {
            return Err(*error);
        }
    }
    Ok(Block { cells, rows, cols })
}

/// Refuse an empty block as `#CALC!`, and one beyond the sheet or the cell
/// budget as `#VALUE!`.
fn check_size(rows: usize, cols: usize) -> Result<(), CalcError> {
    if rows == 0 || cols == 0 {
        return Err(CalcError::Calc);
    }
    if rows > 1_048_576 || cols > 16_384 || rows * cols > MAX_CELLS {
        return Err(CalcError::Value);
    }
    Ok(())
}

/// Dispatch the dynamic-array functions from the main evaluator.
pub(crate) fn eval_dynamic_array_function(
    name: &str,
    args: &[Expr],
    lookup: Lookup,
) -> Option<Value> {
    let result = match name {
        "SORTBY" => sortby(args, lookup),
        "RANDARRAY" => randarray(args, lookup),
        "TAKE" => take_or_drop(args, lookup, true),
        "DROP" => take_or_drop(args, lookup, false),
        "VSTACK" => stack(args, lookup, true),
        "HSTACK" => stack(args, lookup, false),
        "TOCOL" => flatten_block(args, lookup, true),
        "TOROW" => flatten_block(args, lookup, false),
        "WRAPROWS" => wrap(args, lookup, true),
        "WRAPCOLS" => wrap(args, lookup, false),
        "CHOOSECOLS" => choose(args, lookup, true),
        "CHOOSEROWS" => choose(args, lookup, false),
        "EXPAND" => expand(args, lookup),
        _ => return None,
    };
    Some(result.unwrap_or_else(Value::Error))
}

/// Whether an argument is an array to sort by (a range or an array result)
/// rather than a sort order. A single cell is read as an order.
fn is_key(expr: &Expr, lookup: Lookup) -> bool {
    match expr {
        Expr::Range { .. } => true,
        Expr::Cell(_) => false,
        other => matches!(eval_expr(other, lookup), Value::Array(..)),
    }
}

/// Excel's order for sort keys: numbers, then text (case-insensitive), then
/// logicals, then errors. Blank cells always sort last.
fn sort_order_of(left: &Value, right: &Value, descending: bool) -> Ordering {
    match (left, right) {
        (Value::Empty, Value::Empty) => Ordering::Equal,
        (Value::Empty, _) => Ordering::Greater,
        (_, Value::Empty) => Ordering::Less,
        _ => {
            let ordering = kind_rank(left)
                .cmp(&kind_rank(right))
                .then_with(|| same_kind_order(left, right));
            if descending {
                ordering.reverse()
            } else {
                ordering
            }
        }
    }
}

fn kind_rank(value: &Value) -> u8 {
    match value {
        Value::Number(_) => 0,
        Value::Text(_) => 1,
        Value::Bool(_) => 2,
        _ => 3,
    }
}

fn same_kind_order(left: &Value, right: &Value) -> Ordering {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.total_cmp(b),
        (Value::Text(a), Value::Text(b)) => a.to_uppercase().cmp(&b.to_uppercase()),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        _ => Ordering::Equal,
    }
}

/// SORTBY(array, by_array1, [sort_order1], [by_array2, [sort_order2]], ...):
/// a stable sort of the rows (or, for a single-row array, the columns) by one
/// or more key arrays. A sort order is 1 (ascending, the default) or -1.
fn sortby(args: &[Expr], lookup: Lookup) -> Result<Value, CalcError> {
    if args.len() < 2 {
        return Err(CalcError::Value);
    }
    let array = block(&args[0], lookup)?;
    let mut keys: Vec<(Block, bool)> = Vec::new();
    let mut index = 1;
    while index < args.len() {
        if !is_key(&args[index], lookup) {
            return Err(CalcError::Value);
        }
        let key = block(&args[index], lookup)?;
        index += 1;
        let mut descending = false;
        if index < args.len() && !is_key(&args[index], lookup) {
            descending = match eval_expr(&args[index], lookup) {
                Value::Number(1.0) => false,
                Value::Number(-1.0) => true,
                Value::Error(error) => return Err(error),
                _ => return Err(CalcError::Value),
            };
            index += 1;
        }
        keys.push((key, descending));
    }
    let by_rows = keys
        .iter()
        .all(|(key, _)| key.cols == 1 && key.rows == array.rows);
    let by_cols = keys
        .iter()
        .all(|(key, _)| key.rows == 1 && key.cols == array.cols);
    if !by_rows && !by_cols {
        return Err(CalcError::Value);
    }
    let count = if by_rows { array.rows } else { array.cols };
    let mut order: Vec<usize> = (0..count).collect();
    order.sort_by(|&a, &b| {
        for (key, descending) in &keys {
            let ordering = sort_order_of(&key.cells[a], &key.cells[b], *descending);
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        Ordering::Equal
    });
    let mut cells = Vec::with_capacity(array.cells.len());
    if by_rows {
        for &row in &order {
            for col in 0..array.cols {
                cells.push(array.at(row, col).clone());
            }
        }
    } else {
        for row in 0..array.rows {
            for &col in &order {
                cells.push(array.at(row, col).clone());
            }
        }
    }
    Ok(spill(cells, array.rows, array.cols))
}

/// RANDARRAY([rows], [columns], [min], [max], [whole_number]): uniform random
/// numbers in [min, max), or whole numbers in [min, max] when requested.
fn randarray(args: &[Expr], lookup: Lookup) -> Result<Value, CalcError> {
    if args.len() > 5 {
        return Err(CalcError::Value);
    }
    let rows = opt_number(args, 0, 1.0, lookup)?.trunc();
    let cols = opt_number(args, 1, 1.0, lookup)?.trunc();
    let low = opt_number(args, 2, 0.0, lookup)?;
    let high = opt_number(args, 3, 1.0, lookup)?;
    let whole = match args.get(4) {
        Some(expr) => to_bool(eval_expr(expr, lookup))?,
        None => false,
    };
    if rows < 1.0 || cols < 1.0 || low > high {
        return Err(CalcError::Value);
    }
    let (rows, cols) = (rows as usize, cols as usize);
    check_size(rows, cols)?;
    let (low, high) = if whole {
        (low.ceil(), high.floor())
    } else {
        (low, high)
    };
    if low > high {
        return Err(CalcError::Value);
    }
    let cells = (0..rows * cols)
        .map(|_| {
            let unit = crate::functions_math::random_unit();
            let value = if whole {
                low + (unit * (high - low + 1.0)).floor()
            } else {
                low + unit * (high - low)
            };
            Value::Number(value)
        })
        .collect();
    Ok(spill(cells, rows, cols))
}

/// The (start, length) of the rows or columns that TAKE or DROP keeps.
fn span(total: usize, count: i64, take: bool) -> Result<(usize, usize), CalcError> {
    let n = count.unsigned_abs().min(total as u64) as usize;
    let (start, len) = match (take, count >= 0) {
        (true, true) => (0, n),
        (true, false) => (total - n, n),
        (false, true) => (n, total - n),
        (false, false) => (0, total - n),
    };
    if len == 0 {
        return Err(CalcError::Calc);
    }
    Ok((start, len))
}

/// TAKE(array, rows, [columns]) keeps leading (positive) or trailing
/// (negative) rows and columns; DROP(array, rows, [columns]) removes them.
fn take_or_drop(args: &[Expr], lookup: Lookup, take: bool) -> Result<Value, CalcError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let array = block(&args[0], lookup)?;
    let rows = arg_number(args, 1, lookup)?.trunc() as i64;
    let (row_start, row_len) = span(array.rows, rows, take)?;
    let (col_start, col_len) = match args.get(2) {
        Some(_) => span(
            array.cols,
            arg_number(args, 2, lookup)?.trunc() as i64,
            take,
        )?,
        None => (0, array.cols),
    };
    let mut cells = Vec::with_capacity(row_len * col_len);
    for row in row_start..row_start + row_len {
        for col in col_start..col_start + col_len {
            cells.push(array.at(row, col).clone());
        }
    }
    Ok(spill(cells, row_len, col_len))
}

/// VSTACK / HSTACK: join arrays end to end. A shorter array is padded with `#N/A`.
fn stack(args: &[Expr], lookup: Lookup, vertical: bool) -> Result<Value, CalcError> {
    if args.is_empty() {
        return Err(CalcError::Value);
    }
    let blocks = args
        .iter()
        .map(|arg| block(arg, lookup))
        .collect::<Result<Vec<_>, _>>()?;
    let (rows, cols) = if vertical {
        (
            blocks.iter().map(|b| b.rows).sum(),
            blocks.iter().map(|b| b.cols).max().unwrap_or(0),
        )
    } else {
        (
            blocks.iter().map(|b| b.rows).max().unwrap_or(0),
            blocks.iter().map(|b| b.cols).sum(),
        )
    };
    check_size(rows, cols)?;
    let mut cells = vec![Value::Error(CalcError::NA); rows * cols];
    let mut offset = 0;
    for b in &blocks {
        for r in 0..b.rows {
            for c in 0..b.cols {
                let target = if vertical {
                    (offset + r) * cols + c
                } else {
                    r * cols + offset + c
                };
                cells[target] = b.at(r, c).clone();
            }
        }
        offset += if vertical { b.rows } else { b.cols };
    }
    Ok(spill(cells, rows, cols))
}

/// TOCOL / TOROW(array, [ignore], [scan_by_column]): one column or row of the
/// values. ignore: 1 drops blanks, 2 drops errors, 3 drops both.
fn flatten_block(args: &[Expr], lookup: Lookup, column: bool) -> Result<Value, CalcError> {
    if args.is_empty() || args.len() > 3 {
        return Err(CalcError::Value);
    }
    let array = block(&args[0], lookup)?;
    let ignore = opt_number(args, 1, 0.0, lookup)?.trunc() as i64;
    if !(0..=3).contains(&ignore) {
        return Err(CalcError::Value);
    }
    let by_column = match args.get(2) {
        Some(expr) => to_bool(eval_expr(expr, lookup))?,
        None => false,
    };
    let mut ordered: Vec<&Value> = Vec::with_capacity(array.cells.len());
    if by_column {
        for col in 0..array.cols {
            for row in 0..array.rows {
                ordered.push(array.at(row, col));
            }
        }
    } else {
        ordered.extend(array.cells.iter());
    }
    let kept: Vec<Value> = ordered
        .into_iter()
        .filter(|value| {
            let drop_blank = ignore & 1 != 0 && matches!(value, Value::Empty);
            let drop_error = ignore & 2 != 0 && matches!(value, Value::Error(_));
            !(drop_blank || drop_error)
        })
        .cloned()
        .collect();
    if kept.is_empty() {
        return Err(CalcError::Calc);
    }
    let n = kept.len();
    Ok(if column {
        spill(kept, n, 1)
    } else {
        spill(kept, 1, n)
    })
}

/// WRAPROWS / WRAPCOLS(vector, wrap_count, [pad_with]): reshape a one-row or
/// one-column vector into slices of `wrap_count`. The last slice is padded
/// with `#N/A`, or with `pad_with` when given.
fn wrap(args: &[Expr], lookup: Lookup, by_row: bool) -> Result<Value, CalcError> {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let vector = block(&args[0], lookup)?;
    if vector.rows != 1 && vector.cols != 1 {
        return Err(CalcError::Value);
    }
    let size = arg_number(args, 1, lookup)?.trunc();
    if size < 1.0 {
        return Err(CalcError::Value);
    }
    let size = size as usize;
    let pad = match args.get(2) {
        Some(expr) => eval_expr(expr, lookup),
        None => Value::Error(CalcError::NA),
    };
    let slices = vector.cells.len().div_ceil(size);
    let (rows, cols) = if by_row {
        (slices, size)
    } else {
        (size, slices)
    };
    check_size(rows, cols)?;
    let mut cells = vec![pad; rows * cols];
    for (k, value) in vector.cells.iter().enumerate() {
        let (r, c) = if by_row {
            (k / size, k % size)
        } else {
            (k % size, k / size)
        };
        cells[r * cols + c] = value.clone();
    }
    Ok(spill(cells, rows, cols))
}

/// CHOOSECOLS / CHOOSEROWS(array, index1, [index2], ...): the named columns or
/// rows in the order given. A negative index counts from the end.
fn choose(args: &[Expr], lookup: Lookup, columns: bool) -> Result<Value, CalcError> {
    if args.len() < 2 {
        return Err(CalcError::Value);
    }
    let array = block(&args[0], lookup)?;
    let total = if columns { array.cols } else { array.rows };
    let mut picks = Vec::with_capacity(args.len() - 1);
    for arg in &args[1..] {
        let k = to_number(eval_expr(arg, lookup))?.trunc() as i64;
        let magnitude = k.unsigned_abs() as usize;
        if magnitude == 0 || magnitude > total {
            return Err(CalcError::Value);
        }
        picks.push(if k > 0 {
            magnitude - 1
        } else {
            total - magnitude
        });
    }
    let (rows, cols) = if columns {
        (array.rows, picks.len())
    } else {
        (picks.len(), array.cols)
    };
    check_size(rows, cols)?;
    let mut cells = Vec::with_capacity(rows * cols);
    if columns {
        for row in 0..array.rows {
            for &col in &picks {
                cells.push(array.at(row, col).clone());
            }
        }
    } else {
        for &row in &picks {
            for col in 0..array.cols {
                cells.push(array.at(row, col).clone());
            }
        }
    }
    Ok(spill(cells, rows, cols))
}

/// EXPAND(array, [rows], [columns], [pad_with]): pad to a larger size with
/// `#N/A` (or `pad_with`). It never shrinks an array.
fn expand(args: &[Expr], lookup: Lookup) -> Result<Value, CalcError> {
    if args.is_empty() || args.len() > 4 {
        return Err(CalcError::Value);
    }
    let array = block(&args[0], lookup)?;
    let rows = match args.get(1) {
        Some(_) => arg_number(args, 1, lookup)?.trunc() as i64,
        None => array.rows as i64,
    };
    let cols = match args.get(2) {
        Some(_) => arg_number(args, 2, lookup)?.trunc() as i64,
        None => array.cols as i64,
    };
    if rows < array.rows as i64 || cols < array.cols as i64 {
        return Err(CalcError::Value);
    }
    let (rows, cols) = (rows as usize, cols as usize);
    check_size(rows, cols)?;
    let pad = match args.get(3) {
        Some(expr) => eval_expr(expr, lookup),
        None => Value::Error(CalcError::NA),
    };
    let mut cells = Vec::with_capacity(rows * cols);
    for row in 0..rows {
        for col in 0..cols {
            cells.push(if row < array.rows && col < array.cols {
                array.at(row, col).clone()
            } else {
                pad.clone()
            });
        }
    }
    Ok(spill(cells, rows, cols))
}
