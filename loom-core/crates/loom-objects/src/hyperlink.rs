//! Shared hyperlink model and URL safety validation for Loom Office.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Destination of a hyperlink across Writer, Sheets, and Present.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HyperlinkTarget {
    Url(String),
    Bookmark(String),
    Slide(usize),
    CellRange(String),
    Email(String),
}

/// Hyperlink attached to text, a cell, or an object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hyperlink {
    pub target: HyperlinkTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
}

/// Errors produced when validating or normalizing a hyperlink URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HyperlinkError {
    EmptyUrl,
    UnsupportedScheme(String),
}

impl fmt::Display for HyperlinkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyUrl => write!(f, "hyperlink URL cannot be empty"),
            Self::UnsupportedScheme(scheme) => {
                write!(f, "unsupported or unsafe hyperlink scheme: {scheme}")
            }
        }
    }
}

impl std::error::Error for HyperlinkError {}

/// Validates and normalizes an external URL.
///
/// Allows `https`, `http`, `mailto`, and `tel` schemes (case-insensitive).
/// Rejects `javascript:`, `data:`, `file:`, and any other scheme.
/// Bare host/path strings without a scheme are normalized with `https://`.
pub fn normalize_safe_url(raw: &str) -> Result<String, HyperlinkError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() || trimmed.chars().any(char::is_control) {
        return Err(HyperlinkError::EmptyUrl);
    }

    if let Some((scheme, rest)) = split_scheme(trimmed) {
        let lower = scheme.to_ascii_lowercase();
        match lower.as_str() {
            "https" | "http" => Ok(format!("{lower}://{rest}")),
            "mailto" | "tel" => Ok(format!("{lower}:{rest}")),
            _ => Err(HyperlinkError::UnsupportedScheme(lower)),
        }
    } else {
        Ok(format!("https://{trimmed}"))
    }
}

fn split_scheme(input: &str) -> Option<(&str, &str)> {
    let colon = input.find(':')?;
    let scheme = &input[..colon];
    if scheme.is_empty()
        || !scheme
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
    {
        return None;
    }
    let after_colon = &input[colon + 1..];
    if after_colon
        .bytes()
        .next()
        .is_some_and(|b| b.is_ascii_digit())
    {
        return None;
    }
    let rest = after_colon.strip_prefix("//").unwrap_or(after_colon);
    Some((scheme, rest))
}

impl Hyperlink {
    /// Creates a validated URL hyperlink.
    pub fn url(raw: &str, tooltip: Option<String>) -> Result<Self, HyperlinkError> {
        let normalized = normalize_safe_url(raw)?;
        Ok(Self {
            target: HyperlinkTarget::Url(normalized),
            tooltip,
        })
    }

    /// Creates an email hyperlink, stripping any leading `mailto:` prefix.
    pub fn email(address: &str, tooltip: Option<String>) -> Result<Self, HyperlinkError> {
        let trimmed = address.trim();
        let stripped = if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("mailto:") {
            &trimmed[7..]
        } else {
            trimmed
        };
        let addr = stripped.trim();
        if addr.is_empty() {
            return Err(HyperlinkError::EmptyUrl);
        }
        Ok(Self {
            target: HyperlinkTarget::Email(addr.to_string()),
            tooltip,
        })
    }

    /// Validates the hyperlink target if it is a URL.
    pub fn validate(&self) -> Result<(), HyperlinkError> {
        match &self.target {
            HyperlinkTarget::Url(u) => normalize_safe_url(u).map(|_| ()),
            HyperlinkTarget::Email(addr) if addr.trim().is_empty() => Err(HyperlinkError::EmptyUrl),
            _ => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_safe_schemes_and_normalizes() {
        assert_eq!(
            normalize_safe_url("  HTTPS://example.com/docs  ").unwrap(),
            "https://example.com/docs"
        );
        assert_eq!(
            normalize_safe_url("http://localhost:8080/test").unwrap(),
            "http://localhost:8080/test"
        );
        assert_eq!(
            normalize_safe_url("MAILTO:user@example.com").unwrap(),
            "mailto:user@example.com"
        );
        assert_eq!(
            normalize_safe_url("tel:+1-555-0100").unwrap(),
            "tel:+1-555-0100"
        );
        assert_eq!(
            normalize_safe_url("example.com/page").unwrap(),
            "https://example.com/page"
        );
    }

    #[test]
    fn rejects_unsafe_and_empty_urls() {
        assert_eq!(normalize_safe_url("   "), Err(HyperlinkError::EmptyUrl));
        assert_eq!(
            normalize_safe_url("javascript:alert(1)"),
            Err(HyperlinkError::UnsupportedScheme("javascript".into()))
        );
        assert_eq!(
            normalize_safe_url("  JAVASCRIPT:void(0)"),
            Err(HyperlinkError::UnsupportedScheme("javascript".into()))
        );
        assert_eq!(
            normalize_safe_url("data:text/html,<script>alert(1)</script>"),
            Err(HyperlinkError::UnsupportedScheme("data".into()))
        );
        assert_eq!(
            normalize_safe_url("file:///etc/passwd"),
            Err(HyperlinkError::UnsupportedScheme("file".into()))
        );
    }

    #[test]
    fn serde_round_trip_all_targets() {
        let cases = vec![
            Hyperlink::url("https://loom.office/docs", Some("Docs".into())).unwrap(),
            Hyperlink {
                target: HyperlinkTarget::Bookmark("section-2".into()),
                tooltip: None,
            },
            Hyperlink {
                target: HyperlinkTarget::Slide(3),
                tooltip: Some("Go to slide 4".into()),
            },
            Hyperlink {
                target: HyperlinkTarget::CellRange("Sheet1!A1:B4".into()),
                tooltip: None,
            },
            Hyperlink::email("mailto:team@loom.office", None).unwrap(),
        ];

        for link in cases {
            assert!(link.validate().is_ok());
            let json = serde_json::to_string(&link).unwrap();
            let decoded: Hyperlink = serde_json::from_str(&json).unwrap();
            assert_eq!(link, decoded);
        }
    }
}
