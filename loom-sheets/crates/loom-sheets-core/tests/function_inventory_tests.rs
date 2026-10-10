//! Function inventory guard (spike C). Each name in Microsoft's alphabetical
//! function list is probed through the evaluator. A name is supported when
//! `=NAME()` or `=NAME(1,2)` does not come back as `#NAME?`. The generated table
//! in `src/functions/inventory.rs` records the same result.

use loom_sheets_core::{evaluate, CalcError, CellRef, Sheet, Value};

/// Microsoft's alphabetical function names, one per line.
const REFERENCE: &str = include_str!("fixtures/excel_function_names.txt");

/// The supported count recorded when spike C landed. The count must not fall.
const SUPPORTED_FLOOR: usize = 175;

fn value_of(body: &str) -> Option<Value> {
    let mut sheet = Sheet::new("probe");
    sheet.set_str("A1", body);
    evaluate(&sheet)
        .get(&CellRef::parse("A1").unwrap())
        .cloned()
}

fn is_unknown(value: &Option<Value>) -> bool {
    matches!(
        value,
        Some(Value::Error(CalcError::Name)) | Some(Value::Error(CalcError::Parse))
    )
}

fn is_supported(name: &str) -> bool {
    [format!("={name}()"), format!("={name}(1,2)")]
        .iter()
        .any(|body| !is_unknown(&value_of(body)))
}

fn reference_names() -> Vec<&'static str> {
    REFERENCE
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect()
}

#[test]
fn the_reference_list_is_complete() {
    assert_eq!(reference_names().len(), 521);
}

#[test]
fn supported_count_does_not_fall_below_the_recorded_floor() {
    let supported = reference_names()
        .into_iter()
        .filter(|name| is_supported(name))
        .count();
    assert!(
        supported >= SUPPORTED_FLOOR,
        "{supported} supported names, floor {SUPPORTED_FLOOR}"
    );
}

#[test]
fn functions_added_by_spike_c_are_supported() {
    for name in [
        "LET",
        "XMATCH",
        "SORTBY",
        "TAKE",
        "WRAPROWS",
        "TEXTSPLIT",
        "TEXTBEFORE",
        "PERCENTILE.EXC",
        "PERCENTRANK.INC",
        "RANK.AVG",
        "CORREL",
        "FORECAST.LINEAR",
        "IPMT",
        "XNPV",
        "XIRR",
        "DDB",
        "NETWORKDAYS.INTL",
        "WORKDAY.INTL",
        "DATEDIF",
        "WEEKNUM",
        "ISOWEEKNUM",
        "YEARFRAC",
    ] {
        assert!(is_supported(name), "{name} should be supported");
    }
}

#[test]
fn a_name_that_is_not_a_function_stays_unknown() {
    assert!(!is_supported("NOSUCHFUNCTIONXYZ"));
}
