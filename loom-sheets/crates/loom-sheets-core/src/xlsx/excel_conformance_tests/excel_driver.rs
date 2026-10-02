//! Drives real Microsoft Excel through `tests/excel/dump_excel.ps1` and
//! compares what Excel reports with the Loom model, feature by feature.

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value as Json;

use crate::style::{CellAlignment, FillColor};
use crate::{CellRef, ChartKind, NumberFormat, Sheet, SheetObjectKind, Value};

use super::super::cell_refs::column_letters;
use super::super::styles::{number_format_code, rgb_for_fill, XfKey};

/// Cells where Excel's own engine legitimately disagrees with Loom's (the
/// export is right; the Loom evaluator differs). They are compared for
/// presence only, not for value.
///
/// * `Sales!B11` is `=Z99`: Loom shows an empty reference as blank, Excel as 0.
/// * `Modern!A14`/`A15`: Loom's FLOOR and CEILING reject the two-argument form.
/// * `Modern!A18`/`A19`: Loom's FV and PV return the opposite sign of Excel's.
/// * `Modern!A21`: Loom's TRIM keeps interior runs of spaces, Excel collapses them.
/// * `Modern!A24`: COUNTA counts `B11` (an Excel 0) where Loom sees a blank.
/// * `Modern!A16` is `=TODAY()`, which depends on the day the test runs.
/// * `Arrays!A12`: Loom's SUM over a spill range holding an array owner is `#VALUE!`.
const ENGINE_DIFFERENCES: &[(&str, &str)] = &[
    ("Sales", "B11"),
    ("Modern", "A14"),
    ("Modern", "A15"),
    ("Modern", "A16"),
    ("Modern", "A18"),
    ("Modern", "A19"),
    ("Modern", "A21"),
    ("Modern", "A24"),
    ("Arrays", "A12"),
];

/// Formulas Loom cannot parse (so the export keeps their text) are listed by
/// the fixture; this one is Excel-valid but not Loom-valid (`&` operator).
const TEXT_FALLBACKS: &[(&str, &str)] = &[("Sales", "B12"), ("Summary & Notes", "B8")];

const EXCEL_POINTS_PER_PIXEL: f64 = 0.75;

fn a1(cell: CellRef) -> String {
    format!("{}{}", column_letters(cell.col as usize), cell.row + 1)
}

fn normalise_formula(text: &str) -> String {
    text.trim_start_matches(['=', '{'])
        .trim_end_matches('}')
        .replace("_xlfn._xlws.", "")
        .replace("_xlfn.", "")
        .replace(' ', "")
        .to_ascii_uppercase()
}

fn excel_rgb(color: i64) -> String {
    format!(
        "{:02X}{:02X}{:02X}",
        color & 0xFF,
        (color >> 8) & 0xFF,
        (color >> 16) & 0xFF
    )
}

fn expected_error(value: &Value) -> Option<String> {
    match value {
        Value::Error(error) => Some(match error.code() {
            "PARSE!" => "#NAME?".to_string(),
            code => format!("#{code}"),
        }),
        _ => None,
    }
}

fn compare_value(
    expected: &Value,
    kind: &str,
    got: &Json,
    text: &str,
    out: &mut Vec<String>,
    at: &str,
) {
    let describe = || format!("{at}: Loom {expected:?}, Excel {kind} {got} ({text:?})");
    match (expected, kind) {
        (Value::Number(want), "number") => {
            let have = got.as_f64().unwrap_or(f64::NAN);
            if (want - have).abs() > 1e-9 * want.abs().max(1.0) {
                out.push(describe());
            }
        }
        (Value::Text(want), "string") if got.as_str() == Some(want.as_str()) => {}
        (Value::Bool(want), "bool") if got.as_bool() == Some(*want) => {}
        (Value::Empty, "empty") => {}
        (Value::Empty, "string") if got.as_str() == Some("") => {}
        (Value::Array(items, _, _), _) => {
            if let Some(first) = items.first() {
                compare_value(first, kind, got, text, out, at);
            }
        }
        (Value::Error(_), "error") if expected_error(expected).as_deref() == Some(text) => {}
        _ => out.push(describe()),
    }
}

fn expected_chart_types(kind: ChartKind) -> &'static [i64] {
    match kind {
        ChartKind::Bar => &[51],
        ChartKind::Line => &[4, 65],
        ChartKind::Pie => &[5],
        ChartKind::Scatter => &[-4169, 74],
    }
}

