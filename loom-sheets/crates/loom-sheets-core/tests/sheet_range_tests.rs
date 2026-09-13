use loom_sheets_core::{CellRange, CellRef, GridSelection, RangeEdit, Sheet};

#[test]
fn grid_selection_preserves_anchor_and_normalizes_range() {
    let selection =
        GridSelection::new(CellRef::parse("B2").unwrap(), CellRef::parse("D5").unwrap());
    assert_eq!(selection.anchor, CellRef::parse("B2").unwrap());
    assert_eq!(selection.focus, CellRef::parse("D5").unwrap());
    assert_eq!(selection.label(), "B2:D5");

    let reversed = GridSelection::new(CellRef::parse("D5").unwrap(), CellRef::parse("B2").unwrap());
    assert_eq!(reversed.range(), selection.range());

    let collapsed = selection.collapse(CellRef::parse("B2").unwrap());
    assert_eq!(collapsed.anchor, CellRef::parse("B2").unwrap());
    assert_eq!(collapsed.focus, CellRef::parse("B2").unwrap());
    assert_eq!(collapsed.label(), "B2");
}

#[test]
fn range_edit_fills_formulas_and_reverts_without_losing_absent_cells() {
    let mut sheet = Sheet::new("fill");
    sheet.set_str("A1", "10");
    sheet.set_str("B1", "=A1+1");
    sheet.set_str("A2", "20");

    let copy = RangeEdit::copy(
        &sheet,
        CellRange::parse("B1").unwrap(),
        CellRef::parse("C1").unwrap(),
    );
    copy.apply(&mut sheet);
    assert_eq!(sheet.raw(CellRef::parse("C1").unwrap()), Some("=B1+1"));
    copy.revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef::parse("C1").unwrap()), None);

    let fill = RangeEdit::fill(
        &sheet,
        CellRange::parse("A1:A2").unwrap(),
        CellRange::parse("A3:A6").unwrap(),
    );
    fill.apply(&mut sheet);
    assert_eq!(sheet.raw(CellRef::parse("A3").unwrap()), Some("10"));
    assert_eq!(sheet.raw(CellRef::parse("A4").unwrap()), Some("20"));
    assert_eq!(sheet.raw(CellRef::parse("A5").unwrap()), Some("10"));
    assert_eq!(sheet.raw(CellRef::parse("A6").unwrap()), Some("20"));
    fill.revert(&mut sheet);
    for row in 3..=6 {
        assert_eq!(
            sheet.raw(CellRef {
                row: row - 1,
                col: 0
            }),
            None
        );
    }
}

#[test]
fn range_edit_copy_handles_multi_cell_formulas_and_noop_detection() {
    let mut sheet = Sheet::new("copy");
    sheet.set_str("A1", "10");
    sheet.set_str("B1", "=A1+1");
    sheet.set_str("A2", "20");

    let source = CellRange::parse("A1:B2").unwrap();
    let copy = RangeEdit::copy(&sheet, source, CellRef::parse("D3").unwrap());
    assert_eq!(copy.len(), 4);
    assert!(!copy.is_noop());
    copy.apply(&mut sheet);
    assert_eq!(sheet.raw(CellRef::parse("D3").unwrap()), Some("10"));
    assert_eq!(sheet.raw(CellRef::parse("E3").unwrap()), Some("=D3+1"));
    assert_eq!(sheet.raw(CellRef::parse("D4").unwrap()), Some("20"));
    assert_eq!(sheet.raw(CellRef::parse("E4").unwrap()), None);

    copy.revert(&mut sheet);
    assert_eq!(sheet.raw(CellRef::parse("D3").unwrap()), None);
    assert_eq!(sheet.raw(CellRef::parse("E3").unwrap()), None);
    assert_eq!(sheet.raw(CellRef::parse("D4").unwrap()), None);
    assert_eq!(sheet.raw(CellRef::parse("E4").unwrap()), None);

    let noop = RangeEdit::copy(
        &sheet,
        CellRange::parse("A1").unwrap(),
        CellRef::parse("A1").unwrap(),
    );
    assert!(noop.is_noop());
}

#[test]
fn range_edit_replace_preserves_present_empty_and_absent_raw_values() {
    let mut sheet = Sheet::new("replace");
    let cell = CellRef::parse("A1").unwrap();

    let insert_empty = RangeEdit::replace(&sheet, cell, Some(String::new()));
    assert!(!insert_empty.is_noop());
    insert_empty.apply(&mut sheet);
    assert_eq!(sheet.raw(cell), Some(""));
    insert_empty.revert(&mut sheet);
    assert_eq!(sheet.raw(cell), None);

    sheet.set_raw(cell, "old");
    let clear_to_empty = RangeEdit::replace(&sheet, cell, Some(String::new()));
    clear_to_empty.apply(&mut sheet);
    assert_eq!(sheet.raw(cell), Some(""));
    clear_to_empty.revert(&mut sheet);
    assert_eq!(sheet.raw(cell), Some("old"));

    let remove = RangeEdit::replace(&sheet, cell, None);
    remove.apply(&mut sheet);
    assert_eq!(sheet.raw(cell), None);
    remove.revert(&mut sheet);
    assert_eq!(sheet.raw(cell), Some("old"));
}
