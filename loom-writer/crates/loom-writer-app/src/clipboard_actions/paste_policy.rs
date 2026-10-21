//! Which clipboard payloads Paste turns into a table. A table is a table only
//! when it is the whole payload (HTML) or when plain tab-separated text has the
//! shape of a grid; a table inside running text pastes as its text.

/// Header lines a Windows HTML clipboard puts before the markup.
const HTML_HEADER_KEYS: [&str; 8] = [
    "Version:",
    "StartHTML:",
    "EndHTML:",
    "StartFragment:",
    "EndFragment:",
    "StartSelection:",
    "EndSelection:",
    "SourceURL:",
];

/// Whether the HTML contains a table at all.
pub(super) fn html_has_table(html: &str) -> bool {
    html.to_ascii_lowercase().contains("<table")
}

/// Whether a table is the whole of the HTML payload: nothing but white space
/// outside it.
pub(super) fn html_is_only_table(html: &str) -> bool {
    let outside = strip_table_span(&strip_header_lines(html));
    visible_text(&outside).trim().is_empty()
}

/// Whether tab-separated text is a grid: at least two rows, each with the same
/// number of tabs (one or more), and no row indented by a leading tab or space,
/// which is how code is laid out.
pub(super) fn tsv_is_table(tsv: &str) -> bool {
    let rows: Vec<&str> = tsv.lines().filter(|line| !line.trim().is_empty()).collect();
    let Some(first) = rows.first() else {
        return false;
    };
    let tabs = first.matches('\t').count();
    rows.len() >= 2
        && tabs >= 1
        && rows
            .iter()
            .all(|row| row.matches('\t').count() == tabs && !row.starts_with(['\t', ' ']))
}

/// The readable text of HTML: block elements become line breaks, table cells
/// become tabs, markup, comments, head, style and script are dropped.
pub(super) fn visible_text(html: &str) -> String {
    let html = strip_header_lines(html);
    let html = remove_spans(&html, "<!--", "-->");
    let html = remove_spans(&html, "<head", "</head>");
    let html = remove_spans(&html, "<style", "</style>");
    let html = remove_spans(&html, "<script", "</script>");
    let mut out = String::with_capacity(html.len());
    let mut tag = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        if in_tag {
            if c == '>' {
                in_tag = false;
                push_for_tag(&tag, &mut out);
                tag.clear();
            } else {
                tag.push(c);
            }
        } else if c == '<' {
            in_tag = true;
        } else {
            out.push(c);
        }
    }
    decode_entities(&out)
}

fn push_for_tag(tag: &str, out: &mut String) {
    let closing = tag.starts_with('/');
    let name: String = tag
        .trim_start_matches('/')
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_ascii_lowercase();
    match name.as_str() {
        "td" | "th" if closing => out.push('\t'),
        "p" | "div" | "br" | "tr" | "li" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "table"
        | "ul" | "ol" | "blockquote" => out.push('\n'),
        _ => {}
    }
}

fn strip_header_lines(html: &str) -> String {
    html.lines()
        .filter(|line| {
            let trimmed = line.trim();
            !HTML_HEADER_KEYS.iter().any(|key| trimmed.starts_with(key))
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The HTML with everything from the first `<table` to the last `</table>`
/// removed; left as it is when the table is not closed.
fn strip_table_span(html: &str) -> String {
    let lower = html.to_ascii_lowercase();
    match (lower.find("<table"), lower.rfind("</table>")) {
        (Some(start), Some(end)) if end >= start => {
            let close_end = end + "</table>".len();
            format!("{}{}", &html[..start], &html[close_end..])
        }
        _ => html.to_string(),
    }
}

/// Removes each `open ... close` span. ASCII lower-casing keeps byte offsets,
/// so the spans are cut from the original text. An unclosed span runs to the end.
fn remove_spans(html: &str, open: &str, close: &str) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut at = 0;
    while let Some(found) = lower[at..].find(open) {
        let start = at + found;
        out.push_str(&html[at..start]);
        let after = start + open.len();
        match lower[after..].find(close) {
            Some(end) => at = after + end + close.len(),
            None => {
                at = html.len();
                break;
            }
        }
    }
    out.push_str(&html[at..]);
    out
}

fn decode_entities(text: &str) -> String {
    text.replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}
