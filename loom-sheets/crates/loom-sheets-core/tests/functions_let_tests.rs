//! LET (spike C): name binding, scoping and shadowing, references bound as
//! references, errors bound as values, and one evaluation per bound value.

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

fn number(value: Value) -> f64 {
    match value {
        Value::Number(n) => n,
        other => panic!("expected a number, got {other:?}"),
    }
}

#[test]
fn let_binds_names_for_the_calculation() {
    assert_eq!(number(eval(&[], "=LET(x,2,x*3)")), 6.0);
    // Names are case-insensitive, as all Excel names are.
    assert_eq!(number(eval(&[], "=LET(Abc,4,abc+1)")), 5.0);
}

#[test]
fn each_value_sees_the_names_bound_before_it() {
    assert_eq!(number(eval(&[], "=LET(x,2,y,x+1,x*y)")), 6.0);
}

#[test]
fn a_name_bound_twice_in_one_let_is_rejected_on_entry() {
    // Excel refuses to enter a LET that binds one name twice, so the cell reports a parse error.
    assert_eq!(
        eval(&[], "=LET(x,1,x,x+10,x)"),
        Value::Error(CalcError::Parse)
    );
    // Names are case-insensitive, so X and x are the same name.
    assert_eq!(eval(&[], "=LET(x,1,X,2,x)"), Value::Error(CalcError::Parse));
    // Excel checks the whole formula, so a LET in a branch that is not taken is rejected too.
    assert_eq!(
        eval(&[], "=IF(FALSE,LET(a,1,a,2,a),0)"),
        Value::Error(CalcError::Parse)
    );
    assert_eq!(
        eval(&[], "=LET(x,1,LET(y,2,y,3,y)+x)"),
        Value::Error(CalcError::Parse)
    );
}

#[test]
fn a_nested_let_shadows_only_inside_its_own_scope() {
    assert_eq!(number(eval(&[], "=LET(x,1,LET(x,2,x)+x)")), 3.0);
    assert_eq!(number(eval(&[], "=LET(a,LET(b,2,b*b),a+1)")), 5.0);
    assert_eq!(number(eval(&[], "=LET(y,5,LET(y,1,y)+y)")), 6.0);
}

#[test]
fn a_name_outside_every_binding_is_name_error() {
    assert_eq!(eval(&[], "=LET(x,1,y+x)"), Value::Error(CalcError::Name));
}

#[test]
fn a_reference_bound_to_a_name_still_works_as_a_range() {
    let setup = [("A1", "1"), ("A2", "2"), ("A3", "3")];
    assert_eq!(number(eval(&setup, "=LET(r,A1:A3,SUM(r))")), 6.0);
    assert_eq!(number(eval(&setup, "=LET(r,A1:A3,COUNT(r))")), 3.0);
    assert_eq!(number(eval(&setup, "=LET(x,A2,x+1)")), 3.0);
}

#[test]
fn an_error_value_is_bound_without_propagating() {
    assert_eq!(number(eval(&[], "=LET(e,1/0,IFERROR(e,5))")), 5.0);
}

#[test]
fn an_array_value_can_be_bound() {
    assert_eq!(number(eval(&[], "=LET(v,SEQUENCE(2),SUM(v))")), 3.0);
}

#[test]
fn a_volatile_value_is_drawn_once_per_binding() {
    assert_eq!(number(eval(&[], "=LET(r,RAND(),r-r)")), 0.0);
}

#[test]
fn malformed_let_calls_are_value_errors() {
    assert_eq!(eval(&[], "=LET(x,1)"), Value::Error(CalcError::Value));
    assert_eq!(eval(&[], "=LET(x,1,2,3)"), Value::Error(CalcError::Value));
    assert_eq!(eval(&[], "=LET(1,2,3)"), Value::Error(CalcError::Value));
}
