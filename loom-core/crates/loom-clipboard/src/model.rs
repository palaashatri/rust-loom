//! Multi-format clipboard data container and priority-based deserializer.

use loom_objects::Table;
use serde::{Deserialize, Serialize};

/// Standard Loom Office MIME types for clipboard transfer.
pub const MIME_LOOM_OBJECTS: &str = "application/x-loom-objects+json";
pub const MIME_HTML: &str = "text/html";
pub const MIME_TSV: &str = "text/tab-separated-values";
pub const MIME_PLAIN_TEXT: &str = "text/plain";

/// Multi-format payload carried by the system clipboard.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ClipboardData {
    /// High-fidelity private JSON format preserving full Loom styles and spans.
    pub loom_objects_json: Option<String>,
    /// Rich HTML table with inline CSS for cross-application paste (Word, Excel, Browsers).
    pub html: Option<String>,
    /// Tab-separated plain text for spreadsheet grids and terminals.
    pub tsv: Option<String>,
    /// Fallback plain text representation.
    pub plain_text: Option<String>,
}

impl ClipboardData {
    /// Creates a new empty clipboard container.
    pub fn new() -> Self {
        Self::default()
    }

    /// Constructs a multi-format clipboard payload from a `Table`.
    pub fn from_table(table: &Table) -> Self {
        let loom_objects_json = table.to_json().ok();
        let html = Some(crate::html::table_to_html(table));
        let tsv = Some(crate::tsv::table_to_tsv(table));
        let plain_text = tsv.clone();

        Self {
            loom_objects_json,
            html,
            tsv,
            plain_text,
        }
    }

    /// Resolves a `Table` from clipboard data in priority order:
    /// 1. Private `application/x-loom-objects+json`
    /// 2. `text/html` table
    /// 3. `text/tab-separated-values`
    /// 4. `text/plain`
    pub fn to_table(&self) -> Option<Table> {
        // 1. Try private format
        if let Some(json) = &self.loom_objects_json {
            if let Ok(table) = Table::from_json(json) {
                return Some(table);
            }
        }

        // 2. Try HTML format
        if let Some(html) = &self.html {
            if let Some(table) = crate::html::html_to_table(html) {
                return Some(table);
            }
        }

        // 3. Try TSV format
        if let Some(tsv) = &self.tsv {
            if !tsv.trim().is_empty() {
                return Some(crate::tsv::tsv_to_table(tsv));
            }
        }

        // 4. Try plain text
        if let Some(text) = &self.plain_text {
            if !text.trim().is_empty() {
                return Some(crate::tsv::tsv_to_table(text));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_table_populates_all_formats() {
        let mut table = Table::new(2, 2);
        table.set_cell_text(0, 0, "Header A");
        table.set_cell_text(0, 1, "Header B");
        table.set_cell_text(1, 0, "Value 1");
        table.set_cell_text(1, 1, "Value 2");

        let clip = ClipboardData::from_table(&table);
        assert!(clip.loom_objects_json.is_some());
        assert!(clip.html.as_ref().unwrap().contains("<table"));
        assert!(clip.tsv.as_ref().unwrap().contains("Header A\tHeader B"));
        assert_eq!(clip.tsv, clip.plain_text);

        let resolved = clip.to_table().expect("resolves table");
        assert_eq!(resolved.cell_text(0, 0).as_deref(), Some("Header A"));
    }

    #[test]
    fn priority_reads_private_format_first() {
        let table_private = Table::new(1, 1);
        let mut clip = ClipboardData::from_table(&table_private);
        clip.tsv = Some("Override\tTSV".to_string());

        let loaded = clip.to_table().expect("table");
        assert_eq!(loaded.col_count(), 1); // private had 1 col, TSV had 2
    }

    #[test]
    fn fallback_to_tsv_when_private_and_html_absent() {
        let mut clip = ClipboardData::new();
        clip.tsv = Some("A\tB\nC\tD".to_string());

        let table = clip.to_table().expect("table");
        assert_eq!(table.row_count(), 2);
        assert_eq!(table.col_count(), 2);
        assert_eq!(table.cell_text(0, 0).as_deref(), Some("A"));
        assert_eq!(table.cell_text(1, 1).as_deref(), Some("D"));
    }
}
