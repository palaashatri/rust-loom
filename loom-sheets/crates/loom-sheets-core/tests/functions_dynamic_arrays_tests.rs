//! Dynamic-array functions (spike C): SORTBY, RANDARRAY, TAKE, DROP, VSTACK,
//! HSTACK, TOCOL, TOROW, WRAPROWS, WRAPCOLS, CHOOSECOLS, CHOOSEROWS, EXPAND.
//! Results are read from the spill cells that `evaluate` reports, so these
//! tests check the placed grid, not only the anchor.

use std::collections::HashMap;

use loom_sheets_core::{evaluate, CalcError, CellRef, Sheet, Value};

fn run(setup: &[(&str, &str)], formulas: &[(&str, &str)]) -> HashMap<CellRef, Value> {
    let mut sheet = Sheet::new("t");
    for (at, raw) in setup {
        sheet.set_str(at, raw);
    }
    for (at, raw) in formulas {
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

fn num(n: f64) -> Value {
    Value::Number(n)
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn na() -> Value {
    Value::Error(CalcError::NA)
}

/// One column or row of numbers in A1:C3 laid out as 1..9 row by row.
fn grid_3x3() -> Vec<(&'static str, &'static str)> {
    vec![
        ("A1", "1"),
        ("B1", "2"),
        ("C1", "3"),
        ("A2", "4"),
        ("B2", "5"),
        ("C2", "6"),
        ("A3", "7"),
        ("B3", "8"),
        ("C3", "9"),
    ]
}

#[test]
fn sortby_orders_rows_by_a_key_array() {
    let mut setup = vec![("A1", "Bob"), ("A2", "Alice"), ("A3", "Carl")];
    setup.extend([("B1", "3"), ("B2", "1"), ("B3", "2")]);
    let values = run(
        &setup,
        &[
            ("D1", "=SORTBY(A1:A3,B1:B3)"),
            ("E1", "=SORTBY(A1:A3,B1:B3,-1)"),
        ],
    );
    assert_eq!(at(&values, "D1"), text("Alice"));
    assert_eq!(at(&values, "D2"), text("Carl"));
    assert_eq!(at(&values, "D3"), text("Bob"));
    assert_eq!(at(&values, "E1"), text("Bob"));
    assert_eq!(at(&values, "E2"), text("Carl"));
    assert_eq!(at(&values, "E3"), text("Alice"));
}

#[test]
fn sortby_uses_later_keys_to_break_ties() {
    let setup = [
        ("A1", "Ann"),
        ("A2", "Bo"),
        ("A3", "Cy"),
        ("A4", "Di"),
        ("B1", "1"),
        ("B2", "2"),
        ("B3", "1"),
        ("B4", "2"),
        ("C1", "9"),
        ("C2", "8"),
        ("C3", "7"),
        ("C4", "6"),
    ];
    let values = run(&setup, &[("E1", "=SORTBY(A1:A4,B1:B4,1,C1:C4,-1)")]);
    assert_eq!(at(&values, "E1"), text("Ann"));
    assert_eq!(at(&values, "E2"), text("Cy"));
    assert_eq!(at(&values, "E3"), text("Bo"));
    assert_eq!(at(&values, "E4"), text("Di"));
}

#[test]
fn sortby_rejects_mismatched_keys_and_bad_orders() {
    let mut setup = vec![("A1", "x"), ("A2", "y"), ("A3", "z")];
    setup.extend([("B1", "1"), ("B2", "2")]);
    let values = run(
        &setup,
        &[
            ("D1", "=SORTBY(A1:A3,B1:B2)"),
            ("E1", "=SORTBY(A1:A3,A1:A3,2)"),
        ],
    );
    assert_eq!(at(&values, "D1"), Value::Error(CalcError::Value));
    assert_eq!(at(&values, "E1"), Value::Error(CalcError::Value));
}

#[test]
fn take_keeps_leading_or_trailing_rows_and_columns() {
    let values = run(
        &grid_3x3(),
        &[
            ("E1", "=TAKE(A1:C3,2)"),
            ("E5", "=TAKE(A1:C3,-1)"),
            ("I1", "=TAKE(A1:C3,2,2)"),
            ("I5", "=TAKE(A1:C3,-2,-1)"),
        ],
    );
    // TAKE(A1:C3,2): rows 1-2 across all three columns.
    assert_eq!(at(&values, "E1"), num(1.0));
    assert_eq!(at(&values, "G1"), num(3.0));
    assert_eq!(at(&values, "E2"), num(4.0));
    assert_eq!(at(&values, "G2"), num(6.0));
    assert_eq!(at(&values, "E3"), Value::Empty);
    // TAKE(A1:C3,-1): the last row only.
    assert_eq!(at(&values, "E5"), num(7.0));
    assert_eq!(at(&values, "G5"), num(9.0));
    // TAKE(A1:C3,2,2): top-left 2x2.
    assert_eq!(at(&values, "I1"), num(1.0));
    assert_eq!(at(&values, "J1"), num(2.0));
    assert_eq!(at(&values, "I2"), num(4.0));
    assert_eq!(at(&values, "J2"), num(5.0));
    // TAKE(A1:C3,-2,-1): last two rows, last column.
    assert_eq!(at(&values, "I5"), num(6.0));
    assert_eq!(at(&values, "I6"), num(9.0));
}

#[test]
fn take_clips_to_the_array_and_empty_results_are_calc_errors() {
    let values = run(
        &grid_3x3(),
        &[("E1", "=TAKE(A1:C3,10)"), ("I1", "=TAKE(A1:C3,0)")],
    );
    assert_eq!(at(&values, "E3"), num(7.0));
    assert_eq!(at(&values, "I1"), Value::Error(CalcError::Calc));
}

#[test]
fn drop_removes_rows_and_columns_from_either_end() {
    let values = run(
        &grid_3x3(),
        &[
            ("E1", "=DROP(A1:C3,1)"),
            ("E5", "=DROP(A1:C3,-1)"),
            ("I1", "=DROP(A1:C3,1,1)"),
            ("I5", "=DROP(A1:C3,0)"),
            ("M1", "=DROP(A1:C3,3)"),
        ],
    );
    // DROP(A1:C3,1): rows 2-3.
    assert_eq!(at(&values, "E1"), num(4.0));
    assert_eq!(at(&values, "G2"), num(9.0));
    // DROP(A1:C3,-1): rows 1-2.
    assert_eq!(at(&values, "E5"), num(1.0));
    assert_eq!(at(&values, "G6"), num(6.0));
    // DROP(A1:C3,1,1): bottom-right 2x2.
    assert_eq!(at(&values, "I1"), num(5.0));
    assert_eq!(at(&values, "J2"), num(9.0));
    // DROP(A1:C3,0) keeps everything.
    assert_eq!(at(&values, "I5"), num(1.0));
    assert_eq!(at(&values, "K7"), num(9.0));
    // Dropping every row leaves nothing to return.
    assert_eq!(at(&values, "M1"), Value::Error(CalcError::Calc));
}

#[test]
fn vstack_and_hstack_join_arrays_and_pad_with_na() {
    let values = run(
        &[
            ("A1", "1"),
            ("A2", "2"),
            ("B1", "3"),
            ("B2", "4"),
            ("B3", "5"),
        ],
        &[
            ("D1", "=VSTACK(A1:A2,B1:B3)"),
            ("F1", "=HSTACK(A1:A2,B1:B2)"),
            ("H1", "=VSTACK(A1:B1,B1)"),
        ],
    );
    assert_eq!(at(&values, "D1"), num(1.0));
    assert_eq!(at(&values, "D2"), num(2.0));
    assert_eq!(at(&values, "D3"), num(3.0));
    assert_eq!(at(&values, "D5"), num(5.0));
    // HSTACK of two 2-row arrays: 2x2.
    assert_eq!(at(&values, "F1"), num(1.0));
    assert_eq!(at(&values, "G1"), num(3.0));
    assert_eq!(at(&values, "F2"), num(2.0));
    assert_eq!(at(&values, "G2"), num(4.0));
    // A 1x2 row stacked over a 1x1 cell: the short row pads with #N/A.
    assert_eq!(at(&values, "H1"), num(1.0));
    assert_eq!(at(&values, "I1"), num(3.0));
    assert_eq!(at(&values, "H2"), num(3.0));
    assert_eq!(at(&values, "I2"), na());
}

#[test]
fn tocol_and_torow_flatten_and_optionally_ignore_blanks_and_errors() {
    let values = run(
        &[
            ("A1", "1"),
            ("B1", "2"),
            ("A2", "3"),
            ("B2", "4"),
            ("C1", "=1/0"),
            ("C2", "5"),
        ],
        &[
            ("E1", "=TOCOL(A1:B2)"),
            ("F1", "=TOCOL(A1:B2,0,TRUE)"),
            ("H1", "=TOROW(A1:B2)"),
            ("E8", "=TOCOL(C1:C2,2)"),
            ("F8", "=TOCOL(C1:C2)"),
        ],
    );
    // Row-major by default: 1,2,3,4.
    assert_eq!(at(&values, "E1"), num(1.0));
    assert_eq!(at(&values, "E4"), num(4.0));
    // Column-major when scan_by_column is TRUE: 1,3,2,4.
    assert_eq!(at(&values, "F1"), num(1.0));
    assert_eq!(at(&values, "F2"), num(3.0));
    assert_eq!(at(&values, "F3"), num(2.0));
    // TOROW gives one row.
    assert_eq!(at(&values, "H1"), num(1.0));
    assert_eq!(at(&values, "K1"), num(4.0));
    // Ignoring errors drops the #DIV/0!; keeping them returns it.
    assert_eq!(at(&values, "E8"), num(5.0));
    assert_eq!(at(&values, "E9"), Value::Empty);
    assert_eq!(at(&values, "F8"), Value::Error(CalcError::DivZero));
    assert_eq!(at(&values, "F9"), num(5.0));
}

#[test]
fn wraprows_and_wrapcols_reshape_a_vector_and_pad_the_last_slice() {
    let setup: Vec<(String, String)> = (1..=6).map(|i| (format!("A{i}"), i.to_string())).collect();
    let mut sheet = Sheet::new("t");
    for (at, raw) in &setup {
        sheet.set_str(at, raw);
    }
    for (at, raw) in [
        ("C1", "=WRAPROWS(A1:A6,2)"),
        ("F1", "=WRAPROWS(A1:A5,2)"),
        ("I1", "=WRAPROWS(A1:A5,2,0)"),
        ("L1", "=WRAPCOLS(A1:A6,3)"),
        ("P1", "=WRAPROWS(A1:A6,0)"),
    ] {
        sheet.set_str(at, raw);
    }
    let values = evaluate(&sheet);
    assert_eq!(at(&values, "C1"), num(1.0));
    assert_eq!(at(&values, "D1"), num(2.0));
    assert_eq!(at(&values, "C3"), num(5.0));
    assert_eq!(at(&values, "D3"), num(6.0));
    // Five values wrapped by two: the last row pads with #N/A, or with the
    // pad_with argument when one is given.
    assert_eq!(at(&values, "F3"), num(5.0));
    assert_eq!(at(&values, "G3"), na());
    assert_eq!(at(&values, "I3"), num(5.0));
    assert_eq!(at(&values, "J3"), num(0.0));
    // WRAPCOLS fills each column top to bottom: [1,2,3] then [4,5,6].
    assert_eq!(at(&values, "L1"), num(1.0));
    assert_eq!(at(&values, "M1"), num(4.0));
    assert_eq!(at(&values, "L3"), num(3.0));
    assert_eq!(at(&values, "M3"), num(6.0));
    assert_eq!(at(&values, "P1"), Value::Error(CalcError::Value));
}

#[test]
fn choosecols_and_chooserows_pick_positions_with_negatives_from_the_end() {
    let values = run(
        &grid_3x3(),
        &[
            ("E1", "=CHOOSECOLS(A1:C3,3,1)"),
            ("H1", "=CHOOSECOLS(A1:C3,-1)"),
            ("J1", "=CHOOSEROWS(A1:C3,2)"),
            ("E6", "=CHOOSECOLS(A1:C3,0)"),
            ("H6", "=CHOOSEROWS(A1:C3,4)"),
        ],
    );
    assert_eq!(at(&values, "E1"), num(3.0));
    assert_eq!(at(&values, "F1"), num(1.0));
    assert_eq!(at(&values, "E2"), num(6.0));
    assert_eq!(at(&values, "F2"), num(4.0));
    assert_eq!(at(&values, "H3"), num(9.0));
    assert_eq!(at(&values, "J1"), num(4.0));
    assert_eq!(at(&values, "L1"), num(6.0));
    assert_eq!(at(&values, "E6"), Value::Error(CalcError::Value));
    assert_eq!(at(&values, "H6"), Value::Error(CalcError::Value));
}

#[test]
fn expand_pads_to_the_requested_size() {
    let values = run(
        &[("A1", "1"), ("B1", "2")],
        &[("D1", "=EXPAND(A1:B1,2,3)"), ("D5", "=EXPAND(A1:B1,2,3,0)")],
    );
    assert_eq!(at(&values, "D1"), num(1.0));
    assert_eq!(at(&values, "E1"), num(2.0));
    assert_eq!(at(&values, "F1"), na());
    assert_eq!(at(&values, "D2"), na());
    assert_eq!(at(&values, "D5"), num(1.0));
    assert_eq!(at(&values, "F5"), num(0.0));
    assert_eq!(at(&values, "F6"), num(0.0));
}

#[test]
fn randarray_respects_its_shape_and_bounds() {
    let values = run(
        &[],
        &[
            ("A1", "=RANDARRAY(2,3)"),
            ("F1", "=RANDARRAY(1,1,5,10)"),
            ("H1", "=RANDARRAY(2,2,1,6,TRUE)"),
            ("L1", "=RANDARRAY(-1)"),
            ("M1", "=RANDARRAY(1,1,9,2)"),
        ],
    );
    for cell in ["A1", "B1", "C1", "A2", "B2", "C2"] {
        match at(&values, cell) {
            Value::Number(n) => assert!((0.0..1.0).contains(&n), "{cell}={n}"),
            other => panic!("{cell}: {other:?}"),
        }
    }
    assert_eq!(at(&values, "D1"), Value::Empty);
    match at(&values, "F1") {
        Value::Number(n) => assert!((5.0..10.0).contains(&n), "F1={n}"),
        other => panic!("F1: {other:?}"),
    }
    for cell in ["H1", "I1", "H2", "I2"] {
        match at(&values, cell) {
            Value::Number(n) => {
                assert!(n.fract() == 0.0 && (1.0..=6.0).contains(&n), "{cell}={n}")
            }
            other => panic!("{cell}: {other:?}"),
        }
    }
    assert_eq!(at(&values, "L1"), Value::Error(CalcError::Value));
    assert_eq!(at(&values, "M1"), Value::Error(CalcError::Value));
}
