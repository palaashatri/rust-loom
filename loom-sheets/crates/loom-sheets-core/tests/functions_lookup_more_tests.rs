//! XMATCH (spike C): the position of a value, with XLOOKUP's match modes.

use std::collections::HashMap;

use loom_sheets_core::{evaluate, CalcError, CellRef, Sheet, Value};

fn run(setup: &[(&str, &str)], formulas: &[(&str, &str)]) -> HashMap<CellRef, Value> {
    let mut sheet = Sheet::new("t");
    for (at, raw) in setup.iter().chain(formulas.iter()) {
        sheet.set_str(at, raw);
    }
    evaluate(&sheet)
}

fn at(values: &HashMap<CellRef, Value>, a1: &str) -> Value {
    values
        .get(&CellRef::parse(a1).unwrap())
        .cloned()
        .unwrap_or(Value::Empty)
}

fn tens() -> Vec<(&'static str, &'static str)> {
    vec![
        ("A1", "10"),
        ("A2", "20"),
        ("A3", "30"),
        ("A4", "40"),
        ("A5", "50"),
    ]
}

#[test]
fn xmatch_returns_the_one_based_position_of_an_exact_match() {
    let values = run(
        &tens(),
        &[("C1", "=XMATCH(30,A1:A5)"), ("C2", "=XMATCH(35,A1:A5)")],
    );
    assert_eq!(at(&values, "C1"), Value::Number(3.0));
    assert_eq!(at(&values, "C2"), Value::Error(CalcError::NA));
}

#[test]
fn xmatch_approximate_modes_pick_the_next_smaller_or_larger_value() {
    let values = run(
        &tens(),
        &[
            ("C1", "=XMATCH(35,A1:A5,-1)"),
            ("C2", "=XMATCH(35,A1:A5,1)"),
            ("C3", "=XMATCH(5,A1:A5,-1)"),
            ("C4", "=XMATCH(55,A1:A5,1)"),
        ],
    );
    assert_eq!(at(&values, "C1"), Value::Number(3.0));
    assert_eq!(at(&values, "C2"), Value::Number(4.0));
    assert_eq!(at(&values, "C3"), Value::Error(CalcError::NA));
    assert_eq!(at(&values, "C4"), Value::Error(CalcError::NA));
}

#[test]
fn xmatch_search_direction_chooses_which_duplicate_wins() {
    let values = run(
        &[("A1", "1"), ("A2", "2"), ("A3", "2"), ("A4", "3")],
        &[("C1", "=XMATCH(2,A1:A4)"), ("C2", "=XMATCH(2,A1:A4,0,-1)")],
    );
    assert_eq!(at(&values, "C1"), Value::Number(2.0));
    assert_eq!(at(&values, "C2"), Value::Number(3.0));
}

#[test]
fn xmatch_is_case_insensitive_and_supports_wildcards() {
    let values = run(
        &[("A1", "Apple"), ("A2", "Banana")],
        &[
            ("C1", "=XMATCH(\"BANANA\",A1:A2)"),
            ("C2", "=XMATCH(\"b*\",A1:A2,2)"),
        ],
    );
    assert_eq!(at(&values, "C1"), Value::Number(2.0));
    assert_eq!(at(&values, "C2"), Value::Number(2.0));
}

#[test]
fn xmatch_needs_a_one_dimensional_lookup_array() {
    let values = run(
        &[("A1", "1"), ("B1", "2"), ("A2", "3"), ("B2", "4")],
        &[("C1", "=XMATCH(3,A1:B2)")],
    );
    assert_eq!(at(&values, "C1"), Value::Error(CalcError::Value));
}
