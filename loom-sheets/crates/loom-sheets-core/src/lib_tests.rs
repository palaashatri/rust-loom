use super::*;

#[test]
fn workbook_integrity_digest_stability() {
    let make_workbook = || {
        let mut wb = Workbook::with_sheet("Data");
        wb.add_sheet("Summary");
        wb.sheet_mut(0)
            .expect("sheet 0 exists")
            .set_str("A1", "Revenue");
        wb.sheet_mut(0).expect("sheet 0 exists").set_str("B2", "42");
        wb.sheet_mut(1)
            .expect("sheet 1 exists")
            .set_str("A1", "Total");
        wb
    };
    let mut workbook = make_workbook();
    let digest = workbook.integrity_digest();
    assert_eq!(
        digest,
        workbook.integrity_digest(),
        "repeated calls must agree"
    );
    assert_eq!(
        digest,
        make_workbook().integrity_digest(),
        "identical workbooks must produce equal digests"
    );

    workbook
        .sheet_mut(0)
        .expect("sheet 0 exists")
        .set_str("B2", "43");
    let changed_value = workbook.integrity_digest();
    assert_ne!(
        digest, changed_value,
        "changing one cell's raw value must change the digest"
    );

    workbook
        .rename_sheet(1, "Overview")
        .expect("rename succeeds");
    assert_ne!(
        changed_value,
        workbook.integrity_digest(),
        "renaming a sheet must change the digest"
    );

    assert_ne!(
        Workbook::with_sheet("Data").integrity_digest(),
        digest,
        "an empty workbook must not share a digest with a populated one"
    );
}

#[test]
fn ocr_table_to_editable_grid() {
    let cells = vec![
        OcrTableCell {
            row: 0,
            column: 0,
            text: "Item".to_string(),
            confidence: 0.95,
        },
        OcrTableCell {
            row: 0,
            column: 1,
            text: "Qty".to_string(),
            confidence: 0.91,
        },
        OcrTableCell {
            row: 1,
            column: 0,
            text: "Widget".to_string(),
            confidence: 0.42,
        },
        OcrTableCell {
            row: 1,
            column: 1,
            text: "7".to_string(),
            confidence: 0.03,
        },
    ];
    let (text, confidence) = grid_from_ocr_table(&cells).unwrap();
    assert_eq!(
        text,
        vec![
            vec!["Item".to_string(), "Qty".to_string()],
            vec!["Widget".to_string(), "7".to_string()],
        ]
    );
    assert_eq!(confidence, vec![vec![0.95, 0.91], vec![0.42, 0.03]]);

    let row_gap = vec![
        OcrTableCell {
            row: 0,
            column: 0,
            text: "a".to_string(),
            confidence: 0.5,
        },
        OcrTableCell {
            row: 2,
            column: 0,
            text: "b".to_string(),
            confidence: 0.5,
        },
    ];
    let err = grid_from_ocr_table(&row_gap).unwrap_err();
    assert!(err.contains("row 1"), "unexpected error: {err}");

    let duplicated = vec![
        OcrTableCell {
            row: 0,
            column: 0,
            text: "a".to_string(),
            confidence: 0.5,
        },
        OcrTableCell {
            row: 0,
            column: 0,
            text: "b".to_string(),
            confidence: 0.5,
        },
    ];
    assert!(grid_from_ocr_table(&duplicated)
        .unwrap_err()
        .contains("duplicate"));

    assert!(grid_from_ocr_table(&[]).is_err());
}

#[test]
fn cell_ref_parse_render() {
    let a1 = CellRef::parse("A1").unwrap();
    assert_eq!(a1, CellRef { row: 0, col: 0 });
    assert_eq!(a1.to_a1(), "A1");
    assert_eq!(CellRef::parse("B3").unwrap().to_a1(), "B3");
    assert_eq!(CellRef::parse("AA10").unwrap(), CellRef { row: 9, col: 26 });
    assert!(CellRef::parse("").is_none());
    assert!(CellRef::parse("1A").is_none());
    assert!(CellRef::parse("A0").is_none());
}

#[test]
fn basic_arithmetic() {
    let f = parse_formula("1+2*3").unwrap();
    let lookup = |_: CellRef| Value::Empty;
    let v = eval_expr(&f.root, &lookup);
    assert_eq!(v, Value::Number(7.0));
}

#[test]
fn precedence_and_parens() {
    let f = parse_formula("(1+2)*3").unwrap();
    let lookup = |_: CellRef| Value::Empty;
    assert_eq!(eval_expr(&f.root, &lookup), Value::Number(9.0));
    // Excel evaluates ^ left to right: (2^3)^2.
    let f2 = parse_formula("2^3^2").unwrap();
    assert_eq!(eval_expr(&f2.root, &lookup), Value::Number(64.0));
}

#[test]
fn division_by_zero() {
    let f = parse_formula("1/0").unwrap();
    let lookup = |_: CellRef| Value::Empty;
    assert_eq!(
        eval_expr(&f.root, &lookup),
        Value::Error(CalcError::DivZero)
    );
}

#[test]
fn cell_references_resolve() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "10");
    sheet.set_str("B1", "5");
    sheet.set_str("C1", "=A1+B1*2");
    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Number(20.0))
    );
}

#[test]
fn dependency_order_and_cycle() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "=B1+1");
    sheet.set_str("B1", "=C1+1");
    sheet.set_str("C1", "5");
    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(6.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("A1").unwrap()),
        Some(&Value::Number(7.0))
    );

    // Cycle A1 <-> B1.
    let mut cyc = Sheet::new("c");
    cyc.set_str("A1", "=B1");
    cyc.set_str("B1", "=A1");
    let vals = evaluate(&cyc);
    assert_eq!(
        vals.get(&CellRef::parse("A1").unwrap()),
        Some(&Value::Error(CalcError::Ref))
    );
}

#[test]
fn parse_error_reports() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "=1+");
    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("A1").unwrap()),
        Some(&Value::Error(CalcError::Parse))
    );
}

