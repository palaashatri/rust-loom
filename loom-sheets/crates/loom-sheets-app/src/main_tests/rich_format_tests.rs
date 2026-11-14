//! Tests for rich formatting: per-edge borders, strikethrough, merge/unmerge,
//! conditional formatting, and Paste Special (Values Only / Formats Only).

use super::keyboard_flow_tests::launched;
use super::*;
use loom_sheets_core::style::FillColor;
use loom_sheets_core::CellRange;

fn wired(cells: &[(&str, &str)]) -> keyboard_flow_tests::Session {
    let s = launched(cells);
    crate::ink_actions::wire(&s.app, &s.state, &s.menu);
    crate::rich_format_actions::wire(&s.app, &s.state, &s.menu);
    crate::clipboard_actions::wire(&s.app, &s.state, &s.menu);
    s
}

fn select(s: &keyboard_flow_tests::Session, a1: &str) {
    let range = CellRange::parse(a1).unwrap();
    let sel = GridSelection::new(range.start, range.end);
    let sheet = s.state.current.borrow();
    let vals = evaluate(&sheet);
    update_selection_range(&s.app, &sheet, &vals, sel);
}

fn undo_last(s: &keyboard_flow_tests::Session) {
    let tx = s.state.undo_stack.borrow_mut().pop().expect("undo step");
    tx.revert(&mut s.state.current.borrow_mut());
    apply_sheet(&s.app, &s.state);
}

#[test]
fn strikethrough_and_border_presets_roundtrip_with_undo() {
    let s = wired(&[("A1", "Title"), ("B2", "Item")]);
    select(&s, "A1:B2");

    let rf = s.app.global::<CellRichFormat>();
    rf.invoke_toggle_strikethrough();
    assert!(
        s.state
            .current
            .borrow()
            .cell_style(CellRef::parse("A1").unwrap())
            .strikethrough
    );
    assert!(rf.get_selected_strikethrough());

    rf.invoke_apply_border_preset("outer".into());
    let a1_borders = s
        .state
        .current
        .borrow()
        .cell_style(CellRef::parse("A1").unwrap())
        .borders;
    assert!(a1_borders.top && a1_borders.left && !a1_borders.bottom && !a1_borders.right);
    let b2_borders = s
        .state
        .current
        .borrow()
        .cell_style(CellRef::parse("B2").unwrap())
        .borders;
    assert!(b2_borders.bottom && b2_borders.right && !b2_borders.top && !b2_borders.left);

    undo_last(&s);
    assert!(!s
        .state
        .current
        .borrow()
        .cell_style(CellRef::parse("A1").unwrap())
        .borders
        .any());

    undo_last(&s);
    assert!(
        !s.state
            .current
            .borrow()
            .cell_style(CellRef::parse("A1").unwrap())
            .strikethrough
    );
}

#[test]
fn merge_and_unmerge_cells_project_overlay_and_undo_cleanly() {
    let s = wired(&[("A1", "Header"), ("B1", "Extra")]);
    select(&s, "A1:B2");

    let rf = s.app.global::<CellRichFormat>();
    rf.invoke_merge_cells();
    assert_eq!(s.state.current.borrow().merged_ranges.len(), 1);
    assert_eq!(
        s.state.current.borrow().raw(CellRef::parse("B1").unwrap()),
        None
    );
    assert_eq!(rf.get_merged_views().row_count(), 1);
    assert!(rf.get_selected_merged());

    undo_last(&s);
    assert!(s.state.current.borrow().merged_ranges.is_empty());
    assert_eq!(
        s.state.current.borrow().raw(CellRef::parse("B1").unwrap()),
        Some("Extra")
    );

    rf.invoke_merge_cells();
    assert_eq!(s.state.current.borrow().merged_ranges.len(), 1);
    rf.invoke_unmerge_cells();
    assert!(s.state.current.borrow().merged_ranges.is_empty());
}

#[test]
fn conditional_formatting_modal_adds_highlights_and_removes_rules() {
    let s = wired(&[("A1", "150"), ("A2", "50")]);
    select(&s, "A1:A2");

    let rf = s.app.global::<CellRichFormat>();
    rf.invoke_open_conditional_formatting();
    assert!(rf.get_cf_modal_open());

    // op=0 (GreaterThan), value="100", fill=0 (Red), text=3 (Blue)
    rf.invoke_cf_add_rule(0, "100".into(), 0, 3);
    assert_eq!(s.state.current.borrow().conditional_rules.len(), 1);
    assert_eq!(rf.get_cf_rules().row_count(), 1);
    assert_eq!(
        s.app.get_cell_fills().row_data(0),
        Some(FillColor::Red.swatch_index())
    );
    let cols = s.app.get_cols().row_count();
    assert_eq!(s.app.get_cell_fills().row_data(cols), Some(-1));

    rf.invoke_cf_remove_rule(0);
    assert!(s.state.current.borrow().conditional_rules.is_empty());
    assert_eq!(s.app.get_cell_fills().row_data(0), Some(-1));

    rf.invoke_cf_close();
    assert!(!rf.get_cf_modal_open());
}

#[test]
fn paste_special_values_only_and_formats_only() {
    let s = wired(&[("A1", "=2+3"), ("B1", "10")]);
    let a1 = CellRef::parse("A1").unwrap();
    let b1 = CellRef::parse("B1").unwrap();

    // Style A1 with bold + Red fill, B1 with Green fill
    {
        let mut sheet = s.state.current.borrow_mut();
        let mut st_a = sheet.cell_style(a1);
        st_a.bold = true;
        st_a.fill = FillColor::Red;
        sheet.set_cell_style(a1, st_a);

        let mut st_b = sheet.cell_style(b1);
        st_b.fill = FillColor::Green;
        sheet.set_cell_style(b1, st_b);
    }

    // Copy A1
    select(&s, "A1");
    s.app.invoke_copy_selection();

    // Paste Values Only into B1: B1 gets "5" (not "=2+3") and keeps Green fill
    select(&s, "B1");
    s.app.global::<CellRichFormat>().invoke_paste_values_only();
    {
        let sheet = s.state.current.borrow();
        assert_eq!(sheet.raw(b1), Some("5"));
        assert_eq!(sheet.cell_style(b1).fill, FillColor::Green);
        assert!(!sheet.cell_style(b1).bold);
    }

    // Paste Formats Only into B1: B1 keeps "5" and gets Red fill + bold
    s.app.global::<CellRichFormat>().invoke_paste_formats_only();
    {
        let sheet = s.state.current.borrow();
        assert_eq!(sheet.raw(b1), Some("5"));
        assert_eq!(sheet.cell_style(b1).fill, FillColor::Red);
        assert!(sheet.cell_style(b1).bold);
    }
}
