//! Differential test: Loom's formula engine against real Microsoft Excel 16.
//!
//! `tests/fixtures/excel_corpus.json` holds a shared data block, a corpus of
//! formulas and the value Excel produced for each (see `tests/excel/eval_corpus.ps1`).
//! Every formula is evaluated by Loom on a sheet with the same data and compared:
//! numbers within 1e-9 relative tolerance, text exactly, errors by code.
//! Each formula sits at `AD{1 + 10 * n}` so a spilled array cannot be blocked by its neighbour.
//! Formulas Excel refuses to accept (`syntax`) are only checked for not panicking.
//! Known, deliberate differences are listed in `ALLOWED` with a reason; the test
//! also fails when an allowed formula starts to match, so the list stays honest.

use loom_sheets_core::{evaluate, CalcError, CellRef, Sheet, Value};
use serde_json::Value as Json;
use std::collections::HashMap;

/// Formulas whose Loom result intentionally or unavoidably differs from Excel.
const ALLOWED: &[(&str, &str)] = &[
    (
        "=MOD(1E15,7)",
        "Excel MOD reports #NUM! when the quotient is astronomically large; Loom computes the remainder",
    ),
    ("=VALUE(\"1/2/2024\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=VALUE(\"$5\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=TEXT(1E6,\"#,##0.00\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=TEXT(12345678,\"#,##0.00\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=TEXT(0,\"+0;-0;zero\")", "Excel reads the letters e and r in a format code as date/time codes; Loom treats them as literals"),
    ("=IF(FALSE,1,)", "omitted (empty) function arguments are not parsed"),
    ("=IF(TRUE,,2)", "omitted (empty) function arguments are not parsed"),
    ("=XLOOKUP(30,G1:G5,H1:H5,,0,1)", "omitted (empty) function arguments are not parsed"),
    ("=XLOOKUP(40,G5:G1,H5:H1,\"nf\",-1,-2)", "XLOOKUP binary search modes (2, -2) are not implemented; Loom scans linearly"),
    ("=XLOOKUP(35,G5:G1,H5:H1,\"nf\",-1,-2)", "XLOOKUP binary search modes (2, -2) are not implemented; Loom scans linearly"),
    ("=XLOOKUP(30,G1:G5,H1:H5,\"nf\",3)", "XLOOKUP match_mode 3 is invalid; Excel accepts it, Loom reports not found"),
    ("=COLUMNS(A:C)", "whole-column and whole-row references are not supported"),
    ("=ROWS(1:3)", "whole-column and whole-row references are not supported"),
    ("=MONTH(\"3/15/2024\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=DATEVALUE(\"3/15/2024\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=DATEVALUE(\"March 15, 2024\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=DATEVALUE(\"3/1/1900\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=DATEVALUE(\"12/31/9999\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=DATEVALUE(\"3/5\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=DATEVALUE(\"Jan 5 2024\")", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=RATE(10,-100,1000)", "RATE iterates to a different tolerance; both are within 1e-8 of the true rate 0"),
    ("=(0.1+0.2)-0.3", "Excel snaps a final subtraction near zero to exactly 0; Loom keeps the binary residue"),
    ("=1234567890123456&\"\"", "Excel truncates number literals to 15 significant digits while parsing; Loom keeps 16"),
    ("=#NULL!", "#NULL! has no CalcError variant"),
    ("=SUM(1,", "Excel auto-closes an unterminated call on entry; Loom reports a parse error"),
    ("=A1 B1", "the range intersection operator (space) is not supported"),
    ("=A1:A3 A2:B2", "the range intersection operator (space) is not supported"),
    ("=SUM(A1:A3,)", "omitted (empty) function arguments are not parsed"),
    ("=IF(,1,2)", "omitted (empty) function arguments are not parsed"),
    ("=SUM(,1)", "omitted (empty) function arguments are not parsed"),
    ("=\"$5\"+1", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
    ("=\"1/2\"+1", "Excel host locale is en-IN (d/m/y dates, lakh digit grouping, no $ currency); Loom follows en-US"),
];

fn error_code(e: CalcError) -> &'static str {
    match e {
        CalcError::DivZero => "#DIV/0!",
        CalcError::NA => "#N/A",
        CalcError::Value => "#VALUE!",
        CalcError::Name => "#NAME?",
        CalcError::Ref => "#REF!",
        CalcError::Num => "#NUM!",
        CalcError::Spill => "#SPILL!",
        CalcError::Calc => "#CALC!",
        CalcError::Parse => "#PARSE!",
    }
}

fn describe(v: &Value) -> String {
    match v {
        Value::Array(items, ..) => items.first().map(describe).unwrap_or_default(),
        Value::Number(n) => format!("num {n:?}"),
        Value::Text(s) => format!("text {s:?}"),
        Value::Bool(b) => format!("bool {b}"),
        Value::Empty => "empty".into(),
        Value::Error(e) => format!("err {}", error_code(*e)),
    }
}

fn matches(kind: &str, want: &Json, got: &Value) -> bool {
    let got = match got {
        Value::Array(items, ..) => items.first().unwrap_or(&Value::Empty),
        other => other,
    };
    match (kind, got) {
        ("num", Value::Number(n)) => {
            let w = want.as_f64().unwrap();
            (n - w).abs() <= 1e-9 * w.abs().max(n.abs()) || (n - w).abs() < 1e-300
        }
        ("num", Value::Empty) => want.as_f64() == Some(0.0),
        ("text", Value::Text(s)) => want.as_str() == Some(s.as_str()),
        ("text", Value::Empty) => want.as_str() == Some(""),
        ("bool", Value::Bool(b)) => want.as_bool() == Some(*b),
        ("err", Value::Error(e)) => want.as_str() == Some(error_code(*e)),
        ("empty", Value::Empty) => true,
        _ => false,
    }
}

#[test]
fn loom_formulas_match_real_excel() {
    let raw = include_str!("fixtures/excel_corpus.json");
    let fixture: Json = serde_json::from_str(raw).expect("fixture parses");
    let mut sheet = Sheet::new("t");
    for (cell, value) in fixture["data"].as_object().unwrap() {
        sheet.set_str(cell, value.as_str().unwrap());
    }
    let cases = fixture["cases"].as_array().unwrap();
    for (i, case) in cases.iter().enumerate() {
        sheet.set_str(&format!("AD{}", 1 + i * 10), case[0].as_str().unwrap());
    }
    let values = evaluate(&sheet);
    let allowed: HashMap<&str, &str> = ALLOWED.iter().copied().collect();
    let (mut compared, mut skipped, mut bad, mut stale) = (0, 0, Vec::new(), Vec::new());
    for (i, case) in cases.iter().enumerate() {
        let (formula, kind, want) = (
            case[0].as_str().unwrap(),
            case[1].as_str().unwrap(),
            &case[2],
        );
        if kind == "syntax" {
            skipped += 1;
            continue;
        }
        compared += 1;
        let got = values
            .get(&CellRef::parse(&format!("AD{}", 1 + i * 10)).unwrap())
            .cloned()
            .unwrap_or(Value::Empty);
        let ok = matches(kind, want, &got);
        match (ok, allowed.contains_key(formula)) {
            (false, false) => bad.push(format!(
                "{formula}\n    excel: {kind} {want}\n    loom:  {}",
                describe(&got)
            )),
            (true, true) => stale.push(formula.to_string()),
            _ => {}
        }
    }
    println!(
        "compared {compared}, skipped {skipped} (Excel rejects), mismatches {}, allowed {}",
        bad.len(),
        ALLOWED.len()
    );
    assert!(
        stale.is_empty(),
        "allow-listed formulas now match Excel; remove them: {stale:#?}"
    );
    assert!(
        bad.is_empty(),
        "{} formulas differ from Excel:\n{}",
        bad.len(),
        bad.join("\n")
    );
}
