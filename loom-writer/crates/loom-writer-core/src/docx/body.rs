//! `word/document.xml`: Writer blocks as WordprocessingML paragraphs, lists,
//! tables and the page-setup section.

use std::collections::BTreeSet;

use loom_objects::table::{HorizontalAlign, Table};
use loom_text::{Alignment, CharacterStyle, FontWeight, ParagraphDirection, ParagraphStyle};

use super::comments::{self, Anchor, CommentPlan};
use super::footnotes::{ExportPlan as FootnoteExportPlan, InlineReference as FootnoteReference};
use super::inline_style::docx_rgb;
use super::package::{
    DEFAULT_LINE_TWIPS, DEFAULT_SPACE_AFTER_TWIPS, SANS_FAMILY, STYLE_LIST, STYLE_QUOTE,
    STYLE_TABLE,
};
use super::xml::{self, R_NS, W_NS, XML_DECLARATION};
use crate::{
    PageMarginsPreset, PageOrientation, PageStyle, PaperSize, RevisionBoundaryKind, RevisionKind,
    RevisionPayload, RevisionRecord, RichBlock, WriterDocument, TABLE_BLOCK_KIND,
    TABLE_CELL_BASE_FONT_SIZE_PT,
};

/// The body part plus what the numbering part needs to know about it.
pub(super) struct Body {
    pub xml: String,
    /// How many separate numbered lists the body contains.
    pub numbered_lists: usize,
    /// The chart parts embedded in the body.
    pub charts: Vec<(usize, String)>,
    /// The pictures embedded in the body, in order.
    pub images: Vec<BodyImage>,
}

/// A picture part of the package.
pub(super) struct BodyImage {
    pub index: usize,
    pub extension: &'static str,
    pub mime: &'static str,
    pub bytes: std::sync::Arc<Vec<u8>>,
}

/// Twentieths of a point, the unit WordprocessingML measures in.
fn twips(points: f32) -> i32 {
    if points.is_finite() {
        (points * 20.0).round().clamp(-31_680.0, 31_680.0) as i32
    } else {
        0
    }
}

/// Exact page size of a standard paper in twips, portrait.
fn paper_twips(paper: PaperSize) -> (i32, i32) {
    match paper {
        PaperSize::A4 => (11_906, 16_838),
        PaperSize::Letter => (12_240, 15_840),
        PaperSize::Legal => (12_240, 20_160),
        PaperSize::Executive => (10_440, 15_120),
        PaperSize::A3 => (16_838, 23_811),
        PaperSize::A5 => (8_391, 11_906),
        // A custom size uses its stored twips in Geometry::of.
        PaperSize::Custom => (11_906, 16_838),
    }
}

/// Page size and margins in twips, from the document's page setup.
struct Geometry {
    width: i32,
    height: i32,
    top: i32,
    bottom: i32,
    left: i32,
    right: i32,
    landscape: bool,
}

impl Geometry {
    fn of(doc: &WriterDocument) -> Self {
        let custom = doc.page.custom_geometry_points();
        let (width, height, landscape) = if doc.page.paper == PaperSize::Custom {
            if let Some((width, height, _, _, _, _)) = custom {
                let landscape = width > height;
                (twips(width), twips(height), landscape)
            } else {
                let (short, long) = paper_twips(PaperSize::A4);
                (short, long, false)
            }
        } else {
            let (short, long) = paper_twips(doc.page.paper);
            let landscape = doc.page.orientation == PageOrientation::Landscape;
            let (width, height) = if landscape {
                (long, short)
            } else {
                (short, long)
            };
            (width, height, landscape)
        };
        let (top, bottom, left, right) = if doc.page.margins == PageMarginsPreset::Custom {
            custom
                .map(|(_, _, top, bottom, left, right)| (top, bottom, left, right))
                .unwrap_or_else(|| PageMarginsPreset::Normal.margins_pt())
        } else {
            doc.page.margins.margins_pt()
        };
        Self {
            width,
            height,
            top: twips(top),
            bottom: twips(bottom),
            left: twips(left),
            right: twips(right),
            landscape,
        }
    }

    /// Width of the text area, which a table spans.
    fn text_width(&self) -> i32 {
        (self.width - self.left - self.right).max(720)
    }

    fn section_xml(&self, header_references: &str, header_settings: &str) -> String {
        let header = (self.top / 2).min(720);
        let orient = if self.landscape {
            " w:orient=\"landscape\""
        } else {
            ""
        };
        format!(
            "<w:sectPr>{header_references}<w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/>\
             <w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{header}\" w:footer=\"{header}\" w:gutter=\"0\"/>\
             {header_settings}<w:cols w:space=\"720\"/></w:sectPr>",
            self.width, self.height, self.top, self.right, self.bottom, self.left
        )
    }
}

