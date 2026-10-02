//! XML-level regressions for the defects found by opening the export in
//! Microsoft Excel. Each test names the symptom Excel showed; none of them
//! needs Excel to run.

use loom_package::zip::PackageArchive;

use crate::style::FillColor;
use crate::{CellRef, ChartKind, NumberFormat, Sheet, SheetChart, SheetObject};

use super::{package_problems, walk};
use crate::xlsx::export_xlsx_sheets;

fn export(sheets: &[Sheet]) -> Vec<u8> {
    let bytes = export_xlsx_sheets(sheets).expect("export");
    let problems = package_problems(&bytes);
    assert!(
        problems.is_empty(),
        "package problems:\n{}",
        problems.join("\n")
    );
    bytes
}

fn part(bytes: &[u8], path: &str) -> String {
    let archive = PackageArchive::from_bytes(bytes).expect("zip");
    String::from_utf8(
        archive
            .get(path)
            .unwrap_or_else(|| panic!("missing {path}"))
            .to_vec(),
    )
    .expect("utf-8 part")
}

fn cell(a1: &str) -> CellRef {
    CellRef::parse(a1).expect("reference")
}

#[test]
fn text_that_starts_with_a_hash_is_text_not_an_error() {
    // Excel refused the whole package: `#1 seller` was typed as an error.
    let mut sheet = Sheet::new("Tags");
    sheet.set_str("A1", "#1 seller");
    sheet.set_str("A2", "#NOTANERROR");
    let sheet_xml = part(&export(&[sheet]), "xl/worksheets/sheet1.xml");
    assert!(!sheet_xml.contains("t=\"e\""), "{sheet_xml}");
    assert!(sheet_xml.contains("<c r=\"A1\" t=\"s\">"));
}

#[test]
fn formula_text_results_are_typed_str_with_the_text_inline() {
    let mut sheet = Sheet::new("Text");
    sheet.set_str("A1", "ab");
    sheet.set_str("B1", "=UPPER(A1)");
    let sheet_xml = part(&export(&[sheet]), "xl/worksheets/sheet1.xml");
    assert!(
        sheet_xml.contains("<c r=\"B1\" t=\"str\"><f>UPPER(A1)</f><v>AB</v></c>"),
        "{sheet_xml}"
    );
}

#[test]
fn unparseable_formulas_stay_text_and_no_invalid_error_literal_is_written() {
    // `#PARSE!` is not an Excel error; a formula Excel cannot parse makes it
    // repair the file. The text is kept instead.
    let mut sheet = Sheet::new("Bad");
    sheet.set_str("A1", "=SUM(");
    let bytes = export(&[sheet]);
    let sheet_xml = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(!sheet_xml.contains("PARSE"));
    assert!(!sheet_xml.contains("<f>"));
    assert!(part(&bytes, "xl/sharedStrings.xml").contains("=SUM("));
}

#[test]
fn rows_are_unique_and_ascending_with_styled_empty_cells() {
    // Styled cells without a value were appended as duplicate, out-of-order
    // rows, which Excel repairs by dropping content.
    let mut sheet = Sheet::new("Rows");
    sheet.set_str("A1", "x");
    sheet.set_str("A5", "y");
    for (a1, fill) in [
        ("C1", FillColor::Orange),
        ("C3", FillColor::Blue),
        ("C9", FillColor::Red),
    ] {
        let mut style = sheet.cell_style(cell(a1));
        style.fill = fill;
        sheet.set_cell_style(cell(a1), style);
    }
    let sheet_xml = part(&export(&[sheet]), "xl/worksheets/sheet1.xml");
    let rows: Vec<u32> = walk("sheet1", &sheet_xml)
        .expect("well-formed")
        .iter()
        .filter(|node| node.name == "row")
        .map(|node| node.attrs["r"].parse().unwrap())
        .collect();
    assert_eq!(rows, vec![1, 3, 5, 9]);
}

#[test]
fn solid_fills_never_take_the_reserved_second_fill_slot() {
    // The second fill is reserved for gray125: a solid fill stored there is
    // replaced by the hatch, so every filled cell lost its colour.
    let mut sheet = Sheet::new("Fills");
    sheet.set_str("A1", "x");
    let mut style = sheet.cell_style(cell("A1"));
    style.fill = FillColor::Gray;
    sheet.set_cell_style(cell("A1"), style);
    let styles = part(&export(&[sheet]), "xl/styles.xml");
    let fills = styles
        .split("<fills")
        .nth(1)
        .unwrap()
        .split("</fills>")
        .next()
        .unwrap();
    let patterns: Vec<&str> = fills
        .split("patternType=\"")
        .skip(1)
        .map(|rest| rest.split('"').next().unwrap())
        .collect();
    assert_eq!(patterns, vec!["none", "gray125", "solid"]);
    assert!(styles.contains("fillId=\"2\""));
    assert!(
        !styles.contains("<numFmts"),
        "an empty numFmts element is invalid"
    );
}

