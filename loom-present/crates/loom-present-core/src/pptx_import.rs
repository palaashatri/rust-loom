//! Complete PowerPoint (`.pptx`) deck importer for [`PresentationDocument`].
//!
//! Imports slides, titles, subtitles, body text, shapes (rectangles, circles),
//! tables (DrawingML `<a:tbl>`), charts (DrawingML `<c:chart>`), and embedded pictures
//! (`<p:pic>` with media assets).

use loom_package::zip::PackageArchive;
use std::collections::BTreeMap;

use crate::pictures::ImageAsset;
use crate::{ElementStyle, ElementType, PresentationDocument, Slide, SlideElement, TextAlign};

const EMU_PER_UNIT: f32 = 12_192.0;
const PPTX_SLIDE_PART_PREFIX: &str = "ppt/slides/slide";

/// Imports a `.pptx` presentation package archive into a complete [`PresentationDocument`].
pub fn import_pptx(pptx_bytes: &[u8]) -> Result<PresentationDocument, String> {
    let archive = PackageArchive::from_bytes(pptx_bytes)
        .map_err(|err| format!("unreadable pptx archive: {err}"))?;

    let mut slide_parts: Vec<(u64, String)> = Vec::new();
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
            slide_parts.push((parsed, path.to_string()));
        }
    }

    if slide_parts.is_empty() {
        return Err(
            "pptx archive contains no slide parts (expected ppt/slides/slide<N>.xml)".to_string(),
        );
    }
    slide_parts.sort_unstable_by_key(|(number, _)| *number);

    let title = archive
        .get("docProps/core.xml")
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .and_then(extract_core_title)
        .unwrap_or_else(|| "Imported Presentation".to_string());

    let mut doc = PresentationDocument::new("imported-deck", title);
    doc.slides.clear();

    for (index, (slide_num, path)) in slide_parts.iter().enumerate() {
        let data = archive
            .get(path)
            .ok_or_else(|| format!("missing slide part {path}"))?;
        let slide_xml = String::from_utf8_lossy(data);

        // Slide title
        let title = crate::pptx_titles::pptx_slide_title(&slide_xml);
        let mut slide = Slide::new(format!("slide-{}", index + 1), title, "imported-pptx");

        // Parse slide relationship part for images
        let rels_path = format!("ppt/slides/_rels/slide{slide_num}.xml.rels");
        let rids = if let Some(rels_data) = archive.get(&rels_path) {
            parse_rels(&String::from_utf8_lossy(rels_data))
        } else {
            BTreeMap::new()
        };

        // Extract shapes (title, subtitle, body text, rect, circle, stat card)
        let shapes = parse_slide_shapes(&slide_xml);
        slide.elements.extend(shapes);

        // Extract tables (<p:graphicFrame> enclosing <a:tbl>)
        let tables = crate::tables::extract_pptx_slide_tables(&slide_xml);
        slide.elements.extend(tables);

        // Extract charts (<p:graphicFrame> enclosing <c:chart>)
        let charts =
            crate::charts::extract_pptx_slide_charts_with_rels(&slide_xml, &rids, &archive);
        slide.elements.extend(charts);

        // Extract pictures (<p:pic>)
        let pictures = parse_slide_pictures(&slide_xml, &rids, &archive, &mut doc.assets);
        slide.elements.extend(pictures);

        doc.slides.push(slide);
    }

    Ok(doc)
}

/// Inspects a `.pptx` package for unsupported constructs (SmartArt, media, macros, etc.).
pub fn inspect_pptx_unsupported(pptx_bytes: &[u8]) -> Result<Vec<String>, String> {
    let archive = PackageArchive::from_bytes(pptx_bytes)
        .map_err(|err| format!("unreadable pptx archive: {err}"))?;
    let mut warnings = Vec::new();

    for path in archive.paths() {
        if (path.starts_with("ppt/diagrams/") || path.contains("diagram"))
            && !warnings.iter().any(|w: &String| w.contains("SmartArt"))
        {
            warnings.push("SmartArt diagrams are not supported and will be omitted.".to_string());
        }
        if path.starts_with("ppt/media/") {
            let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
            if ["mp4", "mov", "avi", "wmv", "mp3", "wav", "m4a", "wma"].contains(&ext.as_str())
                && !warnings.iter().any(|w: &String| w.contains("media"))
            {
                warnings
                    .push("Embedded audio and video media clips are not supported.".to_string());
            }
        }
        if path.starts_with("ppt/embeddings/")
            && !warnings.iter().any(|w: &String| w.contains("OLE"))
        {
            warnings.push("Embedded OLE objects are not supported.".to_string());
        }
        if path.starts_with("ppt/slides/slide") && path.ends_with(".xml") {
            let has_transition = archive
                .get(path)
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
                .is_some_and(|xml| xml.contains("<p:transition"));
            if has_transition
                && !warnings
                    .iter()
                    .any(|warning: &String| warning.contains("slide transitions"))
            {
                warnings.push(
                    "PowerPoint slide transitions are not supported and will be omitted."
                        .to_string(),
                );
            }
        }
        if path.contains("vbaProject.bin") && !warnings.iter().any(|w: &String| w.contains("VBA")) {
            warnings.push("Macros and VBA code are not supported.".to_string());
        }
    }

    Ok(warnings)
}

