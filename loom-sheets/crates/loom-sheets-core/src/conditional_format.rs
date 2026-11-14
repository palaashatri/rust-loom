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
    if raw.is_empty() {
        return None;
    }
    let cleaned = raw
        .trim()
        .trim_start_matches('$')
        .trim_end_matches('%')
        .replace(',', "");
    cleaned.parse::<f64>().ok().filter(|n| n.is_finite())
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
}
