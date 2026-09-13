//! Formula evaluation coverage for Loom Sheets: core arithmetic, text,
//! conditional, financial, statistical, and lookup functions, evaluated
//! end to end through `evaluate` plus direct helper checks.

use loom_sheets_core::{
    averageif, compute_pivot, countif, evaluate, fv, goal_seek_bisection, hlookup, index_lookup,
    match_lookup, mode_single, pmt, pv, shift_formula_references, split_text_to_columns, stdev_p,
    stdev_s, sumif, sumproduct, text_concatenate, text_join, text_left, text_len, text_lower,
    text_mid, text_proper, text_repeat, text_right, text_substitute, text_trim, text_upper, var_p,
    var_s, vlookup, CalcError, CellRef, PivotAggregation, Sheet, Value,
};

#[test]
fn functions() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "1");
    sheet.set_str("A2", "2");
    sheet.set_str("A3", "3");
    sheet.set_str("B1", "=SUM(A1:A3)");
    sheet.set_str("B2", "=AVERAGE(A1:A3)");
    sheet.set_str("B3", "=ABS(-42)");
    sheet.set_str("B4", "=ROUND(1.567,2)");
    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(6.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Number(2.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B3").unwrap()),
        Some(&Value::Number(42.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B4").unwrap()),
        Some(&Value::Number(1.57))
    );
}

#[test]
fn comparison_and_text() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "=1<2");
    sheet.set_str("A2", "=CONCAT(\"loom\",\"-\",\"sheets\")");
    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("A1").unwrap()),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        vals.get(&CellRef::parse("A2").unwrap()),
        Some(&Value::Text("loom-sheets".to_string()))
    );
}

#[test]
fn if_branches_are_lazy_and_errors_propagate() {
    let mut sheet = Sheet::new("lazy");
    sheet.set_str("A1", "0");
    // Untaken error branch must not poison the result.
    sheet.set_str("B1", "=IF(A1=0, \"zero\", 1/A1)");
    sheet.set_str("B2", "=IF(A1=1, 1/A1, \"one-missing\")");
    // Errors propagate through arithmetic with their code intact.
    sheet.set_str("C1", "=1/0");
    sheet.set_str("C2", "=C1+5");
    sheet.set_str("C3", "=C1=1");

    let vals = evaluate(&sheet);
    let get = |a1: &str| vals.get(&CellRef::parse(a1).unwrap()).unwrap();
    assert_eq!(get("B1").display(), "zero");
    assert_eq!(get("B2").display(), "one-missing");
    assert_eq!(get("C2"), &Value::Error(CalcError::DivZero));
    assert_eq!(get("C3"), &Value::Error(CalcError::DivZero));
}

#[test]
fn absolute_references_parse_and_shift_correctly() {
    // `$` markers parse to plain references for evaluation.
    let mut sheet = Sheet::new("abs");
    sheet.set_str("A1", "4");
    sheet.set_str("B1", "=$A$1*2");
    sheet.set_str("B2", "=A$1+$A2");
    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(8.0))
    );
    // Shifting honors absolutes: column pinned, row pinned, both pinned.
    assert_eq!(shift_formula_references("=$A$1", 3, 3), "=$A$1");
    assert_eq!(shift_formula_references("=$A1", 2, 5), "=$A6");
    assert_eq!(shift_formula_references("=A$1", 2, 5), "=C$1");
    assert_eq!(
        shift_formula_references("=SUM($A$1:B2)", 1, 1),
        "=SUM($A$1:C3)"
    );
}

#[test]
fn math_and_statistical_functions_evaluate_correctly() {
    let mut sheet = Sheet::new("Math");
    sheet.set_str("A1", "=SQRT(16)");
    sheet.set_str("A2", "=POWER(2, 8)");
    sheet.set_str("A3", "=MOD(17, 5)");
    sheet.set_str("A4", "=FLOOR(3.7)");
    sheet.set_str("A5", "=CEILING(3.2)");
    sheet.set_str("A6", "=MEDIAN(10, 20, 30, 40, 50)");

    let evaluated = evaluate(&sheet);
    assert_eq!(
        evaluated.get(&CellRef::parse("A1").unwrap()),
        Some(&Value::Number(4.0))
    );
    assert_eq!(
        evaluated.get(&CellRef::parse("A2").unwrap()),
        Some(&Value::Number(256.0))
    );
    assert_eq!(
        evaluated.get(&CellRef::parse("A3").unwrap()),
        Some(&Value::Number(2.0))
    );
    assert_eq!(
        evaluated.get(&CellRef::parse("A4").unwrap()),
        Some(&Value::Number(3.0))
    );
    assert_eq!(
        evaluated.get(&CellRef::parse("A5").unwrap()),
        Some(&Value::Number(4.0))
    );
    assert_eq!(
        evaluated.get(&CellRef::parse("A6").unwrap()),
        Some(&Value::Number(30.0))
    );
}