/// Unescapes standard XML entities.
fn xml_unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

fn extract_attr(tag_snippet: &str, attr_name: &str) -> Option<String> {
    let needle = format!("{attr_name}=\"");
    let pos = tag_snippet.find(&needle)?;
    let start = pos + needle.len();
    let end = tag_snippet[start..].find('"')?;
    Some(tag_snippet[start..start + end].to_string())
}

fn emu_to_units(emu: i64) -> f32 {
    emu as f32 / EMU_PER_UNIT
}

/// Where a placeholder with no geometry of its own goes, in slide units.
fn placeholder_box(kind: &ElementType) -> (f32, f32, f32, f32) {
    match kind {
        ElementType::Title => (60.0, 40.0, 880.0, 110.0),
        ElementType::Subtitle => (60.0, 170.0, 880.0, 80.0),
        _ => (60.0, 170.0, 880.0, 330.0),
    }
}

fn parse_xfrm(xml: &str) -> (f32, f32, f32, f32, f32) {
    let mut x = 40.0;
    let mut y = 40.0;
    let mut width = 200.0;
    let mut height = 100.0;
    let mut rot = 0.0;

    if let Some(xfrm_pos) = xml.find("<a:xfrm") {
        let xfrm_end = xml[xfrm_pos..]
            .find('>')
            .map(|e| xfrm_pos + e)
            .unwrap_or(xml.len());
        let xfrm_tag = &xml[xfrm_pos..=xfrm_end];
        if let Some(rot_str) = extract_attr(xfrm_tag, "rot") {
            if let Ok(rot_val) = rot_str.parse::<f32>() {
                rot = rot_val / 60_000.0;
            }
        }
        if let Some(off_pos) = xml[xfrm_pos..].find("<a:off") {
            let off_start = xfrm_pos + off_pos;
            let off_end = xml[off_start..]
                .find('>')
                .map(|e| off_start + e)
                .unwrap_or(xml.len());
            let off_tag = &xml[off_start..=off_end];
            if let Some(x_str) = extract_attr(off_tag, "x") {
                if let Ok(x_emu) = x_str.parse::<i64>() {
                    x = emu_to_units(x_emu);
                }
            }
            if let Some(y_str) = extract_attr(off_tag, "y") {
                if let Ok(y_emu) = y_str.parse::<i64>() {
                    y = emu_to_units(y_emu);
                }
            }
        }
        if let Some(ext_pos) = xml[xfrm_pos..].find("<a:ext") {
            let ext_start = xfrm_pos + ext_pos;
            let ext_end = xml[ext_start..]
                .find('>')
                .map(|e| ext_start + e)
                .unwrap_or(xml.len());
            let ext_tag = &xml[ext_start..=ext_end];
            if let Some(cx_str) = extract_attr(ext_tag, "cx") {
                if let Ok(cx_emu) = cx_str.parse::<i64>() {
                    width = emu_to_units(cx_emu);
                }
            }
            if let Some(cy_str) = extract_attr(ext_tag, "cy") {
                if let Ok(cy_emu) = cy_str.parse::<i64>() {
                    height = emu_to_units(cy_emu);
                }
            }
        }
    }
    (x, y, width, height, rot)
}

