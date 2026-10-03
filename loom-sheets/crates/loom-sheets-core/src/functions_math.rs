//! Math functions: INT, TRUNC, ROUNDUP, ROUNDDOWN, SIGN, PI, EXP, LN, LOG,
//! LOG10, PRODUCT, SUMPRODUCT, RAND, RANDBETWEEN, MROUND, QUOTIENT.

use crate::functions_util::{arg_number, clean, flatten, grid, numbers, opt_number, to_number};
use crate::{CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;
type Calc = Result<f64, CalcError>;

/// Dispatch math functions from the main evaluator.
pub(crate) fn eval_math_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result = match name {
        "SUM" => aggregate(args, lookup, |v| Ok(v.iter().sum())),
        "AVERAGE" => aggregate(args, lookup, |v| match v.len() {
            0 => Err(CalcError::DivZero),
            n => Ok(v.iter().sum::<f64>() / n as f64),
        }),
        "MIN" => aggregate(args, lookup, |v| {
            Ok(v.iter().copied().reduce(f64::min).unwrap_or(0.0))
        }),
        "MAX" => aggregate(args, lookup, |v| {
            Ok(v.iter().copied().reduce(f64::max).unwrap_or(0.0))
        }),
        "MEDIAN" => aggregate(args, lookup, median),
        "COUNT" => count(args, lookup),
        "ROUND" => round(args, lookup),
        "INT" => unary(args, lookup, |n| Ok(n.floor())),
        "SIGN" => unary(args, lookup, |n| {
            Ok(if n == 0.0 { 0.0 } else { n.signum() })
        }),
        "EXP" => unary(args, lookup, |n| Ok(n.exp())),
        "LN" => unary(args, lookup, |n| positive(n).map(f64::ln)),
        "LOG10" => unary(args, lookup, |n| positive(n).map(f64::log10)),
        "TRUNC" => round_to(args, lookup, Rounding::Toward0, false),
        "ROUNDUP" => round_to(args, lookup, Rounding::Away, true),
        "ROUNDDOWN" => round_to(args, lookup, Rounding::Toward0, true),
        "PI" | "RAND" if !args.is_empty() => Err(CalcError::Value),
        "PI" => Ok(std::f64::consts::PI),
        "RAND" => Ok(random_unit()),
        "LOG" => log(args, lookup),
        "PRODUCT" => product(args, lookup),
        "SUMPRODUCT" => sumproduct(args, lookup),
        "RANDBETWEEN" => randbetween(args, lookup),
        "MROUND" => mround(args, lookup),
        "QUOTIENT" => quotient(args, lookup),
        _ => return None,
    };
    Some(match result {
        Ok(n) if n.is_finite() => Value::Number(n),
        Ok(_) => Value::Error(CalcError::Num),
        Err(error) => Value::Error(error),
    })
}

/// SUM, AVERAGE, MIN, MAX and MEDIAN see the numbers in their arguments:
/// text, booleans and blanks inside ranges are skipped, a direct non-numeric
/// text argument is `#VALUE!`, and any error value is the result.
fn aggregate(args: &[Expr], lookup: Lookup, f: impl Fn(&[f64]) -> Calc) -> Calc {
    if args.is_empty() {
        return Err(CalcError::Value);
    }
    f(&numbers(args, lookup)?)
}

fn median(values: &[f64]) -> Calc {
    if values.is_empty() {
        return Err(CalcError::Num);
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let mid = sorted.len() / 2;
    Ok(if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    })
}

/// COUNT counts numbers; a direct boolean or numeric-text argument counts,
/// the same text inside a range does not. Errors are not counted.
fn count(args: &[Expr], lookup: Lookup) -> Calc {
    if args.is_empty() {
        return Err(CalcError::Value);
    }
    let mut total = 0usize;
    for arg in args {
        if matches!(arg, Expr::Range { .. } | Expr::Cell(_)) {
            total += flatten(arg, lookup)
                .iter()
                .filter(|v| matches!(v, Value::Number(_)))
                .count();
            continue;
        }
        total += match crate::eval_expr(arg, lookup) {
            Value::Number(_) | Value::Bool(_) => 1,
            Value::Text(text) => usize::from(to_number(Value::Text(text)).is_ok()),
            Value::Array(items, _, _) => items
                .iter()
                .filter(|v| matches!(v, Value::Number(_)))
                .count(),
            _ => 0,
        };
    }
    Ok(total as f64)
}

