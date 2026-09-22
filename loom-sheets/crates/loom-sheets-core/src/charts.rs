//! Chart data models and validation for Loom Sheets.

use serde::{Deserialize, Serialize};

/// Chart type presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChartKind {
    #[default]
    Line,
    Bar,
    Pie,
    Scatter,
}

impl ChartKind {
    /// Canonical lowercase name used in sheet JSON and the GUI.
    pub fn as_str(self) -> &'static str {
        match self {
            ChartKind::Bar => "bar",
            ChartKind::Line => "line",
            ChartKind::Pie => "pie",
            ChartKind::Scatter => "scatter",
        }
    }

    /// Parse a kind name; unknown values map to Bar.
    pub fn parse_kind(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "line" => ChartKind::Line,
            "pie" => ChartKind::Pie,
            "scatter" => ChartKind::Scatter,
            _ => ChartKind::Bar,
        }
    }

    /// Next kind for the overlay kind control (Bar -> Line -> Pie -> Bar).
    /// Scatter stays available to the model but out of the GUI cycle.
    pub fn cycle(self) -> Self {
        match self {
            ChartKind::Bar => ChartKind::Line,
            ChartKind::Line => ChartKind::Pie,
            _ => ChartKind::Bar,
        }
    }
}

/// A chart embedded in a worksheet: live-linked kind/title plus the source
/// columns it derives from (re-derived on every render, persisted as-is).
#[derive(Debug, Clone, PartialEq)]
pub struct SheetChart {
    /// Rendered chart kind.
    pub kind: ChartKind,
    /// Overlay title.
    pub title: String,
    /// Zero-based category (label) column.
    pub cat_col: u32,
    /// Zero-based numeric value column.
    pub val_col: u32,
    /// First data row, zero-based. The preceding row supplies the series label.
    pub start_row: u32,
    /// Last data row, inclusive. None preserves legacy charts that use all rows.
    pub end_row: Option<u32>,
}

impl Default for SheetChart {
    fn default() -> Self {
        Self {
            kind: ChartKind::default(),
            title: String::new(),
            cat_col: 0,
            val_col: 1,
            start_row: 1,
            end_row: None,
        }
    }
}

/// One data series: category/value pairs plus display metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChartSeries {
    pub name: String,
    /// Category labels parallel to `values`; mismatched lengths must be rejected by validate.
    pub categories: Vec<String>,
    pub values: Vec<f64>,
}

/// A chart specification bound to sheet ranges conceptually; pure-data model with validation
/// and derived axis metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChartSpec {
    pub kind: ChartKind,
    pub title: String,
    pub series: Vec<ChartSeries>,
}

impl ChartSpec {
    /// Validates: non-empty title, at least one series, every series has equal-length
    /// categories/values and no NaN values. Err names the violated rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.title.trim().is_empty() {
            return Err("chart title must not be empty".to_string());
        }
        if self.series.is_empty() {
            return Err("chart must contain at least one series".to_string());
        }
        for series in &self.series {
            if series.categories.len() != series.values.len() {
                return Err(format!(
                    "series '{}' has {} categories but {} values; lengths must match",
                    series.name,
                    series.categories.len(),
                    series.values.len()
                ));
            }
            for (index, value) in series.values.iter().enumerate() {
                if value.is_nan() {
                    return Err(format!(
                        "series '{}' contains a NaN value at index {}",
                        series.name, index
                    ));
                }
            }
        }
        Ok(())
    }

    /// (min, max) across all series values; Err when validation fails or no finite values.
    pub fn value_range(&self) -> Result<(f64, f64), String> {
        self.validate()?;
        let mut min = f64::INFINITY;
        let mut max = f64::NEG_INFINITY;
        for series in &self.series {
            for &value in &series.values {
                if value < min {
                    min = value;
                }
                if value > max {
                    max = value;
                }
            }
        }
        if !min.is_finite() || !max.is_finite() {
            return Err("chart contains no finite values".to_string());
        }
        Ok((min, max))
    }

    /// Normalizes each value to 0..=1 against the computed range; Err conditions as above.
    pub fn normalized_points(&self) -> Result<Vec<Vec<f64>>, String> {
        let (min, max) = self.value_range()?;
        Ok(self
            .series
            .iter()
            .map(|series| {
                series
                    .values
                    .iter()
                    .map(|&value| {
                        if min == max {
                            1.0
                        } else {
                            (value - min) / (max - min)
                        }
                    })
                    .collect::<Vec<f64>>()
            })
            .collect())
    }
}

/// How a placed chart receives updates after being exported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ChartUpdatePolicy {
    /// Snapshot only; host never refreshes.
    #[default]
    StaticSnapshot,
    /// Host may re-read the bound range on demand.
    RefreshOnOpen,
}

/// Describes one chart exported to a host document, keeping the source range addressable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChartPlacement {
    pub chart_id: String,
    pub sheet_name: String,
    /// A1-style range feeding the series, e.g. "B2:D9".
    pub source_range: String,
    pub spec: ChartSpec,
    pub update_policy: ChartUpdatePolicy,
}

/// True when `part` is one corner of an A1 range: ASCII column letters followed by ASCII
/// row digits (case-insensitive), e.g. "B2" or "aa10".
fn is_a1_corner(part: &str) -> bool {
    let mut chars = part.chars();
    let mut saw_letter = false;
    let mut saw_digit = false;
    for character in chars.by_ref() {
        if character.is_ascii_alphabetic() {
            if saw_digit {
                return false;
            }
            saw_letter = true;
        } else if character.is_ascii_digit() {
            if !saw_letter {
                return false;
            }
            saw_digit = true;
        } else {
            return false;
        }
    }
    saw_letter && saw_digit
}

impl ChartPlacement {
    /// Validates: non-empty chart id and sheet name; non-empty source range matching
    /// loose A1 grammar of column letters + row digits on both sides of exactly one ':'
    /// (e.g. B2:D9, case-insensitive). Err names the violated rule.
    pub fn validate(&self) -> Result<(), String> {
        if self.chart_id.trim().is_empty() {
            return Err("chart placement id must not be empty".to_string());
        }
        if self.sheet_name.trim().is_empty() {
            return Err("chart placement sheet name must not be empty".to_string());
        }
        if self.source_range.is_empty() {
            return Err("chart placement source range must not be empty".to_string());
        }
        let corners: Vec<&str> = self.source_range.split(':').collect();
        if corners.len() != 2 {
            return Err(format!(
                "source range '{}' must contain exactly one ':' separator",
                self.source_range
            ));
        }
        for corner in corners {
            if !is_a1_corner(corner) {
                return Err(format!(
                    "source range corner '{}' must be column letters followed by row digits",
                    corner
                ));
            }
        }
        Ok(())
    }

    /// True when two placements would collide in a host document: same chart_id, or
    /// identical sheet_name+source_range while at least one side updates non-statically.
    pub fn collides_with(&self, other: &ChartPlacement) -> bool {
        if self.chart_id == other.chart_id {
            return true;
        }
        self.sheet_name == other.sheet_name
            && self.source_range == other.source_range
            && (self.update_policy != ChartUpdatePolicy::StaticSnapshot
                || other.update_policy != ChartUpdatePolicy::StaticSnapshot)
    }
}