#[test]
fn text_manipulation_formulas() {
    assert_eq!(text_concatenate(&["Hello", " ", "World"]), "Hello World");
    assert_eq!(text_left("Quarterly Report", 9), "Quarterly");
    assert_eq!(text_right("Quarterly Report", 6), "Report");
    assert_eq!(text_mid("Loom Studio 2026", 6, 6), "Studio");
    assert_eq!(text_len("Supercalifragilistic"), 20);
    assert_eq!(text_trim("   Too   many   spaces   "), "Too many spaces");
    assert_eq!(text_upper("loom sheets"), "LOOM SHEETS");
    assert_eq!(text_lower("LOOM SHEETS"), "loom sheets");
    assert_eq!(text_proper("the quick brown fox"), "The Quick Brown Fox");
}

#[test]
fn sumproduct_and_conditional_aggregations() {
    let quantities = vec![2.0, 5.0, 10.0];
    let unit_prices = vec![10.0, 20.0, 5.0];

    // SUMPRODUCT: (2*10) + (5*20) + (10*5) = 20 + 100 + 50 = 170.0
    let total = sumproduct(&[&quantities, &unit_prices]).unwrap();
    assert_eq!(total, 170.0);

    let sales = vec![100.0, 250.0, 50.0, 400.0, 150.0];

    // SUMIF sales > 100 -> 250 + 400 + 150 = 800.0
    assert_eq!(sumif(&sales, |v| v > 100.0), 800.0);

    // COUNTIF sales >= 200 -> 2
    assert_eq!(countif(&sales, |v| v >= 200.0), 2);

    // AVERAGEIF sales < 200 -> (100 + 50 + 150) / 3 = 100.0
    assert_eq!(averageif(&sales, |v| v < 200.0), Some(100.0));
}

#[test]
fn financial_formula_pmt_fv_pv() {
    // Loan of $10,000 at 5% annual interest (0.05/12 per month) for 36 months
    let monthly_rate = 0.05 / 12.0;
    let monthly_payment = pmt(monthly_rate, 36.0, 10000.0, 0.0, true).unwrap();
    // PMT should be approximately -$299.71
    assert!((monthly_payment - (-299.71)).abs() < 0.1);

    // Future value of $100/mo at 6% annual for 10 years (120 months)
    let rate_6 = 0.06 / 12.0;
    let future_val = fv(rate_6, 120.0, -100.0, 0.0, true).unwrap();
    // FV should be approximately $16,387.93
    assert!((future_val - 16387.93).abs() < 1.0);

    // Present value of $10,000 in 5 years at 5%
    let present_val = pv(0.05, 5.0, 0.0, 10000.0, true).unwrap();
    // PV should be approximately -$7,835.26
    assert!((present_val - (-7835.26)).abs() < 1.0);
}

#[test]
fn statistical_formulas_mode_stdev_var() {
    let dataset = vec![2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];

    // Mode of dataset is 4.0
    assert_eq!(mode_single(&dataset).unwrap(), 4.0);

    // Population variance & stdev: mean = 5.0, sum((x-5)^2) = 9+1+1+1+0+0+4+16 = 32. var = 32/8 = 4.0. stdev = 2.0
    assert_eq!(var_p(&dataset).unwrap(), 4.0);
    assert_eq!(stdev_p(&dataset).unwrap(), 2.0);

    // Sample variance: 32 / 7 = 4.5714...
    let v_s = var_s(&dataset).unwrap();
    assert!((v_s - (32.0 / 7.0)).abs() < 1e-5);
    assert!((stdev_s(&dataset).unwrap() - (32.0 / 7.0f64).sqrt()).abs() < 1e-5);
}

#[test]
fn text_join_split_substitute_repeat() {
    // TEXTJOIN with and without skipping empties
    let values = vec!["a".to_string(), String::new(), "b".to_string()];
    assert_eq!(text_join("-", false, &values), "a--b");
    assert_eq!(text_join("-", true, &values), "a-b");
    assert_eq!(text_join(",", true, &[]), "");

    // SPLIT keeps empty fields from consecutive delimiters
    let fields = split_text_to_columns("a,,b", ",").unwrap();
    assert_eq!(
        fields,
        vec!["a".to_string(), String::new(), "b".to_string()]
    );
    assert_eq!(
        split_text_to_columns("x|y", "|").unwrap(),
        vec!["x".to_string(), "y".to_string()]
    );
    assert!(split_text_to_columns("abc", "").is_err());

    // REPT
    assert_eq!(text_repeat("ab", 3), "ababab");
    assert_eq!(text_repeat("ab", 0), "");

    // SUBSTITUTE
    // "Banana" contains "na" at bytes 2 and 4; replacing all yields Ba|ny|ny
    assert_eq!(
        text_substitute("Banana", "na", "ny", true, 0).unwrap(),
        "Banyny"
    );
    assert_eq!(
        text_substitute("Banana", "NA", "ny", false, 0).unwrap(),
        "Banyny"
    );
    assert_eq!(
        text_substitute("Banana", "NA", "ny", true, 0).unwrap(),
        "Banana"
    );
    // Only the second instance replaced
    assert_eq!(
        text_substitute("Banana", "na", "X", true, 2).unwrap(),
        "BanaX"
    );
    // Out-of-range instance leaves the text unchanged
    assert_eq!(
        text_substitute("Banana", "na", "X", true, 9).unwrap(),
        "Banana"
    );
    assert!(text_substitute("abc", "", "x", true, 0).is_err());
}

