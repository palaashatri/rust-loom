#[path = "../src/evaluation_cache.rs"]
mod evaluation_cache;

use evaluation_cache::EvaluationCache;
use loom_sheets_core::{CellRef, Value};
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

#[test]
fn current_sheet_values_are_reused_until_a_workbook_edit_refreshes_them() {
    let mut cache = EvaluationCache::default();
    let evaluations = Cell::new(0);
    let address = CellRef::parse("A1").expect("valid cell");

    let first = cache.get_or_calculate(0, || {
        evaluations.set(evaluations.get() + 1);
        HashMap::from([(address, Value::Number(1.0))])
    });
    let reused = cache.get_or_calculate(0, || {
        panic!("selection, scrolling, and resize must reuse calculated values")
    });
    assert!(Rc::ptr_eq(&first, &reused));
    assert_eq!(evaluations.get(), 1);

    let switched = cache.get_or_calculate(1, || {
        evaluations.set(evaluations.get() + 1);
        HashMap::from([(address, Value::Number(2.0))])
    });
    assert_eq!(switched.get(&address), Some(&Value::Number(2.0)));
    assert_eq!(evaluations.get(), 2);
    assert!(!Rc::ptr_eq(&first, &switched));

    let refreshed = cache.refresh(1, || {
        evaluations.set(evaluations.get() + 1);
        HashMap::from([(address, Value::Number(3.0))])
    });
    assert_eq!(refreshed.get(&address), Some(&Value::Number(3.0)));
    assert_eq!(evaluations.get(), 3);
    assert!(!Rc::ptr_eq(&switched, &refreshed));
}

#[test]
fn worker_values_replace_the_cache_without_running_a_calculation() {
    let mut cache = EvaluationCache::default();
    let address = CellRef::parse("A1").expect("valid cell");

    let pending = cache.cached_or_empty(0);
    assert!(pending.is_empty());
    let completed = cache.set_values(0, HashMap::from([(address, Value::Number(9.0))]));
    let reused = cache.cached_or_empty(0);

    assert!(Rc::ptr_eq(&completed, &reused));
    assert_eq!(reused.get(&address), Some(&Value::Number(9.0)));

    let switched = cache.cached_or_empty(1);
    assert!(switched.is_empty());
}
