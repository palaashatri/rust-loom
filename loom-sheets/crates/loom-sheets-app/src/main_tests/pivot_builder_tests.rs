//! Tests for the Interactive Pivot Table Field Builder modal, live preview,
//! command/palette/menu dispatch, and workbook/sheet undo support.

use super::keyboard_flow_tests::launched;
use super::*;
use slint::Model;

fn sample_cells() -> Vec<(&'static str, &'static str)> {
    vec![
        ("A1", "Region"),
        ("B1", "Quarter"),
        ("C1", "Category"),
        ("D1", "Revenue"),
        ("A2", "North"),
        ("B2", "Q1"),
        ("C2", "Software"),
        ("D2", "100"),
        ("A3", "North"),
        ("B3", "Q2"),
        ("C3", "Software"),
        ("D3", "150"),
        ("A4", "South"),
        ("B4", "Q1"),
        ("C4", "Hardware"),
        ("D4", "200"),
        ("A5", "South"),
        ("B5", "Q2"),
        ("C5", "Software"),
        ("D5", "50"),
    ]
}

#[test]
fn opens_builder_with_detected_headers_and_updates_live_preview() {
    let s = launched(&sample_cells());
    crate::pivot_builder::wire(&s.app, &s.state, &s.menu);

    assert!(crate::dispatch_command(&s.app, "table.pivot_builder"));
    let pb = s.app.global::<PivotBuilderState>();
    assert!(pb.get_open());
    assert_eq!(pb.get_source_range().as_str(), "A1:D5");

    let fields = pb.get_fields();
    assert_eq!(fields.row_count(), 4);
    assert_eq!(fields.row_data(0).unwrap().name.as_str(), "Region");
    assert!(fields.row_data(0).unwrap().is_row);
    assert_eq!(fields.row_data(1).unwrap().name.as_str(), "Quarter");
    assert!(fields.row_data(1).unwrap().is_val);

    // Switch value field from Quarter (1) to Revenue (3), and toggle Quarter (1) as Col field.
    pb.invoke_toggle_val_field(1);
    pb.invoke_toggle_val_field(3);
    pb.invoke_toggle_col_field(1);
    assert!(pb.get_error_text().is_empty());

    let preview = pb.get_preview_rows();
    // Header + North + South + Grand Total = 4 rows
    assert_eq!(preview.row_count(), 4);
    let header_row = preview.row_data(0).unwrap();
    assert_eq!(header_row.cells.row_data(0).unwrap().as_str(), "Region");
    assert_eq!(header_row.cells.row_data(1).unwrap().as_str(), "Q1");
    assert_eq!(header_row.cells.row_data(2).unwrap().as_str(), "Q2");
    assert_eq!(
        header_row.cells.row_data(3).unwrap().as_str(),
        "Grand Total"
    );

    // Cycle Revenue aggregation to Average (0 -> 1 Count -> 2 Average)
    pb.invoke_cycle_val_agg(3);
    pb.invoke_cycle_val_agg(3);
    assert_eq!(pb.get_fields().row_data(3).unwrap().val_agg, 2);

    // Filter Category (2) to Software (index 1 in ["(All)", "Software", "Hardware"])
    pb.invoke_select_filter_field(2);
    assert_eq!(pb.get_filter_field_idx(), 2);
    assert_eq!(pb.get_filter_values().row_count(), 3);
    pb.invoke_select_filter_value(1);

    // Apply to new sheet
    pb.invoke_apply_pivot_builder();
    assert!(!pb.get_open());
    assert_eq!(s.state.sheets.borrow().len(), 2);
    assert_eq!(*s.state.active_sheet_index.borrow(), 1);
    let pivot_sheet = s.state.current.borrow();
    assert!(pivot_sheet
        .raw(CellRef::parse("B2").unwrap())
        .unwrap()
        .starts_with("=AVERAGEIFS("));
    drop(pivot_sheet);

    // Undo restores single sheet
    s.app.invoke_undo();
    assert_eq!(s.state.sheets.borrow().len(), 1);
    assert_eq!(*s.state.active_sheet_index.borrow(), 0);
}

#[test]
fn applies_pivot_to_current_sheet_with_undo_and_palette_wiring() {
    let s = launched(&sample_cells());
    crate::pivot_builder::wire(&s.app, &s.state, &s.menu);

    assert!(crate::palette::dispatch_palette_action(
        &s.app,
        crate::palette::PaletteAction::CreatePivotTable
    ));
    let pb = s.app.global::<PivotBuilderState>();
    assert!(pb.get_open());
    pb.invoke_toggle_val_field(1);
    pb.invoke_toggle_val_field(3);
    pb.invoke_set_dest_new_sheet(false);
    pb.invoke_set_dest_cell("F1".into());
    pb.invoke_apply_pivot_builder();

    assert!(!pb.get_open());
    assert_eq!(s.state.sheets.borrow().len(), 1);
    assert_eq!(
        s.state
            .current
            .borrow()
            .raw(CellRef::parse("F1").unwrap())
            .as_deref(),
        Some("Region")
    );
    assert!(s
        .state
        .current
        .borrow()
        .raw(CellRef::parse("G2").unwrap())
        .unwrap()
        .starts_with("=SUMIFS("));

    // Undo clears F1
    s.app.invoke_undo();
    assert_eq!(
        s.state.current.borrow().raw(CellRef::parse("F1").unwrap()),
        None
    );
}
