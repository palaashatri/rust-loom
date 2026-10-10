//! Statistical functions tests: LARGE, SMALL, RANK, STDEV, VAR, MODE,
//! COUNTBLANK, SUMIFS, COUNTIFS, AVERAGEIFS.

use loom_sheets_core::{evaluate, CalcError, CellRef, Sheet, Value};

/// Evaluate `formula` in Z1 against A1:A5 = 10, 20, 30, 40, 50.
fn large_small(formula: &str) -> Value {
    let mut sheet = Sheet::new("t");
    for (row, value) in ["10", "20", "30", "40", "50"].iter().enumerate() {
        sheet.set_str(&format!("A{}", row + 1), value);
    }
    sheet.set_str("Z1", formula);
    evaluate(&sheet)
        .get(&CellRef::parse("Z1").unwrap())
        .cloned()
        .unwrap_or(Value::Empty)
}

#[test]
fn large_and_small_take_a_fractional_k_by_rounding_inside_the_range() {
    // LARGE takes the ceil(k)-th largest and SMALL the floor(k)-th smallest.
    assert_eq!(large_small("=LARGE(A1:A5,2.2)"), Value::Number(30.0));
    assert_eq!(large_small("=LARGE(A1:A5,2.9)"), Value::Number(30.0));
    assert_eq!(large_small("=LARGE(A1:A5,4.999)"), Value::Number(10.0));
    assert_eq!(large_small("=SMALL(A1:A5,2.2)"), Value::Number(20.0));
    assert_eq!(large_small("=SMALL(A1:A5,2.9)"), Value::Number(20.0));
    // k is checked as written, before rounding: 0.5 would round up to 1 and 5.5 down to 5.
    assert_eq!(
        large_small("=LARGE(A1:A5,0.5)"),
        Value::Error(CalcError::Num)
    );
    assert_eq!(
        large_small("=SMALL(A1:A5,5.5)"),
        Value::Error(CalcError::Num)
    );
}

#[test]
fn large_function() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "15");
    sheet.set_str("B1", "=LARGE(A1:A3, 1)"); // 20
    sheet.set_str("B2", "=LARGE(A1:A3, 2)"); // 15
    sheet.set_str("B3", "=LARGE(A1:A3, 3)"); // 10
    sheet.set_str("B4", "=LARGE(A1:A3, 4)"); // #NUM!

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(20.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Number(15.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B3").unwrap()),
        Some(&Value::Number(10.0))
    );
    assert!(matches!(
        vals.get(&CellRef::parse("B4").unwrap()),
        Some(Value::Error(_))
    ));
}

#[test]
fn small_function() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "15");
    sheet.set_str("B1", "=SMALL(A1:A3, 1)"); // 10
    sheet.set_str("B2", "=SMALL(A1:A3, 2)"); // 15
    sheet.set_str("B3", "=SMALL(A1:A3, 3)"); // 20

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(10.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Number(15.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B3").unwrap()),
        Some(&Value::Number(20.0))
    );
}

#[test]
fn rank_descending() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "15");
    sheet.set_str("B1", "=RANK(20, A1:A3, 0)"); // 1 (largest)
    sheet.set_str("B2", "=RANK(15, A1:A3, 0)"); // 2
    sheet.set_str("B3", "=RANK(10, A1:A3, 0)"); // 3 (smallest)

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(1.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Number(2.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B3").unwrap()),
        Some(&Value::Number(3.0))
    );
}

#[test]
fn rank_ascending() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "15");
    sheet.set_str("B1", "=RANK(10, A1:A3, 1)"); // 1 (smallest)
    sheet.set_str("B2", "=RANK(15, A1:A3, 1)"); // 2
    sheet.set_str("B3", "=RANK(20, A1:A3, 1)"); // 3 (largest)

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(1.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Number(2.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B3").unwrap()),
        Some(&Value::Number(3.0))
    );
}