/// Builds the document part.
pub(super) fn build(
    doc: &WriterDocument,
    plan: &CommentPlan,
    footnotes: &FootnoteExportPlan,
    header_references: &str,
    header_settings: &str,
) -> Result<Body, String> {
    let geometry = Geometry::of(doc);
    let tables: Vec<Option<Table>> = doc
        .blocks
        .iter()
        .map(|block| {
            (block.kind == TABLE_BLOCK_KIND)
                .then(|| block.typed_table().cloned())
                .flatten()
                .filter(|table| table.col_count() > 0)
        })
        .collect();

    let mut xml = format!("{XML_DECLARATION}<w:document xmlns:w=\"{W_NS}\" xmlns:r=\"{R_NS}\">");
    if doc.page.appearance.is_black() {
        xml.push_str("<w:background w:color=\"000000\"/>");
    }
    xml.push_str("<w:body>");
    let mut numbered_lists = 0usize;
    let mut previous_numbered = false;
    let mut charts: Vec<(usize, String)> = Vec::new();
    let mut images: Vec<BodyImage> = Vec::new();
    let paragraph_format_ids: Vec<u64> = doc
        .tracked_changes
        .revisions()
        .iter()
        .filter(|revision| {
            doc.tracked_changes
                .paragraph_format_change(revision.id)
                .is_some()
        })
        .map(|revision| revision.id)
        .collect();
    for (index, block) in doc.blocks.iter().enumerate() {
        if let Some(table) = &tables[index] {
            xml.push_str(&table_xml(
                table,
                geometry.text_width(),
                block.id,
                footnotes,
                doc.tracked_changes.revisions(),
            ));
            // Word needs a paragraph to end the body and to keep two tables
            // from merging into one.
            let followed_by_table = tables.get(index + 1).is_some_and(Option::is_some);
            if index + 1 == doc.blocks.len() || followed_by_table {
                xml.push_str("<w:p/>");
            }
            previous_numbered = false;
            continue;
        }
        if let Some(image) = block.image.as_ref().filter(|_| block.is_image()) {
            let index = images.len() + 1;
            let (w, h) = image.fitted_size(
                geometry.text_width() as f32 / 20.0,
                (geometry.height - geometry.top - geometry.bottom).max(720) as f32 / 20.0,
            );
            xml.push_str(&picture_paragraph(block, image, index, w, h));
            images.push(BodyImage {
                index,
                extension: image.format.extension(),
                mime: image.format.mime(),
                bytes: image.bytes.clone(),
            });
            previous_numbered = false;
            continue;
        }
        if block.kind == crate::CHART_BLOCK_KIND {
            if let Some(chart) = crate::chart_from_block(block) {
                let chart_idx = charts.len() + 1;
                let rid = format!("rIdChart{chart_idx}");
                let chart_xml = crate::charts::writer_chart_to_c_chart_xml(&chart)
                    .map_err(|error| format!("chart {}: {error}", chart.id))?;
                charts.push((chart_idx, chart_xml));
                xml.push_str(&chart_xml_paragraph(&rid, chart_idx, block));
                previous_numbered = false;
                continue;
            }
        }
        if let Some(revision) = doc.tracked_changes.revisions().iter().find(|revision| {
            revision.kind == RevisionKind::Deletion
                && revision.boundary_kind == RevisionBoundaryKind::ParagraphMark
                && revision.anchor.block_id == block.id
        }) {
            xml.push_str(&deleted_paragraph_mark_xml(block, revision));
            previous_numbered = false;
            continue;
        }
        let list = ListKind::of(block.kind.as_str());
        let num_id = match list {
            Some(ListKind::Bullet) => Some(1),
            Some(ListKind::Numbered) => {
                if !previous_numbered {
                    numbered_lists += 1;
                }
                Some(1 + numbered_lists)
            }
            None => None,
        };
        // A numbered run goes on through its deeper items, so Word keeps the
        // top-level count as Writer does. A top-level bullet or any other block
        // ends the run.
        previous_numbered = match list {
            Some(ListKind::Numbered) => true,
            Some(ListKind::Bullet) => previous_numbered && block.effective_list_level() > 0,
            None => false,
        };
        xml.push_str("<w:p>");
        let paragraph_marks =
            paragraph_mark_revisions(index, &doc.blocks, doc.tracked_changes.revisions());
        let paragraph_format = doc.tracked_changes.revisions().iter().find_map(|revision| {
            (revision.anchor.block_id == block.id
                && doc
                    .tracked_changes
                    .paragraph_format_change(revision.id)
                    .is_some())
            .then(|| {
                let change = doc
                    .tracked_changes
                    .paragraph_format_change(revision.id)
                    .expect("paragraph format was checked");
                (revision, change.before.kind, change.before.style)
            })
        });
        xml.push_str(&paragraph_properties(
            block,
            num_id,
            &paragraph_marks,
            paragraph_format,
        ));
        xml.push_str(&paragraph_content(
            block,
            index,
            &doc.blocks,
            plan.anchors_for(block.id),
            footnotes.references_for(block.id),
            doc.tracked_changes.revisions(),
            &paragraph_format_ids,
        ));
        xml.push_str("</w:p>");
    }
    xml.push_str(&geometry.section_xml(header_references, header_settings));
    xml.push_str("</w:body></w:document>");
    Ok(Body {
        xml,
        numbered_lists,
        charts,
        images,
    })
}

/// A paragraph holding one inline picture of `w` by `h` points.
fn picture_paragraph(
    block: &RichBlock,
    image: &crate::InlineImage,
    index: usize,
    w: f32,
    h: f32,
) -> String {
    let (cx, cy) = ((w * 12_700.0).round() as i64, (h * 12_700.0).round() as i64);
    let name = xml::attr(&image.name);
    let alignment = paragraph_properties(block, None, &[], None);
    format!(
        "<w:p>{alignment}<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\n         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\n         <wp:docPr id=\"{id}\" name=\"Picture {index}\" descr=\"{name}\"/>\n         <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\n         <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\n         <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\n         <pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"{name}\"/><pic:cNvPicPr/></pic:nvPicPr>\n         <pic:blipFill><a:blip xmlns:r=\"{R_NS}\" r:embed=\"rIdImg{index}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\n         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\n         </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
        id = 1000 + index
    )
}

