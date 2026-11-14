//! Text colour and wrapped text: set from the inspector, drawn by the grid,
//! undone as one step, and kept out of the overflow layer.
use super::keyboard_flow_tests::launched;
use super::*;
use i_slint_backend_testing::ElementHandle;

const A1: CellRef = CellRef { row: 0, col: 0 };

fn wired(cells: &[(&str, &str)]) -> keyboard_flow_tests::Session {
    let s = launched(cells);
    crate::ink_actions::wire(&s.app, &s.state, &s.menu);
    s
}

fn undo_last(s: &keyboard_flow_tests::Session) {
    let tx = s.state.undo_stack.borrow_mut().pop().expect("an undo step");
    tx.revert(&mut s.state.current.borrow_mut());
    apply_sheet(&s.app, &s.state);
}

#[test]
fn a_chosen_text_colour_is_stored_drawn_and_undone_as_one_step() {
    let s = wired(&[("A1", "hello")]);
    let ink = s.app.global::<CellInk>();
    ink.invoke_color_requested(3);
    assert_eq!(
        s.state.current.borrow().cell_style(A1).text_color,
        Some(0x0015_65C0)
    );
    assert_eq!(ink.get_colors().row_data(0), Some(0x0015_65C0));
    assert_eq!(ink.get_selected_slot(), 3, "the inspector shows the choice");
    assert_eq!(
        s.state.undo_stack.borrow().len(),
        1,
        "one step for one choice"
    );

    undo_last(&s);
    assert_eq!(s.state.current.borrow().cell_style(A1).text_color, None);
    assert_eq!(ink.get_colors().row_data(0), Some(-1));
}

#[test]
fn automatic_clears_the_colour_and_choosing_the_same_colour_twice_adds_no_step() {
    let s = wired(&[("A1", "hello")]);
    let ink = s.app.global::<CellInk>();
    ink.invoke_color_requested(0);
    ink.invoke_color_requested(0);
    assert_eq!(s.state.undo_stack.borrow().len(), 1);
    ink.invoke_color_requested(7);
    assert_eq!(s.state.current.borrow().cell_style(A1).text_color, None);
    assert_eq!(ink.get_selected_slot(), -1);
}

#[test]
fn wrapping_grows_the_row_it_needs_and_one_undo_puts_both_back() {
    let long = "a sentence long enough that it cannot fit on one line of a default column";
    let s = wired(&[("A1", long)]);
    let ink = s.app.global::<CellInk>();
    let default_row = s.state.current.borrow().row_height(0);
    ink.invoke_wrap_toggled();
    {
        let sheet = s.state.current.borrow();
        assert!(sheet.cell_style(A1).wrap);
        assert!(
            sheet.row_height(0) > default_row * 2.0,
            "the row grew to fit several lines: {}",
            sheet.row_height(0)
        );
    }
    assert_eq!(ink.get_wraps().row_data(0), Some(true));
    assert!(ink.get_selected_wrap());

    undo_last(&s);
    let sheet = s.state.current.borrow();
    assert!(!sheet.cell_style(A1).wrap);
    assert_eq!(sheet.row_height(0), default_row, "the row went back too");
}

#[test]
fn a_row_the_user_sized_by_hand_keeps_its_height_when_wrap_is_turned_on() {
    let long = "a sentence long enough that it cannot fit on one line of a default column";
    let s = wired(&[("A1", long)]);
    s.state.current.borrow_mut().set_row_height(0, 30.0);
    s.app.global::<CellInk>().invoke_wrap_toggled();
    assert_eq!(s.state.current.borrow().row_height(0), 30.0);
}

#[test]
fn a_wrapped_cell_stays_in_its_own_box_instead_of_spilling_into_neighbours() {
    let cells = vec![
        "a very long piece of text that would run on".to_string(),
        String::new(),
    ];
    let face = |wraps: &[bool]| {
        cell_spill::compute(
            &cell_spill::CellFace {
                cells: &cells,
                aligns: &[0, 0],
                bolds: &[false, false],
                italics: &[false, false],
                sizes: &[14, 14],
                inks: &[-1, -1],
                wraps,
            },
            2,
            &[80.0, 80.0],
            &[24.0],
            1.0,
        )
    };
    assert_eq!(
        face(&[false, false]).items.len(),
        1,
        "unwrapped text spills"
    );
    assert!(
        face(&[true, false]).items.is_empty(),
        "wrapped text does not"
    );
}

#[test]
fn the_inspector_offers_named_swatches_for_every_text_colour() {
    let s = wired(&[("A1", "hello")]);
    s.app.global::<CellInk>().invoke_wrap_toggled();
    s.app.global::<CellInk>().invoke_wrap_toggled();
    let _ = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render");
    for name in [
        "Text red",
        "Text orange",
        "Text green",
        "Text blue",
        "Text purple",
        "Text gray",
        "Text black",
        "Automatic text colour",
        "Wrap text",
    ] {
        assert!(
            ElementHandle::find_by_accessible_label(&s.app, name)
                .next()
                .is_some(),
            "no control is named {name:?}"
        );
    }
}

