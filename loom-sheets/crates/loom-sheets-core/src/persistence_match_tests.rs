use super::*;
use crate::{CellAlignment, CellRef, CellStyle, Sheet};

/// Two tabs with varied content. `variant` changes one cell, so the pair
/// differs; default styles and alignments are stored too, which the saved file
/// leaves out and the comparison must leave out as well.
fn workbook(seed: u32, variant: bool) -> Vec<Sheet> {
    let mut first = Sheet::new("Data");
    for index in 0..(60 + seed) {
        let row = (index * 7 + seed) % 90;
        let col = (index * 3) % 12;
        let raw = if index % 4 == 0 {
            format!("=A{}", index + 1)
        } else {
            index.to_string()
        };
        first.set_raw(CellRef { row, col }, raw);
    }
    first.set_cell_style(
        CellRef { row: 1, col: 1 },
        CellStyle {
            bold: true,
            ..CellStyle::default()
        },
    );
    first
        .styles
        .insert(CellRef { row: 2, col: 2 }, CellStyle::default());
    first.set_cell_alignment(CellRef { row: 3, col: 0 }, CellAlignment::Right);
    first
        .alignments
        .insert(CellRef { row: 4, col: 0 }, CellAlignment::General);
    first.col_widths.insert(2, 140.0);
    first.freeze_rows = seed % 2;
    if variant {
        first.set_raw(CellRef { row: 5, col: 5 }, "changed");
    }
    let mut second = Sheet::new("Other");
    second.set_raw(CellRef { row: 0, col: 0 }, "x");
    vec![first, second]
}

#[test]
fn content_match_agrees_with_comparing_the_saved_json() {
    for seed in 0..40 {
        let left = workbook(seed, false);
        let right = workbook(seed, seed % 3 == 0);
        let saved_equal = workbook_to_json(&left, 0) == workbook_to_json(&right, 0);
        assert_eq!(
            workbook_states_match(&left, 0, &right, 0),
            saved_equal,
            "seed {seed}"
        );
    }
}

#[test]
fn the_active_tab_is_part_of_the_content() {
    let sheets = workbook(3, false);
    assert!(workbook_states_match(&sheets, 0, &sheets, 0));
    assert!(!workbook_states_match(&sheets, 0, &sheets, 1));
    // An active index past the last tab is clamped, as the saved file does.
    assert!(workbook_states_match(&sheets, 1, &sheets, 9));
}

#[test]
fn undoing_back_to_the_same_cells_matches_the_saved_content() {
    let saved = workbook(5, false);
    let mut edited = saved.clone();
    let target = CellRef { row: 8, col: 8 };
    edited[0].set_raw(target, "temporary");
    assert!(!workbook_states_match(&edited, 0, &saved, 0));
    edited[0].clear_cell(target);
    assert!(workbook_states_match(&edited, 0, &saved, 0));
}
