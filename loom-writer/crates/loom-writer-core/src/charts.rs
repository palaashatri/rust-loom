//! Embedded chart block support for Loom Writer.

use crate::{RichBlock, WriterDocument};
use loom_objects::chart::Chart;

/// Block kind for an embedded chart block in a Writer document.
pub const CHART_BLOCK_KIND: &str = "chart";

/// Deserializes a [`Chart`] from a chart block, if this block is a [`CHART_BLOCK_KIND`].
pub fn chart_from_block(block: &RichBlock) -> Option<Chart> {
    if block.kind != CHART_BLOCK_KIND {
        return None;
    }
    Chart::from_json(block.text.as_str()).ok()
}

/// Updates the chart model stored in a chart block.
pub fn update_chart_in_block(block: &mut RichBlock, chart: &Chart) -> bool {
    if block.kind != CHART_BLOCK_KIND {
        return false;
    }
    if let Ok(json) = chart.to_json() {
        block.text = crate::Text::from_str(&json);
        true
    } else {
        false
    }
}

impl WriterDocument {
    /// Inserts an embedded chart block at the given offset.
    pub fn insert_chart_block(&mut self, offset: usize, chart: Chart) -> Option<u64> {
        let json = chart.to_json().ok()?;
        let mut next_id = self.blocks.iter().map(|block| block.id).max().unwrap_or(0) + 1;
        while self.blocks.iter().any(|block| block.id == next_id) {
            next_id += 1;
        }
        let block = RichBlock::new(next_id, CHART_BLOCK_KIND, &json);
        let insert_at = crate::tables::block_insert_index(self, offset);
        self.blocks.insert(insert_at, block);
        Some(next_id)
    }

    /// Retrieves the chart model from the block with id `block_id`.
    pub fn chart_from_block(&self, block_id: u64) -> Option<Chart> {
        let block = self.blocks.iter().find(|b| b.id == block_id)?;
        chart_from_block(block)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_objects::chart::{ChartSeries, ChartType};

    #[test]
    fn test_insert_and_retrieve_chart_block() {
        let mut doc = WriterDocument::new("doc-chart", "Chart Doc");
        doc.push(RichBlock::new(1, "paragraph", "Introduction"));
        let chart = Chart::from_data(
            "writer-chart-1",
            Some("Quarterly Growth".to_string()),
            ChartType::Column,
            vec!["Q1".to_string(), "Q2".to_string()],
            vec![ChartSeries {
                name: "Revenue".to_string(),
                values: vec![50.0, 75.0],
                color: None,
            }],
        );

        let id = doc.insert_chart_block(0, chart.clone()).expect("inserted");
        assert_eq!(doc.blocks.len(), 2);
        let chart_block = &doc.blocks[1];
        assert_eq!(chart_block.id, id);
        assert_eq!(chart_block.kind, CHART_BLOCK_KIND);

        let retrieved = chart_from_block(chart_block).expect("retrieved");
        assert_eq!(retrieved.title.as_deref(), Some("Quarterly Growth"));
        assert_eq!(retrieved.series[0].values, vec![50.0, 75.0]);

        // In-place edit
        let mut modified = retrieved;
        assert!(modified.update_data_point(0, 0, 60.0));
        assert!(update_chart_in_block(&mut doc.blocks[1], &modified));

        let updated = chart_from_block(&doc.blocks[1]).unwrap();
        assert_eq!(updated.series[0].values[0], 60.0);
    }

    #[test]
    fn test_paste_sheets_chart_into_writer() {
        use loom_clipboard::ClipboardData;

        let chart = Chart::from_data(
            "sheets-chart-source",
            Some("Expense Distribution".to_string()),
            ChartType::Pie,
            vec![
                "R&D".to_string(),
                "Marketing".to_string(),
                "Ops".to_string(),
            ],
            vec![ChartSeries {
                name: "Amount".to_string(),
                values: vec![40.0, 35.0, 25.0],
                color: None,
            }],
        );

        let clip = ClipboardData::from_chart(&chart);
        let pasted_chart = clip.to_chart().expect("restored chart");

        let mut doc = WriterDocument::new("doc-paste", "Paste Doc");
        doc.push(RichBlock::new(1, "paragraph", "Overview"));
        let id = doc.insert_chart_block(0, pasted_chart).expect("inserted");
        assert_eq!(doc.blocks.len(), 2);
        assert_eq!(doc.blocks[1].id, id);

        let embedded = chart_from_block(&doc.blocks[1]).expect("embedded chart");
        assert_eq!(embedded.title.as_deref(), Some("Expense Distribution"));
        assert_eq!(embedded.chart_type, ChartType::Pie);
        assert_eq!(embedded.series[0].values, vec![40.0, 35.0, 25.0]);
    }
}