#[test]
fn csv_roundtrip() {
    let mut sheet = Sheet::new("data");
    sheet.set_str("A1", "10");
    sheet.set_str("B1", "\"a,b\"");
    sheet.set_str("A2", "=A1*2");
    let csv = to_csv(&sheet);
    assert!(csv.contains("10"));
    assert!(csv.contains("\"a,b\""));
    let loaded = from_csv("data", &csv);
    let vals = evaluate(&loaded);
    // A1 = 10 (literal), A2 self-referential cycle? No: A2 depends on A1 literal.
    assert_eq!(
        vals.get(&CellRef::parse("A2").unwrap()),
        Some(&Value::Number(20.0))
    );
}

#[test]
fn json_roundtrip() {
    let mut sheet = Sheet::new("t");
    sheet.set_str("A1", "1");
    sheet.set_str("B2", "=A1+1");
    sheet.set_col_width(1, 140.0);
    sheet.set_row_height(1, 32.0);
    let json = sheet_to_json(&sheet);
    let back = sheet_from_json(&json).unwrap();
    assert_eq!(back.name, "t");
    assert_eq!(back.raw(CellRef::parse("B2").unwrap()), Some("=A1+1"));
    assert_eq!(back.col_width(1), 140.0);
    assert_eq!(back.row_height(1), 32.0);
    let vals = evaluate(&back);
    assert_eq!(
        vals.get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Number(2.0))
    );
}

#[test]
fn editing_a_cell_retains_formula_and_empty_raw_semantics() {
    let mut sheet = Sheet::new("t");
    let cell = CellRef::parse("B1").unwrap();
    sheet.set_str("A1", "2");

    sheet.set_raw(cell, "=A1+1");
    assert_eq!(sheet.raw(cell), Some("=A1+1"));
    assert_eq!(evaluate(&sheet).get(&cell), Some(&Value::Number(3.0)));

    sheet.set_raw(cell, "");
    assert_eq!(sheet.raw(cell), Some(""));
    assert_eq!(evaluate(&sheet).get(&cell), Some(&Value::Empty));
}

#[test]
fn formula_edit_transaction_commits_once_and_cancels_without_mutation() {
    let mut canceled = CellEditTransaction::begin(Some("=A1+1"));
    canceled.update("=A1+12");
    assert_eq!(canceled.cancel(), Some("=A1+1".to_string()));

    let mut committed = CellEditTransaction::begin(Some("=A1+1"));
    committed.update("=A1+12");
    let edit = committed.commit().expect("changed draft commits");
    assert_eq!(edit.before(), Some("=A1+1"));
    assert_eq!(edit.after(), "=A1+12");

    let mut unchanged = CellEditTransaction::begin(Some("=A1+1"));
    unchanged.update("=A1+1");
    assert!(unchanged.commit().is_none());

    let mut empty = CellEditTransaction::begin(Some("=A1+1"));
    empty.update("");
    let empty_edit = empty.commit().expect("empty text is a raw edit");
    assert_eq!(empty_edit.after(), "");

    let mut new_empty = CellEditTransaction::begin(None);
    new_empty.update("");
    let new_empty_edit = new_empty
        .commit()
        .expect("empty text differs from absent raw");
    assert_eq!(new_empty_edit.before(), None);
    assert_eq!(new_empty_edit.after(), "");
}

#[test]
fn literal_parsing() {
    assert_eq!(parse_literal("  "), Value::Empty);
    assert_eq!(parse_literal("3.5"), Value::Number(3.5));
    assert_eq!(parse_literal("TRUE"), Value::Bool(true));
    assert_eq!(parse_literal("hello"), Value::Text("hello".to_string()));
}

#[test]
fn named_ranges_validation_and_conditional_formatting_work() {
    let mut sheet = Sheet::new("model");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("B1", "=SUM(DATA)");
    let mut model = SheetModel::new(sheet);
    model
        .set_named_range("DATA", CellRange::parse("A1:A2").unwrap())
        .unwrap();
    model.validations.push((
        CellRange::parse("A1:A2").unwrap(),
        ValidationRule::Number {
            min: Some(0.0),
            max: Some(100.0),
        },
    ));
    model.conditional_formats.push(ConditionalFormatRule {
        range: CellRange::parse("A1:A2").unwrap(),
        condition: FormatCondition::GreaterThan(15.0),
        style_id: "high".into(),
    });
    assert_eq!(
        model.evaluate().get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(30.0))
    );
    assert!(model
        .set_validated(CellRef::parse("A1").unwrap(), "-1")
        .is_err());
    assert_eq!(
        model.conditional_style_ids(CellRef::parse("A2").unwrap()),
        vec!["high"]
    );
}

#[test]
fn rows_sort_filter_and_ranges_are_deterministic() {
    let mut sheet = Sheet::new("rows");
    sheet.set_str("A1", "b");
    sheet.set_str("B1", "2");
    sheet.set_str("A2", "a");
    sheet.set_str("B2", "1");
    let mut model = SheetModel::new(sheet);
    let range = CellRange::parse("A1:B2").unwrap();
    model.sort_rows(range, 1, true).unwrap();
    assert_eq!(model.sheet.raw(CellRef::parse("A1").unwrap()), Some("a"));
    let hidden = model
        .filter_rows(range, 0, &FilterPredicate::Contains("a".into()))
        .unwrap();
    assert_eq!(hidden, vec![1]);
    assert_eq!(range.cells().len(), 4);
}

#[test]
fn calculation_cache_recalculates_transitive_dependents() {
    let mut sheet = Sheet::new("incremental");
    sheet.set_str("A1", "1");
    sheet.set_str("B1", "=A1+1");
    sheet.set_str("C1", "=B1+1");
    sheet.set_str("Z1", "99");
    let mut cache = CalculationCache::default();
    cache.rebuild(&sheet);
    sheet.set_str("A1", "10");
    let affected = cache.recalculate(&sheet, &[CellRef::parse("A1").unwrap()]);
    assert!(affected.contains(&CellRef::parse("C1").unwrap()));
    assert!(!affected.contains(&CellRef::parse("Z1").unwrap()));
    assert_eq!(
        cache.values.get(&CellRef::parse("C1").unwrap()),
        Some(&Value::Number(12.0))
    );
}

