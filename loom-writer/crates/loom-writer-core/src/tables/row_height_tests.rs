//! A table's rows share one height, set by its tallest cell, so a cell that
//! holds a line break makes every row of the table taller. The layout and the
//! editor must agree on that height.

use crate::{PageStyle, RichBlock};
use loom_objects::Table;

fn block_with_cell(text: &str) -> RichBlock {
    let mut table = Table::new(1, 1);
    table.set_cell_text(0, 0, text);
    RichBlock::table_block(1, table)
}

#[test]
fn a_cell_with_a_line_break_makes_the_table_rows_taller() {
    let style = PageStyle::default();
    let one_line = crate::layout::text_line_height(&style, &block_with_cell("ab"));
    let two_lines = crate::layout::text_line_height(&style, &block_with_cell("ab\u{b}cd"));
    // The padding is counted once; the second line adds one body line.
    let body_line = style.body_font_size_pt * style.line_height;
    assert!(
        (two_lines - (one_line + body_line)).abs() < 0.01,
        "a second line adds one body line: {one_line} and {two_lines}"
    );
}

#[test]
fn a_table_without_breaks_keeps_its_one_line_row() {
    let style = PageStyle::default();
    let plain = crate::layout::text_line_height(&style, &block_with_cell("ab"));
    assert_eq!(plain, crate::table_row_height(&style, 1));
}

#[test]
fn a_cell_line_break_is_written_as_br_in_markdown_and_html() {
    let mut table = Table::new(1, 1);
    table.set_cell_text(0, 0, "ab\u{b}cd");
    let markdown = super::table_markdown(&table);
    assert!(markdown.contains("ab<br>cd"), "{markdown}");
    assert!(
        !markdown.contains('\u{b}'),
        "no raw vertical tab: {markdown:?}"
    );
    let html = super::table_html(&table);
    assert!(html.contains("ab<br>cd"), "{html}");
    assert!(!html.contains('\u{b}'), "no raw vertical tab: {html:?}");
}
