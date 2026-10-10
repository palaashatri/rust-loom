//! Financial functions (spike C): IPMT, PPMT, XNPV, XIRR, SLN, SYD, DB and DDB.
//! Loan periods follow the sign convention of PMT: a loan received is positive
//! and its payments negative.

use crate::functions_util::{arg_number, flatten, opt_number, to_number};
use crate::{eval_expr, CalcError, CellRef, Expr, Value};

type Lookup<'a> = &'a dyn Fn(CellRef) -> Value;
type Calc = Result<f64, CalcError>;

/// Dispatch the financial functions from the main evaluator.
pub(crate) fn eval_finance_more_function(
    name: &str,
    args: &[Expr],
    lookup: Lookup,
) -> Option<Value> {
    let result = match name {
        "IPMT" => period_part(args, lookup, false),
        "PPMT" => period_part(args, lookup, true),
        "XNPV" => xnpv_function(args, lookup),
        "XIRR" => xirr_function(args, lookup),
        "SLN" => sln(args, lookup),
        "SYD" => syd(args, lookup),
        "DB" => db(args, lookup),
        "DDB" => ddb(args, lookup),
        _ => return None,
    };
    Some(match result {
        Ok(n) => Value::Number(n),
        Err(error) => Value::Error(error),
    })
}

/// IPMT(rate, per, nper, pv, [fv], [type]) and PPMT (the principal part).
/// For an end-of-period loan the interest is the balance after `per - 1`
/// payments times the rate. For a beginning-of-period loan the first period
/// has no interest.
fn period_part(args: &[Expr], lookup: Lookup, principal: bool) -> Calc {
    if !(4..=6).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let rate = arg_number(args, 0, lookup)?;
    let per = arg_number(args, 1, lookup)?.trunc();
    let nper = arg_number(args, 2, lookup)?;
    let pv = arg_number(args, 3, lookup)?;
    let fv = opt_number(args, 4, 0.0, lookup)?;
    let kind = opt_number(args, 5, 0.0, lookup)?;
    if nper <= 0.0 || per < 1.0 || per > nper || (kind != 0.0 && kind != 1.0) {
        return Err(CalcError::Num);
    }
    let end_of_period = kind == 0.0;
    let payment = crate::pmt(rate, nper, pv, fv, end_of_period).map_err(|_| CalcError::Num)?;
    let interest = if end_of_period {
        crate::fv(rate, per - 1.0, payment, pv, true).map_err(|_| CalcError::Num)? * rate
    } else if per == 1.0 {
        0.0
    } else {
        (crate::fv(rate, per - 2.0, payment, pv, false).map_err(|_| CalcError::Num)? - payment)
            * rate
    };
    Ok(if principal {
        payment - interest
    } else {
        interest
    })
}

/// The cash flows of XNPV / XIRR: values and dates as whole serials, with the
/// first date as the origin. Dates before the origin are `#NUM!`.
fn cash_flows(
    values: &Expr,
    dates: &Expr,
    lookup: Lookup,
) -> Result<(Vec<f64>, Vec<f64>), CalcError> {
    let amounts = flatten(values, lookup);
    let stamps = flatten(dates, lookup);
    if amounts.len() != stamps.len() || amounts.is_empty() {
        return Err(CalcError::Num);
    }
    let mut money = Vec::with_capacity(amounts.len());
    for amount in amounts {
        money.push(match amount {
            Value::Number(n) => n,
            Value::Error(error) => return Err(error),
            _ => return Err(CalcError::Value),
        });
    }
    let mut when = Vec::with_capacity(stamps.len());
    for stamp in stamps {
        when.push(match stamp {
            Value::Number(n) => n.floor(),
            Value::Error(error) => return Err(error),
            _ => return Err(CalcError::Value),
        });
    }
    let origin = when[0];
    if when.iter().any(|day| *day < origin) {
        return Err(CalcError::Num);
    }
    Ok((
        money,
        when.into_iter().map(|day| (day - origin) / 365.0).collect(),
    ))
}

/// XNPV(rate, values, dates): values discounted by actual days over 365.
fn xnpv_function(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 3 {
        return Err(CalcError::Value);
    }
    let rate = arg_number(args, 0, lookup)?;
    let (money, years) = cash_flows(&args[1], &args[2], lookup)?;
    if rate <= -1.0 {
        return Err(CalcError::Num);
    }
    Ok(npv_at(rate, &money, &years))
}

fn npv_at(rate: f64, money: &[f64], years: &[f64]) -> f64 {
    money
        .iter()
        .zip(years)
        .map(|(amount, t)| amount / (1.0 + rate).powf(*t))
        .sum()
}

