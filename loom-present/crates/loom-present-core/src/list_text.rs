use serde::{Deserialize, Serialize};

/// Per-paragraph list style inside a slide text box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ListStyle {
    #[default]
    None,
    Bullet,
    Numbered,
}

/// A single paragraph with list formatting and indentation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListParagraph {
    pub text: String,
    #[serde(default)]
    pub style: ListStyle,
    #[serde(default)]
    pub indent_level: u8,
}

/// Parses plain text into `ListParagraph`s, preserving any existing `• ` or `N. ` prefixes.
pub fn parse_list_paragraphs(text: &str) -> Vec<ListParagraph> {
    text.lines()
        .map(|line| {
            let trimmed_start = line.trim_start_matches("  ");
            let spaces = line.len() - trimmed_start.len();
            let indent_level = (spaces / 2).min(4) as u8;
            if let Some(rest) = trimmed_start.strip_prefix("• ") {
                ListParagraph {
                    text: rest.to_string(),
                    style: ListStyle::Bullet,
                    indent_level,
                }
            } else if let Some((num, rest)) = trimmed_start.split_once(". ") {
                if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
                    return ListParagraph {
                        text: rest.to_string(),
                        style: ListStyle::Numbered,
                        indent_level,
                    };
                }
                ListParagraph {
                    text: trimmed_start.to_string(),
                    style: ListStyle::None,
                    indent_level,
                }
            } else {
                ListParagraph {
                    text: trimmed_start.to_string(),
                    style: ListStyle::None,
                    indent_level,
                }
            }
        })
        .collect()
}

/// Renders a list prefix for a paragraph at `index_in_run` (1-based for Numbered).
pub fn render_list_prefix(style: ListStyle, index_in_run: usize, indent_level: u8) -> String {
    let indent = "  ".repeat(indent_level as usize);
    match style {
        ListStyle::None => indent,
        ListStyle::Bullet => format!("{indent}• "),
        ListStyle::Numbered => format!("{indent}{index_in_run}. "),
    }
}

/// Formats `ListParagraph`s back into display text with bullets/numbers and indentation.
pub fn format_list_paragraphs(paragraphs: &[ListParagraph]) -> String {
    let mut num_counter = 0usize;
    let mut out = Vec::with_capacity(paragraphs.len());
    for p in paragraphs {
        if p.style == ListStyle::Numbered {
            num_counter += 1;
        } else {
            num_counter = 0;
        }
        let prefix = render_list_prefix(p.style, num_counter, p.indent_level);
        out.push(format!("{prefix}{}", p.text));
    }
    out.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_bullet_and_numbered() {
        let src = "• First\n  • Nested\n1. One\n2. Two\nPlain";
        let paras = parse_list_paragraphs(src);
        assert_eq!(paras.len(), 5);
        assert_eq!(paras[0].style, ListStyle::Bullet);
        assert_eq!(paras[1].indent_level, 1);
        assert_eq!(paras[2].style, ListStyle::Numbered);
        assert_eq!(paras[4].style, ListStyle::None);
        assert_eq!(format_list_paragraphs(&paras), src);
    }
}
