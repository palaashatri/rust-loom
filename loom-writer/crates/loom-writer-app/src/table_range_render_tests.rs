//! Keyboard table-range selection must reach the visible cell overlay.
use super::*;

fn dispatch_shifted_key(app: &crate::WriterApp, key: slint::platform::Key) {
    let shift: slint::SharedString = slint::platform::Key::Shift.into();
    let key: slint::SharedString = key.into();
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: shift.clone(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.clone() });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: key });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: shift });
}

#[test]
fn keyboard_table_range_selection_reaches_the_painted_cell_overlay() {
    crate::table_actions::clear_table_cell_selection_range();
    let (app, state) = rendered_window(table_document(3, 3), 1280, 800, 1.0, 0.0);
    let table_id = 1;
    let before = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render before keyboard cell selection");
    for key in [
        slint::platform::Key::RightArrow,
        slint::platform::Key::DownArrow,
    ] {
        dispatch_shifted_key(&app, key);
    }
    assert_eq!(
        crate::table_actions::get_table_cell_selection_range(),
        Some(crate::table_actions::TableCellRange {
            block_id: table_id,
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 1,
        }),
        "Shift+Right then Shift+Down selects the A1:B2 cell range"
    );

    let published_cell_selected = |wanted: (usize, usize)| {
        let rects = app.get_table_cell_rects();
        let current = state.current.borrow();
        (0..rects.row_count()).find_map(|index| {
            let rect = rects.row_data(index)?;
            let location = usize::try_from(rect.caret_offset)
                .ok()
                .and_then(|offset| current.table_cell_at_offset(offset))?;
            (location.block_id == table_id && (location.row, location.col) == wanted)
                .then_some(rect.selected)
        })
    };
    for cell in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        assert_eq!(published_cell_selected(cell), Some(true), "cell {cell:?}");
    }
    assert_eq!(published_cell_selected((1, 2)), Some(false));

    let after = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0)
        .expect("render after keyboard cell selection");
    let origin = fresh_document_origin(&app);
    let sample_point = |cell| {
        let (x, y, width, height) =
            painted_cell_rect(&app, &state, table_id, cell).expect("cell rectangle is published");
        (
            (origin.0 + x + width * 0.8).round() as u32,
            (origin.1 + y + height * 0.75).round() as u32,
        )
    };
    let selected_pixel = sample_point((1, 1));
    let outside_pixel = sample_point((1, 2));
    assert!(selected_pixel.0 < before.width() && selected_pixel.1 < before.height());
    assert!(outside_pixel.0 < before.width() && outside_pixel.1 < before.height());
    assert_ne!(
        *before.get_pixel(selected_pixel.0, selected_pixel.1),
        *after.get_pixel(selected_pixel.0, selected_pixel.1),
        "the selected B2 interior changes when the range overlay is painted"
    );
    assert_eq!(
        *before.get_pixel(outside_pixel.0, outside_pixel.1),
        *after.get_pixel(outside_pixel.0, outside_pixel.1),
        "the outside C2 interior is not tinted by the selected range"
    );
    crate::table_actions::clear_table_cell_selection_range();
}

#[test]
fn keyboard_table_range_overlay_remains_visible_across_the_render_matrix() {
    for (width, height) in [(1024, 720), (1280, 800)] {
        crate::table_actions::clear_table_cell_selection_range();
        let (app, state) = rendered_window(table_document(3, 3), width, height, 1.0, 0.0);
        for theme in ["light", "dark", "high-contrast"] {
            crate::apply_theme(&app, theme);
            for scale in [1.0_f32, 1.5] {
                assert_range_overlay_pixels(&app, &state, width, height, scale, theme);
            }
        }
        crate::table_actions::clear_table_cell_selection_range();
        crate::refresh_render_projection_of(&app, &state);
    }
}

fn assert_range_overlay_pixels(
    app: &crate::WriterApp,
    state: &crate::GuiState,
    width: u32,
    height: u32,
    scale: f32,
    theme: &str,
) {
    crate::table_actions::clear_table_cell_selection_range();
    crate::refresh_render_projection_of(app, state);
    let before =
        loom_test_support::capture::snapshot_component(app, width as f32, height as f32, scale)
            .expect("render before keyboard cell selection");

    for key in [
        slint::platform::Key::RightArrow,
        slint::platform::Key::DownArrow,
    ] {
        dispatch_shifted_key(app, key);
    }

    let table_id = 1;
    let scenario = format!("{theme} {width}x{height} at scale {scale}");
    assert_eq!(
        crate::table_actions::get_table_cell_selection_range(),
        Some(crate::table_actions::TableCellRange {
            block_id: table_id,
            start_row: 0,
            start_col: 0,
            end_row: 1,
            end_col: 1,
        }),
        "Shift+Right then Shift+Down selects A1:B2 at {scenario}"
    );

    let published_cell_selected = |wanted: (usize, usize)| {
        let rects = app.get_table_cell_rects();
        let current = state.current.borrow();
        (0..rects.row_count()).find_map(|index| {
            let rect = rects.row_data(index)?;
            let location = usize::try_from(rect.caret_offset)
                .ok()
                .and_then(|offset| current.table_cell_at_offset(offset))?;
            (location.block_id == table_id && (location.row, location.col) == wanted)
                .then_some(rect.selected)
        })
    };
    for cell in [(0, 0), (0, 1), (1, 0), (1, 1)] {
        assert_eq!(
            published_cell_selected(cell),
            Some(true),
            "cell {cell:?} is selected at {scenario}"
        );
    }
    assert_eq!(
        published_cell_selected((1, 2)),
        Some(false),
        "C2 remains outside the range at {scenario}"
    );

    let after =
        loom_test_support::capture::snapshot_component(app, width as f32, height as f32, scale)
            .expect("render after keyboard cell selection");
    let origin = fresh_document_origin(app);
    let sample_point = |cell| {
        let (x, y, cell_width, cell_height) =
            painted_cell_rect(app, state, table_id, cell).expect("cell rectangle is published");
        (
            ((origin.0 + x + cell_width * 0.8) * scale).round() as u32,
            ((origin.1 + y + cell_height * 0.75) * scale).round() as u32,
        )
    };
    let selected_pixel = sample_point((1, 1));
    let outside_pixel = sample_point((1, 2));
    for (label, (x, y)) in [
        ("selected B2", selected_pixel),
        ("unselected C2", outside_pixel),
    ] {
        assert!(
            x < before.width() && y < before.height(),
            "{scenario}: the {label} sample ({x}, {y}) is inside the rendered window"
        );
    }
    assert_ne!(
        *before.get_pixel(selected_pixel.0, selected_pixel.1),
        *after.get_pixel(selected_pixel.0, selected_pixel.1),
        "{scenario}: selected B2's text-free, border-free interior changes when highlighted"
    );
    assert_eq!(
        *before.get_pixel(outside_pixel.0, outside_pixel.1),
        *after.get_pixel(outside_pixel.0, outside_pixel.1),
        "{scenario}: unselected C2's text-free, border-free interior remains untinted"
    );
    crate::table_actions::clear_table_cell_selection_range();
}