/// XIRR(values, dates, [guess]): the rate that makes XNPV zero. Newton's method
/// from the guess, then bisection if Newton does not settle.
fn xirr_function(args: &[Expr], lookup: Lookup) -> Calc {
    if !(2..=3).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let (money, years) = cash_flows(&args[0], &args[1], lookup)?;
    let guess = match args.get(2) {
        Some(expr) => to_number(eval_expr(expr, lookup))?,
        None => 0.1,
    };
    if !money.iter().any(|m| *m > 0.0) || !money.iter().any(|m| *m < 0.0) {
        return Err(CalcError::Num);
    }
    let mut rate = guess;
    for _ in 0..100 {
        if rate <= -1.0 || !rate.is_finite() {
            break;
        }
        let value = npv_at(rate, &money, &years);
        if value.abs() < 1e-10 {
            return Ok(rate);
        }
        let slope: f64 = money
            .iter()
            .zip(&years)
            .map(|(amount, t)| -t * amount * (1.0 + rate).powf(-t - 1.0))
            .sum();
        if slope == 0.0 || !slope.is_finite() {
            break;
        }
        let next = rate - value / slope;
        if (next - rate).abs() < 1e-12 {
            return Ok(next);
        }
        rate = next;
    }
    bisect_rate(&money, &years)
}

/// Bisection for a sign change of XNPV on (-1, 1000).
fn bisect_rate(money: &[f64], years: &[f64]) -> Calc {
    let (mut low, mut high) = (-0.999_999, 1000.0);
    let mut low_value = npv_at(low, money, years);
    if npv_at(high, money, years).signum() == low_value.signum() {
        return Err(CalcError::Num);
    }
    for _ in 0..400 {
        let mid = (low + high) / 2.0;
        let value = npv_at(mid, money, years);
        if value.abs() < 1e-10 || (high - low).abs() < 1e-14 {
            return Ok(mid);
        }
        if value.signum() == low_value.signum() {
            low = mid;
            low_value = value;
        } else {
            high = mid;
        }
    }
    Err(CalcError::Num)
}

/// SLN(cost, salvage, life): straight-line depreciation for one period.
fn sln(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 3 {
        return Err(CalcError::Value);
    }
    let cost = arg_number(args, 0, lookup)?;
    let salvage = arg_number(args, 1, lookup)?;
    let life = arg_number(args, 2, lookup)?;
    if life == 0.0 {
        return Err(CalcError::DivZero);
    }
    Ok((cost - salvage) / life)
}

/// SYD(cost, salvage, life, per): sum-of-years'-digits depreciation.
fn syd(args: &[Expr], lookup: Lookup) -> Calc {
    if args.len() != 4 {
        return Err(CalcError::Value);
    }
    let cost = arg_number(args, 0, lookup)?;
    let salvage = arg_number(args, 1, lookup)?;
    let life = arg_number(args, 2, lookup)?;
    let per = arg_number(args, 3, lookup)?;
    if life <= 0.0 || per < 1.0 || per > life {
        return Err(CalcError::Num);
    }
    Ok((cost - salvage) * (life - per + 1.0) * 2.0 / (life * (life + 1.0)))
}

/// DB(cost, salvage, life, period, [month]): fixed-rate declining balance. The
/// rate is rounded to three places, and the first and last periods are prorated
/// by the months in service.
fn db(args: &[Expr], lookup: Lookup) -> Calc {
    if !(4..=5).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let cost = arg_number(args, 0, lookup)?;
    let salvage = arg_number(args, 1, lookup)?;
    let life = arg_number(args, 2, lookup)?;
    let period = arg_number(args, 3, lookup)?.trunc();
    let month = opt_number(args, 4, 12.0, lookup)?.trunc();
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || !(1.0..=12.0).contains(&month) {
        return Err(CalcError::Num);
    }
    let last = if month < 12.0 { life + 1.0 } else { life };
    if period < 1.0 || period > last {
        return Err(CalcError::Num);
    }
    if cost == 0.0 {
        return Ok(0.0);
    }
    let rate = ((1.0 - (salvage / cost).powf(1.0 / life)) * 1000.0).round() / 1000.0;
    let mut total = 0.0;
    let mut depreciation = 0.0;
    for step in 1..=period as u64 {
        let step = step as f64;
        depreciation = if step == 1.0 {
            cost * rate * month / 12.0
        } else if step == life + 1.0 {
            (cost - total) * rate * (12.0 - month) / 12.0
        } else {
            (cost - total) * rate
        };
        total += depreciation;
    }
    Ok(depreciation)
}

/// DDB(cost, salvage, life, period, [factor]): declining balance at `factor`
/// (default 2), never taking the book value below salvage.
fn ddb(args: &[Expr], lookup: Lookup) -> Calc {
    if !(4..=5).contains(&args.len()) {
        return Err(CalcError::Value);
    }
    let cost = arg_number(args, 0, lookup)?;
    let salvage = arg_number(args, 1, lookup)?;
    let life = arg_number(args, 2, lookup)?;
    let period = arg_number(args, 3, lookup)?.trunc();
    let factor = opt_number(args, 4, 2.0, lookup)?;
    if cost < 0.0 || salvage < 0.0 || life <= 0.0 || factor <= 0.0 {
        return Err(CalcError::Num);
    }
    if period < 1.0 || period > life {
        return Err(CalcError::Num);
    }
    let mut book = cost;
    let mut depreciation = 0.0;
    for _ in 0..period as u64 {
        depreciation = (book * factor / life).min((book - salvage).max(0.0));
        book -= depreciation;
    }
    Ok(depreciation)
}