#[test]
fn stdev_function() {
    let mut sheet = Sheet::new("t");
    // Sample: 1, 2, 3, 4, 5
    // Mean = 3
    // Squared deviations: 4, 1, 0, 1, 4 = 10
    // Variance = 10 / (5-1) = 2.5
    // StDev = sqrt(2.5) ≈ 1.5811388...
    sheet.set_str("A1", "1");
    sheet.set_str("A2", "2");
    sheet.set_str("A3", "3");
    sheet.set_str("A4", "4");
    sheet.set_str("A5", "5");
    sheet.set_str("B1", "=STDEV(A1:A5)");

    let vals = evaluate(&sheet);
    let result = vals.get(&CellRef::parse("B1").unwrap());
    match result {
        Some(Value::Number(n)) => {
            // sqrt(2.5) ≈ 1.5811388...
            assert!((n - 1.5811388).abs() < 0.0001);
        }
        _ => panic!("Expected number, got {:?}", result),
    }
}

#[test]
fn var_function() {
    let mut sheet = Sheet::new("t");
    // Sample: 1, 2, 3, 4, 5
    // Mean = 3
    // Squared deviations: 4, 1, 0, 1, 4 = 10
    // Variance = 10 / (5-1) = 2.5
    sheet.set_str("A1", "1");
    sheet.set_str("A2", "2");
    sheet.set_str("A3", "3");
    sheet.set_str("A4", "4");
    sheet.set_str("A5", "5");
    sheet.set_str("B1", "=VAR(A1:A5)");

    let vals = evaluate(&sheet);
    let result = vals.get(&CellRef::parse("B1").unwrap());
    match result {
        Some(Value::Number(n)) => {
            assert!((n - 2.5).abs() < 0.0001);
        }
        _ => panic!("Expected number, got {:?}", result),
    }
}

#[test]
fn mode_function() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "1");
    sheet.set_str("A2", "2");
    sheet.set_str("A3", "2");
    sheet.set_str("A4", "3");
    sheet.set_str("A5", "3");
    sheet.set_str("A6", "3");
    sheet.set_str("B1", "=MODE(A1:A6)"); // 3 appears 3 times

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(3.0))
    );
}

#[test]
fn mode_no_repeats() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "1");
    sheet.set_str("A2", "2");
    sheet.set_str("A3", "3");
    sheet.set_str("B1", "=MODE(A1:A3)"); // #N/A (no repeats)

    let vals = evaluate(&sheet);
    assert!(matches!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(Value::Error(_))
    ));
}

#[test]
fn countblank_function() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "1");
    sheet.set_str("A2", "");
    sheet.set_str("A3", "3");
    sheet.set_str("A4", "");
    sheet.set_str("B1", "=COUNTBLANK(A1:A4)"); // 2

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(2.0))
    );
}

#[test]
fn sumifs_single_criteria() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "Food");
    sheet.set_str("A2", "Food");
    sheet.set_str("A3", "Fuel");
    sheet.set_str("B1", "10");
    sheet.set_str("B2", "20");
    sheet.set_str("B3", "30");
    sheet.set_str("C1", "=SUMIFS(B1:B3, A1:A3, \"Food\")"); // 30

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Number(30.0))
    );
}

#[test]
fn sumifs_multiple_criteria() {
    let mut sheet = Sheet::new("t");
    // Data with regions and categories
    sheet.set_str("A1", "North");
    sheet.set_str("A2", "North");
    sheet.set_str("A3", "South");
    sheet.set_str("B1", "Food");
    sheet.set_str("B2", "Fuel");
    sheet.set_str("B3", "Food");
    sheet.set_str("C1", "10");
    sheet.set_str("C2", "20");
    sheet.set_str("C3", "30");
    sheet.set_str("D1", "=SUMIFS(C1:C3, A1:A3, \"North\", B1:B3, \"Food\")"); // 10

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("D1").unwrap()),
        Some(&Value::Number(10.0))
    );
}

