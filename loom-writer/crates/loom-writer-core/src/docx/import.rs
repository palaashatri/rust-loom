//! Word document (`.docx`) import into [`WriterDocument`].
//!
//! Imports paragraphs, headings (1-6), lists (bulleted and numbered),
//! inline character formatting (bold, italic, underline, strikethrough, fonts),
//! tables (`<w:tbl>`), and document core properties (title).

use loom_package::zip::PackageArchive;
use loom_text::{Alignment, CharacterStyle, FontWeight, ParagraphStyle, StyleRun};

use crate::tables::TABLE_BLOCK_KIND;
use crate::{RichBlock, WriterDocument, WriterTable};

/// Unescapes the five predefined XML entities in one left-to-right pass.
pub(crate) fn xml_unescape(value: &str) -> String {
    if !value.contains('&') {
        return value.to_string();
    }
    let mut out = String::with_capacity(value.len());
    let bytes = value.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'&' {
            let rest = &value[index..];
            if rest.starts_with("&lt;") {
                out.push('<');
                index += 4;
                continue;
            }
            if rest.starts_with("&gt;") {
                out.push('>');
                index += 4;
                continue;
            }
            if rest.starts_with("&quot;") {
                out.push('"');
                index += 6;
                continue;
            }
            if rest.starts_with("&apos;") {
                out.push('\'');
                index += 6;
                continue;
            }
            if rest.starts_with("&amp;") {
                out.push('&');
                index += 5;
                continue;
            }
        }
        out.push(bytes[index] as char);
        index += 1;
    }
    out
}

/// True when a list paragraph sits below the first numbering level.
fn has_nested_list_level(xml: &str) -> bool {
    let mut rest = xml;
    while let Some(pos) = rest.find("<w:ilvl w:val=\"") {
        rest = &rest[pos + "<w:ilvl w:val=\"".len()..];
        let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
        if digits.parse::<u32>().is_ok_and(|level| level > 0) {
            return true;
        }
    }
    false
}

/// Inspects a `.docx` package archive for unsupported constructs.
pub fn inspect_docx_unsupported(docx_bytes: &[u8]) -> Result<Vec<String>, String> {
    let archive = PackageArchive::from_bytes(docx_bytes)
        .map_err(|err| format!("unreadable docx archive: {err}"))?;
    let mut warnings = Vec::new();

    for path in archive.paths() {
        if (path.starts_with("word/diagrams/") || path.contains("diagram"))
            && !warnings.iter().any(|w: &String| w.contains("SmartArt"))
        {
            warnings.push("SmartArt diagrams are not supported and will be omitted.".to_string());
        }
        if path.starts_with("word/embeddings/")
            && !warnings.iter().any(|w: &String| w.contains("OLE"))
        {
            warnings.push("Embedded OLE objects are not supported.".to_string());
        }
        if (path.contains("vbaProject.bin") || path.contains("vbaData.xml"))
            && !warnings.iter().any(|w: &String| w.contains("VBA"))
        {
            warnings.push("Macros and VBA code are not supported.".to_string());
        }
        if (path.ends_with("footnotes.xml") || path.ends_with("endnotes.xml"))
            && !warnings.iter().any(|w: &String| w.contains("Footnotes"))
        {
            warnings.push("Footnotes and endnotes are not supported.".to_string());
        }
        let lower = path.to_ascii_lowercase();
        if (path.starts_with("word/media/") || path.starts_with("media/"))
            && ![".png", ".jpg", ".jpeg"]
                .iter()
                .any(|ext| lower.ends_with(ext))
            && !warnings.iter().any(|w: &String| w.contains("PNG and JPEG"))
        {
            warnings.push(
                "Pictures other than PNG and JPEG are not supported and will be omitted."
                    .to_string(),
            );
        }
    }

    if let Some(doc_bytes) = archive.get("word/document.xml") {
        let xml = String::from_utf8_lossy(doc_bytes);
        if (xml.contains("<m:oMath") || xml.contains("<m:oMathPara"))
            && !warnings.iter().any(|w: &String| w.contains("Equation"))
        {
            warnings.push("Mathematical equations (OMML) are not supported.".to_string());
        }
        if (xml.contains("<w:footnoteReference") || xml.contains("<w:endnoteReference"))
            && !warnings.iter().any(|w: &String| w.contains("Footnotes"))
        {
            warnings.push("Footnotes and endnotes are not supported.".to_string());
        }
        if (xml.contains("<w:pict") || xml.contains("<v:imagedata"))
            && !warnings.iter().any(|w: &String| w.contains("older"))
        {
            warnings.push(
                "Pictures in the older Word format are not supported and will be omitted."
                    .to_string(),
            );
        }
        if xml.contains("<wp:anchor") && !warnings.iter().any(|w: &String| w.contains("Floating")) {
            warnings.push("Floating pictures are placed in the text, one to a line.".to_string());
        }
        // Bullets and numbering import; only nested levels are flattened.
        if has_nested_list_level(&xml) {
            warnings.push("Nested list levels are flattened to a single level.".to_string());
        }
    }

    Ok(warnings)
}

