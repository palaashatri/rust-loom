//! Statistics (spike C): PERCENTILE, QUARTILE and PERCENTRANK in their .INC and
//! .EXC forms, RANK.AVG, CORREL, SLOPE, INTERCEPT, FORECAST and FORECAST.LINEAR.
//! Non-numeric cells are skipped, except that paired data drops a whole pair.

use crate::functions_util::{arg_number, flatten, numbers, opt_number};
use crate::{CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;
type Calc = Result<f64, CalcError>;

/// Dispatch the statistics functions from the main evaluator.
pub(crate) fn eval_stats_more_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result = match name {
        "PERCENTILE" | "PERCENTILE.INC" => percentile(args, lookup, false),
        "PERCENTILE.EXC" => percentile(args, lookup, true),
        "QUARTILE" | "QUARTILE.INC" => quartile(args, lookup, false),
        "QUARTILE.EXC" => quartile(args, lookup, true),
        "PERCENTRANK" | "PERCENTRANK.INC" => percentrank(args, lookup, false),
        "PERCENTRANK.EXC" => percentrank(args, lookup, true),
        "RANK.AVG" => rank_avg(args, lookup),
        "CORREL" => correl(args, lookup),
        "SLOPE" => line(args, lookup).map(|(slope, _)| slope),
        "INTERCEPT" => line(args, lookup).map(|(_, intercept)| intercept),
        "FORECAST" | "FORECAST.LINEAR" => forecast(args, lookup),
        _ => return None,
    };
    Some(match result {
        Ok(n) => Value::Number(n),
        Err(error) => Value::Error(error),
    })
}

/// The numbers of the first argument (an array or range), ascending.
fn sorted_numbers(args: &[Expr], lookup: Lookup) -> Result<Vec<f64>, CalcError> {
    let mut values = numbers(&args[..1], lookup)?;
    values.sort_by(f64::total_cmp);
    Ok(values)
}

/// PERCENTILE.INC: linear interpolation at rank k * (n - 1), k in [0, 1].
fn percentile_inc(sorted: &[f64], k: f64) -> Calc {
    if sorted.is_empty() || !(0.0..=1.0).contains(&k) {
        return Err(CalcError::Num);
    }
    let rank = k * (sorted.len() - 1) as f64;
    let low = rank.floor() as usize;
    let fraction = rank - low as f64;
    if fraction > 0.0 && low + 1 < sorted.len() {
        Ok(sorted[low] + fraction * (sorted[low + 1] - sorted[low]))
    } else {
        Ok(sorted[low])
    }
}

/// PERCENTILE.EXC: interpolation at 1-based rank k * (n + 1), k in (0, 1).
fn percentile_exc(sorted: &[f64], k: f64) -> Calc {
    let n = sorted.len();
    if n == 0 || !(k > 0.0 && k < 1.0) {
        return Err(CalcError::Num);
    }
    let rank = k * (n + 1) as f64;
    if rank < 1.0 || rank > n as f64 {
        return Err(CalcError::Num);
    }
    let low = rank.floor() as usize;
    let fraction = rank - low as f64;
    let below = sorted[low - 1];
    if fraction > 0.0 && low < n {
        Ok(below + fraction * (sorted[low] - below))
    } else {
        Ok(below)
    }
}

/// PERCENTILE(.INC) and PERCENTILE.EXC(array, k).
fn percentile(args: &[Expr], lookup: Lookup, exclusive: bool) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let sorted = sorted_numbers(args, lookup)?;
    let k = arg_number(args, 1, lookup)?;
    if exclusive {
        percentile_exc(&sorted, k)
    } else {
        percentile_inc(&sorted, k)
    }
}

/// QUARTILE(.INC) and QUARTILE.EXC(array, quart): the percentile at quart / 4.
/// Out-of-range quartiles fail through the percentile range check.
fn quartile(args: &[Expr], lookup: Lookup, exclusive: bool) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let sorted = sorted_numbers(args, lookup)?;
    let quart = arg_number(args, 1, lookup)?.trunc();
    let k = quart / 4.0;
    if exclusive {
        percentile_exc(&sorted, k)
    } else {
        percentile_inc(&sorted, k)
    }
}

/// Fractional zero-based position of `x` in ascending `sorted`: the first
/// index of an equal value, or an interpolation between two neighbours.
fn position_of(sorted: &[f64], x: f64) -> Option<f64> {
    let (first, last) = (*sorted.first()?, *sorted.last()?);
    if x < first || x > last {
        return None;
    }
    if let Some(index) = sorted.iter().position(|v| *v == x) {
        return Some(index as f64);
    }
    let below = sorted.partition_point(|v| *v < x) - 1;
    let (a, b) = (sorted[below], sorted[below + 1]);
    Some(below as f64 + (x - a) / (b - a))
}

