//! `word/document.xml`: Writer blocks as WordprocessingML paragraphs, lists,
//! tables and the page-setup section.

use std::collections::BTreeSet;

use loom_text::{Alignment, CharacterStyle, FontWeight};

use super::comments::{self, Anchor, CommentPlan};
use super::package::{
    DEFAULT_LINE_TWIPS, DEFAULT_SPACE_AFTER_TWIPS, STYLE_LIST, STYLE_QUOTE, STYLE_TABLE,
};
use super::xml::{self, R_NS, W_NS, XML_DECLARATION};
use crate::{
    parse_table_markdown, PageOrientation, PaperSize, RichBlock, WriterDocument, WriterTable,
    TABLE_BLOCK_KIND,
};

/// The body part plus what the numbering part needs to know about it.
pub(super) struct Body {
    pub xml: String,
    /// How many separate numbered lists the body contains.
    pub numbered_lists: usize,
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
    let tables: Vec<Option<WriterTable>> = doc
        .blocks
        .iter()
        .map(|block| {
            (block.kind == TABLE_BLOCK_KIND)
                .then(|| parse_table_markdown(block.text.as_str()))
                .filter(|table| table.columns() > 0)
        })
        .collect();

    let mut xml =
        format!("{XML_DECLARATION}<w:document xmlns:w=\"{W_NS}\" xmlns:r=\"{R_NS}\"><w:body>");
    let mut numbered_lists = 0usize;
    let mut previous_numbered = false;
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
        previous_numbered = list == Some(ListKind::Numbered);
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
    }
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
        out.push_str(&format!(
            "<w:numPr><w:ilvl w:val=\"0\"/><w:numId w:val=\"{id}\"/></w:numPr>"
        ));
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
        // moved the item.
        if left != 0 || right != 0 {
            indent.push_str(&format!(" w:left=\"{}\"", 720 + left));
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

    match style.alignment {
        Alignment::Left => {}
        Alignment::Center => out.push_str("<w:jc w:val=\"center\"/>"),
        Alignment::Right => out.push_str("<w:jc w:val=\"right\"/>"),
        Alignment::Justify => out.push_str("<w:jc w:val=\"both\"/>"),
    }

    if out.is_empty() {
        out
    } else {
        format!("<w:pPr>{out}</w:pPr>")
    }
}

/// Character formatting of one run, in the order WordprocessingML lists it.
#[derive(Default, PartialEq)]
struct RunProps {
    bold: bool,
    italic: bool,
    underline: bool,
    strike: bool,
    vertical: Option<&'static str>,
    family: Option<String>,
    half_points: Option<u32>,
}

impl RunProps {
    /// What the editor draws (bold at weight 700 and above, italic, underline,
    /// strikethrough) plus the formatting the inspector records explicitly:
    /// super/subscript, a font family other than the default sans, and a
    /// size other than the model's unset default.
    fn of(style: Option<&CharacterStyle>, force_bold: bool) -> Self {
        let mut props = Self {
            bold: force_bold,
            ..Self::default()
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
        props.family = match style.font_family.trim() {
            "" | "Sans" => None,
            "Serif" => Some("Times New Roman".to_string()),
            "Monospace" => Some("Courier New".to_string()),
            other => Some(other.to_string()),
        };
        let size = style.font_size;
        if size.is_finite()
            && (size - CharacterStyle::default().font_size).abs() > 0.01
            && (1.0..=1638.0).contains(&size)
        {
            props.half_points = Some((size * 2.0).round() as u32);
        }
        props
    }

    fn xml(&self) -> String {
        let mut out = String::new();
        if let Some(family) = &self.family {
            let family = xml::attr(family);
            out.push_str(&format!(
                "<w:rFonts w:ascii=\"{family}\" w:hAnsi=\"{family}\" w:cs=\"{family}\"/>"
            ));
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
        if let Some(size) = self.half_points {
            out.push_str(&format!(
                "<w:sz w:val=\"{size}\"/><w:szCs w:val=\"{size}\"/>"
            ));
        }
        if self.underline {
            out.push_str("<w:u w:val=\"single\"/>");
        }
        if let Some(vertical) = self.vertical {
            out.push_str(&format!("<w:vertAlign w:val=\"{vertical}\"/>"));
        }
        if out.is_empty() {
            out
        } else {
            format!("<w:rPr>{out}</w:rPr>")
        }
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
        push_run(&mut out, &RunProps::of(style, false), &text[at..next]);
    }
    out
}

/// A Markdown-source table as a real Word table: equal column widths across
/// the text area and a repeating, shaded, bold header row when the source
/// has a header separator. Short rows are padded to a rectangle.
fn table_xml(table: &WriterTable, text_width: i32) -> String {
    let columns = table.columns();
    let each = text_width / columns as i32;
    let widths: Vec<i32> = (0..columns)
        .map(|column| {
            if column + 1 == columns {
                text_width - each * (columns as i32 - 1)
            } else {
                each
            }
        })
        .collect();

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
    for (row_index, row) in table.rows.iter().enumerate() {
        let header = table.header_row && row_index == 0;
        out.push_str("<w:tr><w:trPr><w:cantSplit/>");
        if header {
            out.push_str("<w:tblHeader/>");
        }
        out.push_str("</w:trPr>");
        for (column, width) in widths.iter().enumerate() {
            let shading = if header {
                "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"F2F2F2\"/>"
            } else {
                ""
            };
            out.push_str(&format!(
                "<w:tc><w:tcPr><w:tcW w:w=\"{width}\" w:type=\"dxa\"/>{shading}</w:tcPr><w:p>"
            ));
            let cell = row.get(column).map_or("", String::as_str);
            let props = RunProps {
                bold: header,
                ..RunProps::default()
            };
            push_run(&mut out, &props, cell);
            out.push_str("</w:p></w:tc>");
        }
        out.push_str("</w:tr>");
    }
    out.push_str("</w:tbl>");
    out
}
