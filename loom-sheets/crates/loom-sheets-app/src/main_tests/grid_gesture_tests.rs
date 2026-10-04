use super::grid_pointer_tests::{projected, selected};
use super::*;
use crate::grid_gestures;
use slint::platform::{Key, WindowEvent};

const DATA: &[(&str, &str)] = &[
    ("B2", "x"),
    ("B3", "x"),
    ("B4", "x"),
    ("B7", "x"),
    ("A1", "Hello wide world"),
];

fn press_with(app: &SheetsApp, modifiers: &[Key], key: impl Into<slint::SharedString>) {
    app.invoke_focus_grid();
    for modifier in modifiers {
        app.window().dispatch_event(WindowEvent::KeyPressed {
            text: (*modifier).into(),
        });
    }
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: key.into() });
    for modifier in modifiers.iter().rev() {
        app.window().dispatch_event(WindowEvent::KeyReleased {
            text: (*modifier).into(),
        });
    }
}

#[test]
fn ctrl_arrow_jumps_to_the_data_edge_and_ctrl_shift_arrow_extends_to_it() {
    let (app, _state) = projected(DATA);
    app.invoke_cell_clicked(1, 1);
    assert_eq!(selected(&app), "B2");

    press_with(&app, &[Key::Control], Key::DownArrow);
    assert_eq!(selected(&app), "B4", "runs to the end of the filled block");
    press_with(&app, &[Key::Control], Key::DownArrow);
    assert_eq!(selected(&app), "B7", "then to the next filled cell");
    press_with(&app, &[Key::Control], Key::UpArrow);
    assert_eq!(selected(&app), "B4");

    press_with(&app, &[Key::Control, Key::Shift], Key::DownArrow);
    assert_eq!(selected(&app), "B4:B7", "Shift extends from the anchor");
    press_with(&app, &[Key::Control], Key::LeftArrow);
    assert_eq!(selected(&app), "A7", "no data to the left: column A");
}

#[test]
fn ctrl_a_and_the_corner_select_everything_used() {
    let (app, _state) = projected(DATA);
    app.invoke_cell_clicked(4, 4);
    press_with(&app, &[Key::Control], "a");
    assert_eq!(selected(&app), "A1:B7");

    app.invoke_cell_clicked(4, 4);
    assert_eq!(selected(&app), "E5");
    app.global::<GridPointer>().invoke_corner_pressed();
    assert_eq!(selected(&app), "A1:B7");
}

#[test]
fn ctrl_a_on_an_empty_sheet_selects_a1() {
    let (app, _state) = projected(&[]);
    app.global::<GridPointer>().invoke_corner_pressed();
    assert_eq!(selected(&app), "A1");
}

#[test]
fn double_clicking_a_column_edge_fits_the_widest_value_in_one_undo_step() {
    let (app, state) = projected(DATA);
    let pointer = app.global::<GridPointer>();
    let before = state.current.borrow().col_width(0);
    pointer.invoke_header_autofit(false, 0);
    let fitted = state.current.borrow().col_width(0);
    let expected = grid_gestures::fit_width(
        &state.current.borrow(),
        &evaluate(&state.current.borrow()),
        0,
    );
    assert_eq!(fitted, expected);
    assert!(fitted > before, "16 characters need more than the default");
    assert_eq!(state.undo_stack.borrow().len(), 1);

    app.invoke_undo();
    assert_eq!(state.current.borrow().col_width(0), before);
}

#[test]
fn autofit_is_clamped_and_an_empty_column_goes_back_to_the_default() {
    let mut sheet = Sheet::new("fit");
    sheet.set_str("A1", &"w".repeat(400));
    sheet.set_str("B1", "7");
    let values = evaluate(&sheet);
    assert_eq!(grid_gestures::fit_width(&sheet, &values, 0), 600.0);
    assert!(grid_gestures::fit_width(&sheet, &values, 1) >= 24.0);
    assert_eq!(
        grid_gestures::fit_width(&sheet, &values, 5),
        loom_sheets_core::DEFAULT_COL_WIDTH
    );
}