/// ROUND half away from zero, on the decimal value as written.
fn round(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let n = arg_number(args, 0, lookup)?;
    let digits = arg_number(args, 1, lookup)?.trunc() as i32;
    Ok(scaled(n, digits).round() / 10f64.powi(digits))
}

fn unary(args: &[Expr], lookup: Lookup, f: impl Fn(f64) -> Calc) -> Calc {
    if args.len() != 1 {
        return Err(CalcError::Value);
    }
    f(arg_number(args, 0, lookup)?)
}

fn positive(n: f64) -> Calc {
    if n > 0.0 {
        Ok(n)
    } else {
        Err(CalcError::Num)
    }
}

#[derive(Clone, Copy)]
enum Rounding {
    Toward0,
    Away,
}

/// Round `n * 10^digits` to 15 significant digits so binary noise such as
/// `1.1 * 10 = 11.000000000000002` cannot push ROUNDUP/TRUNC a whole step.
fn scaled(n: f64, digits: i32) -> f64 {
    clean(n * 10f64.powi(digits))
}

fn round_to(args: &[Expr], lookup: Lookup, mode: Rounding, digits_required: bool) -> Calc {
    let allowed = if digits_required { 2..=2 } else { 1..=2 };
    if !allowed.contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let n = arg_number(args, 0, lookup)?;
    let digits = opt_number(args, 1, 0.0, lookup)?.trunc() as i32;
    let scaled = scaled(n, digits);
    let stepped = match mode {
        Rounding::Toward0 => scaled.trunc(),
        Rounding::Away => scaled.abs().ceil().copysign(n),
    };
    Ok(stepped / 10f64.powi(digits))
}

fn log(args: &[Expr], lookup: Lookup) -> Calc {
    if args.is_empty() || args.len() > 2 {
        return Err(CalcError::Value);
    }
    let n = positive(arg_number(args, 0, lookup)?)?;
    let base = positive(opt_number(args, 1, 10.0, lookup)?)?;
    if base == 1.0 {
        return Err(CalcError::DivZero);
    }
    Ok(n.ln() / base.ln())
}

/// PRODUCT multiplies the numbers it sees; with none it is 0, as in Excel.
fn product(args: &[Expr], lookup: Lookup) -> Calc {
    if args.is_empty() {
        return Err(CalcError::Value);
    }
    let values = numbers(args, lookup)?;
    Ok(if values.is_empty() {
        0.0
    } else {
        values.iter().product()
    })
}

/// SUMPRODUCT of equally sized ranges/arrays; non-numbers count as zero.
fn sumproduct(args: &[Expr], lookup: Lookup) -> Calc {
    if args.is_empty() {
        return Err(CalcError::Value);
    }
    let mut columns: Vec<Vec<f64>> = Vec::new();
    let mut shape = None;
    for arg in args {
        let (values, rows, cols) = grid(arg, lookup);
        if *shape.get_or_insert((rows, cols)) != (rows, cols) {
            return Err(CalcError::Value);
        }
        let mut column = Vec::with_capacity(values.len());
        for value in values {
            column.push(match value {
                Value::Number(n) => n,
                Value::Error(error) => return Err(error),
                _ => 0.0,
            });
        }
        columns.push(column);
    }
    let len = columns[0].len();
    Ok((0..len)
        .map(|i| columns.iter().map(|column| column[i]).product::<f64>())
        .sum())
}

fn random_unit() -> f64 {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
    );
    (hasher.finish() >> 11) as f64 / (1u64 << 53) as f64
}

/// RANDBETWEEN: whole number in `[ceil(bottom), floor(top)]`.
fn randbetween(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let bottom = arg_number(args, 0, lookup)?.ceil();
    let top = arg_number(args, 1, lookup)?.floor();
    if bottom > top {
        return Err(CalcError::Num);
    }
    Ok(bottom + (random_unit() * (top - bottom + 1.0)).floor())
}

/// MROUND: nearest multiple, half away from zero; the signs must agree.
fn mround(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let n = arg_number(args, 0, lookup)?;
    let multiple = arg_number(args, 1, lookup)?;
    if multiple == 0.0 {
        return Ok(0.0);
    }
    if n * multiple < 0.0 {
        return Err(CalcError::Num);
    }
    Ok(scaled(n / multiple, 0).round() * multiple)
}

