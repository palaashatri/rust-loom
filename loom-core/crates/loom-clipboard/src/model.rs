use loom_objects::{Chart, Table};
use serde::{Deserialize, Serialize};

/// Standard Loom Office MIME types for clipboard transfer.
pub const MIME_LOOM_OBJECTS: &str = "application/x-loom-objects+json";
pub const MIME_HTML: &str = "text/html";
pub const MIME_TSV: &str = "text/tab-separated-values";
pub const MIME_PLAIN_TEXT: &str = "text/plain";

/// Resolved content type from clipboard data.
#[derive(Debug, Clone, PartialEq)]
pub enum ClipboardContent {
    Table(Table),
    Chart(Box<Chart>),
    Text(String),
}

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

    /// Constructs a multi-format clipboard payload from a `Chart`.
    pub fn from_chart(chart: &Chart) -> Self {
        let loom_objects_json = chart.to_json().ok();
        let mut tsv = String::new();
        tsv.push_str("Category");
        for s in &chart.series {
            tsv.push('\t');
            tsv.push_str(&s.name);
        }
        tsv.push('\n');
        for (c_idx, cat) in chart.categories.iter().enumerate() {
            tsv.push_str(cat);
            for s in &chart.series {
                tsv.push('\t');
                if let Some(val) = s.values.get(c_idx) {
                    tsv.push_str(&val.to_string());
                } else {
                    tsv.push('0');
                }
            }
            tsv.push('\n');
        }

        let plain_text = Some(tsv.clone());
        let html = Some(format!(
            "<!-- loom-chart -->\n<h3>{}</h3>\n<pre>{}</pre>",
            chart.title.as_deref().unwrap_or("Chart"),
            tsv
        ));

        Self {
            loom_objects_json,
            html,
            tsv: Some(tsv),
            plain_text,
        }
    }

    /// Resolves a `Chart` from clipboard data in priority order.
    pub fn to_chart(&self) -> Option<Chart> {
        if let Some(json) = &self.loom_objects_json {
            if let Ok(chart) = Chart::from_json(json) {
                return Some(chart);
            }
        }
        None
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

    /// Resolves the clipboard data to the most specific available content object.
    pub fn content(&self) -> Option<ClipboardContent> {
        if let Some(chart) = self.to_chart() {
            return Some(ClipboardContent::Chart(Box::new(chart)));
        }
        if let Some(table) = self.to_table() {
            return Some(ClipboardContent::Table(table));
        }
        self.plain_text
            .as_ref()
            .map(|t| ClipboardContent::Text(t.clone()))
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

    #[test]
    fn from_chart_populates_all_formats_and_resolves_back() {
        use loom_objects::{ChartSeries, ChartType};

        let chart = Chart::from_data(
            "chart-clip-1",
            Some("Q1 Summary".to_string()),
            ChartType::Bar,
            vec!["Apples".to_string(), "Oranges".to_string()],
            vec![ChartSeries {
                name: "Count".to_string(),
                values: vec![40.0, 65.0],
                color: None,
            }],
        );

        let clip = ClipboardData::from_chart(&chart);
        assert!(clip.loom_objects_json.is_some());
        assert!(clip.tsv.as_ref().unwrap().contains("Apples\t40"));
        assert!(clip.html.as_ref().unwrap().contains("Q1 Summary"));

        let restored_chart = clip.to_chart().expect("restores chart");
        assert_eq!(restored_chart.id, "chart-clip-1");
        assert_eq!(restored_chart.chart_type, ChartType::Bar);
        assert_eq!(restored_chart.series[0].values, vec![40.0, 65.0]);

        match clip.content() {
            Some(ClipboardContent::Chart(c)) => assert_eq!(c.id, "chart-clip-1"),
            _ => panic!("expected Chart clipboard content"),
        }
    }
}
