//! Excel-exact semantics for FLOOR, CEILING, FV, PV, PMT, TRIM, COUNTA and `&`.

use loom_sheets_core::{evaluate, shift_formula_references, CalcError, CellRef, Sheet, Value};

fn eval(formula: &str) -> Value {
    let mut sheet = Sheet::new("t");
    sheet.set_str("Z1", formula);
    evaluate(&sheet)
        .get(&CellRef::parse("Z1").unwrap())
        .cloned()
        .unwrap_or(Value::Empty)
}

fn num(formula: &str) -> f64 {
    match eval(formula) {
        Value::Number(n) => n,
        other => panic!("{formula} -> {other:?}"),
    }
}

fn close(formula: &str, want: f64) {
    let got = num(formula);
    assert!((got - want).abs() < 1e-6, "{formula}: {got} vs {want}");
}

#[test]
fn floor_matches_excel() {
    assert_eq!(num("=FLOOR(2.5,1)"), 2.0);
    assert_eq!(num("=FLOOR(-2.5,1)"), -3.0);
    assert_eq!(num("=FLOOR(-2.5,-1)"), -2.0);
    assert_eq!(num("=FLOOR(7,2)"), 6.0);
    assert_eq!(eval("=FLOOR(5,0)"), Value::Error(CalcError::DivZero));
    assert_eq!(eval("=FLOOR(5,-1)"), Value::Error(CalcError::Num));
}

#[test]
fn ceiling_matches_excel() {
    assert_eq!(num("=CEILING(2.5,1)"), 3.0);
    assert_eq!(num("=CEILING(-2.5,1)"), -2.0);
    assert_eq!(num("=CEILING(-2.5,-1)"), -3.0);
    assert_eq!(num("=CEILING(7,2)"), 8.0);
    assert_eq!(num("=CEILING(5,0)"), 0.0);
    assert_eq!(eval("=CEILING(5,-1)"), Value::Error(CalcError::Num));
}

#[test]
fn financial_functions_match_excel() {
    close("=FV(0.05,10,-100)", 1257.789253);
    close("=FV(0,10,-100,-1000)", 2000.0);
    close("=PV(0.05,10,-100)", 772.1734929);
    close("=PMT(0.05,10,1000)", -129.5045750);
}

#[test]
fn trim_collapses_spaces_only() {
    assert_eq!(eval("=TRIM(\"  a   b  \")"), Value::Text("a b".into()));
    assert_eq!(eval("=TRIM(\"a\tb\")"), Value::Text("a\tb".into()));
}

#[test]
fn counta_counts_everything_but_blanks() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "1");
    sheet.set_str("A2", "text");
    sheet.set_str("A3", "=1=1");
    sheet.set_str("A4", "=1/0");
    sheet.set_str("A5", "=\"\"");
    // A6 blank
    sheet.set_str("B1", "=COUNTA(A1:A6)");
    let v = evaluate(&sheet);
    assert_eq!(
        v.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(5.0))
    );
}

#[test]
fn ampersand_concatenates_with_excel_precedence() {
    assert_eq!(eval("=1+2&3"), Value::Text("33".into()));
    assert_eq!(eval("=\"a\"&1=\"a1\""), Value::Bool(true));
    assert_eq!(eval("=1.5&\"x\""), Value::Text("1.5x".into()));
    assert_eq!(eval("=(0.1+0.2)&\"\""), Value::Text("0.3".into()));
    assert_eq!(eval("=1=1&\"\""), Value::Bool(false));
    assert_eq!(eval("=(1=1)&\"!\""), Value::Text("TRUE!".into()));
    assert_eq!(eval("=\"a&b\""), Value::Text("a&b".into()));
    assert_eq!(eval("=\"a\"&1/0"), Value::Error(CalcError::DivZero));
    assert_eq!(eval("=\"a\"&B9&\"c\""), Value::Text("ac".into()));
}

