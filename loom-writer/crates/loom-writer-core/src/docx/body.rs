//! `word/document.xml`: Writer blocks as WordprocessingML paragraphs, lists,
//! tables and the page-setup section.

use std::collections::BTreeSet;

use loom_objects::table::{HorizontalAlign, Table, TextRun};
use loom_text::{Alignment, CharacterStyle, FontWeight, ParagraphDirection, ParagraphStyle};

use super::comments::{self, Anchor, CommentPlan};
use super::inline_style::docx_rgb;
use super::package::{
    DEFAULT_LINE_TWIPS, DEFAULT_SPACE_AFTER_TWIPS, SANS_FAMILY, STYLE_LIST, STYLE_QUOTE,
    STYLE_TABLE,
};
use super::xml::{self, R_NS, W_NS, XML_DECLARATION};
use crate::{
    PageOrientation, PageStyle, PaperSize, RichBlock, WriterDocument, TABLE_BLOCK_KIND,
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
        let (short, long) = paper_twips(doc.page.paper);
        let landscape = doc.page.orientation == PageOrientation::Landscape;
        let (width, height) = if landscape {
            (long, short)
        } else {
            (short, long)
        };
        let (top, bottom, left, right) = doc.page.margins.margins_pt();
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

    fn section_xml(&self) -> String {
        let header = (self.top / 2).min(720);
        let orient = if self.landscape {
            " w:orient=\"landscape\""
        } else {
            ""
        };
        format!(
            "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"{orient}/>\
             <w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\" w:header=\"{header}\" w:footer=\"{header}\" w:gutter=\"0\"/>\
             <w:cols w:space=\"720\"/></w:sectPr>",
            self.width, self.height, self.top, self.right, self.bottom, self.left
        )
    }
}

/// Builds the document part.
pub(super) fn build(doc: &WriterDocument, plan: &CommentPlan) -> Body {
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

    let mut xml =
        format!("{XML_DECLARATION}<w:document xmlns:w=\"{W_NS}\" xmlns:r=\"{R_NS}\"><w:body>");
    let mut numbered_lists = 0usize;
    let mut previous_numbered = false;
    let mut charts: Vec<(usize, String)> = Vec::new();
    let mut images: Vec<BodyImage> = Vec::new();
    for (index, block) in doc.blocks.iter().enumerate() {
        if let Some(table) = &tables[index] {
            xml.push_str(&table_xml(table, geometry.text_width()));
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
                let chart_xml = loom_objects::chart_ooxml::chart_to_c_chart_xml(&chart);
                charts.push((chart_idx, chart_xml));
                xml.push_str(&chart_xml_paragraph(&rid, chart_idx, block));
                previous_numbered = false;
                continue;
            }
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
        xml.push_str(&paragraph_properties(block, num_id));
        xml.push_str(&paragraph_content(block, plan.anchors_for(block.id)));
        xml.push_str("</w:p>");
    }
    xml.push_str(&geometry.section_xml());
    xml.push_str("</w:body></w:document>");
    Body {
        xml,
        numbered_lists,
        charts,
        images,
    }
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
    let alignment = paragraph_properties(block, None);
    format!(
        "<w:p>{alignment}<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\">\n         <wp:extent cx=\"{cx}\" cy=\"{cy}\"/>\n         <wp:docPr id=\"{id}\" name=\"Picture {index}\" descr=\"{name}\"/>\n         <a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\">\n         <a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\n         <pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\">\n         <pic:nvPicPr><pic:cNvPr id=\"{id}\" name=\"{name}\"/><pic:cNvPicPr/></pic:nvPicPr>\n         <pic:blipFill><a:blip xmlns:r=\"{R_NS}\" r:embed=\"rIdImg{index}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill>\n         <pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr>\n         </pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>",
        id = 1000 + index
    )
}