#[test]
fn wrapped_rows_keep_a_single_gridline_across_row_boundaries() {
    fn sample_strip(
        image: &image::RgbaImage,
        sample_x: u32,
        boundary_y: f32,
    ) -> (Vec<[u8; 4]>, usize) {
        let base_y = (boundary_y.floor() as i32 - 4).max(0) as u32;
        let strip: Vec<[u8; 4]> = (0..=8)
            .map(|dy| image.get_pixel(sample_x, base_y + dy).0)
            .collect();
        let bg = strip[0];
        let gridline_rows = strip.iter().filter(|&&px| px != bg).count();
        (strip, gridline_rows)
    }

    fn render_and_sample_row_boundary(
        text: &str,
        wrap: bool,
    ) -> (f32, Vec<[u8; 4]>, usize, Vec<[u8; 4]>, usize) {
        let s = wired(&[("A1", text), ("D5", "anchor")]);
        if wrap {
            s.app.global::<CellInk>().invoke_wrap_toggled();
        }
        // Move active cell away from row 0..2 boundaries so the selection ring
        // does not sit on the sampled boundary.
        s.app.invoke_navigate_selection(4, 3);
        let row_height = s.state.current.borrow().row_height(0);
        let image = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render");
        let row1 = ElementHandle::find_by_accessible_label(&s.app, "Row 1")
            .next()
            .expect("Row 1 header");
        let row2 = ElementHandle::find_by_accessible_label(&s.app, "Row 2")
            .next()
            .expect("Row 2 header");
        let row3 = ElementHandle::find_by_accessible_label(&s.app, "Row 3")
            .next()
            .expect("Row 3 header");
        let col_b = ElementHandle::find_by_accessible_label(&s.app, "Column B")
            .next()
            .expect("Column B header");
        let sample_x = (col_b.absolute_position().x + col_b.size().width * 0.5).round() as u32;
        let boundary_y1 = row2.absolute_position().y;
        let boundary_y2 = row3.absolute_position().y;
        assert!(
            (boundary_y1 - (row1.absolute_position().y + row_height)).abs() < 1.5,
            "Row 2 top ({boundary_y1}) should align with Row 1 height ({row_height})"
        );
        let (strip1, lines1) = sample_strip(&image, sample_x, boundary_y1);
        let (strip2, lines2) = sample_strip(&image, sample_x, boundary_y2);
        (row_height, strip1, lines1, strip2, lines2)
    }

    let (unwrapped_h, unwrapped_strip1, unwrapped_lines1, unwrapped_strip2, unwrapped_lines2) =
        render_and_sample_row_boundary("short", false);
    assert_eq!(unwrapped_h, 24.0);
    assert_eq!(
        unwrapped_lines1, 1,
        "unwrapped Row 1->2 boundary must draw a single 1px hairline: {unwrapped_strip1:?}"
    );
    assert_eq!(
        unwrapped_lines2, 1,
        "unwrapped Row 2->3 boundary must draw a single 1px hairline: {unwrapped_strip2:?}"
    );

    // Exercise fractional wrapped heights produced by `wrapped_row_heights`
    // (2 lines ~ 45.8px, 5 lines ~ 102.5px, 11 lines ~ 215.9px).
    for sample_text in [
        "two lines of text in one wrapped cell",
        "five wrapped lines of sample text inside a default width worksheet cell so the boundary falls on half a pixel",
        "eleven wrapped lines of sample text inside one cell to mirror the long wrapped case and confirm that growing the row never draws a doubled horizontal gridline beneath it at any line count",
    ] {
        let (wrapped_h, wrapped_strip1, wrapped_lines1, wrapped_strip2, wrapped_lines2) =
            render_and_sample_row_boundary(sample_text, true);
        assert!(
            wrapped_h > unwrapped_h,
            "wrapped row height {wrapped_h} should grow"
        );
        assert_eq!(
            wrapped_lines1, 1,
            "wrapped row height {wrapped_h} drew {wrapped_lines1} non-paper rows on Row 1->2 instead of 1 hairline: {wrapped_strip1:?}"
        );
        assert_eq!(
            wrapped_strip1, unwrapped_strip1,
            "wrapped row height {wrapped_h} changed Row 1->2 boundary pixels compared to unwrapped {unwrapped_h}: wrapped={wrapped_strip1:?}, unwrapped={unwrapped_strip1:?}"
        );
        assert_eq!(
            wrapped_lines2, 1,
            "wrapped row height {wrapped_h} drew {wrapped_lines2} non-paper rows on Row 2->3 instead of 1 hairline: {wrapped_strip2:?}"
        );
        assert_eq!(
            wrapped_strip2, unwrapped_strip2,
            "wrapped row height {wrapped_h} changed Row 2->3 boundary pixels compared to unwrapped {unwrapped_h}: wrapped={wrapped_strip2:?}, unwrapped={unwrapped_strip2:?}"
        );
    }
}
