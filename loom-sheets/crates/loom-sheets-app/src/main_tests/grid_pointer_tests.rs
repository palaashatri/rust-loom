use super::*;
use slint::Model;

pub(super) fn projected(cells: &[(&str, &str)]) -> (SheetsApp, Rc<GuiState>) {
    set_platform();
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
    app.on_cell_clicked({
        let state = state.clone();
        let app_ref = app.as_weak();
        move |r, c| {
            if crate::grid_pointer::take_swallowed_click() {
                return;
            }
            if let Some(app) = app_ref.upgrade() {
                select_cell(&app, &state, r, c);
                project_current(&app, &state);
            }
        }
    });
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    apply_headless_viewport_size(&app, 1280, 800);
    project_current(&app, &state);
    (app, state)
}

pub(super) fn selected(app: &SheetsApp) -> String {
    selection_from_app(app).label()
}

#[test]
fn pressing_and_dragging_across_cells_selects_the_rectangle_and_the_release_keeps_it() {
    let (app, _state) = projected(&[]);
    let pointer = app.global::<GridPointer>();
    let width = app.get_grid_column_widths().row_data(1).unwrap();
    let height = app.get_grid_row_heights().row_data(1).unwrap();

    // Press in B2, drag to the middle of D4 (two columns and rows over).
    pointer.invoke_cell_pressed(1, 1, false);
    pointer.invoke_cell_dragged(1, 1, width * 2.5, height * 2.5);
    assert_eq!(selected(&app), "B2:D4");

    // The click that ends a drag must not collapse it again.
    app.invoke_cell_clicked(1, 1);
    assert_eq!(selected(&app), "B2:D4");

    // The next plain click selects one cell as usual.
    app.invoke_cell_clicked(5, 5);
    assert_eq!(selected(&app), "F6");
}

#[test]
fn shift_press_extends_from_the_active_cell_and_swallows_its_click() {
    let (app, _state) = projected(&[]);
    app.invoke_cell_clicked(1, 1);
    app.global::<GridPointer>().invoke_cell_pressed(3, 2, true);
    assert_eq!(selected(&app), "B2:C4");
    app.invoke_cell_clicked(3, 2);
    assert_eq!(selected(&app), "B2:C4");
}

#[test]
fn header_clicks_select_whole_columns_and_rows() {
    let (app, _state) = projected(&[("C20", "x")]);
    let pointer = app.global::<GridPointer>();
    pointer.invoke_header_pressed(false, 2, false);
    assert_eq!(selected(&app), "C1:C20");
    pointer.invoke_header_pressed(false, 4, true);
    assert_eq!(selected(&app), "C1:E20");
    pointer.invoke_header_pressed(true, 3, false);
    assert_eq!(selected(&app), "A4:H4");
}

#[test]
fn dragging_a_column_edge_resizes_it_live_and_commits_one_undo_step() {
    let (app, state) = projected(&[("A1", "x")]);
    let pointer = app.global::<GridPointer>();
    let before = state.current.borrow().col_width(0);
    pointer.invoke_header_resized(false, 0, 40.0, false);
    assert_eq!(state.current.borrow().col_width(0), before + 40.0);
    assert!(
        state.undo_stack.borrow().is_empty(),
        "nothing is undoable until the drag ends"
    );
    pointer.invoke_header_resized(false, 0, 55.0, true);
    assert_eq!(state.current.borrow().col_width(0), before + 55.0);
    assert_eq!(state.undo_stack.borrow().len(), 1);

    app.invoke_undo();
    assert_eq!(state.current.borrow().col_width(0), before);
}

#[test]
fn resizing_is_clamped_and_a_zero_drag_changes_nothing() {
    let (app, state) = projected(&[]);
    let pointer = app.global::<GridPointer>();
    let row_before = state.current.borrow().row_height(2);
    pointer.invoke_header_resized(true, 2, -1_000.0, true);
    assert_eq!(state.current.borrow().row_height(2), 12.0);
    app.invoke_undo();
    assert_eq!(state.current.borrow().row_height(2), row_before);

    let col_before = state.current.borrow().col_width(1);
    pointer.invoke_header_resized(false, 1, 0.0, true);
    assert_eq!(state.current.borrow().col_width(1), col_before);
    assert!(state.undo_stack.borrow().is_empty());
}

#[test]
fn the_name_box_goes_to_a_far_cell_and_shows_it_and_rejects_nonsense() {
    let (app, _state) = projected(&[]);
    let pointer = app.global::<GridPointer>();
    pointer.invoke_goto_cell("d50".into());
    assert_eq!(selected(&app), "D50");
    assert!(
        app.get_view_row_origin() > 20,
        "the grid scrolled to show row 50"
    );

    pointer.invoke_goto_cell("B2:C3".into());
    assert_eq!(selected(&app), "B2:C3");

    pointer.invoke_goto_cell("nowhere".into());
    assert_eq!(
        selected(&app),
        "B2:C3",
        "an invalid reference changes nothing"
    );
    assert!(app.get_status_left().contains("not a cell or range"));
}

#[test]
fn hundred_thousand_hidden_rows_stay_out_of_the_materialized_cell_window_and_hit_testing_selects_visible_rows(
) {
    let (app, state) = projected(&[("A1", "Above"), ("B100002", "Below")]);
    {
        let mut sheet = state.current.borrow_mut();
        sheet.hidden_rows.extend(1..=100_000);
    }
    project_current(&app, &state);

    let row_count = app.get_rows().row_count();
    let col_count = app.get_cols().row_count();
    assert!(
        row_count <= 40,
        "expected materialized rows bounded by viewport, got {row_count}"
    );
    assert_eq!(app.get_cells().row_count(), row_count * col_count);
    assert_eq!(app.get_rows().row_data(0), Some(0));
    assert_eq!(app.get_rows().row_data(1), Some(100_001));
    assert_eq!(app.get_row_headers().row_data(0).unwrap().as_str(), "1");
    assert_eq!(
        app.get_row_headers().row_data(1).unwrap().as_str(),
        "100002"
    );

    let pointer = app.global::<GridPointer>();
    let width = app.get_grid_column_widths().row_data(0).unwrap();
    let height = app.get_grid_row_heights().row_data(0).unwrap();
    pointer.invoke_cell_pressed(0, 0, false);
    pointer.invoke_cell_dragged(0, 0, width * 1.5, height * 1.5);
    assert_eq!(selected(&app), "A1:B100002");
    assert_eq!(
        selection_from_app(&app).focus,
        CellRef {
            row: 100_001,
            col: 1
        }
    );

    // An all-hidden viewport span emits zero materialized rows and cells.
    let all_hidden_viewport = SheetViewport {
        first_row: 1,
        first_col: 0,
        visible_rows: 10,
        visible_cols: 4,
    };
    let sheet = state.current.borrow();
    assert!(materialized_rows(&sheet, all_hidden_viewport).is_empty());
    let grid = project_sheet_grid(&sheet, all_hidden_viewport);
    assert!(grid.rows.is_empty());
    assert!(grid.row_headers.is_empty());
    assert!(grid.cells.is_empty());
    assert!(grid.cell_labels.is_empty());
}
