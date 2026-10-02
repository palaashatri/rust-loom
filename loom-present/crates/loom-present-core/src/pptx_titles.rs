//! Slide-title level PowerPoint (`.pptx`) support: a titles-only exporter and a
//! title extractor that also reads decks written by [`crate::export_pptx`].

use crate::Slide;
use loom_package::zip::PackageArchive;

/// Archive prefix under which PPTX slide parts live.
const PPTX_SLIDE_PART_PREFIX: &str = "ppt/slides/slide";

/// Exports slide titles into a minimal valid `.pptx` archive: one slide part per title,
/// each carrying a single text paragraph. Round-trips through [`extract_pptx_titles`]
/// in order; empty titles are preserved as slides with empty first paragraphs.
///
/// This is the titles-only exporter. The full deck exporter is [`crate::export_pptx`].
pub fn export_pptx_from_titles(titles: &[String]) -> Result<Vec<u8>, String> {
    let mut content_overrides = String::new();
    let mut presentation_refs = String::new();
    let mut presentation_rels = String::new();
    let mut slide_parts: Vec<(String, Vec<u8>)> = Vec::new();

    for (index, title) in titles.iter().enumerate() {
        let number = index + 1;
        content_overrides.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{number}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
        presentation_refs.push_str(&format!(
            "<p:sldId id=\"{number}\" r:id=\"rIdSlide{number}\"/>"
        ));
        presentation_rels.push_str(&format!(
            "<Relationship Id=\"rIdSlide{number}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{number}.xml\"/>"
        ));

        let escaped = xml_escape_pptx(title);
        let slide_xml = format!(
            "<?xml version=\"1.0\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:sp><p:txBody><a:p><a:r><a:t>{escaped}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld></p:sld>"
        );
        slide_parts.push((
            format!("ppt/slides/slide{number}.xml"),
            slide_xml.into_bytes(),
        ));
    }

    let mut parts: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".to_string(),
            format!(
                "<?xml version=\"1.0\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>{content_overrides}</Types>"
            )
            .into_bytes(),
        ),
        (
            "_rels/.rels".to_string(),
            "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>".to_string().into_bytes(),
        ),
        (
            "ppt/presentation.xml".to_string(),
            format!(
                "<?xml version=\"1.0\"?><p:presentation xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:sldIdLst>{presentation_refs}</p:sldIdLst></p:presentation>"
            )
            .into_bytes(),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            format!(
                "<?xml version=\"1.0\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{presentation_rels}</Relationships>"
            )
            .into_bytes(),
        ),
    ];
    parts.extend(slide_parts);

    let mut archive = PackageArchive::new();
    for (path, data) in &parts {
        archive
            .add(path, data.clone())
            .map_err(|e| format!("pptx export failed: {e}"))?;
    }
    archive
        .to_bytes()
        .map_err(|e| format!("pptx export failed: {e}"))
}

