//! Slide chart object support and PPTX DrawingML chart interop.

use crate::{ElementType, SlideElement, SLIDE_HEIGHT, SLIDE_WIDTH};
use loom_objects::chart::Chart;
use loom_objects::chart_ooxml::c_chart_xml_to_chart;

const EMU_PER_UNIT: f64 = 12_192.0;

impl SlideElement {
    /// Constructs a new chart element positioned on the slide canvas.
    pub fn new_chart(
        id: impl Into<String>,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        chart: Chart,
    ) -> Self {
        let content = serde_json::to_string(&chart).unwrap_or_default();
        Self {
            id: id.into(),
            element_type: ElementType::Chart,
            content,
            x,
            y,
            width,
            height,
            rotation_deg: 0.0,
            action: None,
            style: Default::default(),
        }
    }

    /// Deserializes the chart object if this element is an [`ElementType::Chart`].
    pub fn chart(&self) -> Option<Chart> {
        if self.element_type != ElementType::Chart {
            return None;
        }
        serde_json::from_str(&self.content).ok()
    }

    /// Updates the chart model stored in this element.
    pub fn set_chart(&mut self, chart: &Chart) {
        if self.element_type == ElementType::Chart {
            if let Ok(json) = serde_json::to_string(chart) {
                self.content = json;
            }
        }
    }
}

/// Creates a new slide chart element from a chart model, centered on the slide.
pub fn chart_element_from_clipboard(chart: Chart, id: impl Into<String>) -> SlideElement {
    let width = 500.0f32.min(SLIDE_WIDTH - 80.0);
    let height = 300.0f32.min(SLIDE_HEIGHT - 80.0);
    let x = ((SLIDE_WIDTH - width) / 2.0).max(40.0);
    let y = ((SLIDE_HEIGHT - height) / 2.0).max(40.0);
    SlideElement::new_chart(id, x, y, width, height, chart)
}

/// Generates a default slide chart with sample bar series.
pub fn default_slide_chart() -> Chart {
    Chart::from_data(
        "chart-1",
        Some("Sales Overview".into()),
        loom_objects::chart::ChartType::Bar,
        vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()],
        vec![
            loom_objects::chart::ChartSeries {
                name: "2025".into(),
                values: vec![120.0, 150.0, 180.0, 220.0],
                color: None,
            },
            loom_objects::chart::ChartSeries {
                name: "2026".into(),
                values: vec![140.0, 175.0, 210.0, 260.0],
                color: None,
            },
        ],
    )
}

impl crate::PresentationSession {
    /// Inserts a chart element onto the active slide as an undoable step.
    pub fn insert_chart(&mut self, chart: Chart) -> Result<String, String> {
        if self.document.active_slide().is_none() {
            return Err("there is no slide to insert the chart on".into());
        }
        self.checkpoint();
        let slide = self.document.active_slide_mut().expect("checked above");
        let mut serial = slide.elements.len() + 1;
        let element_id = loop {
            let candidate = format!("{}-chart-{serial}", slide.id);
            if !slide.elements.iter().any(|item| item.id == candidate) {
                break candidate;
            }
            serial += 1;
        };
        let mut elem = chart_element_from_clipboard(chart, element_id.clone());
        crate::tables::cascade_new_object(&slide.elements, &mut elem);
        slide.elements.push(elem);
        self.selected_elements = vec![element_id.clone()];
        Ok(element_id)
    }
}

/// Generates a PowerPoint `<p:graphicFrame>` enclosing a DrawingML chart reference via relationship ID.
pub fn chart_graphic_frame_with_rid(
    id: u32,
    element: &SlideElement,
    chart: &Chart,
    rid: &str,
) -> String {
    let off_x = (f64::from(element.x) * EMU_PER_UNIT).round() as i64;
    let off_y = (f64::from(element.y) * EMU_PER_UNIT).round() as i64;
    let ext_cx = (f64::from(element.width) * EMU_PER_UNIT).round() as i64;
    let ext_cy = (f64::from(element.height) * EMU_PER_UNIT).round() as i64;

    let title_name = chart.title.as_deref().unwrap_or("Chart");

    format!(
        r#"<p:graphicFrame xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:nvGraphicFramePr>
    <p:cNvPr id="{id}" name="{title_name}"/>
    <p:cNvGraphicFramePr/>
    <p:nvPr/>
  </p:nvGraphicFramePr>
  <p:xfrm>
    <a:off x="{off_x}" y="{off_y}"/>
    <a:ext cx="{ext_cx}" cy="{ext_cy}"/>
  </p:xfrm>
  <a:graphic>
    <a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">
      <c:chart xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" r:id="{rid}"/>
    </a:graphicData>
  </a:graphic>
</p:graphicFrame>"#
    )
}

/// Generates a PowerPoint `<p:graphicFrame>` enclosing a DrawingML chart.
pub fn chart_graphic_frame(id: u32, element: &SlideElement, chart: &Chart) -> String {
    chart_graphic_frame_with_rid(id, element, chart, "rIdChart1")
}

