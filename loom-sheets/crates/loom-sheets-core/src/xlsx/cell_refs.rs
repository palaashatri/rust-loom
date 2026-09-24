//! Convert XLSX cell references and column labels.

use crate::CellRef;

pub(super) fn parse_cell_ref(raw: &str) -> Option<CellRef> {
    CellRef::parse(raw.trim())
}

pub(super) fn column_index(letters: &str) -> u32 {
    letters
        .chars()
        .fold(0u32, |value, character| {
            value
                .saturating_mul(26)
                .saturating_add(character.to_ascii_uppercase() as u32 - 'A' as u32 + 1)
        })
        .saturating_sub(1)
}

pub(super) fn column_letters(mut col: usize) -> String {
    let mut letters = String::new();
    loop {
        letters.insert(0, (b'A' + (col % 26) as u8) as char);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    letters
}

pub(super) fn chart_sheet_reference(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}
