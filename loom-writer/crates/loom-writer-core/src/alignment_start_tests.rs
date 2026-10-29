//! The `Start` alignment: a paragraph the user has not aligned draws at its
//! reading start. Left-to-right text starts on the left; right-to-left text
//! (Arabic, Hebrew, decided by its first strong character) starts on the right.
//! An explicit Left, Center, Right or Justify keeps its meaning.

use crate::{CaretAffinity, DocumentFlow, PageViewport, RichBlock, TextSelection, WriterDocument};
use loom_package::zip::PackageArchive;
use loom_text::{Alignment, ParagraphStyle};
use roxmltree::{Document, Node};

/// Shalom olam: right-to-left letters with one space, drawn as a single line.
const SHALOM_OLAM: &str = "\u{5E9}\u{5DC}\u{5D5}\u{5DD} \u{5E2}\u{5D5}\u{5DC}\u{5DD}";
/// Marhaba: Arabic letters, drawn as a single right-to-left line.
const MARHABA: &str = "\u{0645}\u{0631}\u{062D}\u{0628}\u{0627}";
const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn document(text: &str, alignment: Option<Alignment>) -> WriterDocument {
    let mut block = RichBlock::new(1, "paragraph", text);
    if let Some(alignment) = alignment {
        block.style.alignment = alignment;
    }
    let mut doc = WriterDocument::new("alignment-start", "Alignment start");
    doc.blocks = vec![block];
    doc
}

fn flow_of(doc: &WriterDocument) -> DocumentFlow {
    let style = doc.page.page_style();
    doc.flow(
        &style,
        PageViewport {
            width: style.width_pt,
            height: style.height_pt,
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        },
    )
    .expect("flow")
}

fn whole_text(doc: &WriterDocument) -> TextSelection {
    TextSelection {
        anchor: 0,
        focus: doc.blocks[0].text.as_str().len(),
        affinity: CaretAffinity::Downstream,
    }
}

/// The x of the right edge of the first line's text column, in viewport points.
fn column_right(doc: &WriterDocument, flow: &DocumentFlow) -> f32 {
    flow.lines()[0].bounds.x + doc.blocks[0].text_column_width(&doc.page.page_style())
}

/// The left and right edges of the drawn first line, from its highlight.
fn drawn_extent(doc: &WriterDocument, flow: &DocumentFlow) -> (f32, f32) {
    let rects = flow.selection_rects(doc, &whole_text(doc), 0..flow.page_count());
    assert!(!rects.is_empty(), "the whole line is highlighted");
    let left = rects
        .iter()
        .map(|rect| rect.rect.x)
        .fold(f32::INFINITY, f32::min);
    let right = rects
        .iter()
        .map(|rect| rect.rect.x + rect.rect.width)
        .fold(f32::NEG_INFINITY, f32::max);
    (left, right)
}

#[test]
fn a_new_paragraph_starts_with_the_start_alignment() {
    assert_eq!(ParagraphStyle::default().alignment, Alignment::Start);
    let block = RichBlock::new(1, "paragraph", "new");
    assert_eq!(block.style.alignment, Alignment::Start);
}

#[test]
fn an_arabic_paragraph_with_start_is_flush_right_to_the_column_edge() {
    let doc = document(MARHABA, Some(Alignment::Start));
    let flow = flow_of(&doc);
    let column_right = column_right(&doc, &flow);
    let (_, right) = drawn_extent(&doc, &flow);
    assert!(
        (right - column_right).abs() < 1.0,
        "the text should end at the column's right edge {column_right}, but ends at {right}"
    );
}

#[test]
fn an_english_paragraph_with_start_stays_flush_left() {
    let doc = document("Latin text", Some(Alignment::Start));
    let flow = flow_of(&doc);
    let line_x = flow.lines()[0].bounds.x;
    let (left, _) = drawn_extent(&doc, &flow);
    assert!((left - line_x).abs() < 0.01, "left edge {left} at {line_x}");
}

#[test]
fn an_arabic_paragraph_with_explicit_left_is_flush_left() {
    let doc = document(MARHABA, Some(Alignment::Left));
    let flow = flow_of(&doc);
    let line_x = flow.lines()[0].bounds.x;
    let (left, _) = drawn_extent(&doc, &flow);
    assert!((left - line_x).abs() < 0.01, "left edge {left} at {line_x}");
}

#[test]
fn start_is_saved_as_start_and_loads_back_as_start() {
    let doc = document(SHALOM_OLAM, Some(Alignment::Start));
    let json = doc.to_content_json();
    assert!(
        json.contains("\"alignment\":\"start\""),
        "the style should record start: {json}"
    );
    let back = WriterDocument::from_content_json(&json).expect("the saved document loads");
    assert_eq!(back.blocks[0].style.alignment, Alignment::Start);
}

#[test]
fn a_saved_left_loads_as_an_explicit_left_even_for_right_to_left_text() {
    let json = format!(
        r#"{{"id":"legacy","title":"Legacy","blocks":[{{"id":1,"kind":"paragraph","text":"{MARHABA}","align":"left","style":{{"alignment":"left"}}}}]}}"#
    );
    let doc = WriterDocument::from_content_json(&json).expect("the legacy file loads");
    assert_eq!(doc.blocks[0].style.alignment, Alignment::Left);
}

#[test]
fn a_block_saved_without_any_alignment_keeps_its_old_left_meaning() {
    // Files written before Start existed never recorded "start"; a block with no
    // alignment field must still read as Left rather than change its drawing.
    let json = format!(
        r#"{{"id":"old","title":"Old","blocks":[{{"id":1,"kind":"paragraph","text":"{MARHABA}"}}]}}"#
    );
    let doc = WriterDocument::from_content_json(&json).expect("the old file loads");
    assert_eq!(doc.blocks[0].style.alignment, Alignment::Left);
}

#[test]
fn an_arabic_start_paragraph_exports_as_right_justified_in_docx() {
    let doc = document(MARHABA, Some(Alignment::Start));
    let export = crate::docx::export_docx(&doc).expect("the document exports");
    let archive = PackageArchive::from_bytes(&export.bytes).expect("the package reads back");
    let xml = String::from_utf8(archive.get("word/document.xml").expect("body").to_vec())
        .expect("the body is UTF-8");
    let body = Document::parse(&xml).expect("the body is well-formed XML");
    let paragraph = body
        .descendants()
        .filter(|n| is_w(n, "p"))
        .find(|p| paragraph_text(*p) == MARHABA)
        .expect("the Arabic paragraph is exported");
    let jc = paragraph
        .descendants()
        .find(|n| is_w(n, "jc"))
        .expect("the paragraph writes a justification");
    assert_eq!(jc.attribute((W, "val")), Some("start"));
}

#[test]
fn a_paragraph_that_starts_with_a_space_takes_its_direction_from_its_first_letter() {
    let text = format!(" {MARHABA}");
    let doc = document(&text, Some(Alignment::Start));
    let flow = flow_of(&doc);
    let column_right = column_right(&doc, &flow);
    let (_, right) = drawn_extent(&doc, &flow);
    assert!(
        (right - column_right).abs() < 1.0,
        "the leading space must not decide the direction: ends at {right}, column {column_right}"
    );
}

fn is_w(node: &Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().name() == name && node.tag_name().namespace() == Some(W)
}

fn paragraph_text(p: Node<'_, '_>) -> String {
    p.descendants()
        .filter(|n| is_w(n, "t"))
        .filter_map(|n| n.text())
        .collect()
}