#[test]
fn extended_formula_functions_evaluate_correctly() {
    let mut sheet = Sheet::new("Formulas");
    sheet.set_str("A1", "10");
    sheet.set_str("A2", "20");
    sheet.set_str("A3", "30");
    sheet.set_str("B1", "=COUNT(A1:A3)");
    sheet.set_str("B2", "=IF(A1>5, \"High\", \"Low\")");
    sheet.set_str("B3", "=IF(A1>15, \"High\", \"Low\")");
    sheet.set_str("B4", "=AND(A1>5, A2>15)");
    sheet.set_str("B5", "=OR(A1>100, A2>15)");
    sheet.set_str("B6", "=NOT(A1>100)");

    let vals = evaluate(&sheet);
    assert_eq!(
        vals.get(&CellRef::parse("B1").unwrap()),
        Some(&Value::Number(3.0))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Text("High".into()))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B3").unwrap()),
        Some(&Value::Text("Low".into()))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B4").unwrap()),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B5").unwrap()),
        Some(&Value::Bool(true))
    );
    assert_eq!(
        vals.get(&CellRef::parse("B6").unwrap()),
        Some(&Value::Bool(true))
    );
}

#[test]
fn workbook_sheet_management_operations() {
    let mut wb = Workbook::with_sheet("Summary");
    assert_eq!(wb.len(), 1);
    assert!(!wb.is_empty());

    let idx2 = wb.add_sheet("Expenses");
    assert_eq!(idx2, 1);
    assert_eq!(wb.len(), 2);
    assert_eq!(wb.sheet(1).unwrap().name, "Expenses");

    wb.rename_sheet(1, "Q1 Expenses").unwrap();
    assert_eq!(wb.sheet(1).unwrap().name, "Q1 Expenses");

    wb.remove_sheet(1).unwrap();
    assert_eq!(wb.len(), 1);
    assert!(wb.remove_sheet(0).is_err()); // Cannot remove only remaining sheet
}

#[test]
fn sheet_clear_and_used_range_operations() {
    let mut sheet = Sheet::new("Data");
    assert_eq!(sheet.used_range(), None);

    sheet.set_str("B2", "10");
    sheet.set_str("D5", "20");
    let bounds = sheet.used_range().unwrap();
    assert_eq!(bounds, (1, 1, 3, 4));

    let c1 = CellRef::parse("B2").unwrap();
    let c2 = CellRef::parse("D5").unwrap();
    let cleared = sheet.clear_range(c1, c2);
    assert_eq!(cleared, 2);
    assert_eq!(sheet.used_range(), None);
}

#[test]
fn parse_csv_records_handles_multiline_quotes_and_custom_delimiters() {
    let csv_text =
        "Name,Description,Value\n\"Item 1\",\"Line 1\nLine 2\",100\n\"Item 2\",\"Simple\",200";
    let records = parse_csv_records(csv_text, ',');
    assert_eq!(records.len(), 3);
    assert_eq!(records[0], vec!["Name", "Description", "Value"]);
    assert_eq!(records[1], vec!["Item 1", "Line 1\nLine 2", "100"]);
    assert_eq!(records[2], vec!["Item 2", "Simple", "200"]);

    // Semicolon delimiter
    let semi_csv = "A;B;C\n1;2;3";
    let semi_records = parse_csv_records(semi_csv, ';');
    assert_eq!(semi_records.len(), 2);
    assert_eq!(semi_records[1], vec!["1", "2", "3"]);
}

#[test]
fn csv_dialect_sniffing() {
    // Comma detected with comma.
    let dialect = sniff_csv_dialect("a,b,c\n1,2,3").unwrap();
    assert_eq!(dialect.delimiter, ',');
    assert!(!dialect.quoted);

    // Semicolon-separated input.
    let dialect = sniff_csv_dialect("a;b\nc;d").unwrap();
    assert_eq!(dialect.delimiter, ';');
    assert!(!dialect.quoted);

    // Tab delimiter.
    let dialect = sniff_csv_dialect("a\tb\n1\t2").unwrap();
    assert_eq!(dialect.delimiter, '\t');

    // Pipe delimiter.
    let dialect = sniff_csv_dialect("a|b\n1|2").unwrap();
    assert_eq!(dialect.delimiter, '|');

    // Quoted fields are detected; the comma inside quotes is not counted.
    let dialect = sniff_csv_dialect("\"x,y\",z").unwrap();
    assert_eq!(dialect.delimiter, ',');
    assert!(dialect.quoted);

    // Empty input errors.
    assert!(sniff_csv_dialect("").is_err());
    assert!(sniff_csv_dialect("  \n\t\n").is_err());

    // All candidates at zero occurrences: documented default to ','.
    let dialect = sniff_csv_dialect("abc\ndef").unwrap();
    assert_eq!(dialect.delimiter, ',');
    assert!(!dialect.quoted);
}

#[test]
fn freeze_panes_configuration_and_unfreeze() {
    let mut sheet = Sheet::new("Dashboard");
    assert_eq!(sheet.freeze_rows, 0);
    assert_eq!(sheet.freeze_cols, 0);

    sheet.freeze_panes(2, 1);
    assert_eq!(sheet.freeze_rows, 2);
    assert_eq!(sheet.freeze_cols, 1);

    sheet.unfreeze_panes();
    assert_eq!(sheet.freeze_rows, 0);
    assert_eq!(sheet.freeze_cols, 0);
}

#[test]
fn cell_and_range_alignment() {
    let mut sheet = Sheet::new("Sales");
    let a1 = CellRef::parse("A1").unwrap();
    let b2 = CellRef::parse("B2").unwrap();
    assert_eq!(sheet.cell_alignment(a1), CellAlignment::General);

    sheet.set_cell_alignment(a1, CellAlignment::Center);
    assert_eq!(sheet.cell_alignment(a1), CellAlignment::Center);

    // Range alignment
    let start = CellRef::parse("B1").unwrap();
    let end = CellRef::parse("C3").unwrap();
    sheet.set_range_alignment(start, end, CellAlignment::Right);
    assert_eq!(sheet.cell_alignment(b2), CellAlignment::Right);
    assert_eq!(
        sheet.cell_alignment(CellRef::parse("C3").unwrap()),
        CellAlignment::Right
    );
}

