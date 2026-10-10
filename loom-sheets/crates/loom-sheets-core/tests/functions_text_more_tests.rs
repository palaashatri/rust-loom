//! Text splitting functions (spike C): TEXTBEFORE, TEXTAFTER, TEXTSPLIT.

use std::collections::HashMap;

use loom_sheets_core::{evaluate, CalcError, CellRef, Sheet, Value};

fn run(setup: &[(&str, &str)], formulas: &[(&str, &str)]) -> HashMap<CellRef, Value> {
    let mut sheet = Sheet::new("t");
    for (at, raw) in setup.iter().chain(formulas.iter()) {
        sheet.set_str(at, raw);
    }
    evaluate(&sheet)
}

/// The value a cell shows. A spill anchor holds its whole array, so the anchor
/// shows the array's top-left element; spilled cells hold their own elements.
fn at(values: &HashMap<CellRef, Value>, a1: &str) -> Value {
    match values
        .get(&CellRef::parse(a1).unwrap())
        .cloned()
        .unwrap_or(Value::Empty)
    {
        Value::Array(items, _, _) => items.into_iter().next().unwrap_or(Value::Empty),
        other => other,
    }
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

#[test]
fn textbefore_takes_text_ahead_of_the_chosen_delimiter() {
    let values = run(
        &[],
        &[
            ("A1", "=TEXTBEFORE(\"Red-Green-Blue\",\"-\")"),
            ("A2", "=TEXTBEFORE(\"Red-Green-Blue\",\"-\",2)"),
            ("A3", "=TEXTBEFORE(\"Red-Green-Blue\",\"-\",-1)"),
            ("A4", "=TEXTBEFORE(\"Red-Green-Blue\",\"-\",-2)"),
            ("A5", "=TEXTBEFORE(\"a--b\",\"-\")"),
        ],
    );
    assert_eq!(at(&values, "A1"), text("Red"));
    assert_eq!(at(&values, "A2"), text("Red-Green"));
    assert_eq!(at(&values, "A3"), text("Red-Green"));
    assert_eq!(at(&values, "A4"), text("Red"));
    assert_eq!(at(&values, "A5"), text("a"));
}

#[test]
fn textafter_takes_text_behind_the_chosen_delimiter() {
    let values = run(
        &[],
        &[
            ("A1", "=TEXTAFTER(\"Red-Green-Blue\",\"-\")"),
            ("A2", "=TEXTAFTER(\"Red-Green-Blue\",\"-\",2)"),
            ("A3", "=TEXTAFTER(\"Red-Green-Blue\",\"-\",-1)"),
            ("A4", "=TEXTAFTER(\"Red-Green-Blue\",\"-\",-2)"),
            ("A5", "=TEXTAFTER(\"a--b\",\"-\")"),
        ],
    );
    assert_eq!(at(&values, "A1"), text("Green-Blue"));
    assert_eq!(at(&values, "A2"), text("Blue"));
    assert_eq!(at(&values, "A3"), text("Blue"));
    assert_eq!(at(&values, "A4"), text("Green-Blue"));
    assert_eq!(at(&values, "A5"), text("-b"));
}

#[test]
fn missing_delimiters_and_instances_are_na_unless_a_fallback_is_given() {
    let values = run(
        &[],
        &[
            ("A1", "=TEXTBEFORE(\"Red-Green-Blue\",\"x\")"),
            ("A2", "=TEXTAFTER(\"Red-Green-Blue\",\"-\",3)"),
            ("A3", "=TEXTBEFORE(\"Red-Green-Blue\",\"x\",1,0,0,\"none\")"),
            ("A4", "=TEXTAFTER(\"Red-Green-Blue\",\"-\",3,0,0,\"none\")"),
        ],
    );
    assert_eq!(at(&values, "A1"), Value::Error(CalcError::NA));
    assert_eq!(at(&values, "A2"), Value::Error(CalcError::NA));
    assert_eq!(at(&values, "A3"), text("none"));
    assert_eq!(at(&values, "A4"), text("none"));
}

#[test]
fn instance_zero_is_a_value_error() {
    let values = run(
        &[],
        &[
            ("A1", "=TEXTBEFORE(\"Red-Green-Blue\",\"-\",0)"),
            ("A2", "=TEXTAFTER(\"Red-Green-Blue\",\"-\",0)"),
        ],
    );
    assert_eq!(at(&values, "A1"), Value::Error(CalcError::Value));
    assert_eq!(at(&values, "A2"), Value::Error(CalcError::Value));
}

#[test]
fn match_mode_one_ignores_case_and_match_end_treats_the_end_as_a_delimiter() {
    let values = run(
        &[],
        &[
            ("A1", "=TEXTAFTER(\"AbcXdef\",\"x\",1,1)"),
            ("A2", "=TEXTBEFORE(\"AbcXdef\",\"x\")"),
            ("A3", "=TEXTBEFORE(\"abc\",\"x\",1,0,1)"),
            ("A4", "=TEXTAFTER(\"abc\",\"x\",1,0,1)"),
        ],
    );
    assert_eq!(at(&values, "A1"), text("def"));
    assert_eq!(at(&values, "A2"), Value::Error(CalcError::NA));
    assert_eq!(at(&values, "A3"), text("abc"));
    assert_eq!(at(&values, "A4"), text(""));
}

#[test]
fn textsplit_spills_columns_and_rows() {
    let values = run(
        &[],
        &[
            ("D1", "=TEXTSPLIT(\"a,b,c\",\",\")"),
            ("D3", "=TEXTSPLIT(\"a,b;c,d\",\",\",\";\")"),
            ("D6", "=TEXTSPLIT(\"a,b;c\",\",\",\";\")"),
        ],
    );
    assert_eq!(at(&values, "D1"), text("a"));
    assert_eq!(at(&values, "E1"), text("b"));
    assert_eq!(at(&values, "F1"), text("c"));
    assert_eq!(at(&values, "D3"), text("a"));
    assert_eq!(at(&values, "E3"), text("b"));
    assert_eq!(at(&values, "D4"), text("c"));
    assert_eq!(at(&values, "E4"), text("d"));
    // A short row pads with #N/A.
    assert_eq!(at(&values, "D6"), text("a"));
    assert_eq!(at(&values, "E6"), text("b"));
    assert_eq!(at(&values, "D7"), text("c"));
    assert_eq!(at(&values, "E7"), Value::Error(CalcError::NA));
}

#[test]
fn textsplit_keeps_or_drops_empty_pieces_and_pads_with_a_chosen_value() {
    let values = run(
        &[],
        &[
            ("D9", "=TEXTSPLIT(\"a,,b\",\",\")"),
            ("D11", "=TEXTSPLIT(\"a,,b\",\",\",\"\",TRUE)"),
            ("D13", "=TEXTSPLIT(\"a;b,c\",\";\",\",\",FALSE,0,\"-\")"),
        ],
    );
    assert_eq!(at(&values, "D9"), text("a"));
    assert_eq!(at(&values, "E9"), text(""));
    assert_eq!(at(&values, "F9"), text("b"));
    assert_eq!(at(&values, "D11"), text("a"));
    assert_eq!(at(&values, "E11"), text("b"));
    assert_eq!(at(&values, "F11"), Value::Empty);
    assert_eq!(at(&values, "D13"), text("a"));
    assert_eq!(at(&values, "E13"), text("b"));
    assert_eq!(at(&values, "D14"), text("c"));
    assert_eq!(at(&values, "E14"), text("-"));
}
