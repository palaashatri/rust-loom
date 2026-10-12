use super::*;
use crate::system_clipboard::{
    get_clipboard_data, get_text, set_external_clipboard_for_test, set_external_text_for_test,
    set_write_fails_for_test,
};
use loom_sheets_core::style::{CellAlignment, CellStyle, FillColor};

pub(super) fn state_with(cells: &[(&str, &str)]) -> (SheetsApp, Rc<GuiState>) {
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

#[test]
fn an_empty_system_clipboard_read_after_copy_pastes_the_private_copy() {
    let (app, state) = state_with(&[("A1", "10"), ("B1", "=A1*2")]);
    select(&app, &state, "A1", "B1");
    app.invoke_copy_selection();
    // Some clipboard backends (X11 under WSLg) read back an empty string.
    set_external_text_for_test(Some(""));
    select(&app, &state, "D5", "D5");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "D5"), "10");
    assert_eq!(
        raw(&state, "E5"),
        "=D5*2",
        "the private copy keeps its formula"
    );
    assert_eq!(app.get_status_left().as_str(), "Pasted 2 cells at D5");
}

#[test]
fn a_failed_system_write_pastes_the_private_copy_not_stale_clipboard_text() {
    let (app, state) = state_with(&[("A1", "10"), ("B1", "=A1*2")]);
    set_external_text_for_test(Some("stale text from before"));
    set_write_fails_for_test(true);
    select(&app, &state, "A1", "B1");
    app.invoke_copy_selection();
    set_write_fails_for_test(false);
    select(&app, &state, "D5", "D5");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "D5"), "10");
    assert_eq!(raw(&state, "E5"), "=D5*2");
}

#[test]
fn copy_and_paste_status_use_the_singular_for_one_cell() {
    let (app, state) = state_with(&[("A1", "x")]);
    select(&app, &state, "A1", "A1");
    app.invoke_copy_selection();
    assert_eq!(app.get_status_left().as_str(), "Copied A1 (1 cell)");
    select(&app, &state, "C3", "C3");
    app.invoke_paste_selection();
    assert_eq!(app.get_status_left().as_str(), "Pasted 1 cell at C3");
}

#[test]
fn the_status_line_does_not_claim_offline_for_a_single_cell() {
    let (app, state) = state_with(&[("A1", "x")]);
    select(&app, &state, "A1", "A1");
    project_current(&app, &state);
    assert!(
        !app.get_status_right().contains("Offline"),
        "status line: {:?}",
        app.get_status_right()
    );
}

#[test]
fn copied_text_stays_on_the_system_clipboard_after_the_sheet_changes() {
    // On X11 the copied text is served by this process only while its clipboard
    // owner lives, so the owner is kept for the process. This test uses the
    // injected stand-in: it checks that later edits do not change what Copy gave
    // the system clipboard. The live X11 owner is checked separately.
    let (app, state) = state_with(&[("A1", "copied")]);
    select(&app, &state, "A1", "A1");
    app.invoke_copy_selection();
    state.current.borrow_mut().set_str("A1", "changed");
    assert_eq!(get_text().as_deref(), Some("copied"));
    select(&app, &state, "A1", "A1");
    app.invoke_cut_selection();
    assert_eq!(raw(&state, "A1"), "", "Cut clears the cell");
    assert_eq!(
        get_text().as_deref(),
        Some("changed"),
        "Cut replaces the clipboard with the cell it cleared"
    );
}