fn chart_xml_paragraph(rid: &str, chart_id: usize, block: &RichBlock) -> String {
    let properties = paragraph_properties(block, None);
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

/// `<w:pPr>` for a block. Direct spacing and indentation are written only
/// where the block differs from Writer's defaults, so the Word styles keep
/// control of everything the user did not change.
fn paragraph_properties(block: &RichBlock, num_id: Option<usize>) -> String {
    let style = &block.style;
    let mut out = String::new();
    if let Some(id) = style_id(block.kind.as_str()) {
        out.push_str(&format!("<w:pStyle w:val=\"{id}\"/>"));
    }
    if let Some(id) = num_id {
        let level = block.effective_list_level();
        out.push_str(&format!(
            "<w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{id}\"/></w:numPr>"
        ));
    }

    if let Some(direction) = bidi_property(style) {
        out.push_str(direction);
    }

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
    if !spacing.is_empty() {
        out.push_str(&format!("<w:spacing{spacing}/>"));
    }

    let left = twips(style.left_indent).max(0);
    let right = twips(style.right_indent).max(0);
    let first = twips(style.first_line_indent);
    let mut indent = String::new();
    if num_id.is_some() {
        // The list level supplies a hanging indent; keep it when the user
        // moved the item. A nested item sits 18 pt in per level, as Writer draws it.
        let level_twips = twips(block.list_indent_pt());
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
    if !indent.is_empty() {
        out.push_str(&format!("<w:ind{indent}/>"));
    }

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

    if out.is_empty() {
        out
    } else {
        format!("<w:pPr>{out}</w:pPr>")
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

/// Appends one run of `piece` with `props`, skipping runs with nothing to say.
fn push_run(out: &mut String, props: &RunProps, piece: &str) {
    let mut children = String::new();
    if xml::push_run_children(&mut children, piece) {
        out.push_str("<w:r>");
        out.push_str(&props.xml());
        out.push_str(&children);
        out.push_str("</w:r>");
    }
}

/// The runs of a block. The text is cut wherever the inline style changes or
/// a comment starts or ends, so every piece is uniform. Pieces are styled by
/// the first run that covers them, the same rule the PDF export uses.
fn paragraph_content(block: &RichBlock, anchors: &[Anchor]) -> String {
    let text = block.text.as_str();
    let mut cuts: BTreeSet<usize> = BTreeSet::from([0, text.len()]);
    let boundaries = block
        .runs
        .iter()
        .flat_map(|run| [run.start, run.end])
        .chain(anchors.iter().flat_map(|anchor| [anchor.start, anchor.end]));
    cuts.extend(boundaries.filter(|&at| at <= text.len() && text.is_char_boundary(at)));
    let cuts: Vec<usize> = cuts.into_iter().collect();

    let size = kind_size_pt(&block.kind);
    let mut out = String::new();
    for (index, &at) in cuts.iter().enumerate() {
        comments::push_events(&mut out, anchors, at);
        let Some(&next) = cuts.get(index + 1) else {
            break;
        };
        let style = block
            .runs
            .iter()
            .find(|run| run.start <= at && at < run.end)
            .map(|run| &run.style);
        push_run(&mut out, &RunProps::of(style, false, size), &text[at..next]);
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

/// Writes one cell's text as runs. Each run keeps its stored bold and italic;
/// `bold` is the cell's own bold, from the header row or the table style.
fn push_cell_runs(out: &mut String, runs: &[TextRun], bold: bool) {
    for piece in runs {
        let mut style = CharacterStyle::default();
        style.weight = if piece.bold {
            FontWeight::Bold
        } else {
            FontWeight::Regular
        };
        style.italic = piece.italic;
        style.underline = piece.underline;
        style.strikethrough = piece.strikethrough;
        style.superscript = piece.superscript;
        style.subscript = piece.subscript;
        style.font_family = piece.font_family.clone().unwrap_or_default();
        style.font_size = piece.font_size_pt.unwrap_or(TABLE_CELL_BASE_FONT_SIZE_PT);
        style.explicit_size = piece.font_size_pt.is_some();
        style.color = piece.color.clone();
        let props = RunProps::of(Some(&style), bold, TABLE_CELL_BASE_FONT_SIZE_PT);
        push_run(out, &props, &piece.text);
    }
}

/// The alignment of a column: the first explicit alignment set on its cells.
fn column_alignment(table: &Table, column: usize) -> Option<HorizontalAlign> {
    (0..table.row_count()).find_map(|row| {
        table
            .cell(row, column)
            .and_then(|cell| cell.style_override.as_ref())
            .and_then(|style| style.horizontal_align)
    })
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

/// The runs of one cell, each keeping its bold and italic.
fn cell_runs(table: &Table, row: usize, column: usize) -> Vec<TextRun> {
    table
        .cell(row, column)
        .map_or_else(Vec::new, |cell| cell.content.runs.clone())
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

/// A table as a real Word table: the typed column widths across
/// the text area and a repeating, shaded, bold header row when the source
/// has a header separator. Short rows are padded to a rectangle.
fn table_xml(table: &Table, text_width: i32) -> String {
    let merges = merge_spans(table);
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
            let paragraph = cell_paragraph(column_alignment(table, column).as_ref());
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
                    None,
                );
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
                    let runs = cell_runs(table, row_index, column);
                    push_cell_runs(&mut out, &runs, header || resolved.bold);
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
                None,
            );
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
            let runs = cell_runs(table, row_index, column);
            push_cell_runs(&mut out, &runs, header || resolved.bold);
            out.push_str("</w:p></w:tc>");
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out
}
