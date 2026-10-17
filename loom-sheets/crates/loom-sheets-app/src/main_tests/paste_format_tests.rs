//! Pasting carries each cell's format with its value, as one undo step.
use super::clipboard_tests::state_with;
use super::*;
use crate::system_clipboard::{set_external_clipboard_for_test, set_external_text_for_test};
use loom_sheets_core::style::{CellAlignment, FillColor};

fn select(app: &SheetsApp, state: &GuiState, from: &str, to: &str) {
    let sheet = state.current.borrow();
    let values = evaluate(&sheet);
    update_selection_range(
        app,
        &sheet,
        &values,
        GridSelection::new(CellRef::parse(from).unwrap(), CellRef::parse(to).unwrap()),
    );
}

fn style(state: &GuiState, cell: &str) -> loom_sheets_core::style::CellStyle {
    state
        .current
        .borrow()
        .cell_style(CellRef::parse(cell).unwrap())
}

fn style_a1(state: &GuiState) {
    let a1 = CellRef::parse("A1").unwrap();
    let mut sheet = state.current.borrow_mut();
    let mut s = sheet.cell_style(a1);
    s.bold = true;
    s.fill = FillColor::Yellow;
    s.text_color = Some(0x00C6_2828);
    s.wrap = true;
    sheet.set_cell_style(a1, s);
    sheet.set_cell_alignment(a1, CellAlignment::Right);
}

#[test]
fn a_copy_inside_sheets_pastes_the_format_with_the_value_and_one_undo_removes_both() {
    let (app, state) = state_with(&[("A1", "total"), ("B1", "plain")]);
    style_a1(&state);
    select(&app, &state, "A1", "B1");
    app.invoke_copy_selection();

    select(&app, &state, "D3", "D3");
    app.invoke_paste_selection();
    let d3 = style(&state, "D3");
    assert!(d3.bold && d3.wrap, "bold and wrap came across");
    assert_eq!(d3.fill, FillColor::Yellow);
    assert_eq!(d3.text_color, Some(0x00C6_2828));
    assert_eq!(
        state
            .current
            .borrow()
            .cell_alignment(CellRef::parse("D3").unwrap()),
        CellAlignment::Right
    );
    assert!(
        style(&state, "E3").is_default(),
        "an unformatted cell stays plain"
    );

    let steps = state.undo_stack.borrow().len();
    app.invoke_undo();
    assert_eq!(
        state.undo_stack.borrow().len(),
        steps - 1,
        "one step undone"
    );
    let sheet = state.current.borrow();
    assert_eq!(
        sheet.raw(CellRef::parse("D3").unwrap()),
        None,
        "the value is gone"
    );
    assert!(
        sheet.cell_style(CellRef::parse("D3").unwrap()).is_default(),
        "and so is the format"
    );
}

#[test]
fn one_copied_cell_fills_a_selection_with_its_format() {
    let (app, state) = state_with(&[("A1", "x")]);
    style_a1(&state);
    select(&app, &state, "A1", "A1");
    app.invoke_copy_selection();
    select(&app, &state, "C1", "C3");
    app.invoke_paste_selection();
    for cell in ["C1", "C2", "C3"] {
        assert_eq!(style(&state, cell).fill, FillColor::Yellow, "{cell}");
    }
}

#[test]
fn plain_text_from_another_program_leaves_the_formatting_alone() {
    let (app, state) = state_with(&[("A1", "old")]);
    style_a1(&state);
    set_external_text_for_test(Some("new\tvalue"));
    select(&app, &state, "A1", "A1");
    app.invoke_paste_selection();
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("new")
    );
    let a1 = style(&state, "A1");
    assert!(
        a1.bold && a1.wrap && a1.fill == FillColor::Yellow,
        "format untouched"
    );
}

#[test]
fn a_styled_table_from_writer_or_a_web_page_pastes_its_bold_colours_and_alignment() {
    let (app, state) = state_with(&[]);
    let grid = vec![vec![
        loom_clipboard::SheetsCellData {
            value: "Head".into(),
            formatted_text: "Head".into(),
            bold: true,
            italic: true,
            background_color: Some("#BFDBFE".into()),
            text_color: Some("#1565C0".into()),
            alignment: Some(loom_objects::table::HorizontalAlign::Center),
            width_pt: 80.0,
        },
        loom_clipboard::SheetsCellData {
            value: "plain".into(),
            formatted_text: "plain".into(),
            width_pt: 80.0,
            ..Default::default()
        },
    ]];
    let table = loom_clipboard::sheets_range_to_table(&grid);
    set_external_clipboard_for_test(Some(loom_clipboard::ClipboardData::from_table(&table)));
    select(&app, &state, "A1", "A1");
    app.invoke_paste_selection();

    let a1 = style(&state, "A1");
    assert!(a1.bold && a1.italic);
    assert_eq!(a1.fill, FillColor::Blue, "the nearest Sheets swatch");
    assert_eq!(a1.text_color, Some(0x0015_65C0));
    assert_eq!(
        state
            .current
            .borrow()
            .cell_alignment(CellRef::parse("A1").unwrap()),
        CellAlignment::Center
    );
    assert!(style(&state, "B1").is_default());
}
