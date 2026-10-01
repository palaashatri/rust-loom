use super::*;
use crate::system_clipboard::{get_text, set_external_text_for_test};

fn state_with(cells: &[(&str, &str)]) -> (SheetsApp, Rc<GuiState>) {
    set_platform();
    set_external_text_for_test(None);
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("Data");
    for (cell, raw) in cells {
        sheet.set_str(cell, raw);
    }
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).unwrap(),
        FileFilter::new("CSV", ["csv"]).unwrap(),
        FileFilter::new("CSV", ["csv"]).unwrap(),
        FileFilter::new("Excel", ["xlsx"]).unwrap(),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_sheet_actions(&app, &state, &menu_service);
    register_history_actions(&app, &state, &menu_service);
    (app, state)
}

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

fn raw(state: &GuiState, cell: &str) -> String {
    state
        .current
        .borrow()
        .raw(CellRef::parse(cell).unwrap())
        .unwrap_or_default()
        .to_string()
}

#[test]
fn copy_puts_values_on_the_system_clipboard_and_paste_keeps_formulas_moving() {
    let (app, state) = state_with(&[("A1", "10"), ("B1", "=A1*2"), ("A2", "x\ty")]);
    select(&app, &state, "A1", "B2");
    app.invoke_copy_selection();
    assert_eq!(
        get_text().as_deref(),
        Some("10\t20\r\n\"x\ty\"\t"),
        "other programs get calculated values, quoted where needed"
    );

    // Pasting back inside Sheets keeps the formula and moves its references.
    select(&app, &state, "D5", "D5");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "D5"), "10");
    assert_eq!(
        raw(&state, "E5"),
        "=D5*2",
        "relative reference followed the paste"
    );
    assert_eq!(raw(&state, "D6"), "x\ty");

    // Absolute references stay put.
    state.current.borrow_mut().set_str("C1", "=$A$1+A1");
    select(&app, &state, "C1", "C1");
    app.invoke_copy_selection();
    select(&app, &state, "C10", "C10");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "C10"), "=$A$1+A10");
}

#[test]
fn paste_reads_tab_separated_text_from_other_programs() {
    let (app, state) = state_with(&[("A1", "mine")]);
    set_external_text_for_test(Some(
        "name\tqty\tprice\r\nApples\t3\t1.25\r\n\"Big, red\"\t2\t0.5\r\n",
    ));
    select(&app, &state, "B3", "B3");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "B3"), "name");
    assert_eq!(raw(&state, "D3"), "price");
    assert_eq!(raw(&state, "B4"), "Apples");
    assert_eq!(raw(&state, "C4"), "3");
    assert_eq!(raw(&state, "B5"), "Big, red");
    assert_eq!(
        raw(&state, "A1"),
        "mine",
        "nothing outside the pasted block changes"
    );

    // One undo removes the whole paste.
    app.invoke_undo();
    assert_eq!(raw(&state, "B3"), "");
    assert_eq!(raw(&state, "B5"), "");
}

#[test]
fn text_copied_elsewhere_after_a_copy_wins_over_the_private_copy() {
    let (app, state) = state_with(&[("A1", "from sheets")]);
    select(&app, &state, "A1", "A1");
    app.invoke_copy_selection();
    set_external_text_for_test(Some("from the browser"));
    select(&app, &state, "A3", "A3");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "A3"), "from the browser");
}

#[test]
fn one_cell_fills_a_selection_with_each_formula_copy_shifted() {
    let (app, state) = state_with(&[("A1", "5"), ("B1", "=A1*2")]);
    select(&app, &state, "B1", "B1");
    app.invoke_copy_selection();
    // Selected bottom-to-top: the fill still starts at the range's top-left.
    select(&app, &state, "B12", "B10");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "B10"), "=A10*2");
    assert_eq!(raw(&state, "B11"), "=A11*2");
    assert_eq!(raw(&state, "B12"), "=A12*2");
}

#[test]
fn cut_clears_the_cells_and_a_missing_clipboard_pastes_nothing() {
    let (app, state) = state_with(&[("A1", "gone")]);
    select(&app, &state, "A1", "A1");
    app.invoke_cut_selection();
    assert_eq!(raw(&state, "A1"), "");
    assert_eq!(get_text().as_deref(), Some("gone"));
    select(&app, &state, "A5", "A5");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "A5"), "gone", "cut then paste moves the cell");

    set_external_text_for_test(None);
    select(&app, &state, "A9", "A9");
    app.invoke_paste_selection();
    assert_eq!(
        raw(&state, "A9"),
        "gone",
        "with no system clipboard the private copy is used"
    );
}