fn chart_xml_paragraph(rid: &str, chart_id: usize, block: &RichBlock) -> String {
    let properties = paragraph_properties(block, None, &[], None);
    format!(
        "<w:p>{properties}<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\
         <wp:extent cx=\"5486400\" cy=\"3200400\"/>\
         <wp:docPr id=\"{chart_id}\" name=\"Chart {chart_id}\"/>\
         <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\
         <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\">\
         <c:chart xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\" xmlns:r=\"{R_NS}\" r:id=\"{rid}\"/>\
         </a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ListKind {
    Bullet,
    Numbered,
}

impl ListKind {
    fn of(kind: &str) -> Option<Self> {
        match kind {
            "list-bulleted" => Some(Self::Bullet),
            "list-numbered" => Some(Self::Numbered),
            _ => None,
        }
    }
}

/// The Word paragraph style for a block kind, if it is not Normal.
fn style_id(kind: &str) -> Option<String> {
    if let Some(level) = kind.strip_prefix("heading") {
        if let Ok(level @ 1..=6) = level.parse::<u8>() {
            return Some(format!("Heading{level}"));
        }
    }
    match kind {
        "list-bulleted" | "list-numbered" => Some(STYLE_LIST.to_string()),
        "quote" => Some(STYLE_QUOTE.to_string()),
        _ => None,
    }
}

fn paragraph_properties(
    block: &RichBlock,
    num_id: Option<usize>,
    paragraph_marks: &[&RevisionRecord],
    paragraph_format: Option<(&RevisionRecord, &str, &loom_text::ParagraphStyle)>,
) -> String {
    let mut out = paragraph_property_content(
        &block.kind,
        &block.style,
        num_id,
        block.effective_list_level(),
        block.list_indent_pt(),
        paragraph_marks,
    );
    if let Some((revision, old_kind, old_style)) = paragraph_format {
        let old_properties =
            paragraph_property_content(old_kind, old_style, None, old_style.list_level, 0.0, &[]);
        out.push_str(&format!(
            "<w:pPrChange {}><w:pPr>{old_properties}</w:pPr></w:pPrChange>",
            revision_attributes(revision)
        ));
    }
    if out.is_empty() {
        out
    } else {
        format!("<w:pPr>{out}</w:pPr>")
    }
}

fn paragraph_property_content(
    kind: &str,
    style: &loom_text::ParagraphStyle,
    num_id: Option<usize>,
    list_level: u8,
    list_indent_pt: f32,
    paragraph_marks: &[&RevisionRecord],
) -> String {
    let mut out = String::new();
    if let Some(id) = style_id(kind) {
        out.push_str(&format!("<w:pStyle w:val=\"{id}\"/>"));
    }
    if let Some(id) = num_id {
        out.push_str(&format!(
            "<w:numPr><w:ilvl w:val=\"{list_level}\"/><w:numId w:val=\"{id}\"/></w:numPr>"
        ));
    }
    if let Some(direction) = bidi_property(style) {
        out.push_str(direction);
    }
    out.push_str(&paragraph_spacing_properties(style));
    out.push_str(&paragraph_indent_properties(style, num_id, list_indent_pt));
    out.push_str(&format!(
        "<w:jc w:val=\"{}\"/>",
        match style.alignment {
            Alignment::Start => "start",
            Alignment::End => "end",
            Alignment::Left => "left",
            Alignment::Center => "center",
            Alignment::Right => "right",
            Alignment::Justify => "both",
        }
    ));

    if !paragraph_marks.is_empty() {
        out.push_str("<w:rPr>");
        for revision in paragraph_marks {
            let tag = match revision.kind {
                RevisionKind::Insertion => "ins",
                RevisionKind::Deletion => "del",
                RevisionKind::Formatting => continue,
            };
            out.push_str(&format!("<w:{tag} {}/>", revision_attributes(revision)));
        }
        out.push_str("</w:rPr>");
    }
    out
}

fn paragraph_spacing_properties(style: &loom_text::ParagraphStyle) -> String {
    let mut spacing = String::new();
    let before = twips(style.space_before).max(0);
    if before != 0 {
        spacing.push_str(&format!(" w:before=\"{before}\""));
    }
    let after = twips(style.space_after).max(0);
    if after != DEFAULT_SPACE_AFTER_TWIPS {
        spacing.push_str(&format!(" w:after=\"{after}\""));
    }
    let line = (style.line_spacing * 240.0).round();
    if line.is_finite() && line > 0.0 && line as i32 != DEFAULT_LINE_TWIPS {
        spacing.push_str(&format!(
            " w:line=\"{}\" w:lineRule=\"auto\"",
            line.min(31_680.0) as i32
        ));
    }
    if spacing.is_empty() {
        String::new()
    } else {
        format!("<w:spacing{spacing}/>")
    }
}

fn paragraph_indent_properties(
    style: &loom_text::ParagraphStyle,
    num_id: Option<usize>,
    list_indent_pt: f32,
) -> String {
    let left = twips(style.left_indent).max(0);
    let right = twips(style.right_indent).max(0);
    let first = twips(style.first_line_indent);
    let mut indent = String::new();
    if num_id.is_some() {
        let level_twips = twips(list_indent_pt);
        if left != 0 || right != 0 || level_twips != 0 {
            indent.push_str(&format!(" w:left=\"{}\"", 720 + level_twips + left));
            if right != 0 {
                indent.push_str(&format!(" w:right=\"{right}\""));
            }
            indent.push_str(" w:hanging=\"360\"");
        }
    } else {
        if left != 0 {
            indent.push_str(&format!(" w:left=\"{left}\""));
        }
        if right != 0 {
            indent.push_str(&format!(" w:right=\"{right}\""));
        }
        if first > 0 {
            indent.push_str(&format!(" w:firstLine=\"{first}\""));
        } else if first < 0 {
            indent.push_str(&format!(" w:hanging=\"{}\"", -first));
        }
    }
    if indent.is_empty() {
        String::new()
    } else {
        format!("<w:ind{indent}/>")
    }
}

fn bidi_property(style: &ParagraphStyle) -> Option<&'static str> {
    match style.direction {
        ParagraphDirection::Auto => None,
        ParagraphDirection::LeftToRight => Some("<w:bidi w:val=\"0\"/>"),
        ParagraphDirection::RightToLeft => Some("<w:bidi/>"),
    }
}