fn compare_sheet(
    sheet: &Sheet,
    values: &HashMap<CellRef, Value>,
    report: &Json,
    out: &mut Vec<String>,
) {
    let name = sheet.name.as_str();
    let cells: HashMap<&str, &Json> = report["cells"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|cell| Some((cell["ref"].as_str()?, cell)))
        .collect();

    let mut coordinates: Vec<CellRef> = sheet.cells.keys().copied().collect();
    coordinates.extend(sheet.styles.keys().copied());
    coordinates.extend(sheet.alignments.keys().copied());
    coordinates.sort();
    coordinates.dedup();
    for cell in coordinates {
        let at = format!("{name}!{}", a1(cell));
        let Some(excel) = cells.get(a1(cell).as_str()) else {
            out.push(format!("{at}: outside Excel's used range"));
            continue;
        };
        let raw = sheet.raw(cell).unwrap_or_default();
        let value = values.get(&cell).cloned().unwrap_or(Value::Empty);
        let fallback = TEXT_FALLBACKS.contains(&(name, a1(cell).as_str()));
        if !raw.is_empty() {
            let formula = excel["formula"].as_str().unwrap_or_default();
            if raw.starts_with('=') && !fallback {
                if normalise_formula(formula) != normalise_formula(raw) {
                    out.push(format!("{at}: formula {raw:?} became {formula:?}"));
                }
                if excel["hasFormula"].as_bool() != Some(true) {
                    out.push(format!("{at}: Excel does not see a formula"));
                }
            } else if raw.starts_with('=')
                && (excel["hasFormula"].as_bool() == Some(true) || formula != raw)
            {
                out.push(format!(
                    "{at}: unparseable {raw:?} should stay text, Excel has {formula:?}"
                ));
            }
            if !ENGINE_DIFFERENCES.contains(&(name, a1(cell).as_str())) && !fallback {
                compare_value(
                    &value,
                    excel["kind"].as_str().unwrap_or("?"),
                    &excel["value"],
                    excel["text"].as_str().unwrap_or_default(),
                    out,
                    &at,
                );
            }
        }
        let key = XfKey::from_sheet(sheet, cell);
        let style = key.style;
        let flag = |field: &str| excel[field].as_bool().unwrap_or(false);
        if flag("bold") != style.bold {
            out.push(format!(
                "{at}: bold {} vs Excel {}",
                style.bold,
                flag("bold")
            ));
        }
        if flag("italic") != style.italic {
            out.push(format!(
                "{at}: italic {} vs Excel {}",
                style.italic,
                flag("italic")
            ));
        }
        let underlined = excel["underline"].as_i64() == Some(2);
        if underlined != style.underline {
            out.push(format!(
                "{at}: underline {} vs Excel {underlined}",
                style.underline
            ));
        }
        let want_size = style
            .font_size
            .map_or(11.0, |px| f64::from(px) * EXCEL_POINTS_PER_PIXEL);
        if (excel["fontSize"].as_f64().unwrap_or(0.0) - want_size).abs() > 0.01 {
            out.push(format!(
                "{at}: font size {want_size} vs Excel {}",
                excel["fontSize"]
            ));
        }
        let want_fill =
            (style.fill != FillColor::None).then(|| rgb_for_fill(style.fill).to_string());
        let got_fill = (excel["fillPattern"].as_i64() == Some(1))
            .then(|| excel_rgb(excel["fillColor"].as_i64().unwrap_or(0)));
        if want_fill != got_fill {
            out.push(format!("{at}: fill {want_fill:?} vs Excel {got_fill:?}"));
        }
        let outlined = ["borderLeft", "borderTop", "borderBottom", "borderRight"]
            .iter()
            .all(|side| excel[*side].as_i64().is_some_and(|line| line > 0));
        // A neighbour's border shows on the shared edge, so only a styled
        // border must be present on all four sides.
        if style.border && !outlined {
            out.push(format!("{at}: border missing on a side in Excel"));
        }
        let want_align = match key.alignment {
            CellAlignment::Left => -4131,
            CellAlignment::Center => -4108,
            CellAlignment::Right => -4152,
            CellAlignment::General => 1,
        };
        if excel["hAlign"].as_i64() != Some(want_align) {
            out.push(format!(
                "{at}: alignment {want_align} vs Excel {}",
                excel["hAlign"]
            ));
        }
        let want_format = if style.number_format == NumberFormat::General {
            "General".to_string()
        } else {
            number_format_code(style)
        };
        if excel["numberFormat"].as_str() != Some(want_format.as_str()) {
            out.push(format!(
                "{at}: number format {want_format:?} vs Excel {}",
                excel["numberFormat"]
            ));
        }
    }

    if let Some(frozen) = report["freeze"].as_object() {
        let on = sheet.freeze_rows > 0 || sheet.freeze_cols > 0;
        if frozen["frozen"].as_bool() != Some(on)
            || (on
                && (frozen["splitRow"].as_u64() != Some(u64::from(sheet.freeze_rows))
                    || frozen["splitColumn"].as_u64() != Some(u64::from(sheet.freeze_cols))))
        {
            out.push(format!(
                "{name}: freeze panes {on} {}x{} vs Excel {frozen:?}",
                sheet.freeze_rows, sheet.freeze_cols
            ));
        }
    }
    for (col, width) in &sheet.col_widths {
        let got = report["columns"][*col as usize]["width"]
            .as_f64()
            .unwrap_or(0.0);
        if (got - f64::from(*width) * EXCEL_POINTS_PER_PIXEL).abs() > 0.6 {
            out.push(format!(
                "{name}: column {} width {width}px vs Excel {got}pt",
                col + 1
            ));
        }
    }
    for (row, height) in &sheet.row_heights {
        let got = report["rows"][*row as usize]["height"]
            .as_f64()
            .unwrap_or(0.0);
        if (got - f64::from(*height) * EXCEL_POINTS_PER_PIXEL).abs() > 0.6 {
            out.push(format!(
                "{name}: row {} height {height}px vs Excel {got}pt",
                row + 1
            ));
        }
    }
    // Default sizes carry over so unsized cells keep their Loom proportions.
    let standard = report["standardHeight"].as_f64().unwrap_or(0.0);
    if (standard - f64::from(crate::DEFAULT_ROW_HEIGHT) * EXCEL_POINTS_PER_PIXEL).abs() > 0.6 {
        out.push(format!("{name}: default row height {standard}pt"));
    }

    let charts = report["charts"].as_array().cloned().unwrap_or_default();
    match (&sheet.chart, charts.len()) {
        (None, 0) => {}
        (Some(chart), 1) => {
            let got = &charts[0];
            if !expected_chart_types(chart.kind).contains(&got["chartType"].as_i64().unwrap_or(0)) {
                out.push(format!(
                    "{name}: chart type {:?} vs Excel {}",
                    chart.kind, got["chartType"]
                ));
            }
            if got["title"].as_str() != Some(chart.title.as_str()) {
                out.push(format!(
                    "{name}: chart title {:?} vs Excel {}",
                    chart.title, got["title"]
                ));
            }
            let formula = got["series"][0]["formula"].as_str().unwrap_or_default();
            let cat = column_letters(chart.cat_col as usize);
            let val = column_letters(chart.val_col as usize);
            let (first, last) = (chart.start_row + 1, chart.end_row.map_or(0, |row| row + 1));
            for column in [&cat, &val] {
                let range = format!("${column}${first}:${column}${last}");
                if !formula.contains(&range) {
                    out.push(format!("{name}: chart series {formula:?} lacks {range}"));
                }
            }
            if let Some((_, _, max_col, _)) = sheet.used_range() {
                let anchor = got["topLeft"].as_str().unwrap_or_default();
                let letters: String = anchor
                    .chars()
                    .take_while(char::is_ascii_alphabetic)
                    .collect();
                if super::column_number(&letters) <= max_col + 1 {
                    out.push(format!(
                        "{name}: chart anchored at {anchor} covers data up to column {}",
                        max_col + 1
                    ));
                }
            }
        }
        (want, got) => out.push(format!(
            "{name}: expected chart {} but Excel has {got}",
            want.is_some()
        )),
    }

    let shapes = report["shapes"].as_array().cloned().unwrap_or_default();
    for object in &sheet.objects {
        let anchor = a1(object.anchor);
        let width = f64::from(object.width) * EXCEL_POINTS_PER_PIXEL;
        let height = f64::from(object.height) * EXCEL_POINTS_PER_PIXEL;
        let want_type = match object.kind {
            SheetObjectKind::Shape => 1,
            SheetObjectKind::Image => 13,
        };
        let found = shapes.iter().find(|shape| {
            shape["type"].as_i64() == Some(want_type)
                && shape["topLeft"].as_str() == Some(anchor.as_str())
        });
        let Some(found) = found else {
            out.push(format!(
                "{name}: no Excel shape of type {want_type} at {anchor}"
            ));
            continue;
        };
        if (found["width"].as_f64().unwrap_or(0.0) - width).abs() > 0.6
            || (found["height"].as_f64().unwrap_or(0.0) - height).abs() > 0.6
        {
            out.push(format!(
                "{name}: {anchor} size {width}x{height}pt vs Excel {}x{}",
                found["width"], found["height"]
            ));
        }
        if object.kind == SheetObjectKind::Shape {
            if found["text"].as_str() != Some(object.label.as_str()) {
                out.push(format!(
                    "{name}: shape text {:?} vs Excel {}",
                    object.label, found["text"]
                ));
            }
            if found["autoShape"].as_i64() != Some(1) {
                out.push(format!(
                    "{name}: shape is not a rectangle in Excel ({})",
                    found["autoShape"]
                ));
            }
            let fill = excel_rgb(found["fillRgb"].as_i64().unwrap_or(0));
            if fill != rgb_for_fill(object.fill) {
                out.push(format!(
                    "{name}: shape fill {} vs Excel {fill}",
                    rgb_for_fill(object.fill)
                ));
            }
        }
    }
    let images = sheet
        .objects
        .iter()
        .filter(|o| o.kind == SheetObjectKind::Image)
        .count() as u64;
    if report["pictures"].as_u64() != Some(images) {
        out.push(format!(
            "{name}: {images} pictures vs Excel {}",
            report["pictures"]
        ));
    }
}

