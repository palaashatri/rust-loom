//! Import regressions against workbooks authored in real Microsoft Excel.
//!
//! The three fixtures under `tests/fixtures/` were written by Excel (COM
//! automation, `tests/excel/author_import_fixtures.ps1`) and contain no
//! third-party content. The tests need no Excel; they pin what the importer
//! must read from what Excel actually writes.

use crate::style::{CellAlignment, FillColor};
use crate::workbook::evaluate_workbook;
use crate::{CellRef, NumberFormat, Sheet, SheetObjectKind, Value};

use super::{import_xlsx_sheets, XlsxImport, XlsxImportWarning};

fn fixture(name: &str) -> XlsxImport {
    let path = format!("{}/tests/fixtures/{name}.xlsx", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    import_xlsx_sheets(&bytes).expect("Excel workbook imports")
}

fn at(a1: &str) -> CellRef {
    CellRef::parse(a1).expect("cell reference")
}

fn raw<'a>(sheet: &'a Sheet, a1: &str) -> Option<&'a str> {
    sheet.raw(at(a1))
}

fn has(import: &XlsxImport, warning: XlsxImportWarning) -> bool {
    import.warnings.contains(&warning)
}

#[test]
fn excel_values_text_and_numbers_import_exactly() {
    let import = fixture("excel_basic");
    let sheet = &import.sheets[0];
    assert_eq!(sheet.name, "Data & Q1");
    assert_eq!(raw(sheet, "A1"), Some("Item"));
    assert_eq!(raw(sheet, "A4"), Some("Caf\u{e9} \u{65e5}\u{672c}"));
    // Excel stores 0.256 as 0.25600000000000001; the user must see 0.256.
    assert_eq!(raw(sheet, "F2"), Some("0.256"));
    assert_eq!(raw(sheet, "F4"), Some("1234567.891"));
    assert_eq!(raw(sheet, "B2"), Some("1200.5"));
    assert_eq!(
        raw(sheet, "G1"),
        Some("007"),
        "text keeps its leading zeros"
    );
    assert_eq!(raw(sheet, "C2"), Some("TRUE"));
    assert_eq!(raw(sheet, "C3"), Some("FALSE"));
    // Excel stores the line break as CRLF; XML reads it as one line feed.
    assert_eq!(raw(sheet, "F3"), Some("line one\nline two"));
    assert_eq!(raw(sheet, "H1"), Some("plain bold"));
}

#[test]
fn excel_formulas_shared_fills_and_cross_sheet_references_calculate() {
    let import = fixture("excel_basic");
    // D2:D4 was filled down, which Excel stores as one shared formula.
    let sheet = &import.sheets[0];
    assert_eq!(raw(sheet, "D2"), Some("=B2*2"));
    assert_eq!(raw(sheet, "D3"), Some("=B3*2"));
    assert_eq!(raw(sheet, "D4"), Some("=B4*2"));
    let values = evaluate_workbook(&import.sheets);
    assert_eq!(values[0][&at("D3")], Value::Number(900.0));
    assert_eq!(values[0][&at("B5")], Value::Number(1650.6));
    assert_eq!(values[0][&at("E3")], Value::Text("yes".to_string()));
    assert!(matches!(values[0][&at("E1")], Value::Error(_)));
    // Cross-sheet references, including a sheet name with `&` and spaces.
    assert_eq!(raw(&import.sheets[1], "B1"), Some("='Data & Q1'!B2*2"));
    assert_eq!(values[1][&at("B1")], Value::Number(2401.0));
    assert_eq!(values[1][&at("B2")], Value::Text("Other".to_string()));
}

#[test]
fn excel_number_formats_and_dates_map_to_loom_formats() {
    let import = fixture("excel_basic");
    let sheet = &import.sheets[0];
    let currency = sheet.cell_style(at("B2"));
    assert_eq!(currency.number_format, NumberFormat::Currency);
    assert_eq!(currency.decimal_places, Some(2));
    assert_eq!(
        sheet.cell_style(at("F2")).number_format,
        NumberFormat::Percentage
    );
    assert_eq!(
        sheet.cell_style(at("F4")).number_format,
        NumberFormat::Scientific
    );
    assert_eq!(
        sheet.cell_style(at("G1")).number_format,
        NumberFormat::PlainText
    );
    // The date is a serial number with a date format; the grid shows the date.
    let date = sheet.cell_style(at("F1"));
    assert_eq!(date.number_format, NumberFormat::DateIso);
    assert_eq!(raw(sheet, "F1"), Some("45292"));
    assert_eq!(date.format_value("45292"), "2024-01-01");
    assert_eq!(
        sheet.cell_style(at("B2")).format_value("1200.5"),
        "$1,200.50"
    );
}