#[test]
fn currency_and_percentage_formatting() {
    assert_eq!(format_number_currency(1234.56, "$", 2), "$1,234.56");
    assert_eq!(format_number_currency(-999999.0, "£", 0), "-£999,999");
    assert_eq!(format_number_currency(0.0, "$", 2), "$0.00");

    assert_eq!(format_number_percentage(0.255, 1), "25.5%");
    assert_eq!(format_number_percentage(1.0, 0), "100%");
}

#[test]
fn custom_column_and_row_sizing() {
    let mut sheet = Sheet::new("Dimensions");
    assert_eq!(sheet.col_width(0), 80.0);
    assert_eq!(sheet.row_height(0), 24.0);

    sheet.set_col_width(0, 150.0);
    sheet.set_row_height(5, 36.0);
    assert_eq!(sheet.col_width(0), 150.0);
    assert_eq!(sheet.row_height(5), 36.0);

    // Reset with 0.0
    sheet.set_col_width(0, 0.0);
    assert_eq!(sheet.col_width(0), 80.0);
}

#[test]
fn cell_number_format_display() {
    assert_eq!(
        format_cell_display("123.45", NumberFormat::Currency),
        "$123.45"
    );
    assert_eq!(
        format_cell_display("0.75", NumberFormat::Percentage),
        "75.0%"
    );
    assert_eq!(format_cell_display("1000", NumberFormat::Scientific), "1e3");
    assert_eq!(format_cell_display("hello", NumberFormat::General), "hello");
    assert_eq!(format_cell_display("", NumberFormat::Currency), "");
}

#[test]
fn fill_series_and_range_sorting() {
    let linear = generate_fill_series(10.0, 5.0, 4, FillSeriesType::Linear);
    assert_eq!(linear, vec![10.0, 15.0, 20.0, 25.0]);

    let growth = generate_fill_series(2.0, 3.0, 4, FillSeriesType::Growth);
    assert_eq!(growth, vec![2.0, 6.0, 18.0, 54.0]);

    let rows = vec![
        vec!["Cherry".into(), "30".into()],
        vec!["Apple".into(), "10".into()],
        vec!["Banana".into(), "20".into()],
    ];

    // Sort ascending by column 0 (string)
    let sorted_str = sort_range_rows(&rows, 0, true);
    assert_eq!(sorted_str[0][0], "Apple");
    assert_eq!(sorted_str[1][0], "Banana");
    assert_eq!(sorted_str[2][0], "Cherry");

    // Sort descending by column 1 (numeric)
    let sorted_num = sort_range_rows(&rows, 1, false);
    assert_eq!(sorted_num[0][1], "30");
    assert_eq!(sorted_num[1][1], "20");
    assert_eq!(sorted_num[2][1], "10");
}

#[test]
fn formula_reference_shifting() {
    // Relative reference: =A1 shifted right 1 col and down 2 rows -> =B3
    assert_eq!(shift_formula_references("=A1", 1, 2), "=B3");

    // Mixed references: =$A1 + B$2 + $C$3 + D4 shifted right 1 col, down 1 row:
    // $A1 -> $A2 (col absolute, row relative)
    // B$2 -> C$2 (col relative, row absolute)
    // $C$3 -> $C$3 (both absolute)
    // D4 -> E5 (both relative)
    assert_eq!(
        shift_formula_references("=$A1+B$2+$C$3+D4", 1, 1),
        "=$A2+C$2+$C$3+E5"
    );

    // Non-formula string unchanged
    assert_eq!(shift_formula_references("Hello World", 1, 1), "Hello World");
}

#[test]
fn dependency_graph_and_recalculation_order() {
    let mut graph = DependencyGraph::new();
    let a1 = CellRef::parse("A1").unwrap();
    let b1 = CellRef::parse("B1").unwrap();
    let c1 = CellRef::parse("C1").unwrap();

    // B1 depends on A1 (=A1 * 2)
    graph.add_dependency(b1, a1);
    // C1 depends on B1 (=B1 + 10)
    graph.add_dependency(c1, b1);

    assert_eq!(graph.get_direct_dependents(&a1), &[b1]);
    assert_eq!(graph.get_direct_dependents(&b1), &[c1]);

    // Recalculation order starting from dirty cell A1
    let order = graph.get_recalculation_order(&[a1]);
    assert_eq!(order, vec![a1, b1, c1]);
}

#[test]
fn cell_data_validation_rules() {
    let list_rule = DataValidationRule::new(
        ValidationCriteria::List(vec!["Red".into(), "Green".into(), "Blue".into()]),
        "Must be a valid color",
    );
    assert!(list_rule.validate("Red").is_ok());
    assert!(list_rule.validate("green").is_ok());
    assert!(list_rule.validate("Yellow").is_err());
    assert!(list_rule.validate("").is_ok()); // allow_blank = true

    let num_rule = DataValidationRule::new(
        ValidationCriteria::WholeNumberBetween(1, 100),
        "Must be 1..=100",
    );
    assert!(num_rule.validate("50").is_ok());
    assert!(num_rule.validate("101").is_err());
    assert!(num_rule.validate("abc").is_err());
}

