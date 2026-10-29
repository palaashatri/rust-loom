//! Embedded chart block support for Loom Writer.
//!
//! A chart is a block of kind [`CHART_BLOCK_KIND`] whose text is one object
//! character plus a private-use marker, like a picture; the chart's JSON rides
//! on the block ([`RichBlock::chart`]), so the editor text, find, word counts
//! and the Markdown export never see it.

use crate::{RichBlock, WriterDocument};
use loom_objects::chart::{Chart, ChartType};
use std::sync::Arc;

/// Block kind for an embedded chart block in a Writer document.
pub const CHART_BLOCK_KIND: &str = "chart";
/// Height of a chart on the page, in points.
pub const CHART_HEIGHT_PT: f32 = 200.0;
/// Widest a chart is drawn, in points.
pub const CHART_MAX_WIDTH_PT: f32 = 460.0;

/// Numeric X values and the validated visible range for a Writer Scatter chart.
#[derive(Debug, Clone, PartialEq)]
pub struct ScatterAxis {
    pub values: Vec<f64>,
    pub min: f64,
    pub max: f64,
}

/// Uses the same category parser and range rules for Writer's SVG, PDF, and DOCX
/// Scatter projections. The categories argument is the number of leading X values to plot.
pub fn writer_scatter_axis(
    chart: &Chart,
    categories: usize,
) -> Result<Option<ScatterAxis>, String> {
    if chart.chart_type != ChartType::Scatter {
        return Ok(None);
    }
    let values = chart
        .categories
        .iter()
        .take(categories)
        .map(|category| {
            numeric_category_value(category)
                .ok_or_else(|| "scatter charts require numeric X values".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.is_empty() {
        return Err("scatter charts require numeric X values".to_string());
    }
    let data_min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let data_max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let axis_min = chart
        .category_axis
        .min
        .filter(|value| value.is_finite() && value.abs() <= 1.0e100);
    let axis_max = chart
        .category_axis
        .max
        .filter(|value| value.is_finite() && value.abs() <= 1.0e100);
    let mut min = axis_min.unwrap_or(data_min);
    let mut max = axis_max.unwrap_or(data_max);
    if !min.is_finite() || !max.is_finite() || max <= min {
        if axis_min.is_some() || axis_max.is_some() {
            return Err("scatter X-axis minimum must be below its maximum".to_string());
        }
        let padding = min.abs().max(1.0) * 0.1;
        min -= padding;
        max += padding;
    } else {
        let padding = (max - min) * 0.05;
        if axis_min.is_none() {
            min -= padding;
        }
        if axis_max.is_none() {
            max += padding;
        }
    }
    Ok(Some(ScatterAxis { values, min, max }))
}

fn numeric_category_value(raw: &str) -> Option<f64> {
    let raw = raw.trim();
    let raw = raw.strip_prefix('$').unwrap_or(raw).trim();
    let text = raw.strip_suffix('%').unwrap_or(raw).trim();
    if text.contains(',') {
        let integer_end = text.find(['.', 'e', 'E']).unwrap_or(text.len());
        if text[integer_end..].contains(',') {
            return None;
        }
        let integer = text[..integer_end].trim_start_matches(['+', '-']);
        let groups: Vec<_> = integer.split(',').collect();
        if groups.is_empty()
            || !(1..=3).contains(&groups[0].len())
            || !groups[0].bytes().all(|byte| byte.is_ascii_digit())
            || groups
                .iter()
                .skip(1)
                .any(|group| group.len() != 3 || !group.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return None;
        }
    }
    text.replace(',', "")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite() && value.abs() <= 1.0e100)
}

/// Builds Writer's chart part, converting Scatter categories into numeric X values.
pub(crate) fn writer_chart_to_c_chart_xml(chart: &Chart) -> Result<String, String> {
    if chart.chart_type != ChartType::Scatter {
        return Ok(loom_objects::chart_ooxml::chart_to_c_chart_xml(chart));
    }

    let axis = writer_scatter_axis(chart, chart.categories.len())?
        .ok_or_else(|| "scatter chart has no numeric X axis".to_string())?;
    let mut numeric_chart = chart.clone();
    numeric_chart.categories = axis.values.iter().map(ToString::to_string).collect();
    let mut xml = loom_objects::chart_ooxml::chart_to_c_chart_xml(&numeric_chart);

    xml = xml.replacen(
        "<c:scatterChart>",
        "<c:scatterChart>\n        <c:scatterStyle val=\"marker\"/>",
        1,
    );
    xml = xml
        .replace("<c:cat>", "<c:xVal>")
        .replace("</c:cat>", "</c:xVal>")
        .replace(
            "<c:strLit>\n",
            "<c:numLit>\n              <c:formatCode>General</c:formatCode>\n",
        )
        .replace("</c:strLit>", "</c:numLit>")
        .replace("<c:val>", "<c:yVal>")
        .replace("</c:val>", "</c:yVal>")
        .replace("<c:catAx>", "<c:valAx>")
        .replace("</c:catAx>", "</c:valAx>");

    let x_scaling = chart_axis_scaling(Some(axis.min), Some(axis.max));
    let y_scaling = chart_axis_scaling(chart.value_axis.min, chart.value_axis.max);
    let default_scaling = "<c:scaling><c:orientation val=\"minMax\"/></c:scaling>";
    xml = xml.replacen(default_scaling, &x_scaling, 1);
    xml = xml.replacen(default_scaling, &y_scaling, 1);
    Ok(xml)
}

fn chart_axis_scaling(min: Option<f64>, max: Option<f64>) -> String {
    let min = min.filter(|value| value.is_finite() && value.abs() <= 1.0e100);
    let max = max.filter(|value| value.is_finite() && value.abs() <= 1.0e100);
    let mut xml = String::from("<c:scaling><c:orientation val=\"minMax\"/>");
    if let Some(max) = max {
        xml.push_str(&format!("<c:max val=\"{max}\"/>"));
    }
    if let Some(min) = min {
        xml.push_str(&format!("<c:min val=\"{min}\"/>"));
    }
    xml.push_str("</c:scaling>");
    xml
}

/// Deserializes a [`Chart`] from a chart block, if this block is a [`CHART_BLOCK_KIND`].
pub fn chart_from_block(block: &RichBlock) -> Option<Chart> {
    if block.kind != CHART_BLOCK_KIND {
        return None;
    }
    Chart::from_json(block.chart.as_deref()?).ok()
}

/// Updates the chart model stored in a chart block.
pub fn update_chart_in_block(block: &mut RichBlock, chart: &Chart) -> bool {
    if block.kind != CHART_BLOCK_KIND {
        return false;
    }
    match chart.to_json() {
        Ok(json) => {
            block.chart = Some(Arc::new(json));
            true
        }
        Err(_) => false,
    }
}

/// Gives a block read from a file its chart: the saved `chart` field, or, for a
/// document saved before charts had a field of their own, the JSON that was in
/// the block's text (which is replaced by the object text).
pub(crate) fn attach_chart(block: &mut RichBlock, saved: Option<String>) {
    if block.kind != CHART_BLOCK_KIND {
        return;
    }
    let json = saved.or_else(|| {
        Chart::from_json(block.text.as_str())
            .ok()
            .map(|_| block.text.as_str().to_string())
    });
    if let Some(json) = json {
        block.chart = Some(Arc::new(json));
        if !block.text.as_str().starts_with(crate::OBJECT_CHAR) {
            block.text = crate::Text::from_str(&crate::images::picture_text(block.id as u32));
        }
    }
}

impl RichBlock {
    /// A chart block with the given id, object text and chart JSON.
    pub(crate) fn chart_block(id: u64, object_text: &str, chart_json: String) -> Self {
        let mut block = RichBlock::new(id, CHART_BLOCK_KIND, object_text);
        block.chart = Some(Arc::new(chart_json));
        block
    }

    /// Whether this block is an intact chart: kind, object text and payload agree.
    pub fn is_chart(&self) -> bool {
        self.kind == CHART_BLOCK_KIND
            && self.chart.is_some()
            && self.text.as_str().starts_with(crate::OBJECT_CHAR)
    }

    /// The size a chart is drawn at in a text area of the given width.
    pub fn chart_display_size(&self, usable_width: f32) -> Option<(f32, f32)> {
        self.is_chart()
            .then(|| (usable_width.clamp(1.0, CHART_MAX_WIDTH_PT), CHART_HEIGHT_PT))
    }

    /// The size an object block (picture or chart) takes on the page; `None`
    /// for text blocks.
    pub fn object_display_size(&self, usable_width: f32, usable_height: f32) -> Option<(f32, f32)> {
        self.image_display_size(usable_width, usable_height)
            .or_else(|| self.chart_display_size(usable_width))
    }

    /// What an object block reads as where only text can go (Markdown, HTML):
    /// its kind and name in brackets; `None` for every other block.
    pub fn object_placeholder(&self) -> Option<String> {
        if let Some(image) = self.image_placeholder() {
            return Some(image);
        }
        let chart = chart_from_block(self).filter(|_| self.is_chart())?;
        Some(format!(
            "[Chart: {}]",
            chart.title.as_deref().unwrap_or("untitled")
        ))
    }
}

impl WriterDocument {
    /// Inserts an embedded chart block at the given offset.
    pub fn insert_chart_block(&mut self, offset: usize, chart: Chart) -> Option<u64> {
        let json = chart.to_json().ok()?;
        let text = self.new_object_text()?;
        let mut next_id = self.blocks.iter().map(|block| block.id).max().unwrap_or(0) + 1;
        while self.blocks.iter().any(|block| block.id == next_id) {
            next_id += 1;
        }
        let block = RichBlock::chart_block(next_id, &text, json);
        let insert_at = crate::tables::block_insert_index(self, offset);
        self.blocks.insert(insert_at, block);
        if insert_at + 1 == self.blocks.len() {
            self.blocks
                .push(RichBlock::new(next_id + 1, "paragraph", ""));
        }
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
        // The paragraph, the chart, and an empty paragraph to type in below it.
        assert_eq!(doc.blocks.len(), 3);
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
        // The paragraph, the chart, and an empty paragraph to type in below it.
        assert_eq!(doc.blocks.len(), 3);
        assert_eq!(doc.blocks[1].id, id);

        let embedded = chart_from_block(&doc.blocks[1]).expect("embedded chart");
        assert_eq!(embedded.title.as_deref(), Some("Expense Distribution"));
        assert_eq!(embedded.chart_type, ChartType::Pie);
        assert_eq!(embedded.series[0].values, vec![40.0, 35.0, 25.0]);
    }

    #[test]
    fn writer_scatter_ooxml_uses_numeric_x_y_values_and_axis_bounds() {
        let mut chart = Chart::new("scatter-docx", ChartType::Scatter);
        chart.categories = vec!["$1,000".into(), "2".into(), "10".into()];
        chart.add_series("Samples", vec![3.0, 4.0, 5.0], None);
        chart.category_axis.min = Some(0.0);
        chart.category_axis.max = Some(20.0);
        chart.value_axis.min = Some(-5.0);
        chart.value_axis.max = Some(15.0);

        let xml = writer_chart_to_c_chart_xml(&chart).expect("numeric Scatter export");
        assert!(xml.contains("<c:scatterStyle val=\"marker\"/>"));
        assert!(xml.contains("<c:xVal>"));
        assert!(xml.contains("<c:yVal>"));
        assert!(xml.contains("<c:xVal>\n            <c:numLit>"));
        assert!(xml.contains("<c:pt idx=\"0\"><c:v>1000</c:v></c:pt>"));
        assert!(xml.contains("<c:pt idx=\"2\"><c:v>10</c:v></c:pt>"));
        assert!(xml.contains("<c:yVal>\n            <c:numLit>"));
        assert!(xml.contains("<c:pt idx=\"1\"><c:v>4</c:v></c:pt>"));
        assert!(!xml.contains("<c:cat>"));
        assert!(!xml.contains("<c:catAx>"));
        assert_eq!(xml.matches("<c:valAx>").count(), 2);
        assert!(xml.contains("<c:max val=\"20\"/><c:min val=\"0\"/>"));
        assert!(xml.contains("<c:max val=\"15\"/><c:min val=\"-5\"/>"));
    }

    #[test]
    fn writer_scatter_ooxml_rejects_non_numeric_x_values() {
        let mut chart = Chart::new("scatter-invalid-docx", ChartType::Scatter);
        chart.categories = vec!["0".into(), "north".into()];
        chart.add_series("Samples", vec![3.0, 4.0], None);

        let error = writer_chart_to_c_chart_xml(&chart).expect_err("invalid X values fail export");
        assert!(error.contains("numeric X values"));
    }
}

#[cfg(test)]
mod object_text_tests {
    use super::*;
    use loom_objects::chart::{ChartSeries, ChartType};

    fn chart() -> Chart {
        Chart::from_data(
            "c1",
            Some("Quarterly Growth".to_string()),
            ChartType::Column,
            vec!["Q1".to_string(), "Q2".to_string()],
            vec![ChartSeries {
                name: "Revenue".to_string(),
                values: vec![50.0, 75.0],
                color: None,
            }],
        )
    }

    fn doc_with_chart() -> (WriterDocument, u64) {
        let mut doc = WriterDocument::new("d", "D");
        doc.push(RichBlock::new(1, "paragraph", "before"));
        doc.push(RichBlock::new(2, "paragraph", "after"));
        let id = doc.insert_chart_block(6, chart()).unwrap();
        (doc, id)
    }

    #[test]
    fn the_chart_json_is_not_in_the_editor_text_find_or_word_count() {
        let (doc, _) = doc_with_chart();
        let text = doc.editor_text();
        assert!(
            !text.contains("categories") && !text.contains("Revenue"),
            "{text:?}"
        );
        assert!(doc.blocks.iter().any(RichBlock::is_chart));
        assert_eq!(
            doc.estimate_pagination().words,
            2,
            "only `before` and `after`"
        );
    }

    #[test]
    fn a_chart_reads_as_its_title_in_markdown_and_html_and_survives_save_and_reopen() {
        let (doc, id) = doc_with_chart();
        assert!(doc.to_markdown().contains("[Chart: Quarterly Growth]"));
        assert!(doc
            .to_html_string()
            .contains("<p>[Chart: Quarterly Growth]</p>"));
        let back = WriterDocument::from_content_json(&doc.to_content_json()).unwrap();
        let block = back.blocks.iter().find(|b| b.id == id).unwrap();
        assert!(block.is_chart());
        let chart = chart_from_block(block).unwrap();
        assert_eq!(chart.title.as_deref(), Some("Quarterly Growth"));
        assert_eq!(chart.series[0].values, vec![50.0, 75.0]);
    }

    #[test]
    fn a_document_saved_with_the_json_in_the_text_still_opens_as_a_chart() {
        let json = chart().to_json().unwrap();
        let mut old = WriterDocument::new("old", "Old");
        old.push(RichBlock::new(1, "paragraph", "intro"));
        old.push(RichBlock::new(2, CHART_BLOCK_KIND, &json));
        let reopened = WriterDocument::from_content_json(&old.to_content_json()).unwrap();
        let block = reopened
            .blocks
            .iter()
            .find(|b| b.kind == CHART_BLOCK_KIND)
            .unwrap();
        assert!(block.is_chart(), "the old block became a chart object");
        assert!(!block.text.as_str().contains("categories"));
        assert_eq!(
            chart_from_block(block).unwrap().title.as_deref(),
            Some("Quarterly Growth")
        );
    }

    #[test]
    fn typing_in_another_paragraph_keeps_the_chart_and_the_chart_line_is_200_points_tall() {
        let (mut doc, id) = doc_with_chart();
        let mut text = doc.editor_text();
        text.insert_str(2, "XY");
        doc.replace_editor_text_at(&text, Some(4)).unwrap();
        assert!(doc.blocks.iter().find(|b| b.id == id).unwrap().is_chart());
        let style = crate::PageStyle::default();
        let viewport = crate::PageViewport {
            width: 900.0,
            height: 700.0,
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        };
        let flow = doc.flow(&style, viewport).unwrap();
        let line = flow
            .lines()
            .iter()
            .find(|l| doc.blocks[l.block_index].is_chart())
            .expect("the chart has a line");
        assert!(
            (line.bounds.height - CHART_HEIGHT_PT).abs() < 0.01,
            "{}",
            line.bounds.height
        );
    }
}
