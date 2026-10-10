//! Statistics functions: LARGE, SMALL, RANK, RANK.EQ, STDEV, STDEV.S,
//! STDEV.P, VAR, VAR.S, VAR.P, MODE, MODE.SNGL, COUNTBLANK, SUMIFS, COUNTIFS,
//! AVERAGEIFS.

use crate::functions::{criteria_matches, expand_cells};
use crate::functions_util::{arg_number, numbers, opt_number};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;
type Calc = Result<f64, CalcError>;

/// Dispatch statistics functions from the main evaluator.
pub(crate) fn eval_stats_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result = match name {
        "LARGE" => nth(args, lookup, true),
        "SMALL" => nth(args, lookup, false),
        "RANK" | "RANK.EQ" => rank(args, lookup),
        "STDEV" | "STDEV.S" => spread(args, lookup, true, true),
        "STDEV.P" => spread(args, lookup, false, true),
        "VAR" | "VAR.S" => spread(args, lookup, true, false),
        "VAR.P" => spread(args, lookup, false, false),
        "MODE" | "MODE.SNGL" => mode(args, lookup),
        "COUNTBLANK" => countblank(args, lookup),
        "SUMIFS" => ifs(args, lookup, IfsKind::Sum),
        "COUNTIFS" => ifs(args, lookup, IfsKind::Count),
        "AVERAGEIFS" => ifs(args, lookup, IfsKind::Average),
        "MINIFS" => ifs(args, lookup, IfsKind::Min),
        "MAXIFS" => ifs(args, lookup, IfsKind::Max),
        _ => return None,
    };
    Some(match result {
        Ok(n) => Value::Number(n),
        Err(error) => Value::Error(error),
    })
}

/// LARGE/SMALL(array, k): LARGE takes the ceil(k)-th value from the top and SMALL
/// the floor(k)-th from the bottom. k is checked as written, before it is rounded:
/// anything below 1 or above the count is #NUM!.
fn nth(args: &[Expr], lookup: Lookup, largest: bool) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let mut values = numbers(&args[..1], lookup)?;
    let k = arg_number(args, 1, lookup)?;
    if values.is_empty() || k < 1.0 || k > values.len() as f64 {
        return Err(CalcError::Num);
    }
    let rounded = if largest { k.ceil() } else { k.floor() };
    values.sort_by(|a, b| a.total_cmp(b));
    if largest {
        values.reverse();
    }
    Ok(values[rounded as usize - 1])
}

/// RANK(value, ref, [order]): 1 is the largest unless `order` is non-zero.
fn rank(args: &[Expr], lookup: Lookup) -> Calc {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let value = arg_number(args, 0, lookup)?;
    let values = numbers(&args[1..2], lookup)?;
    let ascending = opt_number(args, 2, 0.0, lookup)? != 0.0;
    if !values.contains(&value) {
        return Err(CalcError::NA);
    }
    let ahead = values
        .iter()
        .filter(|v| if ascending { **v < value } else { **v > value })
        .count();
    Ok(ahead as f64 + 1.0)
}

/// Variance or standard deviation of the numbers given, sample or population.
fn spread(args: &[Expr], lookup: Lookup, sample: bool, root: bool) -> Calc {
    if args.is_empty() {
        return Err(CalcError::Value);
    }
    let values = numbers(args, lookup)?;
    let count = values.len() as f64;
    let needed = if sample { 2.0 } else { 1.0 };
    if count < needed {
        return Err(CalcError::DivZero);
    }
    let mean = values.iter().sum::<f64>() / count;
    let squares: f64 = values.iter().map(|v| (v - mean).powi(2)).sum();
    let variance = squares / if sample { count - 1.0 } else { count };
    Ok(if root { variance.sqrt() } else { variance })
}

/// MODE: the most frequent number, the first to reach that count on ties;
/// `#N/A` when nothing repeats.
fn mode(args: &[Expr], lookup: Lookup) -> Calc {
    if args.is_empty() {
        return Err(CalcError::Value);
    }
    let values = numbers(args, lookup)?;
    let mut best: Option<(f64, usize)> = None;
    for value in &values {
        let count = values.iter().filter(|other| *other == value).count();
        if count > 1 && best.map_or(true, |(_, top)| count > top) {
            best = Some((*value, count));
        }
    }
    best.map(|(value, _)| value).ok_or(CalcError::NA)
}

