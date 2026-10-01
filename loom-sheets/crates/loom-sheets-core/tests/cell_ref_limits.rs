use loom_sheets_core::CellRef;

#[test]
fn long_text_is_not_a_cell_reference_and_does_not_overflow() {
    assert!(CellRef::parse("nowhere").is_none());
    assert!(CellRef::parse("ABCDEFGHIJKLMNOP1").is_none());
}

#[test]
fn references_stop_at_the_last_column_and_row_of_other_spreadsheets() {
    let last = CellRef::parse("XFD1048576").unwrap();
    assert_eq!((last.col, last.row), (16_383, 1_048_575));
    assert!(CellRef::parse("XFE1").is_none());
    assert!(CellRef::parse("A1048577").is_none());
    assert_eq!(CellRef::parse("a1").unwrap().to_a1(), "A1");
}