/// Character formatting of one run, in the order WordprocessingML lists it.
/// Every run states its font family and size, so Word never falls back to its
/// own defaults for text the editor draws.
#[derive(PartialEq)]
struct RunProps {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    vertical: Option<&'static str>,
    color: Option<RunColor>,
    family: String,
    half_points: u32,
}

#[derive(PartialEq)]
enum RunColor {
    Automatic,
    Rgb(String),
}

impl RunProps {
    /// What the editor draws (bold at weight 700 and above, italic, underline,
    /// strikethrough), the run's font family and size, and super/subscript.
    /// `kind_pt` is the size the block kind gives text that names none, so a
    /// heading keeps its heading size.
    fn of(style: Option<&CharacterStyle>, force_bold: bool, kind_pt: f32) -> Self {
        let mut props = Self {
            bold: force_bold,
            italic: false,
            underline: false,
            strike: false,
            vertical: None,
            color: None,
            family: family_name(style.map_or("", |style| style.font_family.as_str())),
            half_points: half_points_of(kind_pt),
        };
        let Some(style) = style else { return props };
        props.bold |= style.weight.numeric() >= FontWeight::Bold.numeric();
        props.italic = style.italic;
        props.underline = style.underline;
        props.strike = style.strikethrough;
        props.vertical = if style.superscript {
            Some("superscript")
        } else if style.subscript {
            Some("subscript")
        } else {
            None
        };
        props.color = Some(match style.color.as_deref() {
            None => RunColor::Automatic,
            Some(color) => RunColor::Rgb(
                docx_rgb(color).expect("inline colors are validated before DOCX body export"),
            ),
        });
        // A size set explicitly wins; the default size keeps the block kind's.
        let size = style.font_size;
        if size.is_finite() && style.has_own_size() && (1.0..=1638.0).contains(&size) {
            props.half_points = half_points_of(size);
        }
        props
    }

    fn xml(&self) -> String {
        let mut out = String::new();
        let family = xml::attr(&self.family);
        out.push_str(&format!(
            "<w:rFonts w:ascii=\"{family}\" w:hAnsi=\"{family}\" w:eastAsia=\"{family}\" \
             w:cs=\"{family}\"/>"
        ));
        match &self.color {
            Some(RunColor::Automatic) => out.push_str("<w:color w:val=\"auto\"/>"),
            Some(RunColor::Rgb(rgb)) => out.push_str(&format!("<w:color w:val=\"{rgb}\"/>")),
            None => {}
        }
        if self.bold {
            out.push_str("<w:b/><w:bCs/>");
        }
        if self.italic {
            out.push_str("<w:i/><w:iCs/>");
        }
        if self.strike {
            out.push_str("<w:strike/>");
        }
        let size = self.half_points;
        out.push_str(&format!(
            "<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>"
        ));
        if self.underline {
            out.push_str("<w:u w:val=\"single\"/>");
        }
        if let Some(vertical) = self.vertical {
            out.push_str(&format!("<w:vertAlign w:val=\"{vertical}\"/>"));
        }
        format!("<w:rPr>{out}</w:rPr>")
    }
}

/// The Word face a Writer family is written as: "Sans" and an unnamed family
/// are Arial, "Serif" and "Monospace" (or "Mono") the usual Word faces, and any
/// other name is kept as it is, so a document set in Georgia opens in Georgia.
fn family_name(name: &str) -> String {
    let name = name.trim();
    let is = |generic: &str| name.eq_ignore_ascii_case(generic);
    if name.is_empty() || is("Sans") {
        SANS_FAMILY.to_string()
    } else if is("Serif") {
        "Times New Roman".to_string()
    } else if is("Monospace") || is("Mono") {
        "Courier New".to_string()
    } else {
        name.to_string()
    }
}

/// Half-points, the unit `w:sz` is written in, for a size in points.
fn half_points_of(points: f32) -> u32 {
    (points * 2.0).round().max(2.0) as u32
}

/// The size a block kind gives text that names none: headings take their
/// heading size, every other block the editor's default run size.
fn kind_size_pt(kind: &str) -> f32 {
    if kind.starts_with("heading") {
        PageStyle::default().font_size_for_kind(kind)
    } else {
        CharacterStyle::default().font_size
    }
}

fn push_footnote_reference(out: &mut String, id: i32) {
    out.push_str(&format!("<w:r><w:footnoteReference w:id=\"{id}\"/></w:r>"));
}

fn revision_attributes(revision: &RevisionRecord) -> String {
    format!(
        "w:id=\"{}\" w:author=\"{}\"",
        revision.id,
        xml::attr(&revision.author)
    )
}

fn revision_style_at(revision: &RevisionRecord, offset: usize) -> Option<&CharacterStyle> {
    revision
        .before
        .as_ref()?
        .runs
        .iter()
        .find(|run| run.start <= offset && offset < run.end)
        .map(|run| &run.style)
}

