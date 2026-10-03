//! Finance functions: NPV, IRR, NPER, RATE (PMT, FV and PV live in
//! `functions.rs` on top of [`crate::pmt`], [`crate::fv`] and [`crate::pv`]).

use crate::functions_util::{arg_number, numbers, opt_number};
use crate::{CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;
type Calc = Result<f64, CalcError>;

/// Dispatch finance functions from the main evaluator.
pub(crate) fn eval_finance_function(name: &str, args: &[Expr], lookup: Lookup) -> Option<Value> {
    let result = match name {
        "NPV" => npv(args, lookup),
        "IRR" => irr(args, lookup),
        "NPER" => nper(args, lookup),
        "RATE" => rate(args, lookup),
        _ => return None,
    };
    Some(match result {
        Ok(n) if n.is_finite() => Value::Number(n),
        Ok(_) => Value::Error(CalcError::Num),
        Err(error) => Value::Error(error),
    })
}

/// NPV(rate, value1, ...): flows are discounted from the end of period 1.
fn npv(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() < 2 {
        return Err(CalcError::Value);
    }
    let rate = arg_number(args, 0, lookup)?;
    if rate == -1.0 {
        return Err(CalcError::DivZero);
    }
    let flows = numbers(&args[1..], lookup)?;
    Ok(flows
        .iter()
        .enumerate()
        .map(|(i, flow)| flow / (1.0 + rate).powi(i as i32 + 1))
        .sum())
}

fn npv_at(flows: &[f64], rate: f64) -> f64 {
    flows
        .iter()
        .enumerate()
        .map(|(i, flow)| flow / (1.0 + rate).powi(i as i32))
        .sum()
}

/// Find a root of `f` near `guess`: Newton steps first, then a bracketed
/// bisection over the plausible rate range when Newton fails to converge.
fn solve(f: impl Fn(f64) -> f64, guess: f64) -> Calc {
    let mut x = guess;
    for _ in 0..60 {
        let y = f(x);
        if !y.is_finite() {
            break;
        }
        if y.abs() < 1e-10 {
            return Ok(x);
        }
        let h = 1e-7 * x.abs().max(1.0);
        let slope = (f(x + h) - f(x - h)) / (2.0 * h);
        if !slope.is_finite() || slope == 0.0 {
            break;
        }
        let next = x - y / slope;
        if next <= -1.0 {
            x = (x - 1.0) / 2.0;
            x = x.max(-0.999_999);
            continue;
        }
        if (next - x).abs() < 1e-14 {
            return Ok(next);
        }
        x = next;
    }
    bisect(&f)
}

fn bisect(f: &impl Fn(f64) -> f64) -> Calc {
    const STEPS: i32 = 400;
    let at = |step: i32| -0.99 + f64::from(step) * (100.0 + 0.99) / f64::from(STEPS);
    let mut previous = (at(0), f(at(0)));
    for step in 1..=STEPS {
        let x = at(step);
        let y = f(x);
        if previous.1.is_finite() && y.is_finite() && previous.1 * y <= 0.0 {
            let (mut lo, mut hi) = (previous.0, x);
            for _ in 0..200 {
                let mid = (lo + hi) / 2.0;
                if f(lo) * f(mid) <= 0.0 {
                    hi = mid;
                } else {
                    lo = mid;
                }
            }
            return Ok((lo + hi) / 2.0);
        }
        previous = (x, y);
    }
    Err(CalcError::Num)
}

/// IRR(values, [guess]): needs at least one inflow and one outflow.
fn irr(args: &[Expr], lookup: Lookup) -> Calc {
    if args.is_empty() || args.len() > 2 {
        return Err(CalcError::Value);
    }
    let flows = numbers(&args[..1], lookup)?;
    if !flows.iter().any(|f| *f > 0.0) || !flows.iter().any(|f| *f < 0.0) {
        return Err(CalcError::Num);
    }
    let guess = opt_number(args, 1, 0.1, lookup)?;
    solve(|rate| npv_at(&flows, rate), guess)
}

fn payment_timing(args: &[Expr], index: usize, lookup: Lookup) -> Calc {
    Ok(if opt_number(args, index, 0.0, lookup)? != 0.0 {
        1.0
    } else {
        0.0
    })
}

/// NPER(rate, pmt, pv, [fv], [type]).
fn nper(args: &[Expr], lookup: Lookup) -> Calc {
    if !(3..=5).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let rate = arg_number(args, 0, lookup)?;
    let pmt = arg_number(args, 1, lookup)?;
    let pv = arg_number(args, 2, lookup)?;
    let fv = opt_number(args, 3, 0.0, lookup)?;
    let timing = payment_timing(args, 4, lookup)?;
    if rate == 0.0 {
        return if pmt == 0.0 {
            Err(CalcError::Num)
        } else {
            Ok(-(pv + fv) / pmt)
        };
    }
    if rate <= -1.0 {
        return Err(CalcError::Num);
    }
    let adjusted = pmt * (1.0 + rate * timing);
    let ratio = (adjusted - fv * rate) / (adjusted + pv * rate);
    if !ratio.is_finite() || ratio <= 0.0 {
        return Err(CalcError::Num);
    }
    Ok(ratio.ln() / (1.0 + rate).ln())
}

/// RATE(nper, pmt, pv, [fv], [type], [guess]).
fn rate(args: &[Expr], lookup: Lookup) -> Calc {
    if !(3..=6).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let nper = arg_number(args, 0, lookup)?;
    let pmt = arg_number(args, 1, lookup)?;
    let pv = arg_number(args, 2, lookup)?;
    let fv = opt_number(args, 3, 0.0, lookup)?;
    let timing = payment_timing(args, 4, lookup)?;
    let guess = opt_number(args, 5, 0.1, lookup)?;
    if nper <= 0.0 {
        return Err(CalcError::Num);
    }
    let balance = |rate: f64| {
        if rate.abs() < 1e-12 {
            pv + pmt * nper + fv
        } else {
            let growth = (1.0 + rate).powf(nper);
            pv * growth + pmt * (1.0 + rate * timing) * (growth - 1.0) / rate + fv
        }
    };
    solve(balance, guess)
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

    fn num(setup: &[(&str, &str)], formula: &str) -> f64 {
        match eval(setup, formula) {
            Value::Number(n) => n,
            other => panic!("{formula} -> {other:?}"),
        }
    }

    fn close(got: f64, want: f64, tolerance: f64) {
        assert!((got - want).abs() < tolerance, "{got} vs {want}");
    }

    #[test]
    fn npv_discounts_from_the_end_of_the_first_period() {
        // Excel: NPV(0.1,-1000,300,300,300) = -230.8...; exact value below.
        let want = -1000.0 / 1.1 + 300.0 / 1.21 + 300.0 / 1.331 + 300.0 / 1.4641;
        close(num(&[], "=NPV(0.1,-1000,300,300,300)"), want, 1e-9);
        assert_eq!(num(&[], "=NPV(0,100,100,100)"), 300.0);
    }

    #[test]
    fn npv_reads_ranges_and_skips_text_and_blanks() {
        let cells = [("A1", "100"), ("A2", "note"), ("A4", "100")];
        close(
            num(&cells, "=NPV(0.1,A1:A4)"),
            100.0 / 1.1 + 100.0 / 1.21,
            1e-9,
        );
    }

    #[test]
    fn irr_matches_excel_on_a_range() {
        let cells = [
            ("A1", "-1000"),
            ("A2", "350"),
            ("A3", "350"),
            ("A4", "350"),
            ("A5", "350"),
        ];
        close(num(&cells, "=IRR(A1:A5)"), 0.149_625_44, 1e-4);
        close(num(&cells, "=IRR(A1:A5,0.5)"), 0.149_625_44, 1e-4);
    }

    #[test]
    fn irr_without_sign_change_is_num() {
        let cells = [("A1", "100"), ("A2", "100")];
        assert_eq!(eval(&cells, "=IRR(A1:A2)"), Value::Error(CalcError::Num));
    }

    #[test]
    fn nper_follows_excels_sign_convention() {
        close(num(&[], "=NPER(0.01,-100,1000)"), 10.588_6, 1e-3);
        assert_eq!(num(&[], "=NPER(0,-1000,5000)"), 5.0);
        // Payments at the start of the period need fewer periods.
        close(num(&[], "=NPER(0.01,-100,1000,0,1)"), 10.478_1, 1e-3);
        // Saving towards a future value.
        close(num(&[], "=NPER(0.05,-100,0,1000)"), 8.0, 0.5);
        // Both flows the same sign: the loan "ends" before it starts (negative).
        close(num(&[], "=NPER(0.01,100,1000)"), -9.578_6, 1e-3);
        // A payment that only covers the interest never pays the loan off.
        assert_eq!(
            eval(&[], "=NPER(0.01,-10,1000)"),
            Value::Error(CalcError::Num)
        );
    }

    #[test]
    fn rate_inverts_pmt_and_handles_a_zero_payment() {
        close(num(&[], "=RATE(120,-111.0205,10000)"), 0.005, 1e-5);
        close(num(&[], "=RATE(10,0,-100,200)"), 2f64.powf(0.1) - 1.0, 1e-8);
        close(num(&[], "=RATE(4,0,-100,100)"), 0.0, 1e-9);
        let cells = [("A1", "=PMT(0.007,60,25000)")];
        close(num(&cells, "=RATE(60,A1,25000)"), 0.007, 1e-8);
    }

    #[test]
    fn bad_arguments_are_value_errors() {
        assert_eq!(eval(&[], "=NPV(0.1)"), Value::Error(CalcError::Value));
        assert_eq!(eval(&[], "=NPER(1,2)"), Value::Error(CalcError::Value));
        assert_eq!(eval(&[], "=RATE(0,1,2)"), Value::Error(CalcError::Num));
    }
}