fn parse_txbody_text(xml: &str) -> String {
    let mut lines = Vec::new();
    let mut p_cursor = 0usize;
    while let Some(p_rel) = xml[p_cursor..].find("<a:p>") {
        let p_start = p_cursor + p_rel;
        let Some(p_close) = xml[p_start..].find("</a:p>") else {
            break;
        };
        let p_xml = &xml[p_start..p_start + p_close + "</a:p>".len()];
        p_cursor = p_start + p_close + "</a:p>".len();

        let mut line = String::new();
        let mut t_cursor = 0usize;
        while let Some(t_rel) = p_xml[t_cursor..].find("<a:t>") {
            let t_start = t_cursor + t_rel + "<a:t>".len();
            let Some(close_rel) = p_xml[t_start..].find("</a:t>") else {
                break;
            };
            let content = &p_xml[t_start..t_start + close_rel];
            line.push_str(&xml_unescape(content));
            t_cursor = t_start + close_rel + "</a:t>".len();
        }
        lines.push(line);
    }
    lines.join("\n")
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

/// Parses `<p:sp>` shapes from slide XML into [`SlideElement`]s.
fn parse_slide_shapes(slide_xml: &str) -> Vec<SlideElement> {
    let mut elements = Vec::new();
    let mut sp_cursor = 0usize;
    let mut elem_counter = 1u32;

    while let Some(sp_rel) = find_tag_start(&slide_xml[sp_cursor..], "p:sp") {
        let sp_start = sp_cursor + sp_rel;
        let Some(sp_close) = slide_xml[sp_start..].find("</p:sp>") else {
            break;
        };
        let sp_xml = &slide_xml[sp_start..sp_start + sp_close + "</p:sp>".len()];
        sp_cursor = sp_start + sp_close + "</p:sp>".len();

        let (x, y, width, height, rotation_deg) = parse_xfrm(sp_xml);
        let text = parse_txbody_text(sp_xml);

        let element_type =
            if sp_xml.contains("type=\"title\"") || sp_xml.contains("type=\"ctrTitle\"") {
                ElementType::Title
            } else if sp_xml.contains("type=\"subTitle\"") {
                ElementType::Subtitle
            } else if sp_xml.contains("txBox=\"1\"") || sp_xml.contains("type=\"body\"") {
                ElementType::BodyText
            } else if sp_xml.contains("prst=\"ellipse\"") {
                ElementType::ShapeCircle
            } else if sp_xml.contains("prst=\"rect\"") || sp_xml.contains("prst=\"roundRect\"") {
                if sp_xml.contains("name=\"Stat") || sp_xml.contains("stat-card") {
                    ElementType::StatCard
                } else {
                    ElementType::ShapeRectangle
                }
            } else {
                ElementType::BodyText
            };

        // A placeholder that takes its position from the slide layout carries
        // no geometry of its own. The layout is not read, so it gets the place
        // the same kind of object has on a new Loom slide instead of the
        // collapsed default box.
        let (x, y, width, height) = if sp_xml.contains("<a:off") {
            (x, y, width, height)
        } else {
            placeholder_box(&element_type)
        };
        let id = extract_attr(sp_xml, "name").unwrap_or_else(|| format!("shape-{elem_counter}"));
        elem_counter += 1;
        let style = parse_shape_style(sp_xml, &element_type);

        elements.push(SlideElement {
            id,
            element_type,
            content: text,
            x,
            y,
            width,
            height,
            rotation_deg,
            action: None,
            style,
        });
    }
    elements
}

/// What the exporter draws for each element type when nothing overrides it:
/// `(size pt, bold, centred, fill, text colours)`. A value read from a file that
/// matches this is not a style, so a deck we exported imports unstyled.
fn type_defaults(
    kind: &ElementType,
) -> (
    f32,
    bool,
    bool,
    Option<&'static str>,
    &'static [&'static str],
) {
    match kind {
        ElementType::Title => (27.43, true, true, None, &["1D1D1F", "F5F5F7"]),
        ElementType::Subtitle => (16.0, false, false, None, &["6E6E73", "F5F5F7"]),
        ElementType::StatCard => (22.86, true, true, Some("0071E3"), &["FFFFFF"]),
        ElementType::ShapeRectangle => (16.0, false, false, Some("E6F0FC"), &["1D1D1F"]),
        ElementType::ShapeCircle => (16.0, false, false, Some("E3F6EF"), &["1D1D1F"]),
        _ => (16.0, false, false, None, &["1D1D1F", "F5F5F7"]),
    }
}

/// The first `<a:srgbClr val=...>` inside `xml`, as `#RRGGBB`.
fn first_srgb(xml: &str) -> Option<String> {
    let pos = xml.find("<a:srgbClr")?;
    let tag = &xml[pos..pos + xml[pos..].find('>')?];
    let value = extract_attr(tag, "val")?;
    (value.len() == 6 && value.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("#{}", value.to_ascii_uppercase()))
}