/// COUNTBLANK counts empty cells and cells holding `""`.
fn countblank(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    let cells = expand_cells(&args[0]).ok_or(CalcError::Value)?;
    Ok(cells
        .iter()
        .filter(|cell| match lookup(**cell) {
            Value::Empty => true,
            Value::Text(text) => text.is_empty(),
            _ => false,
        })
        .count() as f64)
}

#[derive(Clone, Copy, PartialEq)]
enum IfsKind {
    Sum,
    Count,
    Average,
    Min,
    Max,
}

/// SUMIFS / COUNTIFS / AVERAGEIFS / MINIFS / MAXIFS: criteria ranges must share one size, and
/// a cell counts when every criterion matches.
fn ifs(args: &[Expr], lookup: Lookup, kind: IfsKind) -> Calc {
    let (target, pairs) = match kind {
        IfsKind::Count => (None, args),
        _ => (args.first(), args.get(1..).unwrap_or(&[])),
    };
    if pairs.is_empty() || pairs.len() % 2 != 0 || (kind != IfsKind::Count && target.is_none()) {
        return Err(CalcError::Value);
    }
    let target_cells = match target {
        Some(expr) => Some(expand_cells(expr).ok_or(CalcError::Value)?),
        None => None,
    };
    let mut matched: Option<Vec<bool>> = None;
    for pair in pairs.chunks(2) {
        let cells = expand_cells(&pair[0]).ok_or(CalcError::Value)?;
        let expected = target_cells.as_ref().map_or(cells.len(), Vec::len);
        let mask = matched.get_or_insert_with(|| vec![true; expected]);
        if cells.len() != mask.len() || cells.is_empty() {
            return Err(CalcError::Value);
        }
        let criteria = match eval_expr(&pair[1], lookup) {
            Value::Error(error) => return Err(error),
            other => other.display(),
        };
        for (hit, cell) in mask.iter_mut().zip(&cells) {
            *hit = *hit && criteria_matches(&lookup(*cell), &criteria);
        }
    }
    let mask = matched.unwrap_or_default();
    let Some(target_cells) = target_cells else {
        return Ok(mask.iter().filter(|hit| **hit).count() as f64);
    };
    let (mut total, mut count) = (0.0, 0usize);
    let mut extreme: Option<f64> = None;
    for (hit, cell) in mask.iter().zip(&target_cells) {
        if !hit {
            continue;
        }
        match lookup(*cell) {
            Value::Number(n) => {
                total += n;
                count += 1;
                extreme = Some(match (kind, extreme) {
                    (IfsKind::Min, Some(current)) => current.min(n),
                    (IfsKind::Max, Some(current)) => current.max(n),
                    _ => n,
                });
            }
            Value::Error(error) => return Err(error),
            _ => {}
        }
    }
    match kind {
        IfsKind::Average if count == 0 => Err(CalcError::DivZero),
        IfsKind::Average => Ok(total / count as f64),
        IfsKind::Min | IfsKind::Max => Ok(extreme.unwrap_or(0.0)),
        _ => Ok(total),
    }
}

#[cfg(test)]
mod tests {
    use crate::{evaluate, CalcError, CellRef, Sheet, Value};

    fn eval(setup: &[(&str, &str)], formula: &str) -> Value {
        let mut sheet = Sheet::new("t");
        for (at, raw) in setup {
            sheet.set_str(at, raw);
        }
        sheet.set_str("Z1", formula);
        evaluate(&sheet)
            .get(&CellRef::parse("Z1").unwrap())
            .cloned()
            .unwrap_or(Value::Empty)
    }

    fn column(values: &[&str]) -> Vec<(String, String)> {
        values
            .iter()
            .enumerate()
            .map(|(i, v)| (format!("A{}", i + 1), (*v).to_string()))
            .collect()
    }

    fn run(values: &[&str], formula: &str) -> Value {
        let owned = column(values);
        let cells: Vec<(&str, &str)> = owned
            .iter()
            .map(|(a, b)| (a.as_str(), b.as_str()))
            .collect();
        eval(&cells, formula)
    }

    fn close(value: Value, want: f64) {
        match value {
            Value::Number(n) => assert!((n - want).abs() < 1e-9, "{n} vs {want}"),
            other => panic!("expected {want}, got {other:?}"),
        }
    }

    const DATA: [&str; 8] = ["2", "4", "4", "4", "5", "5", "7", "9"];