#[test]
fn ampersand_cell_references_and_rewrites() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "foo");
    sheet.set_str("B1", "7");
    sheet.set_str("C1", "=A1&B1");
    let v = evaluate(&sheet);
    assert_eq!(
        v.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Text("foo7".into()))
    );
    assert_eq!(
        shift_formula_references("=A1&B1&\"&A1\"", 1, 1),
        "=B2&C2&\"&A1\""
    );
}

#[test]
fn unary_minus_negates_and_binds_tighter_than_power() {
    assert_eq!(num("=-5"), -5.0);
    assert_eq!(num("=2--3"), 5.0);
    assert_eq!(num("=-2^2"), 4.0);
    assert_eq!(num("=2^-1"), 0.5);
    assert_eq!(num("=2^3^2"), 64.0);
    assert_eq!(num("=-(1+2)"), -3.0);
}

#[test]
fn int_matches_excel() {
    assert_eq!(num("=INT(2.7)"), 2.0);
    assert_eq!(num("=INT(-2.7)"), -3.0);
}

#[test]
fn log_matches_excel() {
    close("=LOG(100)", 2.0);
}

#[test]
fn floor_and_ceiling_need_both_arguments() {
    assert_eq!(eval("=FLOOR(3.7)"), Value::Error(CalcError::Value));
    assert_eq!(eval("=CEILING(3.2)"), Value::Error(CalcError::Value));
    assert_eq!(eval("=FLOOR(1,2,3)"), Value::Error(CalcError::Value));
    // Binary noise must not drop a whole step: 0.3 / 0.1 is 2.9999999999999996.
    close("=FLOOR(0.3,0.1)", 0.3);
}

#[test]
fn percent_is_a_postfix_operator_binding_tighter_than_power_and_minus() {
    assert_eq!(num("=50%"), 0.5);
    assert_eq!(num("=-50%"), -0.5);
    assert_eq!(num("=200*10%"), 20.0);
    assert_eq!(num("=50%%"), 0.005);
    assert_eq!(num("=2^50%"), 2f64.sqrt());
    assert_eq!(num("=(1+1)%"), 0.02);
    assert_eq!(num("=10%+5%"), 0.15000000000000002);
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "25");
    sheet.set_str("B1", "=A1%");
    sheet.set_str("C1", "=\"5%\"&\"x\"");
    let v = evaluate(&sheet);
    assert_eq!(
        v.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(0.25))
    );
    assert_eq!(
        v.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Text("5%x".into()))
    );
    assert_eq!(shift_formula_references("=A1%+\"5%\"", 1, 0), "=B1%+\"5%\"");
}

#[test]
fn names_with_dots_and_digits_are_functions_not_cell_references() {
    let mut sheet = Sheet::new("t");
    for (row, v) in [2, 4, 4, 4, 5, 5, 7, 9].iter().enumerate() {
        sheet.set_str(&format!("A{}", row + 1), &v.to_string());
    }
    sheet.set_str("B1", "=STDEV.P(A1:A8)");
    sheet.set_str("B2", "=LOG10(A2*25)");
    sheet.set_str("B3", "=VAR.S(A1:A8)+A1");
    // LOG10 as a name must not be read as the cell LOG10 (column LOG, row 10).
    sheet.set_str("LOG10", "100");
    sheet.set_str("B4", "=LOG10(LOG10)");
    sheet.set_str("B5", "=AB12+LOG10(A1)");
    let v = evaluate(&sheet);
    let at = |a: &str| v.get(&CellRef::parse(a).unwrap()).cloned();
    assert_eq!(at("B1"), Some(Value::Number(2.0)));
    assert_eq!(at("B2"), Some(Value::Number(2.0)));
    assert_eq!(at("B3"), Some(Value::Number(32.0 / 7.0 + 2.0)));
    assert_eq!(at("B4"), Some(Value::Number(2.0)));
    match at("B5") {
        Some(Value::Number(n)) => assert!((n - 2f64.log10()).abs() < 1e-12),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        shift_formula_references("=LOG10(A1)+STDEV.S(B1:B2)+Sheet2!A1", 1, 1),
        "=LOG10(B2)+STDEV.S(C2:C3)+Sheet2!B2"
    );
}

