//! XML text helpers for the WordprocessingML writer.
//!
//! Word refuses a package whose XML holds a character that XML 1.0 forbids
//! (most control characters), so every string that reaches a part goes
//! through [`text`] or [`attr`], which drop those characters and escape the
//! markup characters.

/// Namespace of the WordprocessingML main vocabulary.
pub(super) const W_NS: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
/// Namespace of relationship attributes in the main vocabulary.
pub(super) const R_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
/// Markup-compatibility namespace, for `mc:Ignorable`.
pub(super) const MC_NS: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
/// Word 2010 extension namespace (paragraph ids).
pub(super) const W14_NS: &str = "http://schemas.microsoft.com/office/word/2010/wordml";
/// Word 2012 extension namespace (resolved comments).
pub(super) const W15_NS: &str = "http://schemas.microsoft.com/office/word/2012/wordml";

/// Declaration that starts every part.
pub(super) const XML_DECLARATION: &str =
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";

/// Whether XML 1.0 allows `c` in element and attribute content.
pub(super) fn is_xml_char(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{A}' | '\u{D}' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..='\u{10FFFF}'
    )
}

/// Escapes `value` for element content, dropping characters XML forbids.
pub(super) fn text(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars().filter(|c| is_xml_char(*c)) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            other => out.push(other),
        }
    }
    out
}

/// Escapes `value` for a double-quoted attribute, dropping forbidden
/// characters. Line breaks and tabs become character references so the
/// attribute value survives a parser's whitespace normalization.
pub(super) fn attr(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars().filter(|c| is_xml_char(*c)) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            other => out.push(other),
        }
    }
    out
}

/// Appends the children of a run for `value`: `<w:t>` for text, `<w:tab/>`
/// for a tab and `<w:br/>` for a line break (`\n` or vertical tab). Carriage
/// returns and characters XML forbids are dropped. Returns whether anything
/// was written, so callers can skip an empty run.
pub(super) fn push_run_children(out: &mut String, value: &str) -> bool {
    let start = out.len();
    let mut pending = String::new();
    let flush = |out: &mut String, pending: &mut String| {
        if !pending.is_empty() {
            out.push_str("<w:t xml:space=\"preserve\">");
            out.push_str(&text(pending));
            out.push_str("</w:t>");
            pending.clear();
        }
    };
    for c in value.chars() {
        match c {
            '\t' => {
                flush(out, &mut pending);
                out.push_str("<w:tab/>");
            }
            '\n' | '\u{B}' => {
                flush(out, &mut pending);
                out.push_str("<w:br/>");
            }
            '\r' => {}
            c if is_xml_char(c) => pending.push(c),
            _ => {}
        }
    }
    flush(out, &mut pending);
    out.len() != start
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_escapes_markup_and_drops_forbidden_characters() {
        assert_eq!(
            text("a & b < c > d \"q\" 'x'"),
            "a &amp; b &lt; c &gt; d \"q\" 'x'"
        );
        assert_eq!(text("bell\u{7}\u{0}end\u{FFFE}"), "bellend");
        assert_eq!(
            text("caf\u{e9} \u{65e5}\u{672c} \u{1F600}"),
            "caf\u{e9} \u{65e5}\u{672c} \u{1F600}"
        );
    }

    #[test]
    fn attr_also_escapes_quotes_and_whitespace_controls() {
        assert_eq!(attr("a\"b'c\td\ne"), "a&quot;b&apos;c&#9;d&#10;e");
    }

    #[test]
    fn run_children_map_tab_and_line_break_and_skip_empty_runs() {
        let mut out = String::new();
        assert!(push_run_children(&mut out, "a\tb\nc\r\u{B}d"));
        assert_eq!(
            out,
            "<w:t xml:space=\"preserve\">a</w:t><w:tab/><w:t xml:space=\"preserve\">b</w:t><w:br/>\
             <w:t xml:space=\"preserve\">c</w:t><w:br/><w:t xml:space=\"preserve\">d</w:t>"
        );
        let mut empty = String::new();
        assert!(!push_run_children(&mut empty, "\u{7}\r"));
        assert!(empty.is_empty());
    }
}
