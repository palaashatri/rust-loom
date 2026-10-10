//! Statistical functions (spike C): PERCENTILE.INC/EXC, QUARTILE.INC/EXC,
//! PERCENTRANK.INC/EXC, RANK.AVG, CORREL, SLOPE, INTERCEPT, FORECAST,
//! FORECAST.LINEAR, plus the legacy PERCENTILE and QUARTILE names.

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

fn column(values: &[&str]) -> Vec<(String, String)> {
    values
        .iter()
        .enumerate()
        .map(|(i, v)| (format!("A{}", i + 1), (*v).to_string()))
        .collect()
}

fn run(values: &[&str], formula: &str) -> Value {
    let owned = column(values);
    let setup: Vec<(&str, &str)> = owned
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    eval(&setup, formula)
}

fn close(value: Value, want: f64) {
    match value {
        Value::Number(n) => assert!((n - want).abs() < 1e-9, "{n} vs {want}"),
        other => panic!("expected {want}, got {other:?}"),
    }
}

const FOUR: [&str; 4] = ["1", "2", "3", "4"];

#[test]
fn percentile_inc_interpolates_between_ranked_values() {
    close(run(&FOUR, "=PERCENTILE.INC(A1:A4,0.25)"), 1.75);
    close(run(&FOUR, "=PERCENTILE.INC(A1:A4,0.5)"), 2.5);
    close(run(&FOUR, "=PERCENTILE.INC(A1:A4,0)"), 1.0);
    close(run(&FOUR, "=PERCENTILE.INC(A1:A4,1)"), 4.0);
    assert_eq!(
        run(&FOUR, "=PERCENTILE.INC(A1:A4,1.5)"),
        Value::Error(CalcError::Num)
    );
    assert_eq!(
        run(&FOUR, "=PERCENTILE.INC(A1:A4,-0.1)"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn percentile_exc_needs_a_rank_inside_the_data() {
    close(run(&FOUR, "=PERCENTILE.EXC(A1:A4,0.25)"), 1.25);
    close(run(&FOUR, "=PERCENTILE.EXC(A1:A4,0.5)"), 2.5);
    assert_eq!(
        run(&FOUR, "=PERCENTILE.EXC(A1:A4,0.1)"),
        Value::Error(CalcError::Num)
    );
    assert_eq!(
        run(&FOUR, "=PERCENTILE.EXC(A1:A4,0.9)"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn quartile_inc_and_exc_use_the_matching_percentile() {
    close(run(&FOUR, "=QUARTILE.INC(A1:A4,1)"), 1.75);
    close(run(&FOUR, "=QUARTILE.INC(A1:A4,0)"), 1.0);
    close(run(&FOUR, "=QUARTILE.INC(A1:A4,4)"), 4.0);
    assert_eq!(
        run(&FOUR, "=QUARTILE.INC(A1:A4,5)"),
        Value::Error(CalcError::Num)
    );
    close(run(&FOUR, "=QUARTILE.EXC(A1:A4,1)"), 1.25);
    assert_eq!(
        run(&FOUR, "=QUARTILE.EXC(A1:A4,0)"),
        Value::Error(CalcError::Num)
    );
    assert_eq!(
        run(&FOUR, "=QUARTILE.EXC(A1:A4,4)"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn legacy_percentile_and_quartile_names_match_the_inc_forms() {
    close(run(&FOUR, "=PERCENTILE(A1:A4,0.25)"), 1.75);
    close(run(&FOUR, "=QUARTILE(A1:A4,1)"), 1.75);
}

#[test]
fn percentrank_inc_interpolates_and_truncates_to_significance() {
    let five = ["1", "2", "3", "4", "5"];
    close(run(&five, "=PERCENTRANK.INC(A1:A5,2)"), 0.25);
    close(run(&FOUR, "=PERCENTRANK.INC(A1:A4,2.5)"), 0.5);
    close(run(&FOUR, "=PERCENTRANK.INC(A1:A4,2,1)"), 0.3);
    assert_eq!(
        run(&FOUR, "=PERCENTRANK.INC(A1:A4,0)"),
        Value::Error(CalcError::NA)
    );
}

#[test]
fn percentrank_exc_uses_n_plus_one() {
    let five = ["1", "2", "3", "4", "5"];
    close(run(&five, "=PERCENTRANK.EXC(A1:A5,2)"), 0.333);
    assert_eq!(
        run(&five, "=PERCENTRANK.EXC(A1:A5,0)"),
        Value::Error(CalcError::NA)
    );
}

#[test]
fn rank_avg_gives_tied_values_their_mean_rank() {
    let data = ["7", "3", "3", "1"];
    close(run(&data, "=RANK.AVG(3,A1:A4)"), 2.5);
    close(run(&data, "=RANK.AVG(7,A1:A4)"), 1.0);
    close(run(&data, "=RANK.AVG(1,A1:A4)"), 4.0);
    close(run(&data, "=RANK.AVG(3,A1:A4,1)"), 2.5);
    assert_eq!(
        run(&data, "=RANK.AVG(5,A1:A4)"),
        Value::Error(CalcError::NA)
    );
}

#[test]
fn correlation_is_one_for_a_line_and_minus_one_for_its_mirror() {
    let setup = [
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "3"),
        ("A4", "4"),
        ("B1", "2"),
        ("B2", "4"),
        ("B3", "6"),
        ("B4", "8"),
        ("C1", "4"),
        ("C2", "3"),
        ("C3", "2"),
        ("C4", "1"),
    ];
    close(eval(&setup, "=CORREL(A1:A4,B1:B4)"), 1.0);
    close(eval(&setup, "=CORREL(A1:A4,C1:C4)"), -1.0);
    assert_eq!(
        eval(&setup, "=CORREL(A1:A4,A1:A1)"),
        Value::Error(CalcError::NA)
    );
}

#[test]
fn slope_intercept_and_forecast_fit_a_straight_line() {
    let setup = [
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "3"),
        ("A4", "4"),
        ("B1", "3"),
        ("B2", "5"),
        ("B3", "7"),
        ("B4", "9"),
    ];
    close(eval(&setup, "=SLOPE(B1:B4,A1:A4)"), 2.0);
    close(eval(&setup, "=INTERCEPT(B1:B4,A1:A4)"), 1.0);
    close(eval(&setup, "=FORECAST(5,B1:B4,A1:A4)"), 11.0);
    close(eval(&setup, "=FORECAST.LINEAR(5,B1:B4,A1:A4)"), 11.0);
}

#[test]
fn regression_ignores_pairs_with_text_and_rejects_degenerate_input() {
    let setup = [
        ("A1", "1"),
        ("A2", "2"),
        ("A3", "x"),
        ("A4", "4"),
        ("B1", "2"),
        ("B2", "4"),
        ("B3", "6"),
        ("B4", "8"),
        ("C1", "2"),
        ("C2", "2"),
        ("C3", "2"),
        ("C4", "2"),
    ];
    // Pairs (1,2), (2,4), (4,8) remain: y = 2x.
    close(eval(&setup, "=SLOPE(B1:B4,A1:A4)"), 2.0);
    close(eval(&setup, "=INTERCEPT(B1:B4,A1:A4)"), 0.0);
    assert_eq!(
        eval(&setup, "=SLOPE(B1:B4,C1:C4)"),
        Value::Error(CalcError::DivZero)
    );
    assert_eq!(
        eval(&setup, "=SLOPE(B1:B4,A1:A3)"),
        Value::Error(CalcError::NA)
    );
}