#[test]
fn chart_spec_validation_and_normalization() {
    let series_a = ChartSeries {
        name: "Revenue".to_string(),
        categories: vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()],
        values: vec![0.0, 5.0, 10.0],
    };
    let series_b = ChartSeries {
        name: "Costs".to_string(),
        categories: vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()],
        values: vec![10.0, 5.0, 0.0],
    };
    let spec = ChartSpec {
        kind: ChartKind::Bar,
        title: "Quarterly".to_string(),
        series: vec![series_a.clone(), series_b.clone()],
    };

    assert!(spec.validate().is_ok());
    assert_eq!(spec.value_range().unwrap(), (0.0, 10.0));

    let points = spec.normalized_points().unwrap();
    assert_eq!(points.len(), 2);
    assert_eq!(points[0], vec![0.0, 0.5, 1.0]);
    assert_eq!(points[1], vec![1.0, 0.5, 0.0]);
    for series_points in &points {
        for &point in series_points {
            assert!((0.0..=1.0).contains(&point));
        }
    }

    // A constant-value series maps to 1.0 everywhere (min == max).
    let constant = ChartSpec {
        kind: ChartKind::Line,
        title: "Flat".to_string(),
        series: vec![ChartSeries {
            name: "Constant".to_string(),
            categories: vec!["a".to_string(), "b".to_string()],
            values: vec![7.0, 7.0],
        }],
    };
    assert_eq!(constant.value_range().unwrap(), (7.0, 7.0));
    assert_eq!(constant.normalized_points().unwrap(), vec![vec![1.0, 1.0]]);

    // Validation errors name the violated rule.
    let empty_title = ChartSpec {
        kind: ChartKind::Pie,
        title: String::new(),
        series: vec![series_a.clone()],
    };
    assert_eq!(
        empty_title.validate().unwrap_err(),
        "chart title must not be empty"
    );

    let no_series = ChartSpec {
        kind: ChartKind::Line,
        title: "Empty".to_string(),
        series: Vec::new(),
    };
    assert_eq!(
        no_series.validate().unwrap_err(),
        "chart must contain at least one series"
    );

    let mismatch = ChartSpec {
        kind: ChartKind::Scatter,
        title: "Mismatched".to_string(),
        series: vec![ChartSeries {
            name: series_b.name.clone(),
            categories: vec!["Q1".to_string()],
            values: vec![10.0, 5.0],
        }],
    };
    assert_eq!(
        mismatch.validate().unwrap_err(),
        "series 'Costs' has 1 categories but 2 values; lengths must match"
    );

    let nan = ChartSpec {
        kind: ChartKind::Line,
        title: "NaN".to_string(),
        series: vec![ChartSeries {
            name: series_a.name.clone(),
            categories: vec!["Q1".to_string(), "Q2".to_string()],
            values: vec![1.0, f64::NAN],
        }],
    };
    assert_eq!(
        nan.validate().unwrap_err(),
        "series 'Revenue' contains a NaN value at index 1"
    );
}

#[test]
fn chart_placement_export_validation() {
    let spec = ChartSpec {
        kind: ChartKind::Bar,
        title: "Quarterly".to_string(),
        series: vec![ChartSeries {
            name: "Revenue".to_string(),
            categories: vec!["Q1".to_string(), "Q2".to_string()],
            values: vec![1.0, 2.0],
        }],
    };
    let placement = |chart_id: &str, range: &str, policy: ChartUpdatePolicy| ChartPlacement {
        chart_id: chart_id.to_string(),
        sheet_name: "Sales".to_string(),
        source_range: range.to_string(),
        spec: spec.clone(),
        update_policy: policy,
    };

    // Valid ranges pass, including lowercase corners.
    let valid = placement("chart-1", "B2:D9", ChartUpdatePolicy::RefreshOnOpen);
    assert!(valid.validate().is_ok());
    assert!(
        placement("chart-1", "b2:d9", ChartUpdatePolicy::StaticSnapshot)
            .validate()
            .is_ok()
    );
    assert!(
        placement("chart-1", "AA10:AB11", ChartUpdatePolicy::StaticSnapshot)
            .validate()
            .is_ok()
    );

    // Bad range shapes name the violated rule.
    let missing_separator = placement("chart-1", "B2", ChartUpdatePolicy::StaticSnapshot);
    assert_eq!(
        missing_separator.validate().unwrap_err(),
        "source range 'B2' must contain exactly one ':' separator"
    );

    let digits_first = placement("chart-1", "2B:B2", ChartUpdatePolicy::StaticSnapshot);
    assert_eq!(
        digits_first.validate().unwrap_err(),
        "source range corner '2B' must be column letters followed by row digits"
    );

    let open_ended = placement("chart-1", "B2:", ChartUpdatePolicy::StaticSnapshot);
    assert_eq!(
        open_ended.validate().unwrap_err(),
        "source range corner '' must be column letters followed by row digits"
    );

    let letters_only = placement("chart-1", "B:D", ChartUpdatePolicy::StaticSnapshot);
    assert!(letters_only.validate().is_err());

    // Empty ids and sheet names are rejected.
    assert!(placement("", "B2:D9", ChartUpdatePolicy::StaticSnapshot)
        .validate()
        .is_err());
    let no_sheet = ChartPlacement {
        chart_id: "chart-1".to_string(),
        sheet_name: String::new(),
        source_range: "B2:D9".to_string(),
        spec: spec.clone(),
        update_policy: ChartUpdatePolicy::StaticSnapshot,
    };
    assert!(no_sheet.validate().is_err());

    // Collision rule 1: same chart_id collides regardless of range or policy.
    let same_id = placement("chart-1", "C3:E8", ChartUpdatePolicy::StaticSnapshot);
    assert!(valid.collides_with(&same_id));
    assert!(same_id.collides_with(&valid));

    // Collision rule 2: identical sheet+range with a non-static policy collides.
    let refreshed = placement("chart-2", "B2:D9", ChartUpdatePolicy::RefreshOnOpen);
    assert!(valid.collides_with(&refreshed));

    // A static twin still collides because the refreshed side re-reads the shared range.
    let static_twin = placement("chart-2", "B2:D9", ChartUpdatePolicy::StaticSnapshot);
    assert!(valid.collides_with(&static_twin));

    // Two pure snapshots sharing the range never fight over updates.
    let static_pair_a = placement("chart-2", "B2:D9", ChartUpdatePolicy::StaticSnapshot);
    let static_pair_b = placement("chart-3", "B2:D9", ChartUpdatePolicy::StaticSnapshot);
    assert!(!static_pair_a.collides_with(&static_pair_b));

    // Fully distinct placements never collide.
    let distinct =
        placement("chart-4", "F1:G4", ChartUpdatePolicy::RefreshOnOpen).collides_with(&valid);
    assert!(!distinct);
}