    #[test]
    fn dotted_function_names_calculate() {
        close(run(&DATA, "=STDEV.P(A1:A8)"), 2.0);
        close(run(&DATA, "=VAR.P(A1:A8)"), 4.0);
        close(run(&DATA, "=VAR.S(A1:A8)"), 32.0 / 7.0);
        close(run(&DATA, "=STDEV.S(A1:A8)"), (32.0f64 / 7.0).sqrt());
        close(run(&DATA, "=STDEV(A1:A8)"), (32.0f64 / 7.0).sqrt());
        close(run(&DATA, "=MODE.SNGL(A1:A8)"), 4.0);
        close(run(&DATA, "=RANK.EQ(5,A1:A8)"), 3.0);
        close(run(&DATA, "=RANK.EQ(5,A1:A8,1)"), 5.0);
    }

    #[test]
    fn spread_of_too_few_numbers_is_div_zero() {
        assert_eq!(
            run(&["3"], "=STDEV.S(A1:A1)"),
            Value::Error(CalcError::DivZero)
        );
        assert_eq!(run(&["3"], "=VAR.P(A1:A1)"), Value::Number(0.0));
        assert_eq!(
            run(&[], "=STDEV.P(A1:A3)"),
            Value::Error(CalcError::DivZero)
        );
    }

    #[test]
    fn large_and_small_respect_the_count_of_numbers() {
        close(run(&DATA, "=LARGE(A1:A8,1)"), 9.0);
        close(run(&DATA, "=SMALL(A1:A8,2)"), 4.0);
        // Blank and text cells are not numbers: asking past them is #NUM!, not a crash.
        let sparse = ["1", "", "text", "3"];
        close(run(&sparse, "=LARGE(A1:A4,2)"), 1.0);
        assert_eq!(
            run(&sparse, "=LARGE(A1:A4,3)"),
            Value::Error(CalcError::Num)
        );
        assert_eq!(run(&DATA, "=SMALL(A1:A8,0)"), Value::Error(CalcError::Num));
    }

    #[test]
    fn rank_of_a_missing_value_is_na() {
        assert_eq!(run(&DATA, "=RANK(6,A1:A8)"), Value::Error(CalcError::NA));
        close(run(&DATA, "=RANK(9,A1:A8)"), 1.0);
    }

    #[test]
    fn mode_returns_the_first_of_tied_values_and_na_without_repeats() {
        close(run(&["3", "1", "1", "3", "2"], "=MODE(A1:A5)"), 3.0);
        assert_eq!(
            run(&["1", "2", "3"], "=MODE(A1:A3)"),
            Value::Error(CalcError::NA)
        );
    }

    #[test]
    fn errors_in_data_propagate() {
        assert_eq!(
            run(&["1", "=1/0", "3"], "=STDEV.S(A1:A3)"),
            Value::Error(CalcError::DivZero)
        );
    }

    #[test]
    fn countblank_counts_empty_strings_too() {
        let cells = [("A1", "x"), ("A3", "=\"\""), ("B1", "=COUNTBLANK(A1:A4)")];
        let mut sheet = Sheet::new("t");
        for (at, raw) in cells {
            sheet.set_str(at, raw);
        }
        let values = evaluate(&sheet);
        assert_eq!(
            values.get(&CellRef::parse("B1").unwrap()),
            Some(&Value::Number(3.0))
        );
    }

    #[test]
    fn conditional_aggregates_match_all_criteria() {
        let cells = [
            ("A1", "red"),
            ("A2", "blue"),
            ("A3", "red"),
            ("B1", "10"),
            ("B2", "20"),
            ("B3", "30"),
        ];
        close(
            eval(&cells, "=SUMIFS(B1:B3,A1:A3,\"red\",B1:B3,\">10\")"),
            30.0,
        );
        close(eval(&cells, "=COUNTIFS(A1:A3,\"red\")"), 2.0);
        close(eval(&cells, "=AVERAGEIFS(B1:B3,A1:A3,\"red\")"), 20.0);
        assert_eq!(
            eval(&cells, "=AVERAGEIFS(B1:B3,A1:A3,\"green\")"),
            Value::Error(CalcError::DivZero)
        );
        assert_eq!(
            eval(&cells, "=SUMIFS(B1:B3,A1:A2,\"red\")"),
            Value::Error(CalcError::Value)
        );
    }
}
