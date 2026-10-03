//! Compare what Loom calculates for imported formulas with the value Excel
//! stored next to each formula.
//!
//! A formula can import perfectly and still show a different (or error)
//! result in Loom: a function Loom does not have yet, an operator it does not
//! parse, a defined name it cannot resolve. Excel's stored result is the
//! reference, so a disagreement is reported before the workbook replaces the
//! one the user has open.

use std::collections::HashMap;

use crate::workbook::evaluate_workbook;
use crate::{CellRef, Sheet, Value};

use super::sheet_reader::Cached;

/// Functions whose result changes on every calculation; Excel's stored value
/// is stale by definition, so they are never compared.
const VOLATILE: &[&str] = &["TODAY(", "NOW(", "RAND(", "RANDBETWEEN(", "RANDARRAY("];

fn same(excel: &Cached, loom: Option<&Value>) -> bool {
    match (excel, loom) {
        (Cached::Empty, _) => true,
        (Cached::Number(a), Some(Value::Number(b))) => {
            (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0)
        }
        // A reference to an empty cell is 0 in Excel and empty in Loom.
        (Cached::Number(a), Some(Value::Empty) | None) => *a == 0.0,
        (Cached::Text(a), Some(Value::Text(b))) => a == b,
        (Cached::Text(a), Some(Value::Empty) | None) => a.is_empty(),
        (Cached::Bool(a), Some(Value::Bool(b))) => a == b,
        (Cached::Error(_), Some(Value::Error(_))) => true,
        // A spilled array's formula cell stores the top-left result.
        (excel, Some(Value::Array(items, _, _))) => {
            items.first().is_some_and(|first| same(excel, Some(first)))
        }
        _ => false,
    }
}

/// `calculated[i]` lists the formula cells of `sheets[i]` with Excel's stored
/// result. True when any of them differs from Loom's calculation.
pub(super) fn formula_results_differ(
    sheets: &[Sheet],
    calculated: &[Vec<(CellRef, Cached)>],
) -> bool {
    let results: Vec<HashMap<CellRef, Value>> = evaluate_workbook(sheets);
    sheets.iter().enumerate().any(|(index, sheet)| {
        calculated[index].iter().any(|(at, excel)| {
            let volatile = sheet.raw(*at).is_some_and(|raw| {
                VOLATILE
                    .iter()
                    .any(|name| raw.to_ascii_uppercase().contains(name))
            });
            !volatile && !same(excel, results[index].get(at))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(cells: &[(&str, &str)]) -> Sheet {
        let mut sheet = Sheet::new("S");
        for (at, raw) in cells {
            sheet.set_str(at, raw);
        }
        sheet
    }

    #[test]
    fn agreeing_results_are_not_reported() {
        let sheets = [sheet(&[
            ("A1", "2"),
            ("B1", "=A1*2"),
            ("C1", "=A1&\"x\""),
            ("D1", "=1/0"),
        ])];
        let at = |a: &str| CellRef::parse(a).unwrap();
        let ok = vec![vec![
            (at("B1"), Cached::Number(4.0)),
            (at("D1"), Cached::Error("#DIV/0!".into())),
        ]];
        assert!(!formula_results_differ(&sheets, &ok));
        // Loom now evaluates `&` like Excel; a differing cached text is reported.
        let same = vec![vec![(at("C1"), Cached::Text("2x".into()))]];
        assert!(!formula_results_differ(&sheets, &same));
        let bad = vec![vec![(at("C1"), Cached::Text("2y".into()))]];
        assert!(formula_results_differ(&sheets, &bad));
        let wrong = vec![vec![(at("B1"), Cached::Number(5.0))]];
        assert!(formula_results_differ(&sheets, &wrong));
    }

    #[test]
    fn a_spilled_array_is_compared_by_its_top_left_result() {
        let sheets = [sheet(&[("A1", "=SEQUENCE(2,2)")])];
        let at = CellRef::parse("A1").unwrap();
        assert!(!formula_results_differ(
            &sheets,
            &[vec![(at, Cached::Number(1.0))]]
        ));
        assert!(formula_results_differ(
            &sheets,
            &[vec![(at, Cached::Number(2.0))]]
        ));
    }

    #[test]
    fn volatile_and_uncached_formulas_are_ignored() {
        let sheets = [sheet(&[("A1", "=NOW()")])];
        let at = CellRef::parse("A1").unwrap();
        assert!(!formula_results_differ(
            &sheets,
            &[vec![(at, Cached::Number(45000.5))]]
        ));
        assert!(!formula_results_differ(
            &sheets,
            &[vec![(at, Cached::Empty)]]
        ));
    }
}
