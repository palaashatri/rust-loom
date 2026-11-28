//! The virtual grid stays materialized across Slint's animated wheel scrolls.

use super::*;
use i_slint_backend_testing::ElementHandle;

const WIDTH: u32 = 902;
const HEIGHT: u32 = 832;
const WHEEL_ANIMATION_FRAME_MS: u64 = 10;
const WHEEL_ANIMATION_MS: u64 = 180;
const WHEEL_SETTLE_MS: u64 = 50;
const INITIAL_SCROLL_ROW: f32 = 50.0;

fn scrollable_sheet() -> (SheetsApp, Rc<GuiState>) {
    i_slint_backend_testing::init_no_event_loop();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("Untitled");
    // Keep the test view far from the sheet's end so each real wheel delta is
    // accepted by Flickable instead of being clamped to the current tail.
    sheet.set_str("A1000", "tail");
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).expect("workbook filter"),
        FileFilter::new("CSV", ["csv"]).expect("CSV filter"),
        FileFilter::new("CSV", ["csv"]).expect("CSV export filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("Excel filter"),
    ));

    let app_weak = app.as_weak();
    let state_for_scroll = state.clone();
    app.on_grid_scrolled(move || {
        if let Some(app) = app_weak.upgrade() {
            scroll_projection::project_scroll(&app, &state_for_scroll);
            scroll_projection::settle_after_scroll(&app_weak, &state_for_scroll);
        }
    });

    app.window().set_size(PhysicalSize::new(WIDTH, HEIGHT));
    apply_layout_breakpoints(&app, WIDTH);
    apply_headless_viewport_size(&app, WIDTH, HEIGHT);
    project_current(&app, &state);
    // Project the complete sheet first: the bound Flickable clamps offsets
    // against its current content extent, which has not been published yet
    // during app construction.
    app.set_grid_scroll_y(-INITIAL_SCROLL_ROW * GRID_ROW_HEIGHT);
    scroll_projection::project_scroll(&app, &state);
    i_slint_backend_testing::testing_backend::mock_elapsed_time(0);
    assert!(
        app.get_view_row_origin() > 0,
        "test setup must begin below the first worksheet rows"
    );
    (app, state)
}

fn assert_projected_rows_cover_the_viewport(app: &SheetsApp, canvas: &ElementHandle, phase: &str) {
    let projected_top = app.get_grid_row_offset() + app.get_grid_scroll_y();
    let rows = app.get_rows();
    let row_heights = app.get_grid_row_heights();
    assert_eq!(
        rows.row_count(),
        row_heights.row_count(),
        "each projected worksheet row must have a materialized height during {phase}"
    );
    let projected_cell_height = (0..rows.row_count())
        .map(|row| {
            row_heights
                .row_data(row)
                .expect("projected worksheet row must have a height")
        })
        .sum::<f32>();
    let projected_bottom = projected_top + projected_cell_height;
    let viewport_bottom = canvas.size().height - GRID_COLUMN_HEADER_HEIGHT;
    assert!(
        projected_top <= 0.5,
        "the virtual grid leaves a blank strip below the pinned column headers during {phase}: projected top is {projected_top:.1}px (row offset {:.1}px, scroll {:.1}px)",
        app.get_grid_row_offset(),
        app.get_grid_scroll_y(),
    );
    assert!(
        projected_bottom + 0.5 >= viewport_bottom,
        "the projected worksheet cells leave a blank strip at the bottom during {phase}: cell bottom is {projected_bottom:.1}px, viewport bottom is {viewport_bottom:.1}px (visible cell height {projected_cell_height:.1}px, grid height {:.1}px)",
        app.get_grid_visible_height(),
    );
}

fn send_moved_wheel_event(app: &SheetsApp, canvas: &ElementHandle, delta_y: f32) {
    let origin = canvas.absolute_position();
    let size = canvas.size();
    let position =
        slint::LogicalPosition::new(origin.x + size.width / 2.0, origin.y + size.height / 2.0);
    let position = i_slint_core::lengths::logical_point_from_api(position);
    let result = slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
        .process_mouse_input(i_slint_core::input::MouseEvent::Wheel {
            position,
            delta_x: 0.0,
            delta_y: delta_y as i_slint_core::Coord,
            phase: i_slint_core::input::TouchPhase::Moved,
        });
    assert!(
        result.is_some_and(|result| result.accepted),
        "the worksheet Flickable must accept the moved wheel event"
    );
}

fn advance_wheel_animation(app: &SheetsApp, canvas: &ElementHandle, delta_y: f32, label: &str) {
    let previous_scroll = app.get_grid_scroll_y();
    // Inject at Slint's core input boundary after the Winit adapter. Keep the
    // raw Slint sign explicit: Flickable adds delta_y to viewport_y, so a
    // negative value moves grid_scroll_y more negative and reveals later rows.
    send_moved_wheel_event(app, canvas, delta_y);
    assert_projected_rows_cover_the_viewport(app, canvas, &format!("{label} at input"));

    let frames = WHEEL_ANIMATION_MS / WHEEL_ANIMATION_FRAME_MS;
    for frame in 1..=frames {
        i_slint_backend_testing::testing_backend::mock_elapsed_time(WHEEL_ANIMATION_FRAME_MS);
        assert_projected_rows_cover_the_viewport(
            app,
            canvas,
            &format!("{label} at {frame} × {WHEEL_ANIMATION_FRAME_MS} ms"),
        );
    }

    // The app also schedules one 50 ms projection settle after each wheel
    // event. This first fires at t=50 ms; advancing another 50 ms checks the
    // final view after the complete 180 ms wheel animation.
    i_slint_backend_testing::testing_backend::mock_elapsed_time(WHEEL_SETTLE_MS);

    assert!(
        if delta_y < 0.0 {
            app.get_grid_scroll_y() < previous_scroll
        } else {
            app.get_grid_scroll_y() > previous_scroll
        },
        "raw Slint delta {delta_y} must move the grid in the expected direction during {label}"
    );
    assert_projected_rows_cover_the_viewport(
        app,
        canvas,
        &format!("{label} 50 ms after animation"),
    );
}

