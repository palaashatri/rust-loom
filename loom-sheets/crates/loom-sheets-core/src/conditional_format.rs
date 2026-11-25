//! Conditional formatting rules for worksheet cell ranges.

use crate::style::FillColor;
use crate::CellRange;

/// Comparison operator for a conditional formatting rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConditionalOperator {
    GreaterThan,
    LessThan,
    EqualTo,
    Contains,
}

impl ConditionalOperator {
    /// Canonical lowercase key used in persistence and command strings.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GreaterThan => "gt",
            Self::LessThan => "lt",
            Self::EqualTo => "eq",
            Self::Contains => "contains",
        }
    }

    /// Human-readable label for UI display.
    pub fn label(self) -> &'static str {
        match self {
            Self::GreaterThan => "Greater than",
            Self::LessThan => "Less than",
            Self::EqualTo => "Equal to",
            Self::Contains => "Contains",
        }
    }

    /// Parse an operator key (case-insensitive).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "gt" | ">" | "greater" | "greater_than" => Some(Self::GreaterThan),
            "lt" | "<" | "less" | "less_than" => Some(Self::LessThan),
            "eq" | "=" | "==" | "equal" | "equal_to" | "equals" => Some(Self::EqualTo),
            "contains" => Some(Self::Contains),
            _ => None,
        }
    }
}

/// A single conditional formatting rule targeting a range of cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellConditionalRule {
    /// Target cell range (inclusive).
    pub range: CellRange,
    /// Comparison operator.
    pub operator: ConditionalOperator,
    /// Operand value to compare against.
    pub value: String,
    /// Highlight fill color when the rule matches.
    pub fill: FillColor,
    /// Optional highlight text color (`0xRRGGBB`) when the rule matches.
    pub text_color: Option<u32>,
}

impl CellConditionalRule {
    /// Returns `true` if `display_value` satisfies this rule.
    pub fn matches(&self, display_value: &str) -> bool {
        let cell_trimmed = display_value.trim();
        let target_trimmed = self.value.trim();
        match self.operator {
            ConditionalOperator::GreaterThan => {
                match (parse_numeric(cell_trimmed), parse_numeric(target_trimmed)) {
                    (Some(a), Some(b)) => a > b,
                    _ => false,
                }
            }
            ConditionalOperator::LessThan => {
                match (parse_numeric(cell_trimmed), parse_numeric(target_trimmed)) {
                    (Some(a), Some(b)) => a < b,
                    _ => false,
                }
            }
            ConditionalOperator::EqualTo => {
                match (parse_numeric(cell_trimmed), parse_numeric(target_trimmed)) {
                    (Some(a), Some(b)) => (a - b).abs() < 1e-9,
                    _ => !cell_trimmed.is_empty() && cell_trimmed == target_trimmed,
                }
            }
            ConditionalOperator::Contains => {
                !target_trimmed.is_empty()
                    && cell_trimmed
                        .to_ascii_lowercase()
                        .contains(&target_trimmed.to_ascii_lowercase())
            }
        }
    }
}

fn parse_numeric(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (body, is_percent) = match trimmed.strip_suffix('%') {
        Some(rest) => (rest.trim_end(), true),
        None => (trimmed, false),
    };
    let cleaned = body.trim_start_matches('$').replace(',', "");
    let value = cleaned.parse::<f64>().ok().filter(|n| n.is_finite())?;
    let scaled = if is_percent { value / 100.0 } else { value };
    scaled.is_finite().then_some(scaled)
}

/// Data bar conditional formatting rule for a numeric range.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DataBarRule {
    pub range: CellRange,
    pub color: u32,
}

impl DataBarRule {
    /// Returns the normalized `[0.0, 1.0]` bar fill fraction for `value`.
    pub fn fraction(&self, value: f64, min: f64, max: f64) -> f32 {
        if !value.is_finite() || !min.is_finite() || !max.is_finite() || (max - min).abs() < 1e-12 {
            return 0.0;
        }
        ((value - min) / (max - min)).clamp(0.0, 1.0) as f32
    }
}

/// 2-color scale conditional formatting rule for a numeric range.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ColorScaleRule {
    pub range: CellRange,
    pub min_color: u32,
    pub max_color: u32,
}