/// Extracts chart slide elements from a slide XML string, resolving chart parts when available.
pub fn extract_pptx_slide_charts_with_rels(
    slide_xml: &str,
    rids: &std::collections::BTreeMap<String, String>,
    archive: &loom_package::zip::PackageArchive,
) -> Vec<SlideElement> {
    let mut elements = Vec::new();
    let mut cursor = 0;

    while let Some(frame_start) = slide_xml[cursor..].find("<p:graphicFrame") {
        let abs_start = cursor + frame_start;
        let Some(frame_end_rel) = slide_xml[abs_start..].find("</p:graphicFrame>") else {
            break;
        };
        let frame_end = abs_start + frame_end_rel + "</p:graphicFrame>".len();
        let frame_chunk = &slide_xml[abs_start..frame_end];
        cursor = frame_end;

        if !frame_chunk.contains("drawingml/2006/chart") {
            continue;
        }

        // Extract geometry
        let (x, y, w, h) = extract_frame_bounds(frame_chunk);
        let id = format!("pptx-chart-{}", elements.len() + 1);

        // 1. Try resolving via r:id from relationships
        if let Some(chart_tag_pos) = frame_chunk.find("<c:chart") {
            let chart_tag_slice = &frame_chunk[chart_tag_pos..];
            if let Some(tag_close) = chart_tag_slice.find('>') {
                let tag_str = &chart_tag_slice[..=tag_close];
                if let Some(rid) = extract_attr_str(tag_str, "r:id") {
                    if let Some(target_path) = rids.get(&rid) {
                        if let Some(chart_bytes) = archive.get(target_path) {
                            let chart_xml = String::from_utf8_lossy(chart_bytes);
                            if let Ok(chart) = c_chart_xml_to_chart(&chart_xml, &id) {
                                elements.push(SlideElement::new_chart(
                                    id.clone(),
                                    x,
                                    y,
                                    w,
                                    h,
                                    chart,
                                ));
                                continue;
                            }
                        }
                    }
                }
            }
        }

        // 2. Fallback to inline chartSpace
        if let Some(chart_start) = frame_chunk.find("<c:chartSpace") {
            let chart_slice = &frame_chunk[chart_start..];
            if let Some(chart_end) = chart_slice.find("</c:chartSpace>") {
                let chart_xml = &chart_slice[..chart_end + "</c:chartSpace>".len()];
                if let Ok(chart) = c_chart_xml_to_chart(chart_xml, &id) {
                    elements.push(SlideElement::new_chart(id, x, y, w, h, chart));
                }
            }
        }
    }

    elements
}

fn extract_attr_str(tag: &str, attr: &str) -> Option<String> {
    let needle = format!("{attr}=\"");
    let pos = tag.find(&needle)?;
    let start = pos + needle.len();
    let end = tag[start..].find('"')?;
    Some(tag[start..start + end].to_string())
}

/// Extracts chart slide elements from a slide XML string.
pub fn extract_pptx_slide_charts(slide_xml: &str) -> Vec<SlideElement> {
    let dummy_rids = std::collections::BTreeMap::new();
    let dummy_archive = loom_package::zip::PackageArchive::new();
    extract_pptx_slide_charts_with_rels(slide_xml, &dummy_rids, &dummy_archive)
}

fn extract_frame_bounds(chunk: &str) -> (f32, f32, f32, f32) {
    let mut x = 100.0f32;
    let mut y = 100.0f32;
    let mut w = 400.0f32;
    let mut h = 250.0f32;

    if let Some(off_pos) = chunk.find("<a:off ") {
        let tag = &chunk[off_pos..];
        if let Some(end_tag) = tag.find("/>") {
            let attrs = &tag[..end_tag];
            if let Some(val) = extract_attr_i64(attrs, "x") {
                x = (val as f64 / EMU_PER_UNIT) as f32;
            }
            if let Some(val) = extract_attr_i64(attrs, "y") {
                y = (val as f64 / EMU_PER_UNIT) as f32;
            }
        }
    }

    if let Some(ext_pos) = chunk.find("<a:ext ") {
        let tag = &chunk[ext_pos..];
        if let Some(end_tag) = tag.find("/>") {
            let attrs = &tag[..end_tag];
            if let Some(val) = extract_attr_i64(attrs, "cx") {
                w = (val as f64 / EMU_PER_UNIT) as f32;
            }
            if let Some(val) = extract_attr_i64(attrs, "cy") {
                h = (val as f64 / EMU_PER_UNIT) as f32;
            }
        }
    }

    (x, y, w, h)
}