#[test]
fn compact_grid_stays_covered_during_moved_wheel_animation_and_settle() {
    let (app, _state) = scrollable_sheet();
    let canvas = ElementHandle::find_by_accessible_label(&app, "Untitled cell canvas")
        .next()
        .expect("worksheet cell canvas");

    for (delta, label) in [
        (-2400.0, "first raw negative 2400 px sheet-down scroll"),
        (600.0, "raw positive 600 px sheet-up reverse scroll"),
        (-2400.0, "second raw negative 2400 px sheet-down scroll"),
    ] {
        advance_wheel_animation(&app, &canvas, delta, label);
    }
}

fn assert_projected_cols_cover_the_viewport(app: &SheetsApp, canvas: &ElementHandle, phase: &str) {
    let projected_left = app.get_grid_col_offset() + app.get_grid_scroll_x();
    let cols = app.get_cols();
    let col_widths = app.get_grid_column_widths();
    assert_eq!(
        cols.row_count(),
        col_widths.row_count(),
        "each projected worksheet column must have a materialized width during {phase}"
    );
    let projected_cell_width = (0..cols.row_count())
        .map(|col| {
            col_widths
                .row_data(col)
                .expect("projected worksheet column must have a width")
        })
        .sum::<f32>();
    let projected_right = projected_left + projected_cell_width;
    let viewport_right = canvas.size().width - GRID_ROW_HEADER_WIDTH;
    assert!(
        projected_left <= 0.5,
        "the virtual grid leaves a blank strip right of row headers during {phase}: projected left is {projected_left:.1}px"
    );
    assert!(
        projected_right + 0.5 >= viewport_right,
        "the projected worksheet cells leave a blank strip at the right during {phase}: cell right is {projected_right:.1}px, viewport right is {viewport_right:.1}px"
    );
}

#[test]
fn trackpad_horizontal_and_diagonal_wheel_scrolls_keep_columns_and_rows_covered() {
    let (app, state) = scrollable_sheet();
    // Extend sheet horizontally as well so horizontal wheel deltas are not clamped.
    state.current.borrow_mut().set_str("AZ1000", "far-right");
    project_current(&app, &state);
    app.set_grid_scroll_x(-10.0 * GRID_COL_WIDTH);
    app.set_grid_scroll_y(-INITIAL_SCROLL_ROW * GRID_ROW_HEIGHT);
    scroll_projection::project_scroll(&app, &state);
    i_slint_backend_testing::testing_backend::mock_elapsed_time(0);

    let canvas = ElementHandle::find_by_accessible_label(&app, "Untitled cell canvas")
        .next()
        .expect("worksheet cell canvas");
    let origin = canvas.absolute_position();
    let size = canvas.size();
    let position = i_slint_core::lengths::logical_point_from_api(slint::LogicalPosition::new(
        origin.x + size.width / 2.0,
        origin.y + size.height / 2.0,
    ));

    for (delta_x, delta_y, label) in [
        (-800.0f32, 0.0f32, "horizontal trackpad scroll right"),
        (320.0, 0.0, "horizontal trackpad scroll left"),
        (-480.0, -720.0, "diagonal 2-finger trackpad scroll"),
    ] {
        let prev_x = app.get_grid_scroll_x();
        let prev_y = app.get_grid_scroll_y();
        let result = slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
            .process_mouse_input(i_slint_core::input::MouseEvent::Wheel {
                position,
                delta_x: delta_x as i_slint_core::Coord,
                delta_y: delta_y as i_slint_core::Coord,
                phase: i_slint_core::input::TouchPhase::Moved,
            });
        assert!(
            result.is_some_and(|r| r.accepted),
            "Flickable must accept {label}"
        );
        for frame in 1..=(WHEEL_ANIMATION_MS / WHEEL_ANIMATION_FRAME_MS) {
            i_slint_backend_testing::testing_backend::mock_elapsed_time(WHEEL_ANIMATION_FRAME_MS);
            assert_projected_cols_cover_the_viewport(
                &app,
                &canvas,
                &format!("{label} frame {frame}"),
            );
            assert_projected_rows_cover_the_viewport(
                &app,
                &canvas,
                &format!("{label} frame {frame}"),
            );
        }
        i_slint_backend_testing::testing_backend::mock_elapsed_time(WHEEL_SETTLE_MS);
        if delta_x < 0.0 {
            assert!(app.get_grid_scroll_x() < prev_x, "{label} must advance x");
        } else if delta_x > 0.0 {
            assert!(app.get_grid_scroll_x() > prev_x, "{label} must reverse x");
        }
        if delta_y < 0.0 {
            assert!(app.get_grid_scroll_y() < prev_y, "{label} must advance y");
        }
        assert_projected_cols_cover_the_viewport(&app, &canvas, &format!("{label} settled"));
        assert_projected_rows_cover_the_viewport(&app, &canvas, &format!("{label} settled"));
    }
}