fn revision_style_xml(style: Option<&CharacterStyle>, kind_size: f32) -> String {
    let mut properties = RunProps::of(style, false, kind_size).xml();
    if style.map_or(true, |style| style.font_family.is_empty()) {
        remove_run_property(&mut properties, "w:rFonts");
    }
    if style.map_or(true, |style| !style.has_own_size()) {
        remove_run_property(&mut properties, "w:sz");
        remove_run_property(&mut properties, "w:szCs");
    }
    if style.is_some_and(|style| style.color.is_none()) {
        remove_run_property(&mut properties, "w:color");
    }
    properties
}

fn remove_run_property(properties: &mut String, property: &str) {
    let Some(start) = properties.find(&format!("<{property}")) else {
        return;
    };
    let Some(end) = properties[start..].find("/>") else {
        return;
    };
    properties.replace_range(start..start + end + 2, "");
}

fn push_deleted_revision(out: &mut String, revision: &RevisionRecord, kind_size: f32) {
    let Some(payload) = revision.before.as_ref() else {
        return;
    };
    out.push_str(&format!("<w:del {}>", revision_attributes(revision)));
    let mut cuts = BTreeSet::from([0, payload.text.len()]);
    cuts.extend(
        payload
            .runs
            .iter()
            .flat_map(|run| [run.start, run.end])
            .filter(|&at| at <= payload.text.len() && payload.text.is_char_boundary(at)),
    );
    let cuts: Vec<usize> = cuts.into_iter().collect();
    for pair in cuts.windows(2) {
        let [start, end] = pair else {
            continue;
        };
        let style = payload
            .runs
            .iter()
            .find(|run| run.start <= *start && *start < run.end)
            .map(|run| &run.style);
        let mut children = String::new();
        if xml::push_deleted_run_children(&mut children, &payload.text[*start..*end]) {
            out.push_str("<w:r>");
            out.push_str(&revision_style_xml(style, kind_size));
            out.push_str(&children);
            out.push_str("</w:r>");
        }
    }
    out.push_str("</w:del>");
}

fn push_deleted_revision_slice(
    out: &mut String,
    revision: &RevisionRecord,
    range: std::ops::Range<usize>,
    kind_size: f32,
) {
    let Some(payload) = revision.before.as_ref() else {
        return;
    };
    let text = payload.text.get(range.clone()).unwrap_or_default();
    if text.is_empty() {
        return;
    }
    let mut fragment = revision.clone();
    fragment.before = Some(RevisionPayload {
        text: text.to_owned(),
        runs: payload
            .runs
            .iter()
            .filter_map(|run| {
                let start = run.start.max(range.start);
                let end = run.end.min(range.end);
                (start < end).then(|| loom_text::StyleRun {
                    start: start - range.start,
                    end: end - range.start,
                    style: run.style.clone(),
                })
            })
            .collect(),
    });
    push_deleted_revision(out, &fragment, kind_size);
}

fn rich_block_text_slice(block: &RichBlock, range: std::ops::Range<usize>) -> RichBlock {
    let mut slice = RichBlock::new(block.id, &block.kind, &block.text.as_str()[range.clone()]);
    slice.style = block.style.clone();
    slice.runs = block
        .runs
        .iter()
        .filter_map(|run| {
            let start = run.start.max(range.start);
            let end = run.end.min(range.end);
            (start < end).then(|| loom_text::StyleRun {
                start: start - range.start,
                end: end - range.start,
                style: run.style.clone(),
            })
        })
        .collect();
    slice
}

fn deleted_paragraph_mark_xml(block: &RichBlock, revision: &RevisionRecord) -> String {
    let split = revision.anchor.start;
    let text = block.text.as_str();
    let first = rich_block_text_slice(block, 0..split);
    let second = rich_block_text_slice(block, split..text.len());
    let deleted_payload = revision
        .before
        .as_ref()
        .expect("preflight checked deletion payload");
    let mark_offset = deleted_payload
        .text
        .find('\n')
        .expect("preflight checked paragraph-mark payload");
    let size = kind_size_pt(&block.kind);
    let mut xml = String::new();

    xml.push_str("<w:p>");
    xml.push_str(&paragraph_properties(&first, None, &[revision], None));
    xml.push_str(&paragraph_content(
        &first,
        0,
        std::slice::from_ref(&first),
        &[],
        &[],
        &[],
        &[],
    ));
    push_deleted_revision_slice(&mut xml, revision, 0..mark_offset, size);
    xml.push_str("</w:p>");

    xml.push_str("<w:p>");
    xml.push_str(&paragraph_properties(&second, None, &[], None));
    push_deleted_revision_slice(
        &mut xml,
        revision,
        mark_offset + 1..deleted_payload.text.len(),
        size,
    );
    xml.push_str(&paragraph_content(
        &second,
        0,
        std::slice::from_ref(&second),
        &[],
        &[],
        &[],
        &[],
    ));
    xml.push_str("</w:p>");
    xml
}

fn push_revision_run(
    out: &mut String,
    props: &RunProps,
    style: Option<&CharacterStyle>,
    piece: &str,
    formatting: Option<(&RevisionRecord, usize)>,
    kind_size: f32,
    tracked: bool,
) {
    let mut children = String::new();
    if !xml::push_run_children(&mut children, piece) {
        return;
    }
    let mut run_props = if tracked {
        revision_style_xml(style, kind_size)
    } else {
        props.xml()
    };
    if let Some((revision, offset)) = formatting {
        let previous = revision_style_xml(revision_style_at(revision, offset), kind_size);
        let previous = previous
            .strip_prefix("<w:rPr>")
            .and_then(|value| value.strip_suffix("</w:rPr>"))
            .unwrap_or(&previous);
        let change = format!(
            "<w:rPrChange {}><w:rPr>{previous}</w:rPr></w:rPrChange>",
            revision_attributes(revision)
        );
        let close = run_props
            .rfind("</w:rPr>")
            .expect("revision style always writes a closing rPr");
        run_props.insert_str(close, &change);
    }
    out.push_str("<w:r>");
    out.push_str(&run_props);
    out.push_str(&children);
    out.push_str("</w:r>");
}