/// Size, weight, italics, colours and alignment a shape sets itself, beyond
/// what its type already draws.
fn parse_shape_style(sp_xml: &str, kind: &ElementType) -> ElementStyle {
    let (default_pt, default_bold, default_centred, default_fill, default_inks) =
        type_defaults(kind);
    let mut style = ElementStyle::default();
    if let Some(pos) = find_tag_start(sp_xml, "a:rPr") {
        let end = sp_xml[pos..]
            .find("</a:rPr>")
            .map_or(sp_xml.len(), |e| pos + e);
        let tag_end = sp_xml[pos..].find('>').map_or(end, |e| pos + e);
        let tag = &sp_xml[pos..tag_end];
        if let Some(size) = extract_attr(tag, "sz").and_then(|v| v.parse::<f32>().ok()) {
            let pt = size / 100.0;
            if (pt - default_pt).abs() > 0.5 && (6.0..=200.0).contains(&pt) {
                style.font_size_pt = Some(pt);
            }
        }
        if let Some(bold) = extract_attr(tag, "b") {
            let on = bold == "1" || bold == "true";
            if on != default_bold {
                style.bold = Some(on);
            }
        }
        style.italic = matches!(extract_attr(tag, "i").as_deref(), Some("1" | "true"));
        if let Some(color) = first_srgb(&sp_xml[pos..end]) {
            if !default_inks
                .iter()
                .any(|ink| color.trim_start_matches('#').eq_ignore_ascii_case(ink))
            {
                style.color = Some(color);
            }
        }
    }
    if let Some(pos) = find_tag_start(sp_xml, "a:pPr") {
        let tag_end = sp_xml[pos..].find('>').map_or(sp_xml.len(), |e| pos + e);
        let align = match extract_attr(&sp_xml[pos..tag_end], "algn").as_deref() {
            Some("l") => Some(TextAlign::Left),
            Some("ctr") => Some(TextAlign::Center),
            Some("r") => Some(TextAlign::Right),
            _ => None,
        };
        if let Some(align) = align {
            if (align == TextAlign::Center) != default_centred
                || (default_centred && align == TextAlign::Right)
                || (!default_centred && align == TextAlign::Right)
            {
                style.align = Some(align);
            }
        }
    }
    // The shape's own fill is the solid fill in its shape properties, before any outline.
    if let Some(start) = sp_xml.find("<p:spPr>") {
        let end = sp_xml[start..]
            .find("</p:spPr>")
            .map_or(sp_xml.len(), |e| start + e);
        let props = &sp_xml[start..end];
        let before_outline = props.find("<a:ln").map_or(props, |ln| &props[..ln]);
        if let Some(pos) = before_outline.find("<a:solidFill>") {
            if let Some(fill) = first_srgb(&before_outline[pos..]) {
                let is_default = default_fill
                    .is_some_and(|d| fill.trim_start_matches('#').eq_ignore_ascii_case(d));
                if !is_default {
                    style.fill = Some(fill);
                }
            }
        }
    }
    style
}

#[cfg(test)]
pub(crate) fn parse_shape_style_for_tests(sp_xml: &str, kind: &ElementType) -> ElementStyle {
    parse_shape_style(sp_xml, kind)
}

/// Parses `.rels` XML mapping Relationship ID (`rId...`) to target archive path.
fn parse_rels(rels_xml: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    let mut cursor = 0usize;
    while let Some(rel_pos) = rels_xml[cursor..].find("<Relationship") {
        let start = cursor + rel_pos;
        let Some(end) = rels_xml[start..].find('>') else {
            break;
        };
        let tag = &rels_xml[start..=start + end];
        cursor = start + end + 1;

        if let (Some(id), Some(target)) = (extract_attr(tag, "Id"), extract_attr(tag, "Target")) {
            // Target is typically "../media/image1.png" -> normalize to "ppt/media/image1.png"
            let normalized = if target.starts_with("../") {
                format!("ppt/{}", target.trim_start_matches("../"))
            } else if target.starts_with('/') {
                target.trim_start_matches('/').to_string()
            } else {
                format!("ppt/slides/{target}")
            };
            map.insert(id, normalized);
        }
    }
    map
}

