//! Windows CF_HTML clipboard envelope encoder and decoder.

/// Wraps an HTML fragment inside a Windows CF_HTML clipboard container with exact byte offsets.
pub fn encode_cf_html(fragment: &str) -> String {
    let header_prefix = "Version:0.9\r\nStartHTML:00000000\r\nEndHTML:00000000\r\nStartFragment:00000000\r\nEndFragment:00000000\r\n";
    let html_open = "<html>\r\n<body>\r\n<!--StartFragment-->";
    let html_close = "<!--EndFragment-->\r\n</body>\r\n</html>";

    let header_len = header_prefix.len();
    let start_html = header_len;
    let start_fragment = header_len + html_open.len();
    let end_fragment = start_fragment + fragment.len();
    let end_html = end_fragment + html_close.len();

    format!(
        "Version:0.9\r\nStartHTML:{:08}\r\nEndHTML:{:08}\r\nStartFragment:{:08}\r\nEndFragment:{:08}\r\n{}{}{}",
        start_html, end_html, start_fragment, end_fragment, html_open, fragment, html_close
    )
}

/// Extracts the HTML fragment from a Windows CF_HTML container.
pub fn decode_cf_html(cf_html: &str) -> String {
    if let (Some(sf_start), Some(ef_start)) = (
        cf_html.find("<!--StartFragment-->"),
        cf_html.find("<!--EndFragment-->"),
    ) {
        let content_start = sf_start + "<!--StartFragment-->".len();
        if content_start < ef_start {
            return cf_html[content_start..ef_start].to_string();
        }
    }

    // Fallback: parse offsets
    if let (Some(sf_val), Some(ef_val)) = (
        parse_offset(cf_html, "StartFragment:"),
        parse_offset(cf_html, "EndFragment:"),
    ) {
        if sf_val < ef_val && ef_val <= cf_html.len() {
            return cf_html[sf_val..ef_val].to_string();
        }
    }

    cf_html.to_string()
}

fn parse_offset(text: &str, key: &str) -> Option<usize> {
    let start = text.find(key)? + key.len();
    let line_end = text[start..]
        .find("\r\n")
        .or_else(|| text[start..].find('\n'))?
        + start;
    text[start..line_end].trim().parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cf_html_round_trip() {
        let html = "<table><tr><td>Cell</td></tr></table>";
        let encoded = encode_cf_html(html);
        assert!(encoded.starts_with("Version:0.9"));
        assert!(encoded.contains("StartFragment:"));
        assert!(encoded.contains("<!--StartFragment-->"));

        let decoded = decode_cf_html(&encoded);
        assert_eq!(decoded, html);
    }
}