fn paragraph_revision_segments<'a>(
    block_index: usize,
    blocks: &'a [RichBlock],
    revisions: &'a [RevisionRecord],
    paragraph_format_ids: &[u64],
) -> Vec<(&'a RevisionRecord, usize, usize, usize)> {
    let block = &blocks[block_index];
    let block_start = blocks[..block_index]
        .iter()
        .map(|block| block.text.len_bytes() + 1)
        .sum::<usize>();
    let mut segments = Vec::new();
    for revision in revisions {
        if paragraph_format_ids.contains(&revision.id) {
            continue;
        }
        let Some(start_index) = blocks
            .iter()
            .position(|candidate| candidate.id == revision.anchor.block_id)
        else {
            continue;
        };
        let end_id = revision
            .anchor
            .end_block_id
            .unwrap_or(revision.anchor.block_id);
        let Some(end_index) = blocks.iter().position(|candidate| candidate.id == end_id) else {
            continue;
        };
        if block_index < start_index || block_index > end_index {
            continue;
        }
        let start = if block_index == start_index {
            revision.anchor.start
        } else {
            0
        };
        let end = if block_index == end_index {
            revision.anchor.end
        } else {
            block.text.len_bytes()
        };
        if start > end
            || end > block.text.len_bytes()
            || (start == end && revision.kind != RevisionKind::Deletion)
        {
            continue;
        }
        let revision_start = blocks[..start_index]
            .iter()
            .map(|block| block.text.len_bytes() + 1)
            .sum::<usize>()
            + revision.anchor.start;
        let payload_offset = block_start + start - revision_start;
        segments.push((revision, start, end, payload_offset));
    }
    segments
}

fn paragraph_mark_revisions<'a>(
    block_index: usize,
    blocks: &[RichBlock],
    revisions: &'a [RevisionRecord],
) -> Vec<&'a RevisionRecord> {
    let mut starts = Vec::with_capacity(blocks.len());
    let mut offset = 0usize;
    for (index, block) in blocks.iter().enumerate() {
        starts.push(offset);
        offset += block.text.len_bytes() + usize::from(index + 1 < blocks.len());
    }
    let separator = starts[block_index] + blocks[block_index].text.len_bytes();
    revisions
        .iter()
        .filter(|revision| {
            if revision.kind != RevisionKind::Insertion
                || revision.boundary_kind != RevisionBoundaryKind::ParagraphMark
            {
                return false;
            }
            let Some(start_index) = blocks
                .iter()
                .position(|block| block.id == revision.anchor.block_id)
            else {
                return false;
            };
            let end_block_id = revision
                .anchor
                .end_block_id
                .unwrap_or(revision.anchor.block_id);
            let Some(end_index) = blocks.iter().position(|block| block.id == end_block_id) else {
                return false;
            };
            let start = starts[start_index] + revision.anchor.start;
            let end = starts[end_index] + revision.anchor.end;
            start <= separator && separator < end
        })
        .collect()
}

/// The runs of a block. The text is cut at style, comment, footnote, and
/// revision boundaries so each generated run has one stable set of properties.
fn paragraph_content(
    block: &RichBlock,
    block_index: usize,
    blocks: &[RichBlock],
    anchors: &[Anchor],
    footnotes: &[FootnoteReference],
    all_revisions: &[RevisionRecord],
    paragraph_format_ids: &[u64],
) -> String {
    let text = block.text.as_str();
    let revisions =
        paragraph_revision_segments(block_index, blocks, all_revisions, paragraph_format_ids);
    let mut cuts: BTreeSet<usize> = BTreeSet::from([0, text.len()]);
    let boundaries = block
        .runs
        .iter()
        .flat_map(|run| [run.start, run.end])
        .chain(anchors.iter().flat_map(|anchor| [anchor.start, anchor.end]))
        .chain(footnotes.iter().map(|reference| reference.offset));
    cuts.extend(boundaries.filter(|&at| at <= text.len() && text.is_char_boundary(at)));
    for (revision, start, end, payload_offset) in revisions.iter().copied() {
        cuts.extend([start, end]);
        if revision.kind == RevisionKind::Formatting {
            if let Some(before) = &revision.before {
                let payload_end = payload_offset + end - start;
                for run in &before.runs {
                    let run_start = run.start.max(payload_offset);
                    let run_end = run.end.min(payload_end);
                    if run_start < run_end {
                        cuts.insert(start + run_start - payload_offset);
                        cuts.insert(start + run_end - payload_offset);
                    }
                }
            }
        }
    }
    let cuts: Vec<usize> = cuts.into_iter().collect();

    let size = kind_size_pt(&block.kind);
    let mut out = String::new();
    let mut footnote_index = 0usize;
    let mut open_insertion = None;
    for (index, &at) in cuts.iter().enumerate() {
        comments::push_events(&mut out, anchors, at);
        for (revision, start, _, _) in
            revisions.iter().copied().filter(|(revision, start, _, _)| {
                revision.kind == RevisionKind::Deletion && *start == at
            })
        {
            let _ = start;
            push_deleted_revision(&mut out, revision, size);
        }
        while let Some(reference) = footnotes
            .get(footnote_index)
            .filter(|reference| reference.offset == at)
        {
            push_footnote_reference(&mut out, reference.ooxml_id);
            footnote_index += 1;
        }
        let Some(&next) = cuts.get(index + 1) else {
            break;
        };
        let insertion = revisions.iter().copied().find(|(revision, start, end, _)| {
            revision.kind == RevisionKind::Insertion && *start <= at && next <= *end
        });
        let insertion_id = insertion.map(|(revision, _, _, _)| revision.id);
        if open_insertion != insertion_id {
            if open_insertion.is_some() {
                out.push_str("</w:ins>");
                open_insertion = None;
            }
            if let Some((revision, _, _, _)) = insertion {
                out.push_str(&format!("<w:ins {}>", revision_attributes(revision)));
                open_insertion = Some(revision.id);
            }
        }
        let style = if let Some((revision, start, _, payload_offset)) = insertion {
            // A tracked insertion draws the style stored with its payload; an
            // empty payload means the range has no explicit character style.
            revision_style_at(revision, payload_offset + at - start)
        } else {
            block
                .runs
                .iter()
                .find(|run| run.start <= at && at < run.end)
                .map(|run| &run.style)
        };
        let props = RunProps::of(style, false, size);
        let formatting = revisions
            .iter()
            .copied()
            .find(|(revision, start, end, _)| {
                revision.kind == RevisionKind::Formatting && *start <= at && next <= *end
            })
            .map(|(revision, start, _, payload_offset)| (revision, payload_offset + at - start));
        let tracked = insertion.is_some() || formatting.is_some();
        push_revision_run(
            &mut out,
            &props,
            style,
            &text[at..next],
            formatting,
            size,
            tracked,
        );
        if insertion.is_some_and(|(_, _, end, _)| end == next) {
            out.push_str("</w:ins>");
            open_insertion = None;
        }
    }
    if open_insertion.is_some() {
        out.push_str("</w:ins>");
    }
    out
}