/// Imports a `.docx` package archive into a complete [`WriterDocument`].
pub fn import_docx(docx_bytes: &[u8]) -> Result<WriterDocument, String> {
    let archive = PackageArchive::from_bytes(docx_bytes)
        .map_err(|e| format!("unreadable docx archive: {e}"))?;
    let document_xml = archive
        .get("word/document.xml")
        .ok_or_else(|| "docx archive missing word/document.xml".to_string())?;
    let xml = std::str::from_utf8(document_xml)
        .map_err(|_| "document.xml is not valid UTF-8".to_string())?;

    // Document title from docProps/core.xml when available.
    let title = archive
        .get("docProps/core.xml")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(extract_core_title)
        .unwrap_or_else(|| "Untitled Document".to_string());

    let mut doc = WriterDocument::new("imported-doc", title);
    doc.blocks = parse_document_body_with_archive(xml, Some(&archive))?;

    Ok(doc)
}

/// Parses the contents of `<w:body>` into a list of [`RichBlock`]s.
pub fn parse_document_body(xml: &str) -> Result<Vec<RichBlock>, String> {
    parse_document_body_with_archive(xml, None)
}

/// Parses the contents of `<w:body>` with optional archive for embedded media/charts.
pub fn parse_document_body_with_archive(
    xml: &str,
    archive: Option<&PackageArchive>,
) -> Result<Vec<RichBlock>, String> {
    let body_start_tag = "<w:body";
    let body_start = match xml.find(body_start_tag) {
        Some(pos) => {
            let after = pos + body_start_tag.len();
            let close = xml[after..]
                .find('>')
                .ok_or_else(|| "malformed <w:body> tag".to_string())?;
            after + close + 1
        }
        None => return Err("docx missing <w:body>".to_string()),
    };

    let body_end = xml[body_start..]
        .find("</w:body>")
        .map(|pos| body_start + pos)
        .unwrap_or(xml.len());

    let body_xml = &xml[body_start..body_end];
    let mut blocks = Vec::new();
    let mut cursor = 0usize;
    let mut next_id = 1u64;

    while cursor < body_xml.len() {
        let rest = &body_xml[cursor..];
        let p_pos = find_tag_start(rest, "w:p");
        let tbl_pos = find_tag_start(rest, "w:tbl");

        match (p_pos, tbl_pos) {
            (Some(p_rel), Some(tbl_rel)) if tbl_rel < p_rel => {
                let tbl_abs = cursor + tbl_rel;
                if let Some((block, next_cursor)) = parse_table(body_xml, tbl_abs, next_id) {
                    blocks.push(block);
                    next_id += 1;
                    cursor = next_cursor;
                } else {
                    cursor = tbl_abs + "<w:tbl".len();
                }
            }
            (Some(_), Some(_)) | (Some(_), None) => {
                let p_rel = p_pos.unwrap();
                let p_abs = cursor + p_rel;
                if let Some((block_opt, next_cursor)) =
                    parse_paragraph(body_xml, p_abs, next_id, archive)
                {
                    if let Some(block) = block_opt {
                        blocks.push(block);
                        next_id += 1;
                    }
                    cursor = next_cursor;
                } else {
                    cursor = p_abs + "<w:p".len();
                }
            }
            (None, Some(tbl_rel)) => {
                let tbl_abs = cursor + tbl_rel;
                if let Some((block, next_cursor)) = parse_table(body_xml, tbl_abs, next_id) {
                    blocks.push(block);
                    next_id += 1;
                    cursor = next_cursor;
                } else {
                    cursor = tbl_abs + "<w:tbl".len();
                }
            }
            (None, None) => break,
        }
    }

    Ok(blocks)
}

