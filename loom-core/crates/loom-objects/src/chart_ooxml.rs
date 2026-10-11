//! DrawingML / ChartML (`c:chartSpace`) generation and parsing for Loom Office charts.

use crate::chart::{Chart, ChartSeries, ChartType};

/// Generates a complete standalone DrawingML `<c:chartSpace>` XML document from a [`Chart`].
pub fn chart_to_c_chart_xml(chart: &Chart) -> String {
    let mut xml = String::with_capacity(2048);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n");
    xml.push_str(
        "<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" ",
    );
    xml.push_str("xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" ");
    xml.push_str(
        "xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\">\n",
    );
    xml.push_str("  <c:chart>\n");

    // Title
    if let Some(title) = &chart.title {
        xml.push_str("    <c:title>\n");
        xml.push_str("      <c:tx>\n");
        xml.push_str("        <c:rich>\n");
        xml.push_str("          <a:bodyPr/>\n");
        xml.push_str("          <a:lstStyle/>\n");
        xml.push_str("          <a:p><a:r><a:t>");
        xml.push_str(&quick_xml_escape(title));
        xml.push_str("</a:t></a:r></a:p>\n");
        xml.push_str("        </c:rich>\n");
        xml.push_str("      </c:tx>\n");
        xml.push_str("    </c:title>\n");
    }

    xml.push_str("    <c:plotArea>\n");
    xml.push_str("      <c:layout/>\n");

    let type_tag = match chart.chart_type {
        ChartType::Column => "c:barChart",
        ChartType::Bar => "c:barChart",
        ChartType::Line => "c:lineChart",
        ChartType::Pie => "c:pieChart",
        ChartType::Area => "c:areaChart",
        ChartType::Scatter => "c:scatterChart",
    };

    xml.push_str("      <");
    xml.push_str(type_tag);
    xml.push_str(">\n");

    if chart.chart_type == ChartType::Column {
        xml.push_str("        <c:barDir val=\"col\"/>\n");
        xml.push_str("        <c:grouping val=\"standard\"/>\n");
    } else if chart.chart_type == ChartType::Bar {
        xml.push_str("        <c:barDir val=\"bar\"/>\n");
        xml.push_str("        <c:grouping val=\"standard\"/>\n");
    }

    // Series
    for (s_idx, series) in chart.series.iter().enumerate() {
        xml.push_str("        <c:ser>\n");
        xml.push_str(&format!("          <c:idx val=\"{s_idx}\"/>\n"));
        xml.push_str(&format!("          <c:order val=\"{s_idx}\"/>\n"));
        xml.push_str("          <c:tx><c:v>");
        xml.push_str(&quick_xml_escape(&series.name));
        xml.push_str("</c:v></c:tx>\n");

        // Categories
        xml.push_str("          <c:cat>\n");
        xml.push_str("            <c:strLit>\n");
        xml.push_str(&format!(
            "              <c:ptCount val=\"{}\"/>\n",
            chart.categories.len()
        ));
        for (c_idx, cat) in chart.categories.iter().enumerate() {
            xml.push_str(&format!(
                "              <c:pt idx=\"{c_idx}\"><c:v>{}</c:v></c:pt>\n",
                quick_xml_escape(cat)
            ));
        }
        xml.push_str("            </c:strLit>\n");
        xml.push_str("          </c:cat>\n");

        // Values
        xml.push_str("          <c:val>\n");
        xml.push_str("            <c:numLit>\n");
        xml.push_str(&format!(
            "              <c:ptCount val=\"{}\"/>\n",
            series.values.len()
        ));
        for (v_idx, &val) in series.values.iter().enumerate() {
            xml.push_str(&format!(
                "              <c:pt idx=\"{v_idx}\"><c:v>{val}</c:v></c:pt>\n"
            ));
        }
        xml.push_str("            </c:numLit>\n");
        xml.push_str("          </c:val>\n");

        xml.push_str("        </c:ser>\n");
    }

    if chart.chart_type != ChartType::Pie {
        xml.push_str("        <c:axId val=\"1001\"/>\n");
        xml.push_str("        <c:axId val=\"1002\"/>\n");
    }

    xml.push_str("      </");
    xml.push_str(type_tag);
    xml.push_str(">\n");

    if chart.chart_type != ChartType::Pie {
        xml.push_str("      <c:catAx>\n");
        xml.push_str("        <c:axId val=\"1001\"/>\n");
        xml.push_str("        <c:scaling><c:orientation val=\"minMax\"/></c:scaling>\n");
        xml.push_str("        <c:axPos val=\"b\"/>\n");
        xml.push_str("        <c:crossAx val=\"1002\"/>\n");
        xml.push_str("      </c:catAx>\n");

        xml.push_str("      <c:valAx>\n");
        xml.push_str("        <c:axId val=\"1002\"/>\n");
        xml.push_str("        <c:scaling><c:orientation val=\"minMax\"/></c:scaling>\n");
        xml.push_str("        <c:axPos val=\"l\"/>\n");
        xml.push_str("        <c:crossAx val=\"1001\"/>\n");
        xml.push_str("      </c:valAx>\n");
    }

    xml.push_str("    </c:plotArea>\n");

    if chart.style.show_legend {
        xml.push_str("    <c:legend>\n");
        xml.push_str("      <c:legendPos val=\"r\"/>\n");
        xml.push_str("    </c:legend>\n");
    }

    xml.push_str("    <c:plotVisOnly val=\"1\"/>\n");
    xml.push_str("  </c:chart>\n");
    xml.push_str("</c:chartSpace>\n");

    xml
}

/// Parses a DrawingML `<c:chartSpace>` XML string back into a [`Chart`].
pub fn c_chart_xml_to_chart(xml: &str, chart_id: impl Into<String>) -> Result<Chart, String> {
    let title = extract_title(xml);
    let chart_type = detect_chart_type(xml);

    let (categories, series) = extract_series_and_categories(xml)?;

    let mut chart = Chart::from_data(chart_id, title, chart_type, categories, series);
    chart.style.show_legend = xml.contains("<c:legend");
    Ok(chart)
}

fn quick_xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn extract_title(xml: &str) -> Option<String> {
    if let Some(pos) = xml.find("<c:title>") {
        let sub = &xml[pos..];
        if let Some(end) = sub.find("</c:title>") {
            let title_chunk = &sub[..end];
            if let Some(t_start) = title_chunk.find("<a:t>") {
                let after = &title_chunk[t_start + 5..];
                if let Some(t_end) = after.find("</a:t>") {
                    return Some(after[..t_end].trim().to_string());
                }
            }
        }
    }
    None
}

fn detect_chart_type(xml: &str) -> ChartType {
    if xml.contains("<c:pieChart") {
        ChartType::Pie
    } else if xml.contains("<c:lineChart") {
        ChartType::Line
    } else if xml.contains("<c:areaChart") {
        ChartType::Area
    } else if xml.contains("<c:scatterChart") {
        ChartType::Scatter
    } else if xml.contains("<c:barChart") {
        if xml.contains("val=\"bar\"") {
            ChartType::Bar
        } else {
            ChartType::Column
        }
    } else {
        ChartType::Column
    }
}

fn extract_series_and_categories(xml: &str) -> Result<(Vec<String>, Vec<ChartSeries>), String> {
    let mut categories = Vec::new();
    let mut series = Vec::new();

    let mut cursor = 0;
    while let Some(ser_start) = xml[cursor..].find("<c:ser>") {
        let abs_start = cursor + ser_start;
        let Some(ser_end) = xml[abs_start..].find("</c:ser>") else {
            break;
        };
        let chunk = &xml[abs_start..abs_start + ser_end + 8];
        cursor = abs_start + ser_end + 8;

        // Series name
        let name = if let Some(tx_start) = chunk.find("<c:tx>") {
            let tx_sub = &chunk[tx_start..];
            if let Some(v_start) = tx_sub.find("<c:v>") {
                let v_sub = &tx_sub[v_start + 5..];
                v_sub.find("</c:v>").map_or("Series", |e| &v_sub[..e])
            } else {
                "Series"
            }
        } else {
            "Series"
        };

        // Extract categories if not already populated
        if categories.is_empty() {
            if let Some(cat_start) = chunk.find("<c:cat>") {
                let cat_slice = &chunk[cat_start..];
                let cat_len = cat_slice.find("</c:cat>").unwrap_or(cat_slice.len());
                let cat_sub = &cat_slice[..cat_len];
                let mut cat_cur = 0;
                while let Some(pt_idx) = cat_sub[cat_cur..].find("<c:pt ") {
                    let pt_sub = &cat_sub[cat_cur + pt_idx..];
                    if let Some(v_idx) = pt_sub.find("<c:v>") {
                        let val_sub = &pt_sub[v_idx + 5..];
                        if let Some(v_end) = val_sub.find("</c:v>") {
                            categories.push(val_sub[..v_end].to_string());
                        }
                    }
                    if let Some(end_pt) = pt_sub.find("</c:pt>") {
                        cat_cur += pt_idx + end_pt + 7;
                    } else {
                        break;
                    }
                }
            }
        }

        // Values
        let mut values = Vec::new();
        if let Some(val_start) = chunk.find("<c:val>") {
            let val_slice = &chunk[val_start..];
            let val_len = val_slice.find("</c:val>").unwrap_or(val_slice.len());
            let val_sub = &val_slice[..val_len];
            let mut val_cur = 0;
            while let Some(pt_idx) = val_sub[val_cur..].find("<c:pt ") {
                let pt_sub = &val_sub[val_cur + pt_idx..];
                if let Some(v_idx) = pt_sub.find("<c:v>") {
                    let v_str = &pt_sub[v_idx + 5..];
                    if let Some(v_end) = v_str.find("</c:v>") {
                        if let Ok(num) = v_str[..v_end].parse::<f64>() {
                            values.push(num);
                        }
                    }
                }
                if let Some(end_pt) = pt_sub.find("</c:pt>") {
                    val_cur += pt_idx + end_pt + 7;
                } else {
                    break;
                }
            }
        }

        series.push(ChartSeries {
            name: name.to_string(),
            values,
            color: None,
        });
    }

    Ok((categories, series))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_chart_xml_round_trip() {
        let categories = vec!["East".to_string(), "West".to_string(), "North".to_string()];
        let s1 = ChartSeries {
            name: "Q1 Revenue".to_string(),
            values: vec![150.0, 240.0, 180.0],
            color: None,
        };
        let s2 = ChartSeries {
            name: "Q2 Revenue".to_string(),
            values: vec![170.0, 210.0, 220.0],
            color: None,
        };
        let chart = Chart::from_data(
            "chart-rev-10",
            Some("Regional Sales".to_string()),
            ChartType::Column,
            categories,
            vec![s1, s2],
        );

        let xml = chart_to_c_chart_xml(&chart);
        assert!(xml.contains("<c:chartSpace"));
        assert!(xml.contains("Regional Sales"));
        assert!(xml.contains("<c:barChart>"));
        assert!(xml.contains("<c:barDir val=\"col\"/>"));
        assert!(xml.contains("Q1 Revenue"));
        assert!(xml.contains("150"));

        let parsed = c_chart_xml_to_chart(&xml, "imported-chart").expect("parse chart xml");
        assert_eq!(parsed.id, "imported-chart");
        assert_eq!(parsed.title.as_deref(), Some("Regional Sales"));
        assert_eq!(parsed.chart_type, ChartType::Column);
        assert_eq!(parsed.categories, vec!["East", "West", "North"]);
        assert_eq!(parsed.series.len(), 2);
        assert_eq!(parsed.series[0].name, "Q1 Revenue");
        assert_eq!(parsed.series[0].values, vec![150.0, 240.0, 180.0]);
        assert_eq!(parsed.series[1].name, "Q2 Revenue");
        assert_eq!(parsed.series[1].values, vec![170.0, 210.0, 220.0]);
        assert!(parsed.style.show_legend);
    }
}