#[test]
fn sizes_and_frozen_panes_reach_the_worksheet() {
    // None of these were exported at all.
    let mut sheet = Sheet::new("Layout");
    sheet.set_str("A1", "x");
    sheet.set_col_width(1, 140.0);
    sheet.set_row_height(2, 48.0);
    sheet.freeze_panes(2, 1);
    let sheet_xml = part(&export(&[sheet]), "xl/worksheets/sheet1.xml");
    assert!(
        sheet_xml.contains("<col min=\"2\" max=\"2\" width=\"20\" customWidth=\"1\"/>"),
        "{sheet_xml}"
    );
    assert!(
        sheet_xml.contains("<row r=\"3\" ht=\"36\" customHeight=\"1\"/>"),
        "{sheet_xml}"
    );
    assert!(sheet_xml.contains(
        "<pane xSplit=\"1\" ySplit=\"2\" topLeftCell=\"B3\" activePane=\"bottomRight\" state=\"frozen\"/>"
    ));
    // Loom's default cell size is kept so unsized cells look the same.
    assert!(sheet_xml.contains("defaultRowHeight=\"18\""));
}

#[test]
fn only_one_frozen_axis_uses_the_matching_pane() {
    let mut rows = Sheet::new("Rows");
    rows.set_str("A1", "x");
    rows.freeze_panes(1, 0);
    let mut cols = Sheet::new("Cols");
    cols.set_str("A1", "x");
    cols.freeze_panes(0, 2);
    let bytes = export(&[rows, cols]);
    assert!(part(&bytes, "xl/worksheets/sheet1.xml").contains(
        "<pane ySplit=\"1\" topLeftCell=\"A2\" activePane=\"bottomLeft\" state=\"frozen\"/>"
    ));
    assert!(part(&bytes, "xl/worksheets/sheet2.xml").contains(
        "<pane xSplit=\"2\" topLeftCell=\"C1\" activePane=\"topRight\" state=\"frozen\"/>"
    ));
}

#[test]
fn spilled_arrays_are_array_formulas_with_every_cached_cell() {
    // Without the array flag Excel showed a single implicitly-intersected
    // value and left the neighbours as unrelated constants.
    let mut sheet = Sheet::new("Arrays");
    sheet.set_str("B1", "=SEQUENCE(2,3)");
    let sheet_xml = part(&export(&[sheet]), "xl/worksheets/sheet1.xml");
    assert!(
        sheet_xml.contains("<f t=\"array\" ref=\"B1:D2\">_xlfn.SEQUENCE(2,3)</f><v>1</v>"),
        "{sheet_xml}"
    );
    for (reference, value) in [("C1", 2), ("D1", 3), ("B2", 4), ("C2", 5), ("D2", 6)] {
        assert!(
            sheet_xml.contains(&format!("<c r=\"{reference}\"><v>{value}</v></c>")),
            "{reference} in {sheet_xml}"
        );
    }
}

#[test]
fn functions_added_after_excel_2007_carry_their_prefix() {
    // Unprefixed, Excel evaluates these as unknown names.
    let mut sheet = Sheet::new("Modern");
    sheet.set_str("A1", "b");
    sheet.set_str("A2", "a");
    sheet.set_str("B1", "=TEXTJOIN(\",\",TRUE,A1:A2)");
    sheet.set_str("B2", "=MAXIFS(A1:A2,A1:A2,\"a\")");
    sheet.set_str("B3", "=SORT(A1:A2)");
    sheet.set_str("B6", "=SUM(1,2)");
    let sheet_xml = part(&export(&[sheet]), "xl/worksheets/sheet1.xml");
    assert!(
        sheet_xml.contains("<f>_xlfn.TEXTJOIN(\",\",TRUE,A1:A2)</f>"),
        "{sheet_xml}"
    );
    assert!(sheet_xml.contains("_xlfn.MAXIFS("));
    assert!(sheet_xml.contains("_xlfn._xlws.SORT(A1:A2)"));
    assert!(sheet_xml.contains("<f>SUM(1,2)</f>"));
}

