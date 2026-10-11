//! Shared chart object model with data snapshot embedding and in-place editing.

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

impl ChartType {
    pub fn as_str(self) -> &'static str {
        match self {
            ChartType::Column => "column",
            ChartType::Bar => "bar",
            ChartType::Line => "line",
            ChartType::Pie => "pie",
            ChartType::Area => "area",
            ChartType::Scatter => "scatter",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "bar" => ChartType::Bar,
            "line" => ChartType::Line,
            "pie" => ChartType::Pie,
            "area" => ChartType::Area,
            "scatter" => ChartType::Scatter,
            _ => ChartType::Column,
        }
    }
}

/// Axis display configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChartAxis {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default = "default_true")]
    pub gridlines: bool,
}

fn default_true() -> bool {
    true
}

/// Visual styling and legend placement for charts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartStyle {
    pub palette_id: String,
    pub show_legend: bool,
    pub show_data_labels: bool,
}

impl Default for ChartStyle {
    fn default() -> Self {
        Self {
            palette_id: "Office".to_string(),
            show_legend: true,
            show_data_labels: false,
        }
    }
}

/// A single data series in a chart.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    pub name: String,
    pub values: Vec<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A frozen data snapshot of source cells for embedded charts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChartDataSnapshot {
    pub sheet_name: Option<String>,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl ChartDataSnapshot {
    /// Creates a new snapshot from category headers and series rows.
    pub fn new(sheet_name: Option<String>, headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        Self {
            sheet_name,
            headers,
            rows,
        }
    }

    /// Mutates a cell value in the snapshot (useful for small in-place data editing).
    pub fn update_cell(&mut self, row: usize, col: usize, val: &str) -> bool {
        if let Some(r) = self.rows.get_mut(row) {
            if let Some(c) = r.get_mut(col) {
                *c = val.to_string();
                return true;
            }
        }
        false
    }
}

/// A complete chart document object embeddable into Sheets, Writer, or Present.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub chart_type: ChartType,
    pub width_pt: f32,
    pub height_pt: f32,
    pub series: Vec<ChartSeries>,
    pub categories: Vec<String>,
    pub category_axis: ChartAxis,
    pub value_axis: ChartAxis,
    pub style: ChartStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub range_reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<ChartDataSnapshot>,
}

impl Chart {
    /// Creates a new empty chart with default dimensions (400x250 pt).
    pub fn new(id: impl Into<String>, chart_type: ChartType) -> Self {
        Self {
            id: id.into(),
            title: None,
            chart_type,
            width_pt: 400.0,
            height_pt: 250.0,
            series: Vec::new(),
            categories: Vec::new(),
            category_axis: ChartAxis::default(),
            value_axis: ChartAxis::default(),
            style: ChartStyle::default(),
            range_reference: None,
            snapshot: None,
        }
    }

    /// Constructs a chart with initial series data and automatic snapshot generation.
    pub fn from_data(
        id: impl Into<String>,
        title: Option<String>,
        chart_type: ChartType,
        categories: Vec<String>,
        series: Vec<ChartSeries>,
    ) -> Self {
        let mut chart = Self::new(id, chart_type);
        chart.title = title;
        chart.categories = categories;
        chart.series = series;
        chart.sync_snapshot_from_data(None);
        chart
    }

    /// Appends a new data series to the chart and refreshes the snapshot.
    pub fn add_series(&mut self, name: impl Into<String>, values: Vec<f64>, color: Option<String>) {
        self.series.push(ChartSeries {
            name: name.into(),
            values,
            color,
        });
        let sheet_name = self.snapshot.as_ref().and_then(|s| s.sheet_name.clone());
        self.sync_snapshot_from_data(sheet_name);
    }

    /// Synchronizes the data snapshot table from the chart's categories and series values.
    pub fn sync_snapshot_from_data(&mut self, sheet_name: Option<String>) {
        let mut headers = vec!["Category".to_string()];
        for s in &self.series {
            headers.push(s.name.clone());
        }

        let mut rows = Vec::with_capacity(self.categories.len());
        for (cat_idx, cat) in self.categories.iter().enumerate() {
            let mut row = vec![cat.clone()];
            for s in &self.series {
                if let Some(val) = s.values.get(cat_idx) {
                    row.push(val.to_string());
                } else {
                    row.push("0".to_string());
                }
            }
            rows.push(row);
        }

        self.snapshot = Some(ChartDataSnapshot::new(sheet_name, headers, rows));
    }