#[test]
fn unquoted_sheet_names_may_contain_digits() {
    let mut data = Sheet::new("Sheet2");
    data.set_str("A1", "21");
    let mut main = Sheet::new("Main");
    main.set_str("A1", "=Sheet2!A1*2");
    main.set_str("A2", "=SUM(Sheet2!A1:A1)+'Sheet2'!A1");
    let v = loom_sheets_core::workbook::evaluate_workbook(&[main, data]);
    assert_eq!(
        v[0].get(&CellRef::parse("A1").unwrap()),
        Some(&Value::Number(42.0))
    );
    assert_eq!(
        v[0].get(&CellRef::parse("A2").unwrap()),
        Some(&Value::Number(42.0))
    );
}

#[test]
fn non_ascii_text_survives_the_lexer() {
    assert_eq!(eval("=\"héllo €\""), Value::Text("héllo €".into()));
    assert_eq!(eval("=LEN(\"héllo\")"), Value::Number(5.0));
    let mut data = Sheet::new("Données");
    data.set_str("A1", "7");
    let mut main = Sheet::new("Main");
    main.set_str("A1", "='Données'!A1+1");
    let v = loom_sheets_core::workbook::evaluate_workbook(&[main, data]);
    assert_eq!(
        v[0].get(&CellRef::parse("A1").unwrap()),
        Some(&Value::Number(8.0))
    );
}

#[test]
fn blanks_and_booleans_take_part_in_arithmetic() {
    assert_eq!(num("=B9+1"), 1.0);
    assert_eq!(num("=B9*5"), 0.0);
    assert_eq!(num("=TRUE+1"), 2.0);
    assert_eq!(num("=\"3\"+1"), 4.0);
    assert_eq!(eval("=\"x\"+1"), Value::Error(CalcError::Value));
}

#[test]
fn comparisons_follow_excels_type_order_and_ignore_case() {
    assert_eq!(eval("=\"a\"=\"A\""), Value::Bool(true));
    assert_eq!(eval("=\"a\"<\"b\""), Value::Bool(true));
    assert_eq!(eval("=1=\"1\""), Value::Bool(false));
    assert_eq!(eval("=1<\"1\""), Value::Bool(true));
    assert_eq!(eval("=\"z\"<TRUE"), Value::Bool(true));
    assert_eq!(eval("=B9=0"), Value::Bool(true));
    assert_eq!(eval("=B9=\"\""), Value::Bool(true));
    assert_eq!(eval("=B9<1"), Value::Bool(true));
}

#[test]
fn sum_style_aggregates_skip_text_inside_ranges() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "Amount");
    sheet.set_str("A2", "4");
    sheet.set_str("A3", "6");
    sheet.set_str("B1", "=SUM(A1:A3)");
    sheet.set_str("B2", "=AVERAGE(A1:A3)");
    sheet.set_str("B3", "=MAX(A1:A3)");
    sheet.set_str("B4", "=MIN(A1:A3)");
    sheet.set_str("B5", "=COUNT(A1:A3)");
    sheet.set_str("B6", "=SUM(\"x\")");
    let v = evaluate(&sheet);
    let at = |a: &str| v.get(&CellRef::parse(a).unwrap()).cloned();
    assert_eq!(at("B1"), Some(Value::Number(10.0)));
    assert_eq!(at("B2"), Some(Value::Number(5.0)));
    assert_eq!(at("B3"), Some(Value::Number(6.0)));
    assert_eq!(at("B4"), Some(Value::Number(4.0)));
    assert_eq!(at("B5"), Some(Value::Number(2.0)));
    assert_eq!(at("B6"), Some(Value::Error(CalcError::Value)));
}
