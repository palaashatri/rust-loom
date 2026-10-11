//! Shared chart object model with data snapshot embedding.

use serde::{Deserialize, Serialize};

/// Chart visualization type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChartType {
    #[default]
    Column,
    Bar,
    Line,
    Pie,
    Area,
    Scatter,
}

/// A chart object embeddable into Writer documents or Present slides.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub id: String,
    pub title: Option<String>,
    pub chart_type: ChartType,
    pub width_pt: f32,
    pub height_pt: f32,
    pub series: Vec<ChartSeries>,
    pub categories: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<ChartDataSnapshot>,
}

impl Chart {
    /// Creates a new chart with default dimensions (400x250 pt).
    pub fn new(id: impl Into<String>, chart_type: ChartType) -> Self {
        Self {
            id: id.into(),
            title: None,
            chart_type,
            width_pt: 400.0,
            height_pt: 250.0,
            series: Vec::new(),
            categories: Vec::new(),
            range_reference: None,
            snapshot: None,
        }
    }
}

/// A data series in a chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A frozen data snapshot of source cells for embedded charts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartDataSnapshot {
    pub sheet_name: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}