fn find_tag_start(xml: &str, tag_name: &str) -> Option<usize> {
    let mut offset = 0;
    let needle = format!("<{tag_name}");
    while let Some(pos) = xml[offset..].find(&needle) {
        let candidate = offset + pos;
        let after = candidate + needle.len();
        if after < xml.len() {
            let next_byte = xml.as_bytes()[after];
            if next_byte == b'>'
                || next_byte == b' '
                || next_byte == b'/'
                || next_byte == b'\n'
                || next_byte == b'\r'
                || next_byte == b'\t'
            {
                return Some(candidate);
            }
        }
        offset = candidate + needle.len();
    }
    None
}

/// A paragraph holding one inline picture as a picture block. `None` when
/// the paragraph has no picture, or its picture is not a PNG or JPEG the
/// package carries (the paragraph then imports as text, as before).
fn parse_picture(p_xml: &str, id: u64, archive: &PackageArchive) -> Option<RichBlock> {
    let drawing = &p_xml[p_xml.find("<w:drawing")?..];
    if drawing.contains("<c:chart") {
        return None;
    }
    let blip = &drawing[drawing.find("<a:blip")?..];
    let rid = extract_attribute_value(blip, "r:embed")?;
    let rels_xml = archive
        .get("word/_rels/document.xml.rels")
        .map(|b| String::from_utf8_lossy(b).to_string())
        .unwrap_or_default();
    let target = parse_rels(&rels_xml).get(&rid)?.clone();
    let path = match target.strip_prefix('/') {
        Some(absolute) => absolute.to_string(),
        None => format!("word/{target}"),
    };
    let name = extract_attribute_value(drawing, "descr")
        .filter(|descr| !descr.trim().is_empty())
        .or_else(|| path.rsplit('/').next().map(str::to_string))
        .unwrap_or_else(|| "picture".to_string());
    let mut image = crate::InlineImage::from_bytes(&name, archive.get(&path)?.to_vec()).ok()?;
    // The size Word shows it at, in English Metric Units.
    if let Some(extent) = drawing.find("<wp:extent") {
        let extent = &drawing[extent..];
        let emu = |attr: &str| {
            extract_attribute_value(extent, attr)
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| *v > 0.0)
        };
        if let (Some(cx), Some(cy)) = (emu("cx"), emu("cy")) {
            image.width_pt = cx / 12_700.0;
            image.height_pt = cy / 12_700.0;
        }
    }
    let text = crate::images::picture_text(id as u32);
    let mut block = RichBlock::new(id, crate::IMAGE_BLOCK_KIND, &text);
    block.image = Some(image);
    if p_xml.contains("<w:jc w:val=\"center\"") {
        block.style.alignment = loom_text::Alignment::Center;
    } else if p_xml.contains("<w:jc w:val=\"right\"") {
        block.style.alignment = loom_text::Alignment::Right;
    }
    Some(block)
}