#[test]
fn sheets_copy_writes_multiformat_payload_including_html_and_loom_objects() {
    let (app, state) = state_with(&[("A1", "Header"), ("A2", "100"), ("B2", "=A2*2")]);
    {
        let mut sheet = state.current.borrow_mut();
        let a1 = CellRef::parse("A1").unwrap();
        sheet.set_cell_style(
            a1,
            CellStyle {
                bold: true,
                fill: FillColor::Blue,
                ..Default::default()
            },
        );
        sheet.set_cell_alignment(a1, CellAlignment::Center);
    }
    select(&app, &state, "A1", "B2");
    app.invoke_copy_selection();

    let clip = get_clipboard_data().expect("clipboard data must be populated");
    assert!(
        clip.loom_objects_json.is_some(),
        "must write application/x-loom-objects+json"
    );
    let table = clip.to_table().expect("must parse back to Table");
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.col_count(), 2);
    assert_eq!(table.cell_text(0, 0).as_deref(), Some("Header"));
    assert_eq!(table.cell_text(1, 1).as_deref(), Some("200")); // evaluated formula

    let html = clip.html.expect("must write text/html");
    assert!(html.contains("<table"), "HTML must contain <table>");
    assert!(html.contains("Header"), "HTML must contain cell text");
    assert!(
        html.contains("font-weight: bold") || html.contains("<b>"),
        "HTML must include bold formatting"
    );

    assert!(clip.tsv.is_some(), "must write TSV");
    assert!(clip.plain_text.is_some(), "must write plain text");
}

#[test]
fn sheets_paste_prefers_private_over_html_and_html_over_tsv() {
    let (app, state) = state_with(&[("A1", "original")]);

    // Scenario 1: Private loom objects JSON wins over HTML and TSV
    let mut table_private = loom_objects::Table::new(1, 1);
    table_private.set_cell_text(0, 0, "PrivateWinner");

    let mut clip = loom_clipboard::ClipboardData::from_table(&table_private);
    clip.html = Some("<table><tr><td>HtmlLoser</td></tr></table>".into());
    clip.tsv = Some("TsvLoser".into());
    clip.plain_text = Some("TsvLoser".into());

    set_external_clipboard_for_test(Some(clip));
    select(&app, &state, "A1", "A1");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "A1"), "PrivateWinner");

    // Scenario 2: HTML table wins over TSV when private is absent
    let clip_html_only = loom_clipboard::ClipboardData {
        html: Some("<table><tr><td>HtmlWinner</td></tr></table>".into()),
        tsv: Some("TsvLoser2".into()),
        plain_text: Some("TsvLoser2".into()),
        ..Default::default()
    };

    set_external_clipboard_for_test(Some(clip_html_only));
    select(&app, &state, "B1", "B1");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "B1"), "HtmlWinner");

    // Scenario 3: TSV used when private and HTML are absent
    let clip_tsv_only = loom_clipboard::ClipboardData {
        tsv: Some("TsvWinner".into()),
        plain_text: Some("TsvWinner".into()),
        ..Default::default()
    };

    set_external_clipboard_for_test(Some(clip_tsv_only));
    select(&app, &state, "C1", "C1");
    app.invoke_paste_selection();
    assert_eq!(raw(&state, "C1"), "TsvWinner");
}

#[test]
fn sheets_paste_parses_external_html_table() {
    let (app, state) = state_with(&[]);
    let external_html = r#"<table border="1">
        <tr><th>Product</th><th>Price</th></tr>
        <tr><td>Apples</td><td>1.99</td></tr>
        <tr><td>Oranges</td><td>2.49</td></tr>
    </table>"#;

    let clip = loom_clipboard::ClipboardData {
        html: Some(external_html.to_string()),
        plain_text: Some("Product\tPrice\nApples\t1.99\nOranges\t2.49".to_string()),
        ..Default::default()
    };

    set_external_clipboard_for_test(Some(clip));
    select(&app, &state, "A1", "A1");
    app.invoke_paste_selection();

    assert_eq!(raw(&state, "A1"), "Product");
    assert_eq!(raw(&state, "B1"), "Price");
    assert_eq!(raw(&state, "A2"), "Apples");
    assert_eq!(raw(&state, "B2"), "1.99");
    assert_eq!(raw(&state, "A3"), "Oranges");
    assert_eq!(raw(&state, "B3"), "2.49");
}