#[test]
fn sumifs_numeric_criteria() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "30");
    sheet.set_str("B1", "100");
    sheet.set_str("B2", "200");
    sheet.set_str("B3", "300");
    sheet.set_str("C1", "=SUMIFS(B1:B3, A1:A3, \">15\")"); // 200 + 300 = 500

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Number(500.0))
    );
}

#[test]
fn countifs_single_criteria() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "Food");
    sheet.set_str("A2", "Food");
    sheet.set_str("A3", "Fuel");
    sheet.set_str("B1", "=COUNTIFS(A1:A3, \"Food\")"); // 2

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(2.0))
    );
}

#[test]
fn countifs_multiple_criteria() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "North");
    sheet.set_str("A2", "North");
    sheet.set_str("A3", "South");
    sheet.set_str("B1", "Food");
    sheet.set_str("B2", "Fuel");
    sheet.set_str("B3", "Food");
    sheet.set_str("C1", "=COUNTIFS(A1:A3, \"North\", B1:B3, \"Food\")"); // 1

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Number(1.0))
    );
}

#[test]
fn averageifs_single_criteria() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "Food");
    sheet.set_str("A2", "Food");
    sheet.set_str("A3", "Fuel");
    sheet.set_str("B1", "10");
    sheet.set_str("B2", "20");
    sheet.set_str("B3", "30");
    sheet.set_str("C1", "=AVERAGEIFS(B1:B3, A1:A3, \"Food\")"); // (10+20)/2 = 15

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Number(15.0))
    );
}

#[test]
fn averageifs_multiple_criteria() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "North");
    sheet.set_str("A2", "North");
    sheet.set_str("A3", "South");
    sheet.set_str("B1", "Food");
    sheet.set_str("B2", "Fuel");
    sheet.set_str("B3", "Food");
    sheet.set_str("C1", "10");
    sheet.set_str("C2", "20");
    sheet.set_str("C3", "30");
    sheet.set_str(
        "D1",
        "=AVERAGEIFS(C1:C3, A1:A3, \"North\", B1:B3, \"Food\")",
    ); // 10

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("D1").unwrap()),
        Some(&Value::Number(10.0))
    );
}

#[test]
fn averageifs_numeric_criteria() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "30");
    sheet.set_str("B1", "100");
    sheet.set_str("B2", "200");
    sheet.set_str("B3", "300");
    sheet.set_str("C1", "=AVERAGEIFS(B1:B3, A1:A3, \">15\")"); // (200+300)/2 = 250

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Number(250.0))
    );
}

#[test]
fn wildcard_criteria_in_sumifs() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "Apple");
    sheet.set_str("A2", "Apricot");
    sheet.set_str("A3", "Banana");
    sheet.set_str("B1", "10");
    sheet.set_str("B2", "20");
    sheet.set_str("B3", "30");
    sheet.set_str("C1", "=SUMIFS(B1:B3, A1:A3, \"A*\")"); // 10 + 20 = 30

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Number(30.0))
    );
}

#[test]
fn criteria_with_operators() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "30");
    sheet.set_str("B1", "=SUMIFS(A1:A3, A1:A3, \">=20\")"); // 20 + 30 = 50

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(50.0))
    );
}

#[test]
fn rank_default_descending() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "15");
    sheet.set_str("B1", "=RANK(20, A1:A3)"); // 1 (default descending)

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(1.0))
    );
}

#[test]
fn stdev_single_value() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "5");
    sheet.set_str("B1", "=STDEV(A1)"); // #NUM! (need at least 2 values)

    let vals = evaluate(&sheet);
    assert!(matches!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(Value::Error(_))
    ));
}

#[test]
fn var_two_values() {
    let mut sheet = Sheet::new("t");
    // 1, 2: mean = 1.5
    // variance = ((1-1.5)^2 + (2-1.5)^2) / (2-1) = (0.25 + 0.25) / 1 = 0.5
    sheet.set_str("A1", "1");
    sheet.set_str("A2", "2");
    sheet.set_str("B1", "=VAR(A1:A2)");

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(0.5))
    );
}