#[test]
fn date_arithmetic_civil_calendar() {
    // Leap years: divisible by 4, except centuries unless divisible by 400.
    assert!(is_leap_year(2024));
    assert!(!is_leap_year(1900));
    assert!(is_leap_year(2000));

    assert_eq!(days_in_month(2024, 2).unwrap(), 29);
    assert_eq!(days_in_month(1900, 2).unwrap(), 28);
    assert!(days_in_month(2024, 13).is_err());

    // Month and year boundary crossings in both directions.
    assert_eq!(add_days(2024, 1, 31, 1).unwrap(), (2024, 2, 1));
    assert_eq!(add_days(2023, 12, 31, 1).unwrap(), (2024, 1, 1));
    assert_eq!(add_days(2024, 3, 1, -1).unwrap(), (2024, 2, 29));

    // Whole-day difference spans a leap day; sign reflects direction.
    assert_eq!(days_between(2023, 3, 1, 2024, 3, 1).unwrap(), 366);
    assert_eq!(days_between(2024, 3, 1, 2023, 3, 1).unwrap(), -366);

    // Invalid civil dates are rejected before any arithmetic.
    assert!(add_days(2024, 2, 30, 1).is_err());
    assert!(days_between(2024, 2, 30, 2024, 3, 1).is_err());
    assert!(days_in_month(2024, 0).is_err());

    // Large-offset round trip: +10000 days, measure, then subtract back.
    let (y0, m0, d0) = (2021, 6, 15u32);
    let (y1, m1, d1) = add_days(y0, m0, d0, 10_000).unwrap();
    assert_eq!(days_between(y0, m0, d0, y1, m1, d1).unwrap(), 10_000);
    assert_eq!(add_days(y1, m1, d1, -10_000).unwrap(), (y0, m0, d0));
}

/// Builds an in-memory xlsx image with the given part contents.
fn build_xlsx(parts: &[(&str, &str)]) -> Vec<u8> {
    let mut archive = PackageArchive::new();
    for (path, xml) in parts {
        archive
            .add(path, xml.as_bytes().to_vec())
            .expect("part adds");
    }
    archive.to_bytes().expect("archive serializes")
}

#[test]
fn extract_xlsx_grid_resolves_shared_inline_numeric_and_boolean_cells() {
    let xlsx = build_xlsx(&[
        (
            "xl/sharedStrings.xml",
            r#"<sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                <si><t>Alpha</t></si>
                <si><t>Beta &amp; Gamma</t></si>
            </sst>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
                <sheetData>
                    <row r="1">
                        <c r="A1" t="s"><v>0</v></c>
                        <c r="B1" t="s"><v>1</v></c>
                    </row>
                    <row r="2">
                        <c r="A2"><v>42</v></c>
                        <c r="B2" t="b"><v>1</v></c>
                    </row>
                    <row r="3">
                        <c r="C3" t="inlineStr"><is><t>Inline</t></is></c>
                    </row>
                </sheetData>
            </worksheet>"#,
        ),
    ]);

    let grid = extract_xlsx_grid(&xlsx).expect("extraction succeeds");
    assert_eq!(grid.len(), 3);
    assert_eq!(
        grid,
        vec![
            vec![
                "Alpha".to_string(),
                "Beta & Gamma".to_string(),
                String::new()
            ],
            vec!["42".to_string(), "TRUE".to_string(), String::new()],
            vec![String::new(), String::new(), "Inline".to_string()],
        ]
    );
}

#[test]
fn extract_xlsx_grid_resolves_shared_formula_members_with_relative_offsets() {
    // The member appears before the master to cover the two-pass index.
    let sheet = r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">
        <sheetData>
            <row r="1">
                <c r="A1"><v>10</v></c>
                <c r="B1"><f t="shared" si="0" ref="B1:B3">A1*2</f><v>20</v></c>
            </row>
            <row r="2">
                <c r="A2"><v>50</v></c>
                <c r="B2"><f t="shared" si="0"/><v>100</v></c>
            </row>
            <row r="3">
                <c r="A3"><v>7</v></c>
                <c r="B3"><f t="shared" si="0"/><v>14</v></c>
            </row>
        </sheetData>
    </worksheet>"#;

    let formulas = extract_sheet_grid_with_formulas(sheet, &[]).expect("formula extraction");
    assert_eq!(formulas[0][1], "=A1*2");
    assert_eq!(formulas[1][1], "=A2*2");
    assert_eq!(formulas[2][1], "=A3*2");

    let mut imported = Sheet::new("Shared");
    for (row, values) in formulas.iter().enumerate() {
        for (col, value) in values.iter().enumerate() {
            if !value.is_empty() {
                imported.set_raw(
                    CellRef {
                        row: row as u32,
                        col: col as u32,
                    },
                    value,
                );
            }
        }
    }
    let evaluated = workbook::evaluate_workbook(std::slice::from_ref(&imported));
    assert_eq!(
        evaluated[0].get(&CellRef::parse("B2").unwrap()),
        Some(&Value::Number(100.0))
    );
}

#[test]
fn extract_xlsx_grid_errs_on_corrupt_archive_or_missing_sheet_part() {
    assert!(extract_xlsx_grid(b"not a zip at all")
        .unwrap_err()
        .contains("unreadable xlsx archive"));

    // A well-formed archive without the worksheet part must also err.
    let xlsx = build_xlsx(&[("xl/sharedStrings.xml", "<sst><si><t>Alpha</t></si></sst>")]);
    assert!(extract_xlsx_grid(&xlsx)
        .unwrap_err()
        .contains("missing worksheet part xl/worksheets/sheet1.xml"));
}