#[test]
fn excel_formulas_loom_cannot_calculate_are_reported() {
    // NA(), TEXT() and the & operator all calculate in Excel; Loom shows
    // errors for them, so the import must say so before replacing a workbook.
    let import = fixture("excel_basic");
    assert!(has(&import, XlsxImportWarning::FormulaResultsDiffer));
}

#[test]
fn excel_hidden_sheet_row_and_column_are_reported() {
    let import = fixture("excel_basic");
    assert_eq!(import.sheets.len(), 3);
    assert_eq!(import.sheets[2].name, "Secret");
    assert!(has(&import, XlsxImportWarning::HiddenContent));
    assert!(has(&import, XlsxImportWarning::RichTextRuns));
}

#[test]
fn excel_cell_formatting_widths_heights_and_freeze_panes_import() {
    let import = fixture("excel_features");
    let sheet = &import.sheets[0];
    let header = sheet.cell_style(at("A1"));
    assert!(header.bold && header.italic && header.underline);
    assert_eq!(header.fill, FillColor::Yellow);
    assert_eq!(header.font_size, Some(21), "16 pt is 21 px");
    assert_eq!(sheet.cell_alignment(at("A1")), CellAlignment::Center);
    assert_eq!(sheet.cell_alignment(at("A2")), CellAlignment::Right);
    assert!(sheet.cell_style(at("A3")).border);
    // Number formats keep their decimals: `0.000`, `#,##0` and `0%`.
    assert_eq!(sheet.cell_style(at("B1")).format_value("5"), "5.000");
    assert_eq!(sheet.cell_style(at("B2")).format_value("5"), "5");
    assert_eq!(sheet.cell_style(at("B3")).format_value("0.5"), "50%");
    // `[$EUR] #,##0.00` is not scientific notation and not dollars.
    let euro = sheet.cell_style(at("B4"));
    assert_eq!(euro.number_format, NumberFormat::Number);
    assert_eq!(euro.decimal_places, Some(2));
    // Column A is 30 characters wide and row 2 is 40 points high in Excel.
    let width = sheet.col_widths[&0];
    assert!(
        (width - 215.0).abs() < 8.0,
        "30 characters is about 215 px, got {width}"
    );
    assert!((sheet.row_heights[&1] - 40.0 / 0.75).abs() < 0.5);
    assert_eq!((sheet.freeze_rows, sheet.freeze_cols), (1, 1));
}

#[test]
fn excel_features_loom_cannot_keep_are_reported() {
    let import = fixture("excel_features");
    assert!(has(&import, XlsxImportWarning::MergedCells));
    assert!(has(&import, XlsxImportWarning::DefinedNames));
    assert!(has(&import, XlsxImportWarning::ConditionalFormatting));
    assert!(has(&import, XlsxImportWarning::DataValidation));
    assert!(has(&import, XlsxImportWarning::ArrayFormulas));
    assert!(has(&import, XlsxImportWarning::NotesAndLinks));
    assert!(has(&import, XlsxImportWarning::TablesAndFilters));
    // The euro symbol of `[$EUR] #,##0.00` cannot be shown.
    assert!(has(&import, XlsxImportWarning::UnsupportedNumberFormats));
    // Frozen panes and sizes are imported, so they are no longer "lost".
    let labels: Vec<_> = import.warnings.iter().map(|w| w.label()).collect();
    assert!(!labels.iter().any(|label| label.contains("frozen")));
    assert!(!labels.iter().any(|label| label.contains("row heights")));
}

#[test]
fn excel_defined_names_that_point_at_cells_keep_calculating() {
    let import = fixture("excel_features");
    let sheet = &import.sheets[0];
    assert_eq!(raw(sheet, "G1"), Some("=Styled!$B$1*2"));
    let values = evaluate_workbook(&import.sheets);
    assert_eq!(values[0][&at("G1")], Value::Number(10.0));
}

#[test]
fn excel_chart_picture_and_shape_import_with_their_real_sizes() {
    let import = fixture("excel_visuals");
    assert!(import.warnings.is_empty(), "{:?}", import.warnings);
    let sheet = &import.sheets[0];
    let chart = sheet.chart.as_ref().expect("chart imports");
    assert_eq!((chart.cat_col, chart.val_col), (0, 1));
    assert_eq!((chart.start_row, chart.end_row), (1, Some(3)));
    assert_eq!(chart.title, "Spend");
    let image = sheet
        .objects
        .iter()
        .find(|object| object.kind == SheetObjectKind::Image)
        .expect("picture imports");
    assert!(image
        .embedded
        .as_ref()
        .is_some_and(|bytes| bytes.starts_with(b"\x89PNG")));
    // The picture is 40 pt square (about 53 px), not the 240x112 default.
    assert_eq!((image.width, image.height), (53, 53));
    let shape = sheet
        .objects
        .iter()
        .find(|object| object.kind == SheetObjectKind::Shape)
        .expect("shape imports");
    assert_eq!(shape.label, "Note box");
    assert_eq!((shape.width, shape.height), (160, 53));
}

