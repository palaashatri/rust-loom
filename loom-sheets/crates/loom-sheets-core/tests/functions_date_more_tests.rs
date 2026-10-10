//! Date and working-day functions (spike C): NETWORKDAYS, NETWORKDAYS.INTL,
//! WORKDAY, WORKDAY.INTL, DATEDIF, WEEKNUM, ISOWEEKNUM, YEARFRAC.

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

fn num(value: Value) -> f64 {
    match value {
        Value::Number(n) => n,
        other => panic!("expected a number, got {other:?}"),
    }
}

fn close(value: Value, want: f64, tolerance: f64) {
    let n = num(value);
    assert!((n - want).abs() < tolerance, "{n} vs {want}");
}

#[test]
fn networkdays_counts_weekdays_inclusive() {
    // October 2012 has 31 days and eight weekend days.
    assert_eq!(
        num(eval(&[], "=NETWORKDAYS(DATE(2012,10,1),DATE(2012,10,31))")),
        23.0
    );
    assert_eq!(
        num(eval(&[], "=NETWORKDAYS(DATE(2012,10,31),DATE(2012,10,1))")),
        -23.0
    );
}

#[test]
fn networkdays_skips_holidays() {
    let setup = [("A1", "=DATE(2012,10,8)")];
    assert_eq!(
        num(eval(
            &setup,
            "=NETWORKDAYS(DATE(2012,10,1),DATE(2012,10,31),A1)"
        )),
        22.0
    );
}

#[test]
fn networkdays_intl_takes_a_weekend_code_or_pattern() {
    assert_eq!(
        num(eval(
            &[],
            "=NETWORKDAYS.INTL(DATE(2012,10,1),DATE(2012,10,31),11)"
        )),
        27.0
    );
    assert_eq!(
        num(eval(
            &[],
            "=NETWORKDAYS.INTL(DATE(2012,10,1),DATE(2012,10,31),\"0000011\")"
        )),
        23.0
    );
}

#[test]
fn workday_moves_by_working_days_in_both_directions() {
    assert_eq!(
        eval(&[], "=WORKDAY(DATE(2012,10,1),10)=DATE(2012,10,15)"),
        Value::Bool(true)
    );
    assert_eq!(
        eval(&[], "=WORKDAY(DATE(2012,10,15),-10)=DATE(2012,10,1)"),
        Value::Bool(true)
    );
    assert_eq!(
        eval(&[], "=WORKDAY(DATE(2012,10,6),1)=DATE(2012,10,8)"),
        Value::Bool(true)
    );
    assert_eq!(
        eval(&[], "=WORKDAY(DATE(2012,10,1),0)=DATE(2012,10,1)"),
        Value::Bool(true)
    );
}

#[test]
fn workday_skips_holidays() {
    let setup = [("A1", "=DATE(2012,10,9)")];
    assert_eq!(
        eval(&setup, "=WORKDAY(DATE(2012,10,1),10,A1)=DATE(2012,10,16)"),
        Value::Bool(true)
    );
}

#[test]
fn workday_intl_supports_weekend_codes_and_patterns() {
    assert_eq!(
        // Only Saturday is off: Oct 2-5 and 7-12 are the ten working days.
        eval(&[], "=WORKDAY.INTL(DATE(2012,10,1),10,17)=DATE(2012,10,12)"),
        Value::Bool(true)
    );
    assert_eq!(
        eval(
            &[],
            "=WORKDAY.INTL(DATE(2012,10,1),10,\"0000011\")=DATE(2012,10,15)"
        ),
        Value::Bool(true)
    );
}

#[test]
fn datedif_reports_whole_years_months_and_days() {
    assert_eq!(
        num(eval(&[], "=DATEDIF(DATE(2001,1,1),DATE(2003,1,1),\"Y\")")),
        2.0
    );
    assert_eq!(
        num(eval(&[], "=DATEDIF(DATE(2001,1,1),DATE(2003,1,1),\"M\")")),
        24.0
    );
    assert_eq!(
        num(eval(&[], "=DATEDIF(DATE(2001,1,1),DATE(2003,1,1),\"D\")")),
        730.0
    );
    assert_eq!(
        num(eval(
            &[],
            "=DATEDIF(DATE(2001,1,15),DATE(2002,3,10),\"YM\")"
        )),
        1.0
    );
    assert_eq!(
        num(eval(
            &[],
            "=DATEDIF(DATE(2001,1,15),DATE(2002,3,10),\"YD\")"
        )),
        54.0
    );
}

#[test]
fn datedif_rejects_reversed_dates_and_unknown_units() {
    assert_eq!(
        eval(&[], "=DATEDIF(DATE(2003,1,1),DATE(2001,1,1),\"Y\")"),
        Value::Error(CalcError::Num)
    );
    assert_eq!(
        eval(&[], "=DATEDIF(DATE(2001,1,1),DATE(2003,1,1),\"Q\")"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn weeknum_numbers_weeks_from_the_week_containing_january_first() {
    assert_eq!(num(eval(&[], "=WEEKNUM(DATE(2012,3,9))")), 10.0);
    assert_eq!(num(eval(&[], "=WEEKNUM(DATE(2012,1,1))")), 1.0);
    assert_eq!(num(eval(&[], "=WEEKNUM(DATE(2012,3,9),2)")), 11.0);
    assert_eq!(num(eval(&[], "=WEEKNUM(DATE(2012,1,1),21)")), 52.0);
}

#[test]
fn isoweeknum_follows_iso_8601() {
    assert_eq!(num(eval(&[], "=ISOWEEKNUM(DATE(2012,3,9))")), 10.0);
    assert_eq!(num(eval(&[], "=ISOWEEKNUM(DATE(2012,1,1))")), 52.0);
}

#[test]
fn yearfrac_follows_each_day_count_basis() {
    // 1 Jan to 30 Jul 2012: 209 days on a 30/360 calendar, 211 actual days.
    close(
        eval(&[], "=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30))"),
        209.0 / 360.0,
        1e-12,
    );
    close(
        eval(&[], "=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),1)"),
        211.0 / 366.0,
        1e-12,
    );
    close(
        eval(&[], "=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),2)"),
        211.0 / 360.0,
        1e-12,
    );
    close(
        eval(&[], "=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),3)"),
        211.0 / 365.0,
        1e-12,
    );
    close(
        eval(&[], "=YEARFRAC(DATE(2012,1,1),DATE(2012,7,30),4)"),
        209.0 / 360.0,
        1e-12,
    );
}

#[test]
fn yearfrac_ignores_argument_order() {
    close(
        eval(&[], "=YEARFRAC(DATE(2012,7,30),DATE(2012,1,1))"),
        209.0 / 360.0,
        1e-12,
    );
}