#[test]
fn xlsx_extraction_helpers_handle_entities_coordinates_and_gaps() {
    // Single-pass entity decoding must not double-decode.
    assert_eq!(xml_unescape("Beta &amp; Gamma"), "Beta & Gamma");
    assert_eq!(xml_unescape("&amp;lt;"), "&lt;");
    assert_eq!(
        xml_unescape("&lt;a&gt; &quot;q&quot; &apos;p&apos;"),
        "<a> \"q\" 'p'"
    );
    assert_eq!(
        xml_unescape("plain & unknown; stays"),
        "plain & unknown; stays"
    );

    // Coordinate parsing: letters to zero-based column, digits to row-1.
    assert_eq!(parse_cell_coordinate("A1"), Some((0, 0)));
    assert_eq!(parse_cell_coordinate("AB12"), Some((27, 11)));
    assert_eq!(parse_cell_coordinate(" C3 "), Some((2, 2)));
    assert_eq!(parse_cell_coordinate(""), None);
    assert_eq!(parse_cell_coordinate("1A"), None);
    assert_eq!(parse_cell_coordinate("A0"), None);

    // Out-of-range shared-string index errs instead of emitting a value.
    let sheet = r#"<sheetData><row r="1"><c r="A1" t="s"><v>9</v></c></row></sheetData>"#;
    let err = extract_sheet_grid(sheet, &["Alpha".to_string()]).unwrap_err();
    assert!(err.contains("out of range"), "unexpected error: {err}");

    // Cells beyond the used range densify as empty strings.
    let sparse = r#"<sheetData>
        <row r="1"><c r="B1"><v>7</v></c></row>
        <row r="4"><c r="E4"><v>x</v></c></row>
    </sheetData>"#;
    let grid = extract_sheet_grid(sparse, &[]).expect("sparse extraction");
    assert_eq!(grid.len(), 4);
    assert!(grid.iter().all(|row| row.len() == 5));
    assert_eq!(grid[0][1], "7");
    assert_eq!(grid[3][4], "x");
    assert_eq!(grid[0][0], "");
}

#[test]
fn xlsx_export_round_trips_through_import() {
    let grid = vec![
        vec!["Region".to_string(), "Q1".to_string(), "Q2".to_string()],
        vec!["East".to_string(), "10".to_string(), String::new()],
        vec![
            "West & Co <Ltd>".to_string(),
            String::new(),
            "TRUE".to_string(),
        ],
        vec![String::new(); 3],
    ];

    let xlsx = export_xlsx_from_grid(&grid).expect("export succeeds");
    let parsed = extract_xlsx_grid(&xlsx).expect("re-import succeeds");

    // Populated rows round-trip exactly; the trailing all-empty row is dropped
    // (documented rule: absence is not content).
    assert_eq!(parsed.len(), grid.len() - 1);
    for (expected, actual) in grid.iter().zip(parsed.iter()) {
        assert_eq!(actual, expected);
    }
    assert_eq!(parsed[2][0], "West & Co <Ltd>");
    assert_eq!(parsed[1][2], "");
    assert_eq!(parsed[0], grid[0]);

    // Repeated strings share one shared-string entry (structural check).
    // Numbers and booleans write inline (native `<v>` / `t="b"`), so only
    // the five text displays land in the table.
    let archive = PackageArchive::from_bytes(&xlsx).unwrap();
    let sst = std::str::from_utf8(archive.get("xl/sharedStrings.xml").unwrap()).unwrap();
    let unique = sst.matches("<si>").count();
    let referenced = sst.matches("count=\"").count() > 0;
    assert!(referenced && unique == 5, "unique={unique}");

    // Empty grids export an empty sheet and import back empty.
    let empty = export_xlsx_from_grid(&[]).unwrap();
    assert!(extract_xlsx_grid(&empty).unwrap().is_empty());
}

#[test]
fn xlsx_workbook_export_preserves_sheets_and_formulas() {
    let mut first = Sheet::new("First");
    first.set_str("A1", "10");
    first.set_str("B1", "=A1*2");
    let mut second = Sheet::new("Second");
    second.set_str("A1", "=First!B1+5");
    let evaluated = workbook::evaluate_workbook(&[first.clone(), second.clone()]);
    let sheets = [&first, &second]
        .iter()
        .zip(evaluated.iter())
        .map(|(sheet, vals)| sheet_to_xlsx_data(sheet, vals))
        .collect::<Vec<_>>();
    let bytes = export_xlsx_workbook(&sheets).expect("workbook export");
    let text = String::from_utf8_lossy(&bytes);

    // Both tabs ship as worksheet parts with sanitized names.
    assert!(text.contains("worksheets/sheet1.xml"));
    assert!(text.contains("worksheets/sheet2.xml"));
    assert!(text.contains("name=\"First\""));
    assert!(text.contains("name=\"Second\""));
    // Formulas ship as <f> elements with cached display values.
    assert!(text.contains("<f>A1*2</f>"));
    assert!(text.contains("<f>First!B1+5</f>"));
    // Numeric cached values are native, not shared strings.
    assert!(text.contains("<v>20</v>"));

    // First-sheet import still reads displayed values.
    let grid = extract_xlsx_grid(&bytes).expect("re-import");
    assert_eq!(grid[0][0], "10");
    assert_eq!(grid[0][1], "20");

    let workbook = extract_xlsx_workbook(&bytes).expect("all sheets re-import");
    assert_eq!(workbook.len(), 2);
    assert_eq!(workbook[0].0, "First");
    assert_eq!(workbook[1].0, "Second");
    assert_eq!(workbook[0].1[0][1], "=A1*2");
    assert_eq!(workbook[1].1[0][0], "=First!B1+5");
}

#[test]
fn xlsx_sheet_names_sanitize_to_excel_rules() {
    assert_eq!(sanitize_xlsx_sheet_name("Data", 0), "Data");
    assert_eq!(sanitize_xlsx_sheet_name("A/B:C", 1), "ABC");
    assert_eq!(sanitize_xlsx_sheet_name("  ", 2), "Sheet3");
    assert_eq!(sanitize_xlsx_sheet_name("\"Q\"", 0), "Q");
    let long = "x".repeat(40);
    assert_eq!(sanitize_xlsx_sheet_name(&long, 0).chars().count(), 31);
    assert!(export_xlsx_workbook(&[]).is_err());
}