    /// Edits a data point in the specified series and updates the snapshot.
    pub fn update_data_point(&mut self, series_idx: usize, cat_idx: usize, new_val: f64) -> bool {
        if let Some(s) = self.series.get_mut(series_idx) {
            if let Some(v) = s.values.get_mut(cat_idx) {
                *v = new_val;
                if let Some(snap) = &mut self.snapshot {
                    snap.update_cell(cat_idx, series_idx + 1, &new_val.to_string());
                }
                return true;
            }
        }
        false
    }

    /// Validates the chart structure. Returns `Ok(())` or an informative error string.
    pub fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty() {
            return Err("chart id must not be empty".to_string());
        }
        if self.series.is_empty() {
            return Err("chart must have at least one data series".to_string());
        }
        for s in &self.series {
            if s.name.trim().is_empty() {
                return Err("series name must not be empty".to_string());
            }
            for (idx, &v) in s.values.iter().enumerate() {
                if v.is_nan() {
                    return Err(format!("series '{}' contains NaN at index {idx}", s.name));
                }
            }
        }
        Ok(())
    }

    /// Computes the overall `(min, max)` value across all series.
    pub fn value_range(&self) -> (f64, f64) {
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for s in &self.series {
            for &val in &s.values {
                if val < min {
                    min = val;
                }
                if val > max {
                    max = val;
                }
            }
        }
        if !min.is_finite() || !max.is_finite() {
            (0.0, 1.0)
        } else if min == max {
            (min, min + 1.0)
        } else {
            (min, max)
        }
    }

    /// Returns normalized series values mapped into [0.0, 1.0].
    pub fn normalized_series_values(&self) -> Vec<Vec<f64>> {
        let (min, max) = self.value_range();
        let span = max - min;
        self.series
            .iter()
            .map(|s| {
                s.values
                    .iter()
                    .map(|&v| if span > 0.0 { (v - min) / span } else { 0.5 })
                    .collect()
            })
            .collect()
    }

    /// Serializes the chart to JSON format.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserializes a chart from JSON format.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chart_creation_and_snapshot_sync() {
        let categories = vec![
            "Q1".to_string(),
            "Q2".to_string(),
            "Q3".to_string(),
            "Q4".to_string(),
        ];
        let s1 = ChartSeries {
            name: "Sales".to_string(),
            values: vec![120.0, 150.0, 180.0, 220.0],
            color: Some("#2563EB".to_string()),
        };
        let mut chart = Chart::from_data(
            "chart-sales-1",
            Some("Annual Revenue".to_string()),
            ChartType::Column,
            categories,
            vec![s1],
        );

        assert_eq!(chart.id, "chart-sales-1");
        assert_eq!(chart.chart_type, ChartType::Column);
        assert_eq!(chart.series.len(), 1);

        let snap = chart.snapshot.as_ref().expect("snapshot must be generated");
        assert_eq!(snap.headers, vec!["Category", "Sales"]);
        assert_eq!(snap.rows.len(), 4);
        assert_eq!(snap.rows[0], vec!["Q1", "120"]);

        // Update data point
        assert!(chart.update_data_point(0, 0, 135.0));
        assert_eq!(chart.series[0].values[0], 135.0);
        let snap2 = chart.snapshot.as_ref().unwrap();
        assert_eq!(snap2.rows[0][1], "135");

        // Value range and normalization
        let (min, max) = chart.value_range();
        assert_eq!(min, 135.0);
        assert_eq!(max, 220.0);
        let norm = chart.normalized_series_values();
        assert_eq!(norm.len(), 1);
        assert!((norm[0][0] - 0.0).abs() < 1e-6);
        assert!((norm[0][3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_chart_json_round_trip() {
        let categories = vec!["A".to_string(), "B".to_string()];
        let s1 = ChartSeries {
            name: "Alpha".to_string(),
            values: vec![10.0, 20.0],
            color: None,
        };
        let chart = Chart::from_data(
            "c-json",
            Some("Test Chart".to_string()),
            ChartType::Pie,
            categories,
            vec![s1],
        );

        let json = chart.to_json().expect("to json");
        let restored = Chart::from_json(&json).expect("from json");
        assert_eq!(chart, restored);
    }
}
