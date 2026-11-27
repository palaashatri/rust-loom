//! Minimal sanitized HTML <-> `loom_text::StyleRun` adapter for cross-app rich clipboard.

use loom_text::{CharacterStyle, FontWeight, StyleRun};

/// Converts plain text + `StyleRun`s into minimal sanitized HTML (`<b>`, `<i>`, `<u>`, `<s>`).
/// Overlapping runs are clamped to `cursor`; unsupported style attributes are omitted.
pub fn runs_to_html(text: &str, runs: &[StyleRun]) -> String {
    if runs.is_empty() {
        return escape_html(text);
    }
    let mut out = String::new();
    let mut cursor = 0usize;
    let mut sorted: Vec<&StyleRun> = runs.iter().collect();
    sorted.sort_by_key(|r| r.start);
    for run in sorted {
        let start = run.start.max(cursor).min(text.len());
        let end = run.end.min(text.len());
        if end <= start {
            continue;
        }
        if start > cursor {
            out.push_str(&escape_html(&text[cursor..start]));
        }
        let chunk = escape_html(&text[start..end]);
        out.push_str(&wrap_style(&chunk, &run.style));
        cursor = end;
    }
    if cursor < text.len() {
        out.push_str(&escape_html(&text[cursor..]));
    }
    out
}

fn wrap_style(inner: &str, style: &CharacterStyle) -> String {
    let mut s = inner.to_string();
    if style.weight == FontWeight::Bold || style.weight == FontWeight::Black {
        s = format!("<b>{s}</b>");
    }
    if style.italic {
        s = format!("<i>{s}</i>");
    }
    if style.underline {
        s = format!("<u>{s}</u>");
    }
    if style.strikethrough {
        s = format!("<s>{s}</s>");
    }
    s
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Parses minimal HTML (`<b>`, `<i>`, `<u>`, `<s>`) back into `(plain_text, Vec<StyleRun>)`.
pub fn html_to_runs(html: &str) -> (String, Vec<StyleRun>) {
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut bold = 0usize;
    let mut italic = 0usize;
    let mut underline = 0usize;
    let mut strike = 0usize;
    let mut rest = html;

    while let Some(lt) = rest.find('<') {
        let plain = unescape_html(&rest[..lt]);
        push_styled(
            &mut text,
            &mut runs,
            &plain,
            bold > 0,
            italic > 0,
            underline > 0,
            strike > 0,
        );
        let after_lt = &rest[lt + 1..];
        let Some(gt) = after_lt.find('>') else {
            rest = &rest[lt..];
            break;
        };
        let tag = after_lt[..gt].trim().to_ascii_lowercase();
        match tag.as_str() {
            "b" | "strong" => bold += 1,
            "/b" | "/strong" => bold = bold.saturating_sub(1),
            "i" | "em" => italic += 1,
            "/i" | "/em" => italic = italic.saturating_sub(1),
            "u" => underline += 1,
            "/u" => underline = underline.saturating_sub(1),
            "s" | "del" => strike += 1,
            "/s" | "/del" => strike = strike.saturating_sub(1),
            _ => {}
        }
        rest = &after_lt[gt + 1..];
    }
    if !rest.is_empty() {
        let plain = unescape_html(rest);
        push_styled(
            &mut text,
            &mut runs,
            &plain,
            bold > 0,
            italic > 0,
            underline > 0,
            strike > 0,
        );
    }
    (text, runs)
}

fn push_styled(
    text: &mut String,
    runs: &mut Vec<StyleRun>,
    plain: &str,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
) {
    if plain.is_empty() {
        return;
    }
    let start = text.len();
    text.push_str(plain);
    let end = text.len();
    if bold || italic || underline || strikethrough {
        runs.push(StyleRun {
            start,
            end,
            style: CharacterStyle {
                weight: if bold {
                    FontWeight::Bold
                } else {
                    FontWeight::Regular
                },
                italic,
                underline,
                strikethrough,
                ..Default::default()
            },
        });
    }
}

fn unescape_html(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_minimal_html_runs() {
        let runs = vec![StyleRun {
            start: 6,
            end: 11,
            style: CharacterStyle {
                weight: FontWeight::Bold,
                italic: true,
                ..Default::default()
            },
        }];
        let html = runs_to_html("Hello world!", &runs);
        assert_eq!(html, "Hello <i><b>world</b></i>!");
        let (plain, parsed) = html_to_runs(&html);
        assert_eq!(plain, "Hello world!");
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].start, 6);
        assert_eq!(parsed[0].end, 11);
        assert_eq!(parsed[0].style.weight, FontWeight::Bold);
        assert!(parsed[0].style.italic);
    }

    #[test]
    fn empty_run_and_incomplete_tag_do_not_duplicate_text() {
        let empty_run = StyleRun {
            start: 2,
            end: 2,
            style: CharacterStyle {
                weight: FontWeight::Bold,
                ..Default::default()
            },
        };
        assert_eq!(runs_to_html("abcd", &[empty_run]), "abcd");

        let (plain, runs) = html_to_runs("A<");
        assert_eq!(plain, "A<");
        assert!(runs.is_empty());
    }
}