#[test]
fn xlsx_export_keeps_sanitized_sheet_names_unique_and_rewrites_references() {
    let mut slash_name = Sheet::new("A/B");
    slash_name.set_str("A1", "10");
    slash_name.set_str("B1", "20");
    slash_name.chart = Some(SheetChart {
        kind: ChartKind::Line,
        cat_col: 0,
        val_col: 1,
        ..Default::default()
    });

    let mut exact_name = Sheet::new("AB");
    exact_name.set_str("A1", "30");

    let long_name = "Very Long Sheet Name That Exceeds 31 Characters";
    let mut long_sheet = Sheet::new(long_name);
    long_sheet.set_str("A1", "40");

    let mut consumer = Sheet::new("Consumer");
    consumer.set_str(
        "A1",
        &format!("='A/B'!A1+'AB'!A1+'{long_name}'!A1+\"A/B!A1\""),
    );

    let bytes = export_xlsx_sheets(&[slash_name, exact_name, long_sheet, consumer])
        .expect("sanitized workbook export");
    let imported = extract_xlsx_workbook(&bytes).expect("sanitized workbook import");

    assert_eq!(imported[0].0, "AB");
    assert_eq!(imported[1].0, "AB (2)");
    assert_eq!(imported[2].0, sanitize_xlsx_sheet_name(long_name, 2));
    assert_eq!(
        imported[3].1[0][0],
        format!(
            "=AB!A1+'AB (2)'!A1+'{}'!A1+\"A/B!A1\"",
            sanitize_xlsx_sheet_name(long_name, 2)
        )
    );

    let archive = PackageArchive::from_bytes(&bytes).expect("zip archive");
    let chart = String::from_utf8_lossy(archive.get("xl/charts/chart1.xml").unwrap());
    assert!(chart.contains("'AB'!$A$2:$A$2"));
}

#[test]
fn viewport_reveals_a_selection_outside_its_visible_window() {
    let mut viewport = SheetViewport::new(15, 8);

    viewport.reveal(CellRef { row: 24, col: 9 });

    assert_eq!(viewport.first_row, 10);
    assert_eq!(viewport.first_col, 2);
    assert!(viewport.contains(CellRef { row: 24, col: 9 }));
    assert_eq!(viewport.row_at(0), Some(10));
    assert_eq!(viewport.column_at(0), Some(2));
}

#[test]
fn viewport_projects_scroll_offsets_against_workbook_dimensions() {
    let dimensions = SheetDimensions::new(1_000, 52);
    let viewport = SheetViewport::from_scroll(180.0, 672.0, 360.0, 280.0, 28.0, 90.0, dimensions);

    assert_eq!(viewport.first_row, 24);
    assert_eq!(viewport.first_col, 2);
    assert_eq!(viewport.visible_rows, 10);
    assert_eq!(viewport.visible_cols, 4);
    assert_eq!(viewport.row_at(0), Some(24));
    assert_eq!(viewport.column_at(3), Some(5));
    assert_eq!(dimensions.content_size(28.0, 90.0), (4_680.0, 28_000.0));
}

#[test]
fn viewport_clamps_invalid_and_out_of_range_scroll_offsets() {
    let dimensions = SheetDimensions::new(10, 8);
    let origin = SheetViewport::from_scroll(-100.0, f32::NAN, 160.0, 48.0, 24.0, 80.0, dimensions);
    assert_eq!(origin.first_row, 0);
    assert_eq!(origin.first_col, 0);
    assert_eq!(origin.visible_rows, 2);
    assert_eq!(origin.visible_cols, 2);

    let tail = SheetViewport::from_scroll(10_000.0, 10_000.0, 160.0, 48.0, 24.0, 80.0, dimensions);
    assert_eq!(tail.first_row, 8);
    assert_eq!(tail.first_col, 6);
    assert_eq!(tail.visible_rows, 2);
    assert_eq!(tail.visible_cols, 2);
}

#[test]
fn sheet_dimensions_follow_sparse_used_cells_with_nonempty_minimum() {
    let mut empty = Sheet::new("empty");
    assert_eq!(empty.dimensions(), SheetDimensions::new(1, 1));

    empty.set_str("AZ1000", "tail");
    assert_eq!(empty.dimensions(), SheetDimensions::new(1_000, 52));

    let mut object_sheet = Sheet::new("objects");
    object_sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 4, col: 3 }, "Note"));
    assert_eq!(object_sheet.dimensions(), SheetDimensions::new(5, 4));
}

#[test]
fn sheet_grid_defaults_are_shared_and_invalid_dimensions_fall_back() {
    let mut sheet = Sheet::new("defaults");
    assert_eq!(DEFAULT_COL_WIDTH, 80.0);
    assert_eq!(DEFAULT_ROW_HEIGHT, 24.0);
    assert_eq!(sheet.col_width(0), DEFAULT_COL_WIDTH);
    assert_eq!(sheet.row_height(0), DEFAULT_ROW_HEIGHT);

    sheet.set_col_width(0, f32::NAN);
    sheet.set_col_width(1, f32::INFINITY);
    sheet.set_row_height(0, f32::NAN);
    sheet.set_row_height(1, f32::INFINITY);
    assert_eq!(sheet.col_width(0), DEFAULT_COL_WIDTH);
    assert_eq!(sheet.col_width(1), DEFAULT_COL_WIDTH);
    assert_eq!(sheet.row_height(0), DEFAULT_ROW_HEIGHT);
    assert_eq!(sheet.row_height(1), DEFAULT_ROW_HEIGHT);
}

#[test]
fn grid_selection_preserves_anchor_and_normalizes_range() {
    let anchor = CellRef::parse("D7").unwrap();
    let focus = CellRef::parse("B3").unwrap();
    let selection = GridSelection::new(anchor, focus);

    assert_eq!(selection.anchor, anchor);
    assert_eq!(selection.focus, focus);
    assert_eq!(selection.range().to_a1(), "B3:D7");
    assert!(selection.contains(CellRef::parse("C5").unwrap()));
    assert!(!selection.contains(CellRef::parse("E7").unwrap()));
    assert_eq!(selection.label(), "B3:D7");
}

#[test]
fn grid_selection_extend_and_collapse_keep_keyboard_semantics() {
    let anchor = CellRef::parse("C3").unwrap();
    let selection = GridSelection::new(anchor, anchor).extend(CellRef::parse("E5").unwrap());
    assert_eq!(selection.anchor, anchor);
    assert_eq!(selection.focus, CellRef::parse("E5").unwrap());
    assert_eq!(selection.label(), "C3:E5");

    let collapsed = selection.collapse(CellRef::parse("B2").unwrap());
    assert_eq!(collapsed.anchor, CellRef::parse("B2").unwrap());
    assert_eq!(collapsed.focus, CellRef::parse("B2").unwrap());
    assert_eq!(collapsed.label(), "B2");
}