#[test]
fn excel_newer_functions_dynamic_arrays_and_quoted_sheet_names() {
    let import = fixture("excel_edge");
    let sheet = &import.sheets[0];
    assert_eq!(sheet.name, "It's 100%");
    // Excel writes `_xlfn.CONCAT(`; Loom knows the plain function name.
    assert_eq!(raw(sheet, "B1"), Some("=CONCAT(A1,A2)"));
    assert_eq!(raw(sheet, "B2"), Some("=TEXTJOIN(\"-\",TRUE,A1:A2)"));
    assert_eq!(raw(&import.sheets[1], "A1"), Some("='It''s 100%'!C3+1"));
    // The spilled results Excel stored next to =SEQUENCE are not imported:
    // they would block Loom's own spill.
    assert_eq!(raw(sheet, "D1"), Some("=SEQUENCE(2,2)"));
    assert_eq!(raw(sheet, "E1"), None);
    assert_eq!(raw(sheet, "E2"), None);
    let values = evaluate_workbook(&import.sheets);
    assert_eq!(values[0][&at("B1")], Value::Text("xy".to_string()));
    assert_eq!(values[0][&at("B2")], Value::Text("x-y".to_string()));
    assert_eq!(values[0][&at("B3")], Value::Text("none".to_string()));
    assert_eq!(
        values[0][&at("D1")],
        Value::Array(
            vec![
                Value::Number(1.0),
                Value::Number(2.0),
                Value::Number(3.0),
                Value::Number(4.0)
            ],
            2,
            2
        ),
        "the formula spills into D1:E2 by itself"
    );
    assert_eq!(values[1][&at("A1")], Value::Number(10.0));
    assert!(!has(&import, XlsxImportWarning::ArrayFormulas));
}

#[test]
fn excel_unrepresentable_values_and_formats_are_reported() {
    let import = fixture("excel_edge");
    // STDEV.S is not a Loom function; the text "=not a formula" would be read
    // as a formula; a time of day cannot be shown.
    assert!(has(&import, XlsxImportWarning::FormulaResultsDiffer));
    assert!(has(&import, XlsxImportWarning::TextReadAsValue));
    assert!(has(&import, XlsxImportWarning::UnsupportedNumberFormats));
    let sheet = &import.sheets[0];
    assert_eq!(
        sheet.cell_style(at("F2")).number_format,
        NumberFormat::DateIso
    );
    assert_eq!(
        sheet.cell_style(at("F2")).format_value("45292.75"),
        "2024-01-01"
    );
}

#[test]
fn excel_1904_date_system_is_reported() {
    let import = fixture("excel_1904");
    assert!(has(&import, XlsxImportWarning::Date1904System));
    assert_eq!(raw(&import.sheets[0], "A1"), Some("100"));
}

#[test]
fn a_workbook_exported_by_loom_imports_without_any_warning() {
    use crate::style::CellStyle;
    let mut first = Sheet::new("First");
    first.set_str("A1", "10");
    first.set_str("B1", "=A1*2");
    first.set_str("C1", "text with é and \"quotes\"");
    first.set_cell_style(
        at("A1"),
        CellStyle {
            bold: true,
            fill: FillColor::Green,
            number_format: NumberFormat::Currency,
            decimal_places: Some(2),
            ..CellStyle::default()
        },
    );
    first.set_col_width(0, 140.0);
    first.set_row_height(2, 36.0);
    first.freeze_panes(1, 0);
    let mut second = Sheet::new("Second");
    second.set_str("A1", "=First!B1+5");
    let bytes = super::export_xlsx_sheets(&[first.clone(), second]).unwrap();
    let import = import_xlsx_sheets(&bytes).unwrap();
    assert!(import.warnings.is_empty(), "{:?}", import.warnings);
    let back = &import.sheets[0];
    assert_eq!(raw(back, "B1"), Some("=A1*2"));
    assert_eq!(raw(back, "C1"), first.raw(at("C1")));
    assert_eq!(back.cell_style(at("A1")), first.cell_style(at("A1")));
    assert_eq!((back.freeze_rows, back.freeze_cols), (1, 0));
    assert!((back.col_widths[&0] - 140.0).abs() < 1.0);
    assert!((back.row_heights[&2] - 36.0).abs() < 1.0);
    assert_eq!(raw(&import.sheets[1], "A1"), Some("=First!B1+5"));
}
