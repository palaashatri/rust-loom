//! Rich XLSX interop for the worksheet model.

mod cell_refs;
mod export;
mod import;
mod package_parts;
mod styles;
mod warnings;
mod xml;

pub use export::export_xlsx_sheets;
pub use import::{extract_xlsx_sheets, import_xlsx_sheets, XlsxImport};
pub use warnings::XlsxImportWarning;

const MAIN_NS: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PACKAGE_REL_NS: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const DRAWING_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing";
const CHART_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/chart";
const DRAWINGML_NS: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const EMU_PER_PIXEL: f32 = 9_525.0;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::{CellAlignment, FillColor};
    use crate::{CellRef, ChartKind, Sheet, SheetChart, SheetObject};
    use loom_package::zip::PackageArchive;
    use std::collections::BTreeMap;

    #[test]
    fn rich_xlsx_roundtrip_preserves_styles_charts_and_embedded_images() {
        let mut sheet = Sheet::new("Sales");
        sheet.set_str("A1", "Quarter");
        sheet.set_str("B1", "Revenue");
        sheet.set_str("A2", "Q1");
        sheet.set_str("B2", "120");
        sheet.set_str("A3", "Q2");
        sheet.set_str("B3", "180");
        sheet.set_cell_alignment(CellRef { row: 0, col: 1 }, CellAlignment::Right);
        let mut style = sheet.cell_style(CellRef { row: 0, col: 1 });
        style.bold = true;
        style.fill = FillColor::Yellow;
        sheet.set_cell_style(CellRef { row: 0, col: 1 }, style);
        sheet.chart = Some(SheetChart {
            kind: ChartKind::Bar,
            title: "Revenue".to_string(),
            cat_col: 0,
            val_col: 1,
            ..Default::default()
        });
        let mut image =
            SheetObject::image(CellRef { row: 4, col: 0 }, "hero.png").expect("image object");
        image.embedded = Some(vec![137, 80, 78, 71]);
        sheet.objects.push(image);

        let bytes = export_xlsx_sheets(&[sheet]).expect("rich export");
        let archive = PackageArchive::from_bytes(&bytes).expect("zip");
        assert!(archive.get("xl/styles.xml").is_some());
        assert!(archive.get("xl/charts/chart1.xml").is_some());
        assert!(archive.get("xl/media/sheet1-object0.png").is_some());

        let sheets = extract_xlsx_sheets(&bytes).expect("rich import");
        assert_eq!(sheets[0].cell_style(CellRef { row: 0, col: 1 }), style);
        assert_eq!(
            sheets[0].cell_alignment(CellRef { row: 0, col: 1 }),
            CellAlignment::Right
        );
        assert_eq!(
            sheets[0].chart.as_ref().map(|chart| chart.kind),
            Some(ChartKind::Bar)
        );
        assert_eq!(
            sheets[0].objects[0].embedded.as_deref(),
            Some(&[137, 80, 78, 71][..])
        );
    }

    #[test]
    fn rich_xlsx_roundtrip_keeps_drawing_relationships_per_sheet() {
        let mut first = Sheet::new("First");
        first.set_str("A1", "one");
        first.chart = Some(SheetChart {
            kind: ChartKind::Line,
            title: "First chart".to_string(),
            cat_col: 0,
            val_col: 1,
            ..Default::default()
        });
        let mut second = Sheet::new("Second");
        second.set_str("A1", "two");
        second.objects.push(SheetObject::shape(
            CellRef { row: 1, col: 1 },
            "Second shape",
        ));

        let bytes = export_xlsx_sheets(&[first, second]).expect("rich export");
        let archive = PackageArchive::from_bytes(&bytes).expect("zip");
        assert!(archive.get("xl/drawings/drawing1.xml").is_some());
        assert!(archive.get("xl/drawings/drawing2.xml").is_some());
        assert!(archive.get("xl/charts/chart1.xml").is_some());
        assert!(archive.get("xl/worksheets/_rels/sheet2.xml.rels").is_some());

        let sheets = extract_xlsx_sheets(&bytes).expect("rich import");
        assert_eq!(sheets.len(), 2);
        assert_eq!(
            sheets[0].chart.as_ref().map(|chart| chart.title.as_str()),
            Some("First chart")
        );
        assert_eq!(sheets[1].objects[0].label, "Second shape");
    }

    #[test]
    fn rich_xlsx_escapes_chart_sheet_references_and_binds_drawing_namespace() {
        let mut sheet = Sheet::new("R&D");
        sheet.set_str("A2", "North");
        sheet.set_str("B2", "10");
        sheet.chart = Some(SheetChart {
            kind: ChartKind::Line,
            title: "Revenue <Q1>".to_string(),
            cat_col: 0,
            val_col: 1,
            ..Default::default()
        });

        let bytes = export_xlsx_sheets(&[sheet]).expect("rich export");
        let archive = PackageArchive::from_bytes(&bytes).expect("zip");
        let worksheet = String::from_utf8_lossy(archive.get("xl/worksheets/sheet1.xml").unwrap());
        assert!(worksheet.contains("<drawing xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" r:id=\"rId1\"/>")
        );

        let chart = String::from_utf8_lossy(archive.get("xl/charts/chart1.xml").unwrap());
        assert!(chart.contains("'R&amp;D'!$A$2:$A$2"));
        assert!(chart.contains("Revenue &lt;Q1&gt;"));
        assert!(!chart.contains("'R&D'!$A$2:$A$2"));

        let imported = extract_xlsx_sheets(&bytes).expect("rich import");
        assert_eq!(imported[0].name, "R&D");
        assert_eq!(
            imported[0].chart.as_ref().map(|chart| chart.cat_col),
            Some(0)
        );
    }
    #[test]
    fn chart_explicit_range_roundtrips_without_total_row() {
        let mut sheet = Sheet::new("Budget");
        for (cell, value) in [
            ("A1", "Item"),
            ("B1", "USD"),
            ("A2", "Rent"),
            ("B2", "1200"),
            ("A3", "Food"),
            ("B3", "450"),
            ("A4", "Travel"),
            ("B4", "150"),
            ("A5", "Total"),
            ("B5", "1800"),
        ] {
            sheet.set_str(cell, value);
        }
        let chart = SheetChart {
            title: "Expenses".into(),
            start_row: 1,
            end_row: Some(3),
            ..Default::default()
        };
        sheet.chart = Some(chart.clone());
        let bytes = export_xlsx_sheets(&[sheet]).unwrap();
        let archive = PackageArchive::from_bytes(&bytes).unwrap();
        let xml = String::from_utf8_lossy(archive.get("xl/charts/chart1.xml").unwrap());
        assert!(xml.contains("$A$2:$A$4"));
        assert!(xml.contains("$B$2:$B$4"));
        assert!(!xml.contains("$B$5"));
        let restored = extract_xlsx_sheets(&bytes).unwrap();
        assert_eq!(restored[0].chart, Some(chart));
    }

    #[test]
    fn xlsx_import_reports_workbook_defined_names() {
        let bytes = test_xlsx_with_xml(
            "xl/workbook.xml",
            "</workbook>",
            "<definedNames><definedName name=\"BudgetTotal\">'Budget'!$B$2</definedName></definedNames>",
        );

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported.warnings.contains(&XlsxImportWarning::DefinedNames));
    }

    #[test]
    fn xlsx_import_does_not_warn_for_empty_defined_names_container() {
        let bytes = test_xlsx_with_xml("xl/workbook.xml", "</workbook>", "<definedNames/>");

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(
            !imported.warnings.contains(&XlsxImportWarning::DefinedNames),
            "an empty container has no named range to lose"
        );
    }

    #[test]
    fn xlsx_import_reports_external_link_relationships() {
        let bytes = test_xlsx_with_parts(
            &[(
                "xl/_rels/workbook.xml.rels",
                test_xml_with_snippet(
                    "xl/_rels/workbook.xml.rels",
                    "</Relationships>",
                    "<Relationship Id=\"rIdExternal\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/externalLink\" Target=\"externalLinks/externalLink1.xml\"/>",
                ),
            )],
            &[],
            &[("xl/externalLinks/externalLink1.xml", "<externalLink/>")],
            None,
        );

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported
            .warnings
            .contains(&XlsxImportWarning::ExternalLinks));
    }

    #[test]
    fn xlsx_import_reports_pivot_table_parts() {
        let bytes = test_xlsx_with_parts(
            &[],
            &[],
            &[("xl/pivotTables/pivotTable1.xml", "<pivotTableDefinition/>")],
            None,
        );

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported.warnings.contains(&XlsxImportWarning::PivotTables));
    }

    #[test]
    fn xlsx_import_reports_namespaced_conditional_formatting() {
        let bytes = test_xlsx_with_xml(
            "xl/worksheets/sheet1.xml",
            "</worksheet>",
            "<x14:conditionalFormatting xmlns:x14=\"http://schemas.microsoft.com/office/spreadsheetml/2009/9/main\"/> ",
        );

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported
            .warnings
            .contains(&XlsxImportWarning::ConditionalFormatting));
    }

    #[test]
    fn xlsx_import_reports_data_validation_rules() {
        let bytes = test_xlsx_with_xml(
            "xl/worksheets/sheet1.xml",
            "</worksheet>",
            "<dataValidations count=\"1\"><dataValidation type=\"whole\"/></dataValidations>",
        );

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported
            .warnings
            .contains(&XlsxImportWarning::DataValidation));
    }

    #[test]
    fn xlsx_import_reports_frozen_panes() {
        let bytes = test_xlsx_with_xml(
            "xl/worksheets/sheet1.xml",
            "</worksheet>",
            "<sheetViews><sheetView workbookViewId=\"0\"><pane ySplit=\"1\" topLeftCell=\"A2\" state=\"frozen\"/></sheetView></sheetViews>",
        );

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported.warnings.contains(&XlsxImportWarning::FrozenPanes));
    }

    #[test]
    fn xlsx_import_reports_custom_row_and_column_sizes() {
        let bytes = test_xlsx_with_xml(
            "xl/worksheets/sheet1.xml",
            "</worksheet>",
            "<cols><col min=\"1\" max=\"1\" width=\"22\" customWidth=\"1\"/></cols><sheetData><row r=\"1\" ht=\"26\" customHeight=\"1\"/></sheetData>",
        );

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported
            .warnings
            .contains(&XlsxImportWarning::CustomRowColumnSizes));
    }

    #[test]
    fn xlsx_import_reports_extra_charts_on_one_sheet() {
        let mut sheet = Sheet::new("Budget");
        sheet.set_str("A1", "Category");
        sheet.set_str("B1", "Amount");
        sheet.set_str("A2", "Rent");
        sheet.set_str("B2", "1200");
        sheet.chart = Some(SheetChart::default());
        let base = export_xlsx_sheets(&[sheet]).expect("export workbook");
        let chart = String::from_utf8(
            PackageArchive::from_bytes(&base)
                .expect("read base archive")
                .get("xl/charts/chart1.xml")
                .expect("exported chart")
                .to_vec(),
        )
        .expect("chart xml is utf8");
        let drawing = "<xdr:oneCellAnchor><xdr:from><xdr:col>4</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:ext cx=\"100\" cy=\"100\"/><xdr:graphicFrame><a:graphic><a:graphicData><c:chart r:id=\"rId2\"/></a:graphicData></a:graphic></xdr:graphicFrame><xdr:clientData/></xdr:oneCellAnchor>";
        let bytes = test_xlsx_with_parts(
            &[
                (
                    "xl/drawings/drawing1.xml",
                    test_xml_with_snippet_from(
                        &base,
                        "xl/drawings/drawing1.xml",
                        "</xdr:wsDr>",
                        drawing,
                    ),
                ),
                (
                    "xl/drawings/_rels/drawing1.xml.rels",
                    test_xml_with_snippet_from(
                        &base,
                        "xl/drawings/_rels/drawing1.xml.rels",
                        "</Relationships>",
                        "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart\" Target=\"../charts/chart2.xml\"/>",
                    ),
                ),
            ],
            &[],
            &[("xl/charts/chart2.xml", &chart)],
            Some(&base),
        );

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported
            .warnings
            .contains(&XlsxImportWarning::MultipleChartsOnSheet));
    }

    #[test]
    fn xlsx_import_reports_drawing_relationships_that_point_to_missing_parts() {
        let mut sheet = Sheet::new("Budget");
        sheet.chart = Some(SheetChart::default());
        let base = export_xlsx_sheets(&[sheet]).expect("export workbook");
        let bytes = test_xlsx_with_parts(&[], &["xl/drawings/drawing1.xml"], &[], Some(&base));

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported
            .warnings
            .contains(&XlsxImportWarning::MissingDrawingParts));
    }

    #[test]
    fn xlsx_import_reports_images_with_missing_media_parts() {
        let mut sheet = Sheet::new("Budget");
        let mut image =
            SheetObject::image(CellRef { row: 0, col: 1 }, "chart.png").expect("create image");
        image.embedded = Some(vec![137, 80, 78, 71]);
        sheet.objects.push(image);
        let base = export_xlsx_sheets(&[sheet]).expect("export workbook");
        let bytes = test_xlsx_with_parts(&[], &["xl/media/sheet1-object0.png"], &[], Some(&base));

        let imported = import_xlsx_sheets(&bytes).expect("import workbook");
        assert!(imported
            .warnings
            .contains(&XlsxImportWarning::MissingDrawingParts));
    }

    #[test]
    fn xlsx_import_ignores_feature_words_in_cell_text_and_plain_workbooks() {
        let mut sheet = Sheet::new("Budget");
        sheet.set_str(
            "A1",
            "definedNames externalLinks conditionalFormatting dataValidation",
        );
        let base = export_xlsx_sheets(&[sheet]).expect("export supported workbook");
        let workbook = test_xml_with_snippet_from(
            &base,
            "xl/workbook.xml",
            "</workbook>",
            "<!-- <definedNames><definedName name=\"comment only\"/></definedNames> -->",
        );
        let bytes = test_xlsx_with_parts(&[("xl/workbook.xml", workbook)], &[], &[], Some(&base));

        let imported = import_xlsx_sheets(&bytes).expect("import supported workbook");
        assert!(imported.warnings.is_empty());
        assert_eq!(
            imported.sheets[0].raw(CellRef { row: 0, col: 0 }),
            Some("definedNames externalLinks conditionalFormatting dataValidation")
        );
    }

    fn test_xlsx_with_xml(path: &str, close_tag: &str, snippet: &str) -> Vec<u8> {
        let snippet = test_xml_with_snippet(path, close_tag, snippet);
        test_xlsx_with_parts(&[(path, snippet)], &[], &[], None)
    }

    fn test_xml_with_snippet(path: &str, close_tag: &str, snippet: &str) -> String {
        let base = export_xlsx_sheets(&[Sheet::new("Budget")]).expect("export base workbook");
        test_xml_with_snippet_from(&base, path, close_tag, snippet)
    }

    fn test_xml_with_snippet_from(
        base: &[u8],
        path: &str,
        close_tag: &str,
        snippet: &str,
    ) -> String {
        let archive = PackageArchive::from_bytes(base).expect("read base workbook");
        let xml = String::from_utf8(archive.get(path).expect("base XML part").to_vec())
            .expect("base XML part is UTF-8");
        assert!(xml.contains(close_tag), "{path} should contain {close_tag}");
        xml.replace(close_tag, &format!("{snippet}{close_tag}"))
    }

    fn test_xlsx_with_parts(
        replacements: &[(&str, String)],
        removals: &[&str],
        additions: &[(&str, &str)],
        base: Option<&[u8]>,
    ) -> Vec<u8> {
        let base = base.map(ToOwned::to_owned).unwrap_or_else(|| {
            export_xlsx_sheets(&[Sheet::new("Budget")]).expect("export base workbook")
        });
        let original = PackageArchive::from_bytes(&base).expect("read base workbook");
        let removed = removals
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        let replacements = replacements.iter().cloned().collect::<BTreeMap<_, _>>();
        let mut archive = PackageArchive::new();
        for path in original.paths() {
            if removed.contains(path) {
                continue;
            }
            let bytes = replacements.get(path).map_or_else(
                || original.get(path).unwrap_or_default().to_vec(),
                |xml| xml.as_bytes().to_vec(),
            );
            archive.add(path, bytes).expect("copy workbook part");
        }
        for (path, xml) in additions {
            archive
                .add(path, xml.as_bytes().to_vec())
                .expect("add workbook part");
        }
        archive.to_bytes().expect("write workbook archive")
    }
}