/// Open the exported workbook in Excel and fail with every difference found.
pub(super) fn verify_with_excel(sheets: &[Sheet], path: &Path, dir: &Path) {
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/excel/dump_excel.ps1");
    let out = dir.join("excel-dump.json");
    let _ = std::fs::remove_file(&out);
    let status = std::process::Command::new("powershell")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(&script)
        .arg("-Path")
        .arg(path)
        .arg("-Out")
        .arg(&out)
        .status()
        .expect("start powershell");
    assert!(status.success(), "Excel could not open the workbook");
    let text = std::fs::read_to_string(&out).expect("Excel dump");
    let report: Json =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).expect("dump is JSON");
    eprintln!(
        "Excel {} build {}",
        report["excelVersion"], report["excelBuild"]
    );

    let evaluated = crate::workbook::evaluate_workbook(sheets);
    let mut problems = Vec::new();
    // A package Excel had to repair opens under a changed name or caption.
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    if report["workbookName"].as_str() != Some(file_name)
        || report["caption"]
            .as_str()
            .is_some_and(|caption| caption.contains("Repaired"))
    {
        problems.push(format!(
            "Excel repaired the package: name {} caption {}",
            report["workbookName"], report["caption"]
        ));
    }
    let excel_sheets = report["sheets"].as_array().cloned().unwrap_or_default();
    if excel_sheets.len() != sheets.len() {
        problems.push(format!(
            "{} sheets vs Excel {}",
            sheets.len(),
            excel_sheets.len()
        ));
    }
    for ((sheet, values), excel) in sheets.iter().zip(&evaluated).zip(&excel_sheets) {
        if excel["name"].as_str() != Some(sheet.name.as_str()) {
            problems.push(format!("sheet {:?} vs Excel {}", sheet.name, excel["name"]));
        }
        compare_sheet(sheet, values, excel, &mut problems);
    }
    // Excel's recalculation must agree with the values Loom cached.
    for entry in report["recalculated"].as_array().into_iter().flatten() {
        let sheet_name = entry["sheet"].as_str().unwrap_or_default();
        let reference = entry["ref"].as_str().unwrap_or_default();
        if ENGINE_DIFFERENCES.contains(&(sheet_name, reference))
            || TEXT_FALLBACKS.contains(&(sheet_name, reference))
        {
            continue;
        }
        let Some(index) = sheets.iter().position(|sheet| sheet.name == sheet_name) else {
            continue;
        };
        let Some(cell) = CellRef::parse(reference) else {
            continue;
        };
        let value = evaluated[index].get(&cell).cloned().unwrap_or(Value::Empty);
        let text = entry["value"].as_str().unwrap_or_default();
        compare_value(
            &value,
            entry["kind"].as_str().unwrap_or("?"),
            &entry["value"],
            text,
            &mut problems,
            &format!("recalculated {sheet_name}!{reference}"),
        );
    }
    assert!(
        problems.is_empty(),
        "Excel differs from the Loom model in {} places:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
