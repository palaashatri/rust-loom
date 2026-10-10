//! Financial functions (spike C): IPMT, PPMT, XNPV, XIRR, SLN, SYD, DB, DDB.
//! Expected values come from Excel's documented examples or from the
//! textbook formulas, checked independently of this crate.

use loom_sheets_core::{evaluate, CalcError, CellRef, Sheet, Value};

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

fn close(value: Value, want: f64, tolerance: f64) {
    match value {
        Value::Number(n) => assert!((n - want).abs() < tolerance, "{n} vs {want}"),
        other => panic!("expected {want}, got {other:?}"),
    }
}

/// Values and dates from the Microsoft XNPV/XIRR example (serial dates).
fn cash_flows() -> Vec<(String, String)> {
    let values = ["-10000", "2750", "4250", "3250", "2750"];
    let dates = ["39448", "39508", "39751", "39859", "39904"];
    let mut cells = Vec::new();
    for (i, value) in values.iter().enumerate() {
        cells.push((format!("A{}", i + 1), (*value).to_string()));
        cells.push((format!("B{}", i + 1), dates[i].to_string()));
    }
    cells
}

fn with_flows(formula: &str) -> Value {
    let owned = cash_flows();
    let setup: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    eval(&setup, formula)
}

#[test]
fn ipmt_is_the_interest_part_of_one_period() {
    // Microsoft's PPMT example: 10% a year, paid monthly over two years.
    close(
        eval(&[], "=IPMT(0.1/12,1,24,2000)"),
        -16.666_666_666_666_668,
        1e-9,
    );
    close(eval(&[], "=IPMT(0.1,2,2,1000)"), -52.380_952_380_952, 1e-9);
}

#[test]
fn ppmt_is_the_principal_part_of_one_period() {
    close(
        eval(&[], "=PPMT(0.1/12,1,24,2000)"),
        -75.623_186_008_366,
        1e-9,
    );
}

#[test]
fn annuity_due_has_no_interest_in_the_first_period() {
    close(eval(&[], "=IPMT(0.1,1,2,1000,0,1)"), 0.0, 1e-12);
    close(
        eval(&[], "=IPMT(0.1,2,2,1000,0,1)"),
        -47.619_047_619_047,
        1e-9,
    );
    close(
        eval(&[], "=PPMT(0.1,2,2,1000,0,1)"),
        -476.190_476_190_475,
        1e-9,
    );
}

#[test]
fn ipmt_and_ppmt_reject_periods_outside_the_loan() {
    assert_eq!(
        eval(&[], "=IPMT(0.1,0,2,1000)"),
        Value::Error(CalcError::Num)
    );
    assert_eq!(
        eval(&[], "=IPMT(0.1,3,2,1000)"),
        Value::Error(CalcError::Num)
    );
    assert_eq!(
        eval(&[], "=PPMT(0.1,3,2,1000)"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn xnpv_discounts_from_the_first_date() {
    // Microsoft's XNPV example: 9% rate gives 2,086.65.
    close(
        with_flows("=XNPV(0.09,A1:A5,B1:B5)"),
        2_086.647_602_031_535,
        0.005,
    );
}

#[test]
fn xnpv_rejects_dates_before_the_first() {
    let setup = [
        ("A1", "-100"),
        ("A2", "110"),
        ("B1", "39448"),
        ("B2", "39000"),
    ];
    assert_eq!(
        eval(&setup, "=XNPV(0.1,A1:A2,B1:B2)"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn xirr_finds_the_rate_that_zeroes_xnpv() {
    // Microsoft's XIRR example: about 37.34%.
    close(
        with_flows("=XIRR(A1:A5,B1:B5)"),
        0.373_362_533_518_831,
        1e-6,
    );
    close(
        with_flows("=XIRR(A1:A5,B1:B5,0.1)"),
        0.373_362_533_518_831,
        1e-6,
    );
}

#[test]
fn xirr_needs_both_a_gain_and_a_loss() {
    let setup = [
        ("A1", "100"),
        ("A2", "200"),
        ("B1", "39448"),
        ("B2", "39508"),
    ];
    assert_eq!(
        eval(&setup, "=XIRR(A1:A2,B1:B2)"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn straight_line_and_sum_of_years_digits_depreciation() {
    close(eval(&[], "=SLN(30000,7500,10)"), 2250.0, 1e-9);
    close(
        eval(&[], "=SYD(30000,7500,10,1)"),
        4_090.909_090_909_091,
        1e-9,
    );
    close(
        eval(&[], "=SYD(30000,7500,10,10)"),
        409.090_909_090_909,
        1e-9,
    );
    assert_eq!(
        eval(&[], "=SLN(30000,7500,0)"),
        Value::Error(CalcError::DivZero)
    );
    assert_eq!(
        eval(&[], "=SYD(30000,7500,10,11)"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn fixed_declining_balance_matches_the_microsoft_example() {
    // Rate is 0.319 (rounded to three places); the first year is a partial year.
    // Excel does not round the result: these are its unrounded values.
    close(
        eval(&[], "=DB(1000000,100000,6,1,7)"),
        186_083.333_333_333_34,
        1e-6,
    );
    close(
        eval(&[], "=DB(1000000,100000,6,2,7)"),
        259_639.416_666_666_66,
        1e-6,
    );
    close(eval(&[], "=DB(1000000,100000,6,1)"), 319_000.0, 1e-6);
}

#[test]
fn double_declining_balance_never_drops_below_salvage() {
    close(eval(&[], "=DDB(2400,300,10,1)"), 480.0, 1e-9);
    close(eval(&[], "=DDB(2400,300,10,2)"), 384.0, 1e-9);
    close(eval(&[], "=DDB(2400,300,10,1,1.5)"), 360.0, 1e-9);
    assert_eq!(
        eval(&[], "=DDB(2400,300,10,11)"),
        Value::Error(CalcError::Num)
    );
}