#[test]
fn zero_decimal_formats_have_no_trailing_decimal_point() {
    // `0.%` rendered as `50.%` in Excel.
    let mut sheet = Sheet::new("Formats");
    sheet.set_str("A1", "0.5");
    let mut style = sheet.cell_style(cell("A1"));
    style.number_format = NumberFormat::Percentage;
    style.decimal_places = Some(0);
    sheet.set_cell_style(cell("A1"), style);
    sheet.set_str("A2", "1234");
    let mut style = sheet.cell_style(cell("A2"));
    style.number_format = NumberFormat::Currency;
    style.decimal_places = Some(0);
    sheet.set_cell_style(cell("A2"), style);
    let styles = part(&export(&[sheet]), "xl/styles.xml");
    assert!(styles.contains("formatCode=\"0%\""), "{styles}");
    assert!(styles.contains("formatCode=\"$#,##0\""), "{styles}");
    assert!(!styles.contains(".\""));
    assert!(!styles.contains(".%"));
}

#[test]
fn a_chart_is_placed_beside_the_data_not_over_it() {
    let mut sheet = Sheet::new("Chart");
    for (a1, raw) in [
        ("A1", "k"),
        ("B1", "v"),
        ("C1", "w"),
        ("D1", "x"),
        ("E1", "y"),
        ("A2", "a"),
        ("B2", "1"),
    ] {
        sheet.set_str(a1, raw);
    }
    sheet.chart = Some(SheetChart {
        kind: ChartKind::Bar,
        title: "T".to_string(),
        cat_col: 0,
        val_col: 1,
        start_row: 1,
        end_row: Some(1),
    });
    let drawing = part(&export(&[sheet]), "xl/drawings/drawing1.xml");
    // Data reaches column E (index 4); the chart starts at column G (6).
    assert!(
        drawing.contains("<xdr:from><xdr:col>6</xdr:col>"),
        "{drawing}"
    );
}

#[test]
fn a_shape_is_a_rectangle_with_readable_centred_text() {
    // With no geometry Excel reported "not a primitive" and drew nothing.
    let mut sheet = Sheet::new("Shape");
    sheet.set_str("A1", "x");
    let mut shape = SheetObject::shape(cell("C3"), "Target <&>");
    shape.fill = FillColor::Yellow;
    sheet.objects.push(shape);
    let drawing = part(&export(&[sheet]), "xl/drawings/drawing1.xml");
    assert!(drawing.contains("<a:prstGeom prst=\"rect\">"), "{drawing}");
    assert!(drawing.contains("<a:t>Target &lt;&amp;&gt;</a:t>"));
    assert!(drawing.contains("algn=\"ctr\""));
}

#[test]
fn control_characters_and_lookalike_escapes_survive_in_text() {
    let mut sheet = Sheet::new("Text");
    sheet.set_str("A1", "a\u{1}b\r\nc");
    sheet.set_str("A2", "_x0041_ literal");
    let bytes = export(&[sheet]);
    let strings = part(&bytes, "xl/sharedStrings.xml");
    assert!(strings.contains("a_x0001_b_x000D_\nc"), "{strings}");
    assert!(strings.contains("_x005F_x0041_ literal"), "{strings}");
    assert!(walk("sharedStrings", &strings).is_ok());
}

#[test]
fn shared_string_counts_match_the_cells_that_use_them() {
    let mut sheet = Sheet::new("Counts");
    sheet.set_str("A1", "same");
    sheet.set_str("A2", "same");
    sheet.set_str("A3", "other");
    let strings = part(&export(&[sheet]), "xl/sharedStrings.xml");
    assert!(
        strings.contains("count=\"3\" uniqueCount=\"2\""),
        "{strings}"
    );
}

#[test]
fn valid_excel_formulas_loom_cannot_evaluate_stay_formulas() {
    // Excel showed `=A1&" y"` as literal text when the exporter guessed from
    // Loom's calculation that the formula was unparseable.
    let mut sheet = Sheet::new("Data");
    sheet.set_str("A1", "=SUM(");
    sheet.set_str("B1", "=A1&\" y\"");
    sheet.set_str("B2", "3");
    sheet.set_str("B3", "=STDEV.S(B1:B2)");
    sheet.set_str("D1", "=A1+1");
    let bytes = export(&[sheet]);
    let ws = part(&bytes, "xl/worksheets/sheet1.xml");
    assert!(ws.contains("<f>A1&amp;\" y\"</f>"), "{ws}");
    assert!(ws.contains("<f>_xlfn.STDEV.S(B1:B2)</f>"), "{ws}");
    assert!(ws.contains("<f>A1+1</f>"), "{ws}");
    // Genuinely broken syntax is still kept as text.
    assert!(!ws.contains("<f>SUM(</f>"), "{ws}");
}