/// Parses a single `<w:p>...</w:p>` block.
fn parse_paragraph(
    xml: &str,
    start: usize,
    id: u64,
    archive: Option<&PackageArchive>,
) -> Option<(Option<RichBlock>, usize)> {
    let tag_open_end = xml[start..].find('>')? + start + 1;
    let tag_str = &xml[start..tag_open_end];
    if tag_str.ends_with("/>") {
        return Some((None, tag_open_end));
    }

    let close_tag = "</w:p>";
    let end_rel = xml[start..].find(close_tag)?;
    let p_xml = &xml[start..start + end_rel + close_tag.len()];
    let next_cursor = start + end_rel + close_tag.len();

    // A picture: `<a:blip r:embed>` in a drawing, bytes from the package.
    if let Some(archive) = archive {
        if let Some(block) = parse_picture(p_xml, id, archive) {
            return Some((Some(block), next_cursor));
        }
    }

    // Check for DrawingML chart in <w:drawing>
    if let Some(archive) = archive {
        if let Some(drawing_pos) = p_xml.find("<w:drawing") {
            let drawing_xml = &p_xml[drawing_pos..];
            if drawing_xml.contains("<c:chart") {
                if let Some(rid) = extract_attribute_value(drawing_xml, "r:id") {
                    let rels_xml = archive
                        .get("word/_rels/document.xml.rels")
                        .map(|b| String::from_utf8_lossy(b).to_string())
                        .unwrap_or_default();
                    let rels = parse_rels(&rels_xml);
                    if let Some(target) = rels.get(&rid) {
                        let chart_path = if target.starts_with('/') {
                            target.trim_start_matches('/').to_string()
                        } else {
                            format!("word/{target}")
                        };
                        if let Some(chart_bytes) = archive.get(&chart_path) {
                            let chart_xml = String::from_utf8_lossy(chart_bytes);
                            if let Ok(chart) = loom_objects::chart_ooxml::c_chart_xml_to_chart(
                                &chart_xml,
                                format!("chart-{id}"),
                            ) {
                                if let Ok(json) = chart.to_json() {
                                    let block = RichBlock::new(id, crate::CHART_BLOCK_KIND, &json);
                                    return Some((Some(block), next_cursor));
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Style & Kind
    let mut kind = "paragraph".to_string();
    if let Some(style_start) = p_xml.find("<w:pStyle") {
        if let Some(val) = extract_attribute_value(&p_xml[style_start..], "w:val") {
            let val_lower = val.to_ascii_lowercase();
            if val_lower.starts_with("heading") {
                let num_str: String = val_lower.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(lvl) = num_str.parse::<u8>() {
                    if (1..=6).contains(&lvl) {
                        kind = format!("heading{lvl}");
                    }
                }
            } else if val_lower == "title" {
                kind = "heading1".to_string();
            } else if val_lower == "subtitle" {
                kind = "heading2".to_string();
            } else if val_lower == "listparagraph"
                || val_lower == "bullet"
                || val_lower.contains("list")
            {
                kind = "list-bulleted".to_string();
            }
        }
    }

    // Numbering / list detection
    if let Some(num_start) = p_xml.find("<w:numPr") {
        let num_end = p_xml[num_start..]
            .find("</w:numPr>")
            .unwrap_or(p_xml.len() - num_start);
        let num_snippet = &p_xml[num_start..num_start + num_end];
        if let Some(num_id_pos) = num_snippet.find("<w:numId") {
            if let Some(num_id_val) = extract_attribute_value(&num_snippet[num_id_pos..], "w:val") {
                if let Some(list_kind) = resolve_num_id_list_kind(archive, &num_id_val) {
                    kind = list_kind.to_string();
                }
            } else {
                kind = "list-bulleted".to_string();
            }
        } else {
            kind = "list-bulleted".to_string();
        }
    }

    // Paragraph alignment
    let mut paragraph_style = ParagraphStyle::default();
    if let Some(jc_start) = p_xml.find("<w:jc") {
        if let Some(jc_val) = extract_attribute_value(&p_xml[jc_start..], "w:val") {
            match jc_val.to_ascii_lowercase().as_str() {
                "center" => paragraph_style.alignment = Alignment::Center,
                "right" => paragraph_style.alignment = Alignment::Right,
                "both" => paragraph_style.alignment = Alignment::Justify,
                _ => {}
            }
        }
    }

    // Runs
    let mut text = String::new();
    let mut runs = Vec::new();
    let mut r_cursor = 0usize;

    while let Some(r_open_rel) = find_tag_start(&p_xml[r_cursor..], "w:r") {
        let r_start = r_cursor + r_open_rel;
        let r_open_end = match p_xml[r_start..].find('>') {
            Some(end) => r_start + end + 1,
            None => break,
        };
        let r_tag_str = &p_xml[r_start..r_open_end];
        if r_tag_str.ends_with("/>") {
            r_cursor = r_open_end;
            continue;
        }

        let r_close_rel = match p_xml[r_start..].find("</w:r>") {
            Some(end) => end,
            None => break,
        };
        let r_xml = &p_xml[r_start..r_start + r_close_rel + "</w:r>".len()];
        r_cursor = r_start + r_close_rel + "</w:r>".len();

        let run_style = parse_run_style(r_xml);
        let run_text = parse_run_text(r_xml);
        if !run_text.is_empty() {
            let start_byte = text.len();
            text.push_str(&run_text);
            let end_byte = text.len();
            if run_style != CharacterStyle::default() {
                runs.push(StyleRun {
                    start: start_byte,
                    end: end_byte,
                    style: run_style,
                });
            }
        }
    }

    if text.trim().is_empty() {
        return Some((None, next_cursor));
    }

    let mut block = RichBlock::new(id, &kind, &text);
    block.style = paragraph_style;
    block.runs = runs;
    Some((Some(block), next_cursor))
}

/// Parses character formatting from `<w:rPr>...</w:rPr>`.
fn parse_run_style(r_xml: &str) -> CharacterStyle {
    let mut style = CharacterStyle::default();
    let Some(pr_start) = r_xml.find("<w:rPr") else {
        return style;
    };
    let pr_end = r_xml[pr_start..]
        .find("</w:rPr>")
        .unwrap_or(r_xml.len() - pr_start);
    let pr = &r_xml[pr_start..pr_start + pr_end];

    if tag_is_enabled(pr, "w:b") {
        style.weight = FontWeight::Bold;
    }
    if tag_is_enabled(pr, "w:i") {
        style.italic = true;
    }
    if tag_is_enabled(pr, "w:u") {
        style.underline = true;
    }
    if tag_is_enabled(pr, "w:strike") {
        style.strikethrough = true;
    }
    if let Some(sz_pos) = pr.find("<w:sz") {
        if let Some(val) = extract_attribute_value(&pr[sz_pos..], "w:val") {
            if let Ok(half_pts) = val.parse::<f32>() {
                style.font_size = half_pts / 2.0;
            }
        }
    }
    if let Some(col_pos) = pr.find("<w:color") {
        if let Some(val) = extract_attribute_value(&pr[col_pos..], "w:val") {
            if val != "auto" && !val.is_empty() {
                style.color = Some(format!("#{val}FF"));
            }
        }
    }
    if let Some(fonts_pos) = pr.find("<w:rFonts") {
        if let Some(val) = extract_attribute_value(&pr[fonts_pos..], "w:ascii") {
            if !val.is_empty() {
                style.font_family = val;
            }
        }
    }
    style
}

fn tag_is_enabled(xml: &str, tag_name: &str) -> bool {
    let needle = format!("<{tag_name}");
    if let Some(pos) = xml.find(&needle) {
        let tag_close = match xml[pos..].find('>') {
            Some(c) => pos + c,
            None => return false,
        };
        let tag_str = &xml[pos..=tag_close];
        if let Some(val) = extract_attribute_value(tag_str, "w:val") {
            val != "0" && val != "false" && val != "none"
        } else {
            true
        }
    } else {
        false
    }
}

/// Parses text from `<w:t>`, `<w:tab/>`, `<w:br/>` inside a run.
fn parse_run_text(r_xml: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;
    while cursor < r_xml.len() {
        let rest = &r_xml[cursor..];
        let t_pos = find_tag_start(rest, "w:t");
        let tab_pos = find_tag_start(rest, "w:tab");
        let br_pos = find_tag_start(rest, "w:br");

        let mut next_action: Option<(&str, usize)> = None;
        if let Some(p) = t_pos {
            next_action = Some(("t", p));
        }
        if let Some(p) = tab_pos {
            if next_action.map_or(true, |(_, cur)| p < cur) {
                next_action = Some(("tab", p));
            }
        }
        if let Some(p) = br_pos {
            if next_action.map_or(true, |(_, cur)| p < cur) {
                next_action = Some(("br", p));
            }
        }

        match next_action {
            Some(("t", p)) => {
                let tag_abs = cursor + p;
                let Some(open_close) = r_xml[tag_abs..].find('>') else {
                    break;
                };
                let content_start = tag_abs + open_close + 1;
                let Some(close_rel) = r_xml[content_start..].find("</w:t>") else {
                    break;
                };
                let content = &r_xml[content_start..content_start + close_rel];
                out.push_str(&xml_unescape(content));
                cursor = content_start + close_rel + "</w:t>".len();
            }
            Some(("tab", p)) => {
                out.push('\t');
                let tag_abs = cursor + p;
                cursor = tag_abs + "<w:tab".len();
                if let Some(end) = r_xml[cursor..].find('>') {
                    cursor += end + 1;
                }
            }
            Some(("br", p)) => {
                out.push('\n');
                let tag_abs = cursor + p;
                cursor = tag_abs + "<w:br".len();
                if let Some(end) = r_xml[cursor..].find('>') {
                    cursor += end + 1;
                }
            }
            _ => break,
        }
    }
    out
}

/// Parses a table `<w:tbl>...</w:tbl>`.
fn parse_table(xml: &str, start: usize, id: u64) -> Option<(RichBlock, usize)> {
    let close_tag = "</w:tbl>";
    let end_rel = xml[start..].find(close_tag)?;
    let tbl_xml = &xml[start..start + end_rel + close_tag.len()];
    let next_cursor = start + end_rel + close_tag.len();

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut header_row = false;
    let mut tr_cursor = 0usize;

    while let Some(tr_rel) = find_tag_start(&tbl_xml[tr_cursor..], "w:tr") {
        let tr_start = tr_cursor + tr_rel;
        let tr_close_rel = match tbl_xml[tr_start..].find("</w:tr>") {
            Some(end) => end,
            None => break,
        };
        let tr_xml = &tbl_xml[tr_start..tr_start + tr_close_rel + "</w:tr>".len()];
        tr_cursor = tr_start + tr_close_rel + "</w:tr>".len();

        if tr_xml.contains("<w:tblHeader") {
            header_row = true;
        }

        let mut row_cells: Vec<String> = Vec::new();
        let mut tc_cursor = 0usize;
        while let Some(tc_rel) = find_tag_start(&tr_xml[tc_cursor..], "w:tc") {
            let tc_start = tc_cursor + tc_rel;
            let tc_close_rel = match tr_xml[tc_start..].find("</w:tc>") {
                Some(end) => end,
                None => break,
            };
            let tc_xml = &tr_xml[tc_start..tc_start + tc_close_rel + "</w:tc>".len()];
            tc_cursor = tc_start + tc_close_rel + "</w:tc>".len();

            let mut cell_paragraphs = Vec::new();
            let mut p_cursor = 0usize;
            while let Some(p_rel) = find_tag_start(&tc_xml[p_cursor..], "w:p") {
                let p_start = p_cursor + p_rel;
                if let Some((block_opt, next_p)) = parse_paragraph(tc_xml, p_start, 0, None) {
                    if let Some(block) = block_opt {
                        cell_paragraphs.push(block.text.as_str().to_string());
                    }
                    p_cursor = next_p;
                } else {
                    break;
                }
            }

            let cell_text = cell_paragraphs.join(" ").trim().to_string();
            row_cells.push(cell_text);
        }

        if !row_cells.is_empty() {
            rows.push(row_cells);
        }
    }

    if rows.is_empty() {
        return None;
    }

    let writer_table = WriterTable {
        id: format!("tbl-{id}"),
        rows,
        header_row,
        style_id: "table-style-medium-1".to_string(),
        banded_rows: true,
        first_column: false,
        merges: Vec::new(),
    };
    let markdown = writer_table.to_markdown();
    let block = RichBlock::new(id, TABLE_BLOCK_KIND, &markdown);
    Some((block, next_cursor))
}

fn extract_attribute_value(tag_snippet: &str, attr_name: &str) -> Option<String> {
    let needle = format!("{attr_name}=\"");
    let pos = tag_snippet.find(&needle)?;
    let start = pos + needle.len();
    let end = tag_snippet[start..].find('"')?;
    Some(tag_snippet[start..start + end].to_string())
}

fn extract_core_title(xml: &str) -> Option<String> {
    let start_tag = "<dc:title";
    let start_idx = xml.find(start_tag)?;
    let after_start = start_idx + start_tag.len();
    let tag_close = xml[after_start..].find('>')?;
    let content_start = after_start + tag_close + 1;
    let end_idx = xml[content_start..].find("</dc:title>")?;
    let raw = &xml[content_start..content_start + end_idx];
    let title = xml_unescape(raw).trim().to_string();
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

fn parse_rels(rels_xml: &str) -> std::collections::BTreeMap<String, String> {
    let mut map = std::collections::BTreeMap::new();
    let mut offset = 0;
    while let Some(rel_pos) = rels_xml[offset..].find("<Relationship") {
        let abs_start = offset + rel_pos;
        let tag_end = rels_xml[abs_start..]
            .find('>')
            .unwrap_or(rels_xml.len() - abs_start);
        let tag_str = &rels_xml[abs_start..abs_start + tag_end];
        if let (Some(id), Some(target)) = (
            extract_attribute_value(tag_str, "Id"),
            extract_attribute_value(tag_str, "Target"),
        ) {
            map.insert(id, target);
        }
        offset = abs_start + tag_end;
    }
    map
}

fn resolve_num_id_list_kind(
    archive: Option<&PackageArchive>,
    num_id: &str,
) -> Option<&'static str> {
    if let Some(num_xml) = archive
        .and_then(|a| a.get("word/numbering.xml"))
        .and_then(|b| std::str::from_utf8(b).ok())
    {
        let num_tag = format!("<w:num w:numId=\"{num_id}\"");
        if let Some(num_pos) = num_xml.find(&num_tag) {
            let num_rest = &num_xml[num_pos..];
            let num_end = num_rest.find("</w:num>").unwrap_or(num_rest.len());
            let num_block = &num_rest[..num_end];
            if let Some(abs_pos) = num_block.find("<w:abstractNumId") {
                if let Some(abs_id) = extract_attribute_value(&num_block[abs_pos..], "w:val") {
                    let abs_tag = format!("<w:abstractNum w:abstractNumId=\"{abs_id}\"");
                    if let Some(abs_start) = num_xml.find(&abs_tag) {
                        let abs_rest = &num_xml[abs_start..];
                        let abs_end = abs_rest.find("</w:abstractNum>").unwrap_or(abs_rest.len());
                        let abs_block = &abs_rest[..abs_end];
                        if let Some(fmt_pos) = abs_block.find("<w:numFmt") {
                            if let Some(fmt_val) =
                                extract_attribute_value(&abs_block[fmt_pos..], "w:val")
                            {
                                return match fmt_val.to_ascii_lowercase().as_str() {
                                    "none" => None,
                                    "bullet" => Some("list-bulleted"),
                                    _ => Some("list-numbered"),
                                };
                            }
                        }
                    }
                }
            }
        }
    }
    if num_id == "1" {
        Some("list-bulleted")
    } else {
        Some("list-numbered")
    }
}
