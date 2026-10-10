//! Whether Sort keeps the first row in place.
//!
//! The first row is a header only when it looks like one: it holds text and no
//! numbers while the row below holds numbers, or every filled cell in it is
//! bold. Any other first row is data and sorts with the rest.

use std::collections::HashMap;

use loom_sheets_core::{CellRef, Sheet, Value};

/// True when row 0 reads as column titles. `values` are the sheet's evaluated
/// results, so a formula that yields text counts as text.
pub(crate) fn first_row_is_header(sheet: &Sheet, values: &HashMap<CellRef, Value>) -> bool {
    let cols = sheet.dimensions().cols;
    let mut filled = 0usize;
    let mut text = 0usize;
    let mut numbers = 0usize;
    let mut all_bold = true;
    for col in 0..cols {
        let cell = CellRef { row: 0, col };
        if sheet.raw(cell).unwrap_or_default().is_empty() {
            continue;
        }
        filled += 1;
        match values.get(&cell) {
            Some(Value::Number(_)) => numbers += 1,
            Some(Value::Text(value)) if !value.is_empty() => text += 1,
            _ => {}
        }
        all_bold &= sheet.cell_style(cell).bold;
    }
    if filled == 0 {
        return false;
    }
    if all_bold {
        return true;
    }
    let number_below =
        (0..cols).any(|col| matches!(values.get(&CellRef { row: 1, col }), Some(Value::Number(_))));
    text > 0 && numbers == 0 && number_below
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_sheets_core::style::CellStyle;

    fn header_of(sheet: &Sheet) -> bool {
        first_row_is_header(sheet, &loom_sheets_core::evaluate(sheet))
    }

    fn table(rows: &[&[&str]]) -> Sheet {
        let mut sheet = Sheet::new("Data");
        for (row, cells) in rows.iter().enumerate() {
            for (col, raw) in cells.iter().enumerate() {
                sheet.set_str(&format!("{}{}", (b'A' + col as u8) as char, row + 1), raw);
            }
        }
        sheet
    }

    #[test]
    fn text_above_numbers_is_a_header() {
        let sheet = table(&[&["Item", "Price"], &["Apples", "3"], &["Pears", "5"]]);
        assert!(header_of(&sheet));
    }

    #[test]
    fn a_numeric_first_row_is_data() {
        let sheet = table(&[&["10", "3"], &["7", "5"], &["12", "1"]]);
        assert!(!header_of(&sheet));
    }

    #[test]
    fn a_text_first_row_over_text_rows_is_not_detected() {
        let sheet = table(&[&["Ben", "Oslo"], &["Ada", "Lima"]]);
        assert!(
            !header_of(&sheet),
            "no numbers below, so nothing marks a header"
        );
    }

    #[test]
    fn a_first_row_of_formulas_counts_by_its_results() {
        let sheet = table(&[&["=1+2", "=\"Total\""], &["4", "5"]]);
        assert!(
            !header_of(&sheet),
            "the first cell is numeric, so it is data"
        );
    }

    #[test]
    fn a_bold_first_row_is_a_header_without_numbers_below() {
        let mut sheet = table(&[&["Name", "City"], &["Ben", "Oslo"]]);
        for cell in ["A1", "B1"] {
            let cell = CellRef::parse(cell).unwrap();
            sheet.set_cell_style(
                cell,
                CellStyle {
                    bold: true,
                    ..CellStyle::default()
                },
            );
        }
        assert!(header_of(&sheet));
    }

    #[test]
    fn an_empty_first_row_is_not_a_header() {
        let sheet = table(&[&["", ""], &["Ben", "3"]]);
        assert!(!header_of(&sheet));
    }
}