fn extract_attr_i64(attrs: &str, name: &str) -> Option<i64> {
    let key = format!("{name}=\"");
    let pos = attrs.find(&key)?;
    let rest = &attrs[pos + key.len()..];
    let end = rest.find('"')?;
    rest[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_objects::chart::{ChartSeries, ChartType};

    #[test]
    fn test_slide_element_chart_lifecycle() {
        let chart = Chart::from_data(
            "chart-sales-q",
            Some("Quarterly Performance".to_string()),
            ChartType::Bar,
            vec!["Q1".to_string(), "Q2".to_string(), "Q3".to_string()],
            vec![ChartSeries {
                name: "Revenue".to_string(),
                values: vec![120.0, 160.0, 200.0],
                color: None,
            }],
        );

        let mut elem = SlideElement::new_chart("slide-chart-1", 50.0, 60.0, 500.0, 300.0, chart);
        assert_eq!(elem.element_type, ElementType::Chart);

        let retrieved = elem.chart().expect("chart must deserialize");
        assert_eq!(retrieved.title.as_deref(), Some("Quarterly Performance"));
        assert_eq!(retrieved.chart_type, ChartType::Bar);
        assert_eq!(retrieved.series[0].values, vec![120.0, 160.0, 200.0]);

        // In-place data editing
        let mut modified = retrieved;
        assert!(modified.update_data_point(0, 1, 175.0));
        elem.set_chart(&modified);

        let updated = elem.chart().expect("updated chart");
        assert_eq!(updated.series[0].values[1], 175.0);
    }

    #[test]
    fn test_sheets_clipboard_chart_pastes_as_present_chart() {
        use loom_clipboard::ClipboardData;

        let chart = Chart::from_data(
            "sheets-source-chart",
            Some("Regional Totals".to_string()),
            ChartType::Column,
            vec!["North".to_string(), "South".to_string()],
            vec![ChartSeries {
                name: "Units".to_string(),
                values: vec![300.0, 450.0],
                color: None,
            }],
        );

        let clip = ClipboardData::from_chart(&chart);
        let pasted_chart = clip.to_chart().expect("resolves chart");

        let elem = chart_element_from_clipboard(pasted_chart, "pasted-chart-elem");
        assert_eq!(elem.element_type, ElementType::Chart);
        assert_eq!(elem.width, 500.0);
        assert_eq!(elem.height, 300.0);

        let chart_in_slide = elem.chart().expect("chart in slide");
        assert_eq!(chart_in_slide.title.as_deref(), Some("Regional Totals"));
        assert_eq!(chart_in_slide.series[0].values, vec![300.0, 450.0]);
    }

    #[test]
    fn test_pptx_chart_graphic_frame_export_and_import() {
        let chart = Chart::from_data(
            "frame-chart",
            Some("Growth Trends".to_string()),
            ChartType::Line,
            vec!["2024".to_string(), "2025".to_string()],
            vec![ChartSeries {
                name: "Users".to_string(),
                values: vec![1000.0, 2500.0],
                color: None,
            }],
        );

        let elem = SlideElement::new_chart("chart-elem-1", 80.0, 90.0, 450.0, 260.0, chart);
        let frame_xml = chart_graphic_frame(5, &elem, &elem.chart().unwrap());

        assert!(frame_xml.contains("<p:graphicFrame"));
        assert!(frame_xml.contains("http://schemas.openxmlformats.org/drawingml/2006/chart"));
        assert!(frame_xml.contains("r:id=\"rIdChart1\""));
        assert!(frame_xml.contains("Growth Trends"));

        let off_x = (f64::from(elem.x) * EMU_PER_UNIT).round() as i64;
        let off_y = (f64::from(elem.y) * EMU_PER_UNIT).round() as i64;
        let ext_cx = (f64::from(elem.width) * EMU_PER_UNIT).round() as i64;
        let ext_cy = (f64::from(elem.height) * EMU_PER_UNIT).round() as i64;

        let inline_frame_xml = format!(
            r#"<p:graphicFrame xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main">
  <p:nvGraphicFramePr><p:cNvPr id="5" name="Growth Trends"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr>
  <p:xfrm><a:off x="{off_x}" y="{off_y}"/><a:ext cx="{ext_cx}" cy="{ext_cy}"/></p:xfrm>
  <a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/chart">
    {}
  </a:graphicData></a:graphic>
</p:graphicFrame>"#,
            loom_objects::chart_ooxml::chart_to_c_chart_xml(&elem.chart().unwrap())
        );

        let imported = extract_pptx_slide_charts(&inline_frame_xml);
        assert_eq!(imported.len(), 1);

        let imp_elem = &imported[0];
        assert_eq!(imp_elem.element_type, ElementType::Chart);
        assert!((imp_elem.x - 80.0).abs() < 1.0);
        assert!((imp_elem.y - 90.0).abs() < 1.0);

        let imp_chart = imp_elem.chart().expect("imported chart");
        assert_eq!(imp_chart.title.as_deref(), Some("Growth Trends"));
        assert_eq!(imp_chart.chart_type, ChartType::Line);
        assert_eq!(imp_chart.series[0].values, vec![1000.0, 2500.0]);
    }
}