#[test]
fn double_clicking_a_row_edge_resets_its_height_undoably() {
    let (app, state) = projected(DATA);
    let pointer = app.global::<GridPointer>();
    let default = state.current.borrow().row_height(2);
    pointer.invoke_header_resized(true, 2, 40.0, true);
    assert_eq!(state.current.borrow().row_height(2), default + 40.0);

    pointer.invoke_header_autofit(true, 2);
    assert_eq!(state.current.borrow().row_height(2), default);
    app.invoke_undo();
    assert_eq!(state.current.borrow().row_height(2), default + 40.0);
}

#[test]
fn scroll_step_is_zero_inside_and_grows_with_overshoot_up_to_a_cap() {
    let body = (400.0, 300.0);
    assert_eq!(grid_gestures::scroll_step((10.0, 10.0), body), (0.0, 0.0));
    let (x, y) = grid_gestures::scroll_step((-5.0, 150.0), body);
    assert!(x < 0.0 && y == 0.0);
    let near = grid_gestures::scroll_step((0.0, 305.0), body).1;
    let far = grid_gestures::scroll_step((0.0, 600.0), body).1;
    assert!(near > 0.0 && far > near);
    assert_eq!(grid_gestures::scroll_step((0.0, 99_999.0), body).1, 48.0);
}

#[test]
fn dragging_past_the_bottom_edge_scrolls_and_extends_until_release() {
    let (app, state) = projected(&[]);
    let pointer = app.global::<GridPointer>();
    let width = app.get_grid_column_widths().row_data(1).unwrap();
    assert_eq!(app.get_grid_scroll_y(), 0.0);

    pointer.invoke_cell_pressed(1, 1, false);
    // Pointer far below the grid while the button stays down.
    pointer.invoke_cell_dragged(1, 1, width * 2.5, 5_000.0);
    let first_end = selection_from_app(&app).focus.row;
    for _ in 0..30 {
        grid_gestures::tick(&app, &state);
    }
    assert!(app.get_grid_scroll_y() < 0.0, "the grid scrolled down");
    assert!(app.get_view_row_origin() > 0);
    let selection = selection_from_app(&app);
    assert_eq!(selection.anchor, CellRef::parse("B2").unwrap());
    assert!(
        selection.focus.row > first_end,
        "the selection kept growing while scrolling"
    );

    pointer.invoke_drag_ended();
    let (scroll, focus) = (app.get_grid_scroll_y(), selection_from_app(&app).focus);
    for _ in 0..5 {
        grid_gestures::tick(&app, &state);
    }
    assert_eq!(
        app.get_grid_scroll_y(),
        scroll,
        "release stops the scrolling"
    );
    assert_eq!(selection_from_app(&app).focus, focus);
}

#[test]
fn a_drag_that_stays_inside_the_grid_does_not_scroll() {
    let (app, state) = projected(&[]);
    let pointer = app.global::<GridPointer>();
    pointer.invoke_cell_pressed(1, 1, false);
    pointer.invoke_cell_dragged(1, 1, 60.0, 40.0);
    for _ in 0..5 {
        grid_gestures::tick(&app, &state);
    }
    assert_eq!(app.get_grid_scroll_y(), 0.0);
    assert_eq!(app.get_grid_scroll_x(), 0.0);
    pointer.invoke_drag_ended();
}

#[test]
fn a_number_too_wide_for_its_column_is_never_shown_cut_off() {
    let wide = "12345678901234567890";
    let mut sheet = Sheet::new("wide");
    sheet.set_str("A1", wide);
    let values = evaluate(&sheet);
    let cell = |sheet: &Sheet| {
        let viewport = SheetViewport::from_scroll(
            0.0,
            0.0,
            1_024.0,
            720.0,
            24.0,
            80.0,
            SheetDimensions::new(4, 4),
        );
        project_sheet_grid_with_values(sheet, &values, viewport).cells[0].clone()
    };
    let narrow = cell(&sheet);
    assert!(narrow.contains("E+19"), "{narrow}");
    assert_ne!(narrow, "12345678");

    // The typed digits stay in the cell and autofit sizes to the shown number.
    assert_eq!(sheet.raw(CellRef { row: 0, col: 0 }), Some(wide));
    let width = grid_gestures::fit_width(&sheet, &values, 0);
    assert!(width > loom_sheets_core::DEFAULT_COL_WIDTH);
    sheet.set_col_width(0, width);
    // Like Excel, the shown digits are the 15 significant ones, zero padded.
    let full = cell(&sheet);
    assert_eq!(full, "12345678901234600000");
}