/// QUOTIENT: integer part of a division, truncated toward zero.
fn quotient(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 2 {
        return Err(CalcError::Value);
    }
    let numerator = arg_number(args, 0, lookup)?;
    let denominator = arg_number(args, 1, lookup)?;
    if denominator == 0.0 {
        return Err(CalcError::DivZero);
    }
    Ok(scaled(numerator / denominator, 0).trunc())
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

    fn num(formula: &str) -> f64 {
        match eval(formula) {
            Value::Number(n) => n,
            other => panic!("{formula} -> {other:?}"),
        }
    }

    fn close(formula: &str, want: f64, tolerance: f64) {
        let got = num(formula);
        assert!((got - want).abs() < tolerance, "{formula}: {got} vs {want}");
    }

    fn cells(setup: &[(&str, &str)], probes: &[&str]) -> Vec<Option<Value>> {
        let mut sheet = Sheet::new("t");
        for (at, raw) in setup {
            sheet.set_str(at, raw);
        }
        let values = evaluate(&sheet);
        probes
            .iter()
            .map(|at| values.get(&CellRef::parse(at).unwrap()).cloned())
            .collect()
    }

    #[test]
    fn int_truncates_towards_negative_infinity() {
        assert_eq!(num("=INT(2.7)"), 2.0);
        assert_eq!(num("=INT(-2.7)"), -3.0);
        assert_eq!(num("=INT(2.0)"), 2.0);
    }

    #[test]
    fn trunc_truncates_towards_zero() {
        assert_eq!(num("=TRUNC(2.7)"), 2.0);
        assert_eq!(num("=TRUNC(-2.7)"), -2.0);
        assert_eq!(num("=TRUNC(2.5, 1)"), 2.5);
        assert_eq!(num("=TRUNC(12.345, 2)"), 12.34);
    }

    #[test]
    fn roundup_rounds_away_from_zero() {
        assert_eq!(num("=ROUNDUP(2.3,0)"), 3.0);
        assert_eq!(num("=ROUNDUP(-2.3,0)"), -3.0);
        assert_eq!(num("=ROUNDUP(2.12, 1)"), 2.2);
    }

    #[test]
    fn rounddown_rounds_towards_zero() {
        assert_eq!(num("=ROUNDDOWN(2.7,0)"), 2.0);
        assert_eq!(num("=ROUNDDOWN(-2.7,0)"), -2.0);
        assert_eq!(num("=ROUNDDOWN(2.19, 1)"), 2.1);
    }

    #[test]
    fn rounding_is_not_fooled_by_binary_noise() {
        assert_eq!(num("=ROUNDUP(1.1,1)"), 1.1);
        assert_eq!(num("=TRUNC(0.29,2)"), 0.29);
        assert_eq!(num("=ROUNDDOWN(2.675,2)"), 2.67);
        assert_eq!(num("=ROUNDUP(-1.1,1)"), -1.1);
        assert_eq!(num("=ROUNDUP(1234,-2)"), 1300.0);
    }

    #[test]
    fn sign_returns_negative_zero_or_positive() {
        assert_eq!(num("=SIGN(5)"), 1.0);
        assert_eq!(num("=SIGN(-5)"), -1.0);
        assert_eq!(num("=SIGN(0)"), 0.0);
    }

    #[test]
    fn pi_returns_mathematical_constant() {
        close("=PI()", std::f64::consts::PI, 1e-10);
        assert!(matches!(eval("=PI(1)"), Value::Error(CalcError::Value)));
    }

    #[test]
    fn exp_raises_e_to_power() {
        close("=EXP(1)", std::f64::consts::E, 1e-10);
        close("=EXP(0)", 1.0, 1e-10);
    }

    #[test]
    fn ln_is_natural_logarithm() {
        close("=LN(1)", 0.0, 1e-10);
        close("=LN(2.718281828)", 1.0, 1e-9);
    }

    #[test]
    fn log_uses_specified_base_default_10() {
        close("=LOG(100)", 2.0, 1e-10);
        close("=LOG(100,10)", 2.0, 1e-10);
        close("=LOG(8,2)", 3.0, 1e-10);
        assert!(matches!(
            eval("=LOG(8,1)"),
            Value::Error(CalcError::DivZero)
        ));
    }

    #[test]
    fn log10_name_with_digits_is_a_function() {
        close("=LOG10(1000)", 3.0, 1e-12);
        assert!(matches!(eval("=LOG10(0)"), Value::Error(CalcError::Num)));
    }

    #[test]
    fn product_multiplies_all_numbers() {
        assert_eq!(num("=PRODUCT(2,3,4)"), 24.0);
        assert_eq!(num("=PRODUCT(5)"), 5.0);
    }

    #[test]
    fn product_skips_blanks_and_text_inside_ranges() {
        let got = cells(
            &[
                ("A1", "2"),
                ("A3", "5"),
                ("A4", "text"),
                ("B1", "=PRODUCT(A1:A4)"),
                ("B2", "=PRODUCT(C1:C3)"),
                ("B3", "=PRODUCT(A1:A2,3)"),
            ],
            &["B1", "B2", "B3"],
        );
        assert_eq!(got[0], Some(Value::Number(10.0)));
        assert_eq!(got[1], Some(Value::Number(0.0)));
        assert_eq!(got[2], Some(Value::Number(6.0)));
    }

    #[test]
    fn sumproduct_multiplies_ranges_and_checks_shapes() {
        let got = cells(
            &[
                ("A1", "1"),
                ("A2", "3"),
                ("A3", "5"),
                ("B1", "2"),
                ("B2", "4"),
                ("B3", "6"),
                ("C1", "=SUMPRODUCT(A1:A3,B1:B3)"),
                ("C2", "=SUMPRODUCT(A1:A3)"),
                ("C3", "=SUMPRODUCT(A1:A3,B1:B2)"),
            ],
            &["C1", "C2", "C3"],
        );
        assert_eq!(got[0], Some(Value::Number(44.0)));
        assert_eq!(got[1], Some(Value::Number(9.0)));
        assert_eq!(got[2], Some(Value::Error(CalcError::Value)));
    }

    #[test]
    fn randbetween_stays_inside_its_bounds() {
        for _ in 0..200 {
            let n = num("=RANDBETWEEN(-3,4)");
            assert!((-3.0..=4.0).contains(&n) && n.fract() == 0.0, "{n}");
        }
        assert!(matches!(
            eval("=RANDBETWEEN(5,1)"),
            Value::Error(CalcError::Num)
        ));
        assert!((0.0..1.0).contains(&num("=RAND()")));
    }

    #[test]
    fn mround_rounds_to_nearest_multiple() {
        assert_eq!(num("=MROUND(10,3)"), 9.0);
        assert_eq!(num("=MROUND(10.1,0.5)"), 10.0);
        assert_eq!(num("=MROUND(0,3)"), 0.0);
        assert_eq!(num("=MROUND(-10,-3)"), -9.0);
        assert!(matches!(
            eval("=MROUND(5,-2)"),
            Value::Error(CalcError::Num)
        ));
    }

    #[test]
    fn quotient_truncates_toward_zero() {
        assert_eq!(num("=QUOTIENT(5,2)"), 2.0);
        assert_eq!(num("=QUOTIENT(-5,2)"), -2.0);
        assert!(matches!(
            eval("=QUOTIENT(1,0)"),
            Value::Error(CalcError::DivZero)
        ));
    }

    #[test]
    fn errors_propagate_instead_of_becoming_value_errors() {
        assert!(matches!(
            eval("=INT(1/0)"),
            Value::Error(CalcError::DivZero)
        ));
        assert!(matches!(
            eval("=INT(\"x\")"),
            Value::Error(CalcError::Value)
        ));
    }

    #[test]
    fn round_follows_the_decimal_value_not_the_binary_one() {
        assert_eq!(num("=ROUND(2.675,2)"), 2.68);
        assert_eq!(num("=ROUND(1.005,2)"), 1.01);
        assert_eq!(num("=ROUND(-2.5,0)"), -3.0);
        assert_eq!(num("=ROUND(1234.5,-2)"), 1200.0);
    }

    #[test]
    fn aggregates_of_nothing_follow_excel() {
        assert_eq!(num("=MIN(C1:C3)"), 0.0);
        assert_eq!(num("=MAX(C1:C3)"), 0.0);
        assert_eq!(num("=SUM(C1:C3)"), 0.0);
        assert!(matches!(
            eval("=AVERAGE(C1:C3)"),
            Value::Error(CalcError::DivZero)
        ));
        assert!(matches!(
            eval("=MEDIAN(C1:C3)"),
            Value::Error(CalcError::Num)
        ));
        assert_eq!(num("=COUNT(C1:C3,TRUE,\"2\",\"x\")"), 2.0);
        assert_eq!(num("=MEDIAN(1,3,2,10)"), 2.5);
    }
}