/// Parses `<p:pic>` shapes from slide XML into picture [`SlideElement`]s.
fn parse_slide_pictures(
    slide_xml: &str,
    rids: &BTreeMap<String, String>,
    archive: &PackageArchive,
    assets: &mut BTreeMap<String, ImageAsset>,
) -> Vec<SlideElement> {
    let mut elements = Vec::new();
    let mut pic_cursor = 0usize;
    let mut pic_counter = 1u32;

    while let Some(pic_rel) = find_tag_start(&slide_xml[pic_cursor..], "p:pic") {
        let pic_start = pic_cursor + pic_rel;
        let Some(pic_close) = slide_xml[pic_start..].find("</p:pic>") else {
            break;
        };
        let pic_xml = &slide_xml[pic_start..pic_start + pic_close + "</p:pic>".len()];
        pic_cursor = pic_start + pic_close + "</p:pic>".len();

        let (x, y, width, height, rotation_deg) = parse_xfrm(pic_xml);

        // Find r:embed="rId..."
        let Some(rid) = extract_attr(pic_xml, "r:embed") else {
            continue;
        };
        let Some(target_path) = rids.get(&rid) else {
            continue;
        };

        // Read image bytes from archive
        let Some(img_bytes) = archive.get(target_path) else {
            continue;
        };
        let name = target_path
            .rsplit('/')
            .next()
            .unwrap_or("picture")
            .to_string();

        if let Ok(asset) = ImageAsset::from_bytes(&name, img_bytes.to_vec()) {
            let asset_id = asset.id.clone();
            assets.insert(asset_id.clone(), asset);

            let id =
                extract_attr(pic_xml, "name").unwrap_or_else(|| format!("picture-{pic_counter}"));
            pic_counter += 1;

            elements.push(SlideElement {
                id,
                element_type: ElementType::Picture,
                content: asset_id,
                x,
                y,
                width,
                height,
                rotation_deg,
                action: None,

                style: Default::default(),
            });
        }
    }
    elements
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_pptx;
    use loom_objects::chart::{Chart, ChartSeries, ChartType};
    use loom_objects::Table;

    #[test]
    fn morph_transition_is_reported_as_omitted() {
        let mut archive = PackageArchive::new();
        archive
            .add(
                "ppt/slides/slide1.xml",
                br#"<p:sld xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main"><p:transition><p14:morph/></p:transition></p:sld>"#.to_vec(),
            )
            .expect("slide part");

        let warnings =
            inspect_pptx_unsupported(&archive.to_bytes().expect("pptx archive")).expect("inspect");
        assert_eq!(
            warnings,
            vec!["PowerPoint slide transitions are not supported and will be omitted."]
        );
    }

    #[test]
    fn test_import_pptx_deck_with_tables_and_charts() {
        let mut doc = PresentationDocument::new("deck-test", "Comprehensive Deck");
        doc.slides.clear();

        // Slide 1: Title and body text
        let mut s1 = Slide::new("slide-1", "Executive Summary", "cover");
        s1.add_element(SlideElement {
            id: "title-1".to_string(),
            element_type: ElementType::Title,
            content: "Executive Summary".to_string(),
            x: 40.0,
            y: 20.0,
            width: 920.0,
            height: 60.0,
            rotation_deg: 0.0,
            action: None,

            style: Default::default(),
        });
        s1.add_element(SlideElement {
            id: "body-1".to_string(),
            element_type: ElementType::BodyText,
            content: "Strong revenue growth in Q3".to_string(),
            x: 40.0,
            y: 120.0,
            width: 800.0,
            height: 200.0,
            rotation_deg: 0.0,
            action: None,

            style: Default::default(),
        });
        doc.slides.push(s1);

        // Slide 2: Table
        let mut s2 = Slide::new("slide-2", "Financial Breakdown", "content");
        let mut table = Table::new(3, 2);
        table.set_cell_text(0, 0, "Quarter");
        table.set_cell_text(0, 1, "Revenue");
        table.set_cell_text(1, 0, "Q1");
        table.set_cell_text(1, 1, "$1.2M");
        table.set_cell_text(2, 0, "Q2");
        table.set_cell_text(2, 1, "$1.5M");
        let table_elem = SlideElement::new_table("tbl-1", 50.0, 80.0, 600.0, 300.0, table);
        s2.add_element(table_elem);
        doc.slides.push(s2);

        // Slide 3: Chart
        let mut s3 = Slide::new("slide-3", "Performance Chart", "content");
        let chart = Chart::from_data(
            "chart-perf",
            Some("Q3 Growth".to_string()),
            ChartType::Bar,
            vec!["Product A".to_string(), "Product B".to_string()],
            vec![ChartSeries {
                name: "Units".to_string(),
                values: vec![450.0, 600.0],
                color: None,
            }],
        );
        let chart_elem = SlideElement::new_chart("chart-1", 60.0, 90.0, 500.0, 320.0, chart);
        s3.add_element(chart_elem);
        doc.slides.push(s3);

        // Export to PPTX
        let session = crate::PresentationSession::new(doc);
        let pptx_bytes = export_pptx(&session).expect("export pptx");

        // Import back
        let imported = import_pptx(&pptx_bytes).expect("import pptx");
        assert_eq!(imported.slides.len(), 3);
        assert_eq!(imported.slides[0].title, "Executive Summary");
        assert_eq!(imported.slides[1].title, "Financial Breakdown");
        assert_eq!(imported.slides[2].title, "Performance Chart");

        // Verify table in slide 2
        let imp_tbl_elem = imported.slides[1]
            .elements
            .iter()
            .find(|e| e.element_type == ElementType::Table)
            .expect("table element must be present in slide 2");
        let imp_tbl = imp_tbl_elem.table().expect("table must deserialize");
        assert_eq!(imp_tbl.cell_text(0, 0).as_deref(), Some("Quarter"));
        assert_eq!(imp_tbl.cell_text(1, 1).as_deref(), Some("$1.2M"));

        // Verify chart in slide 3
        let imp_chart_elem = imported.slides[2]
            .elements
            .iter()
            .find(|e| e.element_type == ElementType::Chart)
            .expect("chart element must be present in slide 3");
        let imp_chart = imp_chart_elem.chart().expect("chart must deserialize");
        assert_eq!(imp_chart.title.as_deref(), Some("Q3 Growth"));
        assert_eq!(imp_chart.chart_type, ChartType::Bar);
        assert_eq!(imp_chart.series[0].values, vec![450.0, 600.0]);
    }

    #[test]
    fn test_import_pptx_with_picture() {
        let mut image = image::RgbaImage::new(10, 10);
        for pixel in image.pixels_mut() {
            *pixel = image::Rgba([100, 150, 200, 255]);
        }
        let mut png_bytes = Vec::new();
        image::DynamicImage::ImageRgba8(image)
            .write_to(
                &mut std::io::Cursor::new(&mut png_bytes),
                image::ImageFormat::Png,
            )
            .expect("encode png");

        let mut doc = PresentationDocument::new("deck-pic", "Picture Deck");
        doc.slides.clear();

        let asset = ImageAsset::from_bytes("test_thumb.png", png_bytes).expect("asset");
        let asset_id = asset.id.clone();
        doc.assets.insert(asset_id.clone(), asset);

        let mut s = Slide::new("slide-pic", "Visual Slide", "content");
        s.add_element(SlideElement {
            id: "pic-1".to_string(),
            element_type: ElementType::Picture,
            content: asset_id.clone(),
            x: 100.0,
            y: 100.0,
            width: 300.0,
            height: 200.0,
            rotation_deg: 0.0,
            action: None,

            style: Default::default(),
        });
        doc.slides.push(s);

        let session = crate::PresentationSession::new(doc);
        let pptx_bytes = export_pptx(&session).expect("export pptx");
        let imported = import_pptx(&pptx_bytes).expect("import pptx");

        assert_eq!(imported.slides.len(), 1);
        let pic_elem = imported.slides[0]
            .elements
            .iter()
            .find(|e| e.element_type == ElementType::Picture)
            .expect("picture element must be present");
        assert!(imported.assets.contains_key(&pic_elem.content));
    }

    #[test]
    fn placeholders_without_geometry_do_not_collapse_into_one_box() {
        let slide = concat!(
            "<p:sld><p:cSld><p:spTree>",
            "<p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Title 1\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"ctrTitle\"/></p:nvPr></p:nvSpPr>",
            "<p:spPr/><p:txBody><a:p><a:r><a:t>Quarterly review</a:t></a:r></a:p></p:txBody></p:sp>",
            "<p:sp><p:nvSpPr><p:cNvPr id=\"3\" name=\"Subtitle 2\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"subTitle\" idx=\"1\"/></p:nvPr></p:nvSpPr>",
            "<p:spPr/><p:txBody><a:p><a:r><a:t>Prepared by the team</a:t></a:r></a:p></p:txBody></p:sp>",
            "</p:spTree></p:cSld></p:sld>"
        );
        let elements = parse_slide_shapes(slide);
        assert_eq!(elements.len(), 2);
        let (title, subtitle) = (&elements[0], &elements[1]);
        assert!(title.width > 600.0 && title.height > 60.0, "{title:?}");
        assert!(
            subtitle.y >= title.y + title.height,
            "the subtitle sits below the title, not on it"
        );
    }
}