/// The opening `<w:p>` of a table cell. The column's alignment becomes the
/// paragraph's justification.
fn cell_paragraph(alignment: Option<&HorizontalAlign>) -> &'static str {
    match alignment {
        Some(HorizontalAlign::Center) => "<w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr>",
        Some(HorizontalAlign::Right) => "<w:p><w:pPr><w:jc w:val=\"right\"/></w:pPr>",
        Some(HorizontalAlign::Justify) => "<w:p><w:pPr><w:jc w:val=\"both\"/></w:pPr>",
        _ => "<w:p>",
    }
}

/// The merged cell spans of a table as `(row, column, row_span, column_span)`.
fn merge_spans(table: &Table) -> Vec<(usize, usize, usize, usize)> {
    let mut spans = Vec::new();
    for row in 0..table.row_count() {
        for column in 0..table.col_count() {
            if let Some(cell) = table.cell(row, column) {
                if !cell.is_covered && (cell.row_span > 1 || cell.col_span > 1) {
                    spans.push((row, column, cell.row_span, cell.col_span));
                }
            }
        }
    }
    spans
}

fn cell_footnote_references(
    references: &[FootnoteReference],
    map: &crate::tables::TableTextMap,
    row: usize,
    column: usize,
) -> Vec<FootnoteReference> {
    let Some(range) = map.cell_range(row, column) else {
        return Vec::new();
    };
    let start = references.partition_point(|reference| reference.offset < range.start);
    let end = references.partition_point(|reference| reference.offset <= range.end);
    references[start..end]
        .iter()
        .map(|reference| FootnoteReference {
            offset: reference.offset - range.start,
            ooxml_id: reference.ooxml_id,
        })
        .collect()
}

/// The column widths of a table in twips across the text area: the typed widths
/// scaled to fill it, the same split the page draws, so a table of equal
/// columns is split equally. The last column takes the rounding remainder.
fn column_widths(table: &Table, text_width: i32) -> Vec<i32> {
    let columns = table.col_count();
    if columns == 0 {
        return Vec::new();
    }
    let typed: Vec<f32> = (0..columns)
        .map(|column| {
            table
                .columns
                .get(column)
                .map_or(0.0, |col| col.width_pt.max(0.0))
        })
        .collect();
    let sum: f32 = typed.iter().sum();
    let mut widths: Vec<i32> = if sum <= 0.0 {
        vec![text_width / columns as i32; columns]
    } else {
        typed
            .iter()
            .map(|width| (text_width as f32 * width / sum).round() as i32)
            .collect()
    };
    let used: i32 = widths[..columns - 1].iter().sum();
    widths[columns - 1] = (text_width - used).max(1);
    widths
}

struct CellExportSources<'a> {
    footnotes: &'a [FootnoteReference],
    revisions: &'a [RevisionRecord],
}

fn cell_content(
    block_id: u64,
    table: &Table,
    map: &crate::tables::TableTextMap,
    row: usize,
    column: usize,
    bold: bool,
    sources: CellExportSources<'_>,
) -> String {
    let Some(range) = map.cell_range(row, column) else {
        return String::new();
    };
    let Some(mut block) = crate::table_text::table_cell_rich_block(
        block_id,
        table,
        row,
        column,
        TABLE_CELL_BASE_FONT_SIZE_PT,
        bold,
        None,
    ) else {
        return String::new();
    };
    if block.runs.is_empty() && !block.text.is_empty() {
        // A plain cell run states automatic ink explicitly, so Word does not
        // apply an unrelated inherited colour to the cell's default text.
        block.runs.push(loom_text::StyleRun {
            start: 0,
            end: block.text.len_bytes(),
            style: CharacterStyle {
                color: None,
                ..CharacterStyle::default()
            },
        });
    }
    let local_revisions: Vec<RevisionRecord> = sources
        .revisions
        .iter()
        .filter(|revision| {
            revision.anchor.block_id == block_id
                && revision.anchor.end_block_id.is_none()
                && revision.anchor.start >= range.start
                && revision.anchor.end <= range.end
        })
        .cloned()
        .map(|mut revision| {
            revision.anchor.start -= range.start;
            revision.anchor.end -= range.start;
            revision
        })
        .collect();
    paragraph_content(
        &block,
        0,
        std::slice::from_ref(&block),
        &[],
        sources.footnotes,
        &local_revisions,
        &[],
    )
}