/// Escapes XML attribute/text characters for presentation parts.
fn xml_escape_pptx(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Extracts slide titles from a .pptx archive in slide order. Discovers slide parts among
/// archive paths matching `ppt/slides/slide<N>.xml`, sorts N numerically, then per slide
/// extracts the title: the first paragraph of a title placeholder (`<p:ph type="title"/>`
/// or `ctrTitle`) when the slide has one, otherwise the FIRST `<a:p>...</a:p>` paragraph's
/// concatenated `<a:t>` runs (documented heuristic: first paragraph = title line). Slides
/// whose title paragraph has no text yield an empty-string title. The five predefined XML
/// entities are unescaped in run text. Returns Err on unreadable archives or when no slide
/// parts exist.
///
/// This is a targeted byte scan, not a validating XML parser: malformed or unusual slide
/// markup degrades to an empty title rather than failing the whole import.
pub fn extract_pptx_titles(pptx_bytes: &[u8]) -> Result<Vec<String>, String> {
    let archive = PackageArchive::from_bytes(pptx_bytes)
        .map_err(|err| format!("unreadable pptx archive: {err}"))?;
    let mut slide_parts: Vec<(u64, &str)> = Vec::new();
    for path in archive.paths() {
        let Some(number) = path
            .strip_prefix(PPTX_SLIDE_PART_PREFIX)
            .and_then(|rest| rest.strip_suffix(".xml"))
        else {
            continue;
        };
        if number.is_empty() || !number.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if let Ok(parsed) = number.parse::<u64>() {
            slide_parts.push((parsed, path));
        }
    }
    if slide_parts.is_empty() {
        return Err(
            "pptx archive contains no slide parts (expected ppt/slides/slide<N>.xml)".to_string(),
        );
    }
    slide_parts.sort_unstable_by_key(|(number, _)| *number);
    slide_parts
        .iter()
        .map(|(_, path)| {
            let data = archive
                .get(path)
                .ok_or_else(|| format!("missing slide part {path}"))?;
            Ok(pptx_slide_title(&String::from_utf8_lossy(data)))
        })
        .collect()
}

/// Convenience building a full outline-importable deck skeleton from a .pptx archive: one
/// [`Slide`] per extracted title (see [`extract_pptx_titles`]) with ids `pptx-slide-N`
/// (N is the 1-based slide position) and layout `imported-pptx`. Error conditions are
/// inherited from [`extract_pptx_titles`].
pub fn slides_from_pptx(pptx_bytes: &[u8]) -> Result<Vec<Slide>, String> {
    Ok(extract_pptx_titles(pptx_bytes)?
        .into_iter()
        .enumerate()
        .map(|(index, title)| {
            Slide::new(format!("pptx-slide-{}", index + 1), title, "imported-pptx")
        })
        .collect())
}

/// The slide's title: the first paragraph of its title placeholder when it has one,
/// otherwise the first paragraph on the slide.
fn pptx_slide_title(slide_xml: &str) -> String {
    match title_placeholder_span(slide_xml) {
        Some((open, close)) => pptx_first_paragraph_title(&slide_xml[open..close]),
        None => pptx_first_paragraph_title(slide_xml),
    }
}

/// Byte span of the first shape whose placeholder type is `title` or `ctrTitle`: from its
/// `<p:ph ...>` tag to the end of the enclosing `<p:sp>`.
fn title_placeholder_span(slide_xml: &str) -> Option<(usize, usize)> {
    let mut from = 0usize;
    while let Some(rel) = slide_xml[from..].find("<p:ph") {
        let start = from + rel;
        let after = start + "<p:ph".len();
        let tag_end = after + slide_xml[after..].find('>')?;
        let tag = &slide_xml[start..tag_end];
        let is_title = tag.contains("type=\"title\"") || tag.contains("type=\"ctrTitle\"");
        // A placeholder tag is the start of a shape only for `<p:ph ` and `<p:ph/>`,
        // never for a longer element name that shares the prefix.
        let named_ph = matches!(
            slide_xml.as_bytes().get(after),
            Some(b' ') | Some(b'/') | Some(b'>')
        );
        if named_ph && is_title {
            let end = slide_xml[tag_end..]
                .find("</p:sp>")
                .map(|offset| tag_end + offset)
                .unwrap_or(slide_xml.len());
            return Some((start, end));
        }
        from = tag_end;
    }
    None
}

/// Extracts the concatenated `<a:t>` run text of the FIRST `<a:p>` paragraph in a slide
/// part, or an empty string when the slide has no usable first paragraph.
fn pptx_first_paragraph_title(slide_xml: &str) -> String {
    let Some((open, close)) = next_xml_element_inner(slide_xml, 0, "a:p") else {
        return String::new();
    };
    let paragraph = &slide_xml[open..close];
    let mut title = String::new();
    let mut cursor = 0usize;
    while let Some((run_open, run_close)) = next_xml_element_inner(paragraph, cursor, "a:t") {
        title.push_str(&unescape_xml_entities(&paragraph[run_open..run_close]));
        cursor = run_close;
    }
    title
}

/// Scans `xml` from byte offset `from` for the inner span of the next `<tag ...>...</tag>`
/// element, returning `(inner_start, inner_end)`. Handles attribute-bearing open tags by
/// skipping ahead to the open tag's closing `>`; a self-closing `<tag/>` yields an empty
/// span so callers observe that the element existed without content. Longer tags that share
/// the prefix (such as `<a:pPr>` for tag `a:p`) are skipped. Returns None when no further
/// occurrence exists or an open tag is never closed.
fn next_xml_element_inner(xml: &str, from: usize, tag: &str) -> Option<(usize, usize)> {
    let open_needle = format!("<{tag}");
    let close_needle = format!("</{tag}>");
    let bytes = xml.as_bytes();
    let mut pos = from;
    while let Some(rel) = xml[pos..].find(&open_needle) {
        let after_name = pos + rel + open_needle.len();
        match bytes.get(after_name) {
            Some(b'>') | Some(b' ') | Some(b'\t') | Some(b'\r') | Some(b'\n') => {
                let inner_start =
                    after_name + bytes[after_name..].iter().position(|&b| b == b'>')? + 1;
                let close_rel = xml[inner_start..].find(&close_needle)?;
                return Some((inner_start, inner_start + close_rel));
            }
            Some(b'/') if bytes.get(after_name + 1) == Some(&b'>') => {
                return Some((after_name + 2, after_name + 2));
            }
            _ => pos = after_name,
        }
    }
    None
}

/// Unescapes the five predefined XML entities (`&amp;`, `&lt;`, `&gt;`, `&quot;`,
/// `&apos;`) in a single left-to-right pass, so escaped text such as `&amp;lt;` decodes to
/// `&lt;` rather than being double-decoded to `<`. Unknown or malformed entities pass
/// through unchanged.
fn unescape_xml_entities(text: &str) -> String {
    const ENTITIES: [(&str, char); 5] = [
        ("&amp;", '&'),
        ("&lt;", '<'),
        ("&gt;", '>'),
        ("&quot;", '"'),
        ("&apos;", '\''),
    ];
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut chars = text.char_indices();
    while let Some((index, ch)) = chars.next() {
        if ch == '&' {
            let rest = &text[index..];
            if let Some((entity, decoded)) =
                ENTITIES.iter().find(|(name, _)| rest.starts_with(*name))
            {
                out.push(*decoded);
                for _ in 1..entity.chars().count() {
                    chars.next();
                }
                continue;
            }
        }
        out.push(ch);
    }
    out
}
