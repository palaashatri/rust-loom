//! Multi-format clipboard crate for Loom Office suite.
//!
//! Provides simultaneous multi-format export (`application/x-loom-objects+json`,
//! `text/html`, `text/tab-separated-values`, `text/plain`) and prioritized import
//! across Sheets, Writer, and Present.

pub mod cf_html;
pub mod html;
pub mod model;
pub mod os;
pub mod picture;
pub mod sheets_interop;
pub mod tsv;

pub use cf_html::{decode_cf_html, encode_cf_html};
pub use html::{html_to_table, html_to_table_with_report, table_to_html, HtmlParseReport};
pub use model::{ClipboardData, MIME_HTML, MIME_LOOM_OBJECTS, MIME_PLAIN_TEXT, MIME_TSV};
pub use os::{
    read_clipboard, read_clipboard_image, rich_format_note, set_mock_clipboard_enabled,
    set_mock_clipboard_fails, set_mock_clipboard_image, write_clipboard, ClipboardOsError,
};
pub use sheets_interop::{sheets_range_to_table, table_to_sheets_range, SheetsCellData};
pub use tsv::{table_to_tsv, tsv_to_table};

#[cfg(test)]
mod tests {
    use super::*;
    use loom_objects::Table;

    #[test]
    fn full_clipboard_flow_test() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Suite");
        table.set_cell_text(0, 1, "Status");
        table.set_cell_text(1, 0, "Writer");
        table.set_cell_text(1, 1, "Ready");

        let clip = ClipboardData::from_table(&table);
        let restored = clip.to_table().expect("restored");
        assert_eq!(restored.cell_text(0, 0).as_deref(), Some("Suite"));
        assert_eq!(restored.cell_text(1, 1).as_deref(), Some("Ready"));
    }
}
