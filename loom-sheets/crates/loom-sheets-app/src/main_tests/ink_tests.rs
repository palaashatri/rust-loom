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
fn inspect_wrapped_row_gridline_pixels() {
    let wrapped = "hello world";
    let s = wired(&[("A1", wrapped)]);
    s.app.global::<CellInk>().invoke_wrap_toggled();
    let image = snapshot_component(&s.app, 1280.0, 800.0, 1.0).expect("render wrapped row");
    let label = s.app.get_cell_labels().row_data(0).expect("A1 label");
    let a1 = ElementHandle::find_by_accessible_label(&s.app, label.as_str())
        .next()
        .expect("active A1 element");
    let origin = a1.absolute_position();
    let size = a1.size();
    let next_column_width = s.app.get_grid_column_widths().row_data(1).unwrap();
    let row_height = s.app.get_grid_row_heights().row_data(0).unwrap();
    let a1_center_x = (origin.x + size.width / 2.0).round() as u32;
    let next_center_x = (origin.x + size.width + next_column_width / 2.0).round() as u32;
    let boundary = (origin.y + row_height).round() as i32;
    let a1_pixels: Vec<_> = (-8..=8)
        .map(|offset| image.get_pixel(a1_center_x, (boundary + offset) as u32).0)
        .collect();
    let next_pixels: Vec<_> = (-8..=8)
        .map(|offset| image.get_pixel(next_center_x, (boundary + offset) as u32).0)
        .collect();
    eprintln!(
        "wrapped-row-gridline origin=({}, {}) size=({}, {}) row_height={row_height} boundary={boundary} a1_x={a1_center_x} a1_pixels={a1_pixels:?} next_x={next_center_x} next_pixels={next_pixels:?}",
        origin.x, origin.y, size.width, size.height
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