/// PERCENTRANK.INC(array, x, [significance]) and PERCENTRANK.EXC: the rank of
/// `x` as a fraction, truncated (not rounded) to `significance` digits.
fn percentrank(args: &[Expr], lookup: Lookup, exclusive: bool) -> Calc {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let sorted = sorted_numbers(args, lookup)?;
    let x = arg_number(args, 1, lookup)?;
    let digits = opt_number(args, 2, 3.0, lookup)?.trunc();
    if digits < 1.0 {
        return Err(CalcError::Num);
    }
    let n = sorted.len();
    if n == 0 {
        return Err(CalcError::NA);
    }
    let position = position_of(&sorted, x).ok_or(CalcError::NA)?;
    let rank = if exclusive {
        (position + 1.0) / (n as f64 + 1.0)
    } else if n == 1 {
        1.0
    } else {
        position / (n as f64 - 1.0)
    };
    let factor = 10f64.powi(digits as i32);
    Ok((rank * factor + 1e-9).floor() / factor)
}

/// RANK.AVG(number, ref, [order]): the rank of `number`, where tied values
/// share the mean of the ranks they occupy. Order 0 ranks largest first.
fn rank_avg(args: &[Expr], lookup: Lookup) -> Calc {
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
    let ties = values.iter().filter(|v| **v == value).count();
    Ok(ahead as f64 + (ties as f64 + 1.0) / 2.0)
}

/// Numeric pairs from two equal-length arguments, as (first, second). A pair
/// with text, a logical or a blank on either side is dropped; an error stops.
fn paired(first: &Expr, second: &Expr, lookup: Lookup) -> Result<(Vec<f64>, Vec<f64>), CalcError> {
    let a = flatten(first, lookup);
    let b = flatten(second, lookup);
    if a.len() != b.len() {
        return Err(CalcError::NA);
    }
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for (x, y) in a.into_iter().zip(b) {
        match (x, y) {
            (Value::Error(error), _) | (_, Value::Error(error)) => return Err(error),
            (Value::Number(x), Value::Number(y)) => {
                left.push(x);
                right.push(y);
            }
            _ => {}
        }
    }
    Ok((left, right))
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

/// Least-squares slope and intercept of `ys` on `xs`.
fn fit(xs: &[f64], ys: &[f64]) -> Result<(f64, f64), CalcError> {
    if xs.is_empty() {
        return Err(CalcError::DivZero);
    }
    let (mx, my) = (mean(xs), mean(ys));
    let sxx: f64 = xs.iter().map(|x| (x - mx) * (x - mx)).sum();
    let sxy: f64 = xs.iter().zip(ys).map(|(x, y)| (x - mx) * (y - my)).sum();
    if sxx == 0.0 {
        return Err(CalcError::DivZero);
    }
    let slope = sxy / sxx;
    Ok((slope, my - slope * mx))
}

/// SLOPE / INTERCEPT(known_y's, known_x's).
fn line(args: &[Expr], lookup: Lookup) -> Result<(f64, f64), CalcError> {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let (ys, xs) = paired(&args[0], &args[1], lookup)?;
    fit(&xs, &ys)
}

/// FORECAST(x, known_y's, known_x's) and FORECAST.LINEAR: the fitted value at x.
fn forecast(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 3 {
        return Err(CalcError::Value);
    }
    let x = arg_number(args, 0, lookup)?;
    let (ys, xs) = paired(&args[1], &args[2], lookup)?;
    let (slope, intercept) = fit(&xs, &ys)?;
    Ok(intercept + slope * x)
}

/// CORREL(array1, array2): the Pearson correlation of the numeric pairs.
fn correl(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let (a, b) = paired(&args[0], &args[1], lookup)?;
    if a.len() < 2 {
        return Err(CalcError::DivZero);
    }
    let (ma, mb) = (mean(&a), mean(&b));
    let saa: f64 = a.iter().map(|x| (x - ma) * (x - ma)).sum();
    let sbb: f64 = b.iter().map(|y| (y - mb) * (y - mb)).sum();
    let sab: f64 = a.iter().zip(&b).map(|(x, y)| (x - ma) * (y - mb)).sum();
    if saa == 0.0 || sbb == 0.0 {
        return Err(CalcError::DivZero);
    }
    Ok(sab / (saa * sbb).sqrt())
}
