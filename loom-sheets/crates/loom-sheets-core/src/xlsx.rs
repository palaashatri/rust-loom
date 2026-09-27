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
pub use warnings::{XlsxChartType, XlsxImportWarning};

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
    fn xlsx_export_links_the_styles_part_from_the_workbook() {
        let mut sheet = Sheet::new("Styled");
        sheet.set_str("A1", "total");
        let mut style = sheet.cell_style(CellRef { row: 0, col: 0 });
        style.bold = true;
        sheet.set_cell_style(CellRef { row: 0, col: 0 }, style);

        let bytes = export_xlsx_sheets(&[sheet]).expect("styled export");
        let archive = PackageArchive::from_bytes(&bytes).expect("zip");
        let relationships = String::from_utf8_lossy(
            archive
                .get("xl/_rels/workbook.xml.rels")
                .expect("workbook relationships"),
        );

        assert!(relationships.contains(
            "Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\""
        ));
        assert!(relationships.contains("Target=\"styles.xml\""));
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
    fn xlsx_import_reports_single_unsupported_chart_types_before_dropping_them() {
        for (expected_label, chart_tag) in [
            ("unsupported area charts", "areaChart"),
            ("unsupported doughnut charts", "doughnutChart"),
        ] {
            let bytes = xlsx_with_single_chart_type(chart_tag);
            let imported = import_xlsx_sheets(&bytes).expect("import workbook");

            assert!(imported.sheets[0].chart.is_none());
            assert!(
                imported
                    .warnings
                    .iter()
                    .any(|warning| warning.label() == expected_label),
                "the warning must name the dropped chart as {expected_label}: {:?}",
                imported.warnings
            );
        }
    }

    #[test]
    fn xlsx_import_reports_plot_groups_dropped_from_a_combined_chart() {
        let base = xlsx_with_combined_chart_groups(ChartKind::Bar, ChartKind::Line);
        let archive = PackageArchive::from_bytes(&base).expect("read combined chart package");
        let xml = String::from_utf8(
            archive
                .get("xl/charts/chart1.xml")
                .expect("chart part")
                .to_vec(),
        )
        .expect("chart XML is UTF-8");
        let line_start = xml.find("<c:lineChart>").expect("line plot starts");
        let line_body = &xml[line_start..];
        let series_start = line_start + line_body.find("<c:ser>").expect("line series starts");
        let series_end = series_start
            + xml[series_start..]
                .find("</c:ser>")
                .map(|offset| offset + "</c:ser>".len())
                .expect("line series ends");
        let second_series = xml[series_start..series_end]
            .replace("<c:idx val=\"1\"/>", "<c:idx val=\"2\"/>")
            .replace("<c:order val=\"1\"/>", "<c:order val=\"2\"/>");
        let xml = xml.replacen(
            "</c:lineChart>",
            &format!("{second_series}</c:lineChart>"),
            1,
        );
        let bytes = test_xlsx_with_parts(&[("xl/charts/chart1.xml", xml)], &[], &[], Some(&base));
        let imported = import_xlsx_sheets(&bytes).expect("import combined chart workbook");

        let chart = imported.sheets[0]
            .chart
            .as_ref()
            .expect("imported line chart");
        assert_eq!(
            chart.kind,
            ChartKind::Line,
            "the importer selects the line plot from the combination"
        );
        assert_eq!(
            chart.val_col, 2,
            "the imported line plot must keep its C-column values"
        );
        assert!(
            imported
                .warnings
                .iter()
                .any(|warning| { warning.label() == "additional bar chart plots" }),
            "the warning must name the dropped bar plot: {:?}",
            imported.warnings
        );
        assert!(
            imported
                .warnings
                .iter()
                .any(|warning| { warning.label() == "additional line chart series" }),
            "the warning must also name the dropped extra line series: {:?}",
            imported.warnings
        );
    }

    #[test]
    fn xlsx_import_reports_repeated_plot_groups_dropped_from_a_chart() {
        let bytes = xlsx_with_combined_chart_groups(ChartKind::Bar, ChartKind::Bar);
        let imported = import_xlsx_sheets(&bytes).expect("import repeated chart groups");

        assert_eq!(
            imported.sheets[0].chart.as_ref().map(|chart| chart.kind),
            Some(ChartKind::Bar)
        );
        assert!(
            imported
                .warnings
                .iter()
                .any(|warning| { warning.label() == "additional bar chart plots" }),
            "the warning must report the repeated, dropped bar plot: {:?}",
            imported.warnings
        );
    }

    #[test]
    fn xlsx_import_reports_extra_series_dropped_from_a_supported_plot_group() {
        let bytes = xlsx_with_multiple_series_in_one_plot_group();
        let imported = import_xlsx_sheets(&bytes).expect("import multi-series chart workbook");

        let chart = imported.sheets[0]
            .chart
            .as_ref()
            .expect("the supported line chart remains importable");
        assert_eq!(chart.kind, ChartKind::Line);
        assert_eq!(chart.cat_col, 0, "the first A-column category is retained");
        assert_eq!(chart.val_col, 1, "the first B-column series is retained");
        assert_eq!(
            imported
                .warnings
                .iter()
                .map(|warning| warning.label())
                .collect::<Vec<_>>(),
            vec!["additional line chart series"],
            "the second C-column series must be named before workbook replacement"
        );
    }

    #[test]
    fn xlsx_import_reports_single_unsupported_chart_in_strict_chart_namespace() {
        let bytes = xlsx_with_single_chart_type_in_namespace(
            "areaChart",
            "http://purl.oclc.org/ooxml/drawingml/chart",
        );
        let imported = import_xlsx_sheets(&bytes).expect("import strict-chart workbook");

        assert!(imported.sheets[0].chart.is_none());
        assert!(imported
            .warnings
            .iter()
            .any(|warning| warning.label() == "unsupported area charts"));
    }

    #[test]
    fn xlsx_import_keeps_supported_chart_types_out_of_loss_warnings() {
        for kind in [
            ChartKind::Bar,
            ChartKind::Line,
            ChartKind::Pie,
            ChartKind::Scatter,
        ] {
            let mut sheet = Sheet::new("Budget");
            sheet.chart = Some(SheetChart {
                kind,
                ..Default::default()
            });
            let bytes = export_xlsx_sheets(&[sheet]).expect("export supported chart");
            let imported = import_xlsx_sheets(&bytes).expect("import supported chart");
            assert!(imported.warnings.is_empty(), "{kind:?} chart warning");
            assert_eq!(
                imported.sheets[0].chart.as_ref().map(|chart| chart.kind),
                Some(kind)
            );
        }
    }

    #[test]
    fn xlsx_import_preserves_scatter_x_and_y_reference_columns() {
        let mut sheet = Sheet::new("Budget");
        sheet.set_str("D1", "Elapsed");
        sheet.set_str("D2", "1");
        sheet.set_str("D3", "2");
        sheet.set_str("E1", "Revenue");
        sheet.set_str("E2", "10");
        sheet.set_str("E3", "20");
        sheet.chart = Some(SheetChart {
            kind: ChartKind::Scatter,
            cat_col: 3,
            val_col: 4,
            start_row: 1,
            end_row: Some(2),
            ..Default::default()
        });
        let bytes = export_xlsx_sheets(&[sheet]).expect("export scatter chart");
        let imported = import_xlsx_sheets(&bytes).expect("import scatter chart");
        let chart = imported.sheets[0]
            .chart
            .as_ref()
            .expect("scatter chart remains supported");

        assert_eq!(chart.kind, ChartKind::Scatter);
        assert_eq!(
            chart.cat_col, 3,
            "x values should retain the D-column reference"
        );
        assert_eq!(
            chart.val_col, 4,
            "y values should retain the E-column reference"
        );
        assert!(imported.warnings.is_empty());
    }

    #[test]
    fn xlsx_import_ignores_chart_like_extension_elements_when_selecting_plot_type() {
        let mut sheet = Sheet::new("Budget");
        sheet.chart = Some(SheetChart {
            kind: ChartKind::Bar,
            ..Default::default()
        });
        let base = export_xlsx_sheets(&[sheet]).expect("export base workbook");
        let archive = PackageArchive::from_bytes(&base).expect("read base workbook");
        let chart_xml = String::from_utf8(
            archive
                .get("xl/charts/chart1.xml")
                .expect("chart part")
                .to_vec(),
        )
        .expect("chart XML is UTF-8")
        .replace(
            "</c:barChart>",
            "<x:ser xmlns:x=\"urn:loom:test\"/><x:container xmlns:x=\"urn:loom:test\"><c:ser/></x:container></c:barChart>",
        )
        .replace(
            "</c:chartSpace>",
            "<c:extLst><c:ext uri=\"urn:loom:test\"><x:pieChart xmlns:x=\"urn:loom:test\"/></c:ext></c:extLst></c:chartSpace>",
        );
        let bytes = test_xlsx_with_parts(
            &[("xl/charts/chart1.xml", chart_xml)],
            &[],
            &[],
            Some(&base),
        );

        let imported = import_xlsx_sheets(&bytes).expect("import extension chart workbook");
        assert_eq!(
            imported.sheets[0].chart.as_ref().map(|chart| chart.kind),
            Some(ChartKind::Bar),
            "only actual chart-namespace plot groups determine the imported chart"
        );
        assert!(imported.warnings.is_empty());
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
    fn xlsx_import_warns_when_absolute_anchor_object_is_dropped() {
        let mut sheet = Sheet::new("Budget");
        sheet.objects.push(SheetObject::shape(
            CellRef { row: 0, col: 0 },
            "absolute object",
        ));
        let base = export_xlsx_sheets(&[sheet]).expect("export base workbook");
        let archive = PackageArchive::from_bytes(&base).expect("read base workbook");
        let base_drawing = String::from_utf8(
            archive
                .get("xl/drawings/drawing1.xml")
                .expect("exported drawing")
                .to_vec(),
        )
        .expect("drawing XML is UTF-8");

        let one_cell = import_xlsx_sheets(&base).expect("import one-cell-anchor workbook");
        assert_eq!(one_cell.sheets[0].objects.len(), 1);
        assert!(
            one_cell.warnings.is_empty(),
            "one-cell anchors are supported"
        );

        let from = "<xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>";
        let two_cell_drawing = base_drawing
            .replace(
                from,
                "<xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from><xdr:to><xdr:col>1</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>1</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:to>",
            );
        let extent_start = two_cell_drawing
            .find("<xdr:ext ")
            .expect("one-cell extent starts");
        let extent_end = two_cell_drawing[extent_start..]
            .find("/>")
            .map(|offset| extent_start + offset + 2)
            .expect("one-cell extent ends");
        let two_cell_drawing = format!(
            "{}{}",
            &two_cell_drawing[..extent_start],
            &two_cell_drawing[extent_end..]
        )
        .replace("oneCellAnchor", "twoCellAnchor");
        let two_cell_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", two_cell_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let two_cell =
            import_xlsx_sheets(&two_cell_bytes).expect("import two-cell-anchor workbook");
        assert_eq!(two_cell.sheets[0].objects.len(), 1);
        assert!(
            two_cell.warnings.is_empty(),
            "two-cell anchors are supported"
        );

        let drawing = base_drawing
            .replace(from, "<xdr:pos x=\"9525\" y=\"19050\"/>")
            .replace("<xdr:oneCellAnchor>", "<xdr:absoluteAnchor>")
            .replace("</xdr:oneCellAnchor>", "</xdr:absoluteAnchor>");
        assert!(drawing.contains("<xdr:absoluteAnchor>"));
        assert!(!drawing.contains("<xdr:from>"));
        let bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", drawing.clone())],
            &[],
            &[],
            Some(&base),
        );

        let imported = import_xlsx_sheets(&bytes).expect("import absolute-anchor workbook");
        assert!(
            imported.sheets[0].objects.is_empty(),
            "the object is currently dropped"
        );
        assert_eq!(
            imported
                .warnings
                .iter()
                .map(|warning| warning.label())
                .collect::<Vec<_>>(),
            vec!["objects positioned with absolute anchors"],
            "the import report must name the absolute-anchor loss"
        );

        let extension_drawing = drawing
            .replace(
                "<xdr:absoluteAnchor>",
                "<x:absoluteAnchor xmlns:x=\"urn:loom:test\">",
            )
            .replace("</xdr:absoluteAnchor>", "</x:absoluteAnchor>");
        let extension_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", extension_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let extension_import =
            import_xlsx_sheets(&extension_bytes).expect("import extension-decoy workbook");
        assert!(
            extension_import.warnings.is_empty(),
            "an extension-namespace lookalike is not a spreadsheet drawing anchor"
        );

        let strict_drawing = drawing.replace(
            "http://schemas.openxmlformats.org/drawingml/2006/spreadsheetDrawing",
            "http://purl.oclc.org/ooxml/drawingml/spreadsheetDrawing",
        );
        let strict_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", strict_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let strict_import =
            import_xlsx_sheets(&strict_bytes).expect("import strict absolute-anchor workbook");
        assert!(strict_import
            .warnings
            .contains(&XlsxImportWarning::AbsoluteDrawingAnchors));

        let anchor_close = drawing
            .rfind("</xdr:absoluteAnchor>")
            .expect("absolute anchor closing tag");
        let truncated_drawing = drawing[..anchor_close].to_string();
        let truncated_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", truncated_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let error = import_xlsx_sheets(&truncated_bytes)
            .expect_err("truncated absolute-anchor XML must fail closed");
        assert!(error.contains("unclosed drawing XML element"), "{error}");
    }

    #[test]
    fn xlsx_import_uses_supported_alternate_content_fallback_without_loss_warning() {
        let mut sheet = Sheet::new("Budget");
        sheet.objects.push(SheetObject::shape(
            CellRef { row: 0, col: 0 },
            "Fallback shape",
        ));
        let base = export_xlsx_sheets(&[sheet]).expect("export base workbook");
        let archive = PackageArchive::from_bytes(&base).expect("read base workbook");
        let drawing = String::from_utf8(
            archive
                .get("xl/drawings/drawing1.xml")
                .expect("exported drawing")
                .to_vec(),
        )
        .expect("drawing XML is UTF-8");
        let from = "<xdr:from><xdr:col>0</xdr:col><xdr:colOff>0</xdr:colOff><xdr:row>0</xdr:row><xdr:rowOff>0</xdr:rowOff></xdr:from>";
        let absolute_drawing = drawing
            .replace(from, "<xdr:pos x=\"9525\" y=\"19050\"/>")
            .replace("<xdr:oneCellAnchor>", "<xdr:absoluteAnchor>")
            .replace("</xdr:oneCellAnchor>", "</xdr:absoluteAnchor>");
        let absolute_start = absolute_drawing
            .find("<xdr:absoluteAnchor>")
            .expect("absolute choice anchor starts");
        let absolute_end = absolute_drawing[absolute_start..]
            .find("</xdr:absoluteAnchor>")
            .map(|offset| absolute_start + offset + "</xdr:absoluteAnchor>".len())
            .expect("absolute choice anchor ends");
        let absolute_anchor = &absolute_drawing[absolute_start..absolute_end];
        let fallback_start = drawing
            .find("<xdr:oneCellAnchor>")
            .expect("fallback anchor starts");
        let fallback_end = drawing[fallback_start..]
            .find("</xdr:oneCellAnchor>")
            .map(|offset| fallback_start + offset + "</xdr:oneCellAnchor>".len())
            .expect("fallback anchor ends");
        let fallback_anchor = &drawing[fallback_start..fallback_end];
        let root_end = drawing
            .find("<xdr:oneCellAnchor>")
            .expect("drawing root ends before the anchor");
        let root = drawing[..root_end].replace(
            "xmlns:xdr=",
            "xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:ext=\"urn:loom:test-extension\" xmlns:xdr=",
        );
        let alternate_drawing = format!(
            "{root}<mc:AlternateContent><mc:Choice Requires=\"ext\">{absolute_anchor}</mc:Choice><mc:Fallback>{fallback_anchor}</mc:Fallback></mc:AlternateContent></xdr:wsDr>"
        );
        let bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", alternate_drawing.clone())],
            &[],
            &[],
            Some(&base),
        );

        let imported = import_xlsx_sheets(&bytes).expect("import alternate drawing workbook");
        assert_eq!(imported.sheets[0].objects.len(), 1);
        assert_eq!(imported.sheets[0].objects[0].label, "Fallback shape");
        assert!(
            imported.warnings.is_empty(),
            "an absolute anchor in an unsupported Choice is not lost when a supported Fallback is imported"
        );

        let mismatched_choice_anchor =
            absolute_anchor.replace("<a:t>Fallback shape</a:t>", "<a:t>Choice shape</a:t>");
        let mismatched_choice_drawing =
            alternate_drawing.replace(absolute_anchor, &mismatched_choice_anchor);
        let mismatched_choice_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", mismatched_choice_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let mismatched_choice = import_xlsx_sheets(&mismatched_choice_bytes)
            .expect("import mismatched-choice workbook");
        assert_eq!(
            mismatched_choice.sheets[0].objects[0].label,
            "Fallback shape"
        );
        assert!(mismatched_choice
            .warnings
            .contains(&XlsxImportWarning::AbsoluteDrawingAnchors));

        let selected_choice_drawing =
            alternate_drawing.replace("Requires=\"ext\"", "Requires=\"xdr\"");
        let selected_choice_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", selected_choice_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let selected_choice =
            import_xlsx_sheets(&selected_choice_bytes).expect("import supported-choice workbook");
        assert!(selected_choice
            .warnings
            .contains(&XlsxImportWarning::AbsoluteDrawingAnchors));

        let unselected_choice_drawing = format!(
            "{root}<mc:AlternateContent><mc:Choice Requires=\"xdr\">{fallback_anchor}</mc:Choice><mc:Choice Requires=\"xdr\">{absolute_anchor}</mc:Choice><mc:Fallback/></mc:AlternateContent></xdr:wsDr>"
        );
        let unselected_choice_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", unselected_choice_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let unselected_choice = import_xlsx_sheets(&unselected_choice_bytes)
            .expect("import later supported-choice workbook");
        assert_eq!(unselected_choice.sheets[0].objects.len(), 1);
        assert!(
            unselected_choice.warnings.is_empty(),
            "an absolute anchor in a later supported Choice is not selected by Markup Compatibility"
        );

        let nested_unsupported_choice_drawing = format!(
            "{root}<mc:AlternateContent><mc:Choice Requires=\"xdr\"><mc:AlternateContent><mc:Choice Requires=\"ext\">{absolute_anchor}</mc:Choice><mc:Fallback>{fallback_anchor}</mc:Fallback></mc:AlternateContent></mc:Choice><mc:Fallback/></mc:AlternateContent></xdr:wsDr>"
        );
        let nested_unsupported_choice_bytes = test_xlsx_with_parts(
            &[(
                "xl/drawings/drawing1.xml",
                nested_unsupported_choice_drawing,
            )],
            &[],
            &[],
            Some(&base),
        );
        let nested_unsupported_choice = import_xlsx_sheets(&nested_unsupported_choice_bytes)
            .expect("import nested fallback workbook");
        assert_eq!(nested_unsupported_choice.sheets[0].objects.len(), 1);
        assert!(
            nested_unsupported_choice.warnings.is_empty(),
            "a matching inner Fallback preserves an absolute object inside a selected outer Choice"
        );

        let nested_unselected_outer_drawing = format!(
            "{root}<mc:AlternateContent><mc:Choice Requires=\"ext\"><mc:AlternateContent><mc:Choice Requires=\"xdr\">{absolute_anchor}</mc:Choice></mc:AlternateContent></mc:Choice><mc:Fallback>{fallback_anchor}</mc:Fallback></mc:AlternateContent></xdr:wsDr>"
        );
        let nested_unselected_outer_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", nested_unselected_outer_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let nested_unselected_outer = import_xlsx_sheets(&nested_unselected_outer_bytes)
            .expect("import unsupported outer-choice workbook");
        assert_eq!(nested_unselected_outer.sheets[0].objects.len(), 1);
        assert!(
            nested_unselected_outer.warnings.is_empty(),
            "an unsupported outer Choice uses its matching Fallback even if it contains an inner supported Choice"
        );

        let empty_fallback_drawing = alternate_drawing.replace(
            &format!("<mc:Fallback>{fallback_anchor}</mc:Fallback>"),
            "<mc:Fallback/>",
        );
        let empty_fallback_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", empty_fallback_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let empty_fallback =
            import_xlsx_sheets(&empty_fallback_bytes).expect("import empty-fallback workbook");
        assert!(empty_fallback.sheets[0].objects.is_empty());
        assert!(
            empty_fallback
                .warnings
                .contains(&XlsxImportWarning::AbsoluteDrawingAnchors),
            "an empty fallback does not preserve the absolute-anchor object"
        );

        let unrelated_anchor = fallback_anchor.replace("id=\"1\"", "id=\"99\"");
        assert_ne!(unrelated_anchor, fallback_anchor);
        let unrelated_fallback_drawing = alternate_drawing.replace(
            &format!("<mc:Fallback>{fallback_anchor}</mc:Fallback>"),
            &format!("<mc:Fallback>{unrelated_anchor}</mc:Fallback>"),
        );
        let unrelated_fallback_bytes = test_xlsx_with_parts(
            &[("xl/drawings/drawing1.xml", unrelated_fallback_drawing)],
            &[],
            &[],
            Some(&base),
        );
        let unrelated_fallback = import_xlsx_sheets(&unrelated_fallback_bytes)
            .expect("import unrelated-fallback workbook");
        assert_eq!(unrelated_fallback.sheets[0].objects.len(), 1);
        assert!(unrelated_fallback
            .warnings
            .contains(&XlsxImportWarning::AbsoluteDrawingAnchors));
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

    fn xlsx_with_single_chart_type(chart_tag: &str) -> Vec<u8> {
        xlsx_with_single_chart_type_in_namespace(chart_tag, CHART_NS)
    }

    fn xlsx_with_single_chart_type_in_namespace(chart_tag: &str, namespace: &str) -> Vec<u8> {
        let (kind, source_tag) = match chart_tag {
            "areaChart" => (ChartKind::Line, "lineChart"),
            "doughnutChart" => (ChartKind::Pie, "pieChart"),
            _ => panic!("add a schema-appropriate source fixture for {chart_tag}"),
        };
        let mut sheet = Sheet::new("Budget");
        sheet.chart = Some(SheetChart {
            kind,
            ..Default::default()
        });
        let base = export_xlsx_sheets(&[sheet]).expect("export base workbook");
        let archive = PackageArchive::from_bytes(&base).expect("read base workbook");
        let chart_xml = String::from_utf8(
            archive
                .get("xl/charts/chart1.xml")
                .expect("chart part")
                .to_vec(),
        )
        .expect("chart XML is UTF-8");
        assert!(chart_xml.contains(&format!("<c:{source_tag}>")));
        assert!(chart_xml.contains(&format!("</c:{source_tag}>")));
        let mut chart_xml = chart_xml
            .replace(&format!("<c:{source_tag}>"), &format!("<c:{chart_tag}>"))
            .replace(&format!("</c:{source_tag}>"), &format!("</c:{chart_tag}>"));
        if chart_tag == "doughnutChart" {
            chart_xml = chart_xml.replace(
                "</c:doughnutChart>",
                "<c:holeSize val=\"50\"/></c:doughnutChart>",
            );
        }
        let chart_xml = chart_xml.replace(CHART_NS, namespace);
        let drawing_xml = String::from_utf8(
            archive
                .get("xl/drawings/drawing1.xml")
                .expect("drawing part")
                .to_vec(),
        )
        .expect("drawing XML is UTF-8")
        .replace(CHART_NS, namespace);
        test_xlsx_with_parts(
            &[
                ("xl/charts/chart1.xml", chart_xml),
                ("xl/drawings/drawing1.xml", drawing_xml),
            ],
            &[],
            &[],
            Some(&base),
        )
    }

    fn xlsx_with_combined_chart_groups(first: ChartKind, second: ChartKind) -> Vec<u8> {
        let chart_group = |kind, value_col| {
            let mut sheet = Sheet::new("Budget");
            sheet.set_str("A1", "Q1");
            sheet.set_str("A2", "Q2");
            sheet.set_str("B1", "10");
            sheet.set_str("B2", "20");
            sheet.set_str("C1", "15");
            sheet.set_str("C2", "30");
            sheet.chart = Some(SheetChart {
                kind,
                val_col: value_col,
                ..Default::default()
            });
            let bytes = export_xlsx_sheets(&[sheet]).expect("export chart group");
            let archive = PackageArchive::from_bytes(&bytes).expect("read chart package");
            String::from_utf8(
                archive
                    .get("xl/charts/chart1.xml")
                    .expect("chart part")
                    .to_vec(),
            )
            .expect("chart XML is UTF-8")
        };
        let first_xml = chart_group(first, 1);
        let second_xml = chart_group(second, 2);
        let chart_tag = |kind| match kind {
            ChartKind::Bar => "barChart",
            ChartKind::Line => "lineChart",
            ChartKind::Pie => "pieChart",
            ChartKind::Scatter => "scatterChart",
        };
        let extract_group = |xml: &str, tag: &str| {
            let open = format!("<c:{tag}>");
            let close = format!("</c:{tag}>");
            let start = xml.find(&open).expect("chart group starts");
            let end = xml[start..]
                .find(&close)
                .map(|offset| start + offset + close.len())
                .expect("chart group ends");
            xml[start..end].to_string()
        };
        let first_tag = chart_tag(first);
        let second_group = extract_group(&second_xml, chart_tag(second))
            .replace("<c:idx val=\"0\"/>", "<c:idx val=\"1\"/>")
            .replace("<c:order val=\"0\"/>", "<c:order val=\"1\"/>");
        let combined_xml = first_xml.replacen(
            &format!("</c:{first_tag}>"),
            &format!("</c:{first_tag}>{second_group}"),
            1,
        );
        let mut sheet = Sheet::new("Budget");
        sheet.chart = Some(SheetChart {
            kind: first,
            ..Default::default()
        });
        let base = export_xlsx_sheets(&[sheet]).expect("export base workbook");
        test_xlsx_with_parts(
            &[("xl/charts/chart1.xml", combined_xml)],
            &[],
            &[],
            Some(&base),
        )
    }

    fn xlsx_with_multiple_series_in_one_plot_group() -> Vec<u8> {
        let mut sheet = Sheet::new("Budget");
        sheet.set_str("A1", "Q1");
        sheet.set_str("A2", "Q2");
        sheet.set_str("B1", "10");
        sheet.set_str("B2", "20");
        sheet.set_str("C1", "15");
        sheet.set_str("C2", "30");
        sheet.chart = Some(SheetChart {
            kind: ChartKind::Line,
            val_col: 1,
            end_row: Some(2),
            ..Default::default()
        });
        let base = export_xlsx_sheets(&[sheet]).expect("export base chart workbook");
        let archive = PackageArchive::from_bytes(&base).expect("read chart package");
        let xml = String::from_utf8(
            archive
                .get("xl/charts/chart1.xml")
                .expect("chart part")
                .to_vec(),
        )
        .expect("chart XML is UTF-8")
        .replacen(
            "<c:order val=\"0\"/>",
            "<c:order val=\"0\"/><c:tx><c:strRef><c:f>'Budget'!$B$1</c:f></c:strRef></c:tx>",
            1,
        );
        let series_start = xml.find("<c:ser>").expect("first chart series starts");
        let series_end = xml[series_start..]
            .find("</c:ser>")
            .map(|offset| series_start + offset + "</c:ser>".len())
            .expect("first chart series ends");
        let second_series = xml[series_start..series_end]
            .replace("<c:idx val=\"0\"/>", "<c:idx val=\"1\"/>")
            .replace("<c:order val=\"0\"/>", "<c:order val=\"1\"/>")
            .replace("'Budget'!$B$1", "'Budget'!$C$1")
            .replace("'Budget'!$B$2:$B$3", "'Budget'!$C$2:$C$3");
        assert!(
            second_series.contains("'Budget'!$C$2:$C$3"),
            "the fixture must give the second series a distinct value column: {second_series}"
        );
        let multi_series_xml = xml.replacen(
            "</c:lineChart>",
            &format!("{second_series}</c:lineChart>"),
            1,
        );
        test_xlsx_with_parts(
            &[("xl/charts/chart1.xml", multi_series_xml)],
            &[],
            &[],
            Some(&base),
        )
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