#[test]
fn vlookup_hlookup_and_index_match() {
    let table = vec![
        vec!["ID".into(), "Name".into(), "Price".into()],
        vec!["P101".into(), "Widget".into(), "9.99".into()],
        vec!["P102".into(), "Gadget".into(), "19.99".into()],
        vec!["P103".into(), "Doohickey".into(), "4.99".into()],
    ];

    // VLOOKUP P102 -> Col 2 (Name) = "Gadget"
    assert_eq!(vlookup("P102", &table, 2, true).unwrap(), "Gadget");
    // VLOOKUP P103 -> Col 3 (Price) = "4.99"
    assert_eq!(vlookup("P103", &table, 3, true).unwrap(), "4.99");
    assert!(vlookup("P999", &table, 2, true).is_err());

    // HLOOKUP Price -> Row 3 (P102's price) = "19.99"
    assert_eq!(hlookup("Price", &table, 3, true).unwrap(), "19.99");

    // MATCH Gadget in Col 2
    let names = vec!["Widget".into(), "Gadget".into(), "Doohickey".into()];
    assert_eq!(match_lookup("Gadget", &names, true).unwrap(), 2);

    // INDEX table row 2 (P101), col 2 (Name) = "Widget"
    assert_eq!(index_lookup(&table, 2, 2).unwrap(), "Widget");
}

#[test]
fn goal_seek_solves_equations() {
    let x = goal_seek_bisection(|v| v * v, 0.0, 10.0, 25.0, 1e-6, 200).unwrap();
    assert!((x - 5.0).abs() < 1e-6, "x^2=25 gave {x}");

    let r = goal_seek_bisection(
        |rate| 100.0 * (1.0 + rate).powi(10),
        0.0,
        0.2,
        200.0,
        1e-6,
        200,
    )
    .unwrap();
    let expected_r = 2f64.powf(0.1) - 1.0;
    assert!((r - expected_r).abs() < 1e-6, "rate solve gave {r}");

    // Linear function whose first midpoint is the exact root: returns after
    // only the two bracket evaluations plus one midpoint evaluation.
    let calls = std::cell::Cell::new(0u32);
    let hit = goal_seek_bisection(
        |v| {
            calls.set(calls.get() + 1);
            v - 4.0
        },
        0.0,
        8.0,
        0.0,
        1e-9,
        1000,
    )
    .unwrap();
    assert_eq!(hit, 4.0);
    assert_eq!(calls.get(), 3);

    let same_signs = goal_seek_bisection(|v| v + 10.0, -1.0, 1.0, 0.0, 1e-6, 64).unwrap_err();
    assert!(same_signs.contains("no sign change"), "{same_signs}");

    let inverted = goal_seek_bisection(|v| v * v, 10.0, 0.0, 25.0, 1e-6, 64).unwrap_err();
    assert!(inverted.contains("hi > lo"), "{inverted}");

    let constant = goal_seek_bisection(|_| 7.0, 0.0, 5.0, 3.0, 1e-6, 64).unwrap_err();
    assert!(constant.contains("no sign change"), "{constant}");
}

#[test]
fn pivot_grouping_and_aggregation() {
    let keys = vec![
        "East".to_string(),
        "West".to_string(),
        "East".to_string(),
        "South".to_string(),
        "West".to_string(),
    ];
    let values = vec![10.0, 20.0, 15.0, 30.0, 5.0];

    // SUM: East = 10+15 = 25, South = 30, West = 20+5 = 25.
    // Results are sorted by group key ascending: East < South < West.
    assert_eq!(
        compute_pivot(&keys, &values, PivotAggregation::Sum).unwrap(),
        vec![
            ("East".to_string(), 25.0),
            ("South".to_string(), 30.0),
            ("West".to_string(), 25.0),
        ]
    );

    // AVERAGE: East = (10+15)/2 = 12.5
    let averages = compute_pivot(&keys, &values, PivotAggregation::Average).unwrap();
    assert_eq!(averages[0], ("East".to_string(), 12.5));

    // MIN/MAX/COUNT for the East group: min = 10, max = 15, count = 2.
    let mins = compute_pivot(&keys, &values, PivotAggregation::Min).unwrap();
    let maxes = compute_pivot(&keys, &values, PivotAggregation::Max).unwrap();
    let counts = compute_pivot(&keys, &values, PivotAggregation::Count).unwrap();
    assert_eq!(mins[0], ("East".to_string(), 10.0));
    assert_eq!(maxes[0], ("East".to_string(), 15.0));
    assert_eq!(counts[0], ("East".to_string(), 2.0));

    // Mismatched lengths must be rejected.
    assert!(compute_pivot(&keys[..1], &values, PivotAggregation::Sum).is_err());

    // Empty input yields an empty result.
    let empty: Vec<String> = Vec::new();
    assert!(compute_pivot(&empty, &[], PivotAggregation::Sum)
        .unwrap()
        .is_empty());
}