impl ColorScaleRule {
    /// Linearly interpolates `0xRRGGBB` between `min_color` and `max_color`.
    pub fn interpolate(&self, value: f64, min: f64, max: f64) -> u32 {
        let t = if !value.is_finite()
            || !min.is_finite()
            || !max.is_finite()
            || (max - min).abs() < 1e-12
        {
            0.0
        } else {
            ((value - min) / (max - min)).clamp(0.0, 1.0)
        };
        let lerp = |shift: u32| -> u32 {
            let a = ((self.min_color >> shift) & 0xFF) as f64;
            let b = ((self.max_color >> shift) & 0xFF) as f64;
            (a + (b - a) * t).round().clamp(0.0, 255.0) as u32
        };
        (lerp(16) << 16) | (lerp(8) << 8) | lerp(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CellRef;

    fn sample_rule(op: ConditionalOperator, value: &str) -> CellConditionalRule {
        CellConditionalRule {
            range: CellRange::new(CellRef { row: 0, col: 0 }, CellRef { row: 5, col: 5 }),
            operator: op,
            value: value.to_string(),
            fill: FillColor::Red,
            text_color: Some(0x00C6_2828),
        }
    }

    #[test]
    fn numeric_gt_lt_eq_rules_handle_formatted_numbers() {
        let gt = sample_rule(ConditionalOperator::GreaterThan, "100");
        assert!(gt.matches("150"));
        assert!(gt.matches("$1,200.50"));
        assert!(!gt.matches("100"));
        assert!(!gt.matches("hello"));
        assert!(!gt.matches(""));

        let lt = sample_rule(ConditionalOperator::LessThan, "50");
        assert!(lt.matches("12.5"));
        assert!(lt.matches("-5"));
        assert!(!lt.matches("50"));
        assert!(!lt.matches("abc"));

        let eq = sample_rule(ConditionalOperator::EqualTo, "42");
        assert!(eq.matches("42"));
        assert!(eq.matches("42.0"));
        assert!(!eq.matches("42.1"));
    }

    #[test]
    fn text_eq_and_contains_rules() {
        let eq = sample_rule(ConditionalOperator::EqualTo, "PASS");
        assert!(eq.matches("PASS"));
        assert!(!eq.matches("pass"));
        assert!(!eq.matches(""));

        let contains = sample_rule(ConditionalOperator::Contains, "err");
        assert!(contains.matches("Error: #DIV/0!"));
        assert!(contains.matches("ERR"));
        assert!(!contains.matches("OK"));

        let empty_contains = sample_rule(ConditionalOperator::Contains, "");
        assert!(!empty_contains.matches("anything"));
    }

    #[test]
    fn operator_roundtrip() {
        for op in [
            ConditionalOperator::GreaterThan,
            ConditionalOperator::LessThan,
            ConditionalOperator::EqualTo,
            ConditionalOperator::Contains,
        ] {
            assert_eq!(ConditionalOperator::parse(op.as_str()), Some(op));
        }
        assert_eq!(ConditionalOperator::parse("unknown"), None);
    }

    #[test]
    fn percentage_values_compare_as_numeric_fractions() {
        let gt_point_two = sample_rule(ConditionalOperator::GreaterThan, "0.2");
        assert!(!gt_point_two.matches("12.5%"));
        assert!(gt_point_two.matches("25%"));

        let gt_twenty_pct = sample_rule(ConditionalOperator::GreaterThan, "20%");
        assert!(!gt_twenty_pct.matches("12.5%"));
        assert!(gt_twenty_pct.matches("0.25"));

        let eq_fraction = sample_rule(ConditionalOperator::EqualTo, "0.125");
        assert!(eq_fraction.matches("12.5%"));
    }

    #[test]
    fn databar_and_colorscale_normalize_values() {
        let bar = DataBarRule {
            range: CellRange::new(CellRef { row: 0, col: 0 }, CellRef { row: 2, col: 0 }),
            color: 0x002B_579A,
        };
        assert!((bar.fraction(50.0, 0.0, 100.0) - 0.5).abs() < 1e-6);

        let scale = ColorScaleRule {
            range: CellRange::new(CellRef { row: 0, col: 0 }, CellRef { row: 2, col: 0 }),
            min_color: 0x00FF_0000,
            max_color: 0x0000_FF00,
        };
        assert_eq!(scale.interpolate(0.0, 0.0, 100.0), 0x00FF_0000);
        assert_eq!(scale.interpolate(100.0, 0.0, 100.0), 0x0000_FF00);
    }
}