/// A table as a real Word table: the typed column widths across
/// the text area and a repeating, shaded, bold header row when the source
/// has a header separator. Short rows are padded to a rectangle.
fn table_xml(
    table: &Table,
    text_width: i32,
    block_id: u64,
    footnotes: &FootnoteExportPlan,
    all_revisions: &[RevisionRecord],
) -> String {
    let merges = merge_spans(table);
    let table_text_map = crate::tables::TableTextMap::new(table);
    let footnote_anchors = footnotes.references_for(block_id);
    let columns = table.col_count();
    let widths = column_widths(table, text_width);

    let look = if table.header_row { 1 } else { 0 };
    let mut out = format!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"{STYLE_TABLE}\"/><w:tblW w:w=\"{text_width}\" w:type=\"dxa\"/>\
         <w:tblLayout w:type=\"fixed\"/>\
         <w:tblLook w:val=\"04A0\" w:firstRow=\"{look}\" w:lastRow=\"0\" w:firstColumn=\"0\" w:lastColumn=\"0\" w:noHBand=\"1\" w:noVBand=\"1\"/>\
         </w:tblPr><w:tblGrid>"
    );
    for width in &widths {
        out.push_str(&format!("<w:gridCol w:w=\"{width}\"/>"));
    }
    out.push_str("</w:tblGrid>");
    let catalog_style = loom_objects::TableStyleCatalog::get(&table.style_id)
        .unwrap_or_else(loom_objects::TableStyleCatalog::default_style);
    let options = loom_objects::TableStyleOptions {
        header_row: table.header_row,
        banded_rows: table.banded_rows,
        first_column: table.first_column,
        ..Default::default()
    };
    for row_index in 0..table.row_count() {
        let header = table.header_row && row_index == 0;
        out.push_str("<w:tr><w:trPr><w:cantSplit/>");
        if header {
            out.push_str("<w:tblHeader/>");
        }
        out.push_str("</w:trPr>");
        for (column, width) in widths.iter().enumerate() {
            if let Some(&(mr, mc, mrs, mcs)) = merges.iter().find(|(r, c, rs, cs)| {
                row_index >= *r && row_index < *r + *rs && column >= *c && column < *c + *cs
            }) {
                if column > mc {
                    continue;
                }
                let span_width: i32 = widths[mc..mc + mcs].iter().sum();
                let resolved = catalog_style.resolve_cell(
                    row_index,
                    column,
                    table.row_count(),
                    columns,
                    &options,
                    table.cell(row_index, column),
                );
                let paragraph = cell_paragraph(Some(&resolved.horizontal_align));
                let shading = if let Some(bg) = resolved.background_color {
                    let hex = bg.trim_start_matches('#');
                    format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{hex}\"/>")
                } else if header {
                    "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F2F2F2\"/>".to_string()
                } else {
                    String::new()
                };
                let grid_span = if mcs > 1 {
                    format!("<w:gridSpan w:val=\"{mcs}\"/>")
                } else {
                    String::new()
                };
                let v_merge = if mrs > 1 {
                    if row_index == mr {
                        "<w:vMerge w:val=\"restart\"/>"
                    } else {
                        "<w:vMerge/>"
                    }
                } else {
                    ""
                };
                out.push_str(&format!(
                    "<w:tc><w:tcPr><w:tcW w:w=\"{span_width}\" w:type=\"dxa\"/>{grid_span}{v_merge}{shading}</w:tcPr>{paragraph}"
                ));
                if row_index == mr {
                    let cell_footnotes = cell_footnote_references(
                        footnote_anchors,
                        &table_text_map,
                        row_index,
                        column,
                    );
                    out.push_str(&cell_content(
                        block_id,
                        table,
                        &table_text_map,
                        row_index,
                        column,
                        header || resolved.bold,
                        CellExportSources {
                            footnotes: &cell_footnotes,
                            revisions: all_revisions,
                        },
                    ));
                }
                out.push_str("</w:p></w:tc>");
                continue;
            }

            let resolved = catalog_style.resolve_cell(
                row_index,
                column,
                table.row_count(),
                columns,
                &options,
                table.cell(row_index, column),
            );
            let paragraph = cell_paragraph(Some(&resolved.horizontal_align));
            let shading = if let Some(bg) = resolved.background_color {
                let hex = bg.trim_start_matches('#');
                format!("<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"{hex}\"/>")
            } else if header {
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F2F2F2\"/>".to_string()
            } else {
                String::new()
            };
            out.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/>{shading}</w:tcPr>{paragraph}"
            ));
            let cell_footnotes =
                cell_footnote_references(footnote_anchors, &table_text_map, row_index, column);
            out.push_str(&cell_content(
                block_id,
                table,
                &table_text_map,
                row_index,
                column,
                header || resolved.bold,
                CellExportSources {
                    footnotes: &cell_footnotes,
                    revisions: all_revisions,
                },
            ));
            out.push_str("</w:p></w:tc>");
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out
}
