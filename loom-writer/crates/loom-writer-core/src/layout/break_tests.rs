//! Soft line breaks (vertical tab), page breaks (form feed), widow/orphan
//! control and keep-with-next, in layout, PDF and DOCX.

use crate::{export_pdf, import_docx, PageSetup, PageStyle, PageViewport, WriterDocument};

/// A page that holds exactly five 10-point lines and has no margins.
fn tiny_style() -> PageStyle {
    PageStyle {
        width_pt: 400.0,
        height_pt: 50.0,
        margin_top_pt: 0.0,
        margin_bottom_pt: 0.0,
        margin_left_pt: 0.0,
        margin_right_pt: 0.0,
        body_font_size_pt: 10.0,
        line_height: 1.0,
    }
}

fn document(blocks: &[(&str, &str)]) -> WriterDocument {
    let mut doc = WriterDocument::new("b", "B");
    for (kind, text) in blocks {
        doc.push(crate::tables::test_block(doc.next_id(), kind, text));
    }
    doc
}

/// The text of every line on every page, one `Vec` per page.
fn pages_of(doc: &WriterDocument, style: &PageStyle) -> Vec<Vec<String>> {
    doc.paginate(style)
        .expect("valid style")
        .into_iter()
        .map(|page| page.fragments.into_iter().map(|f| f.text).collect())
        .collect()
}

#[test]
fn a_vertical_tab_is_a_line_break_inside_one_paragraph() {
    let doc = document(&[("paragraph", "A\u{b}B")]);
    assert_eq!(doc.blocks.len(), 1);
    let pages = pages_of(&doc, &PageStyle::default());
    assert_eq!(pages, vec![vec!["A\u{b}".to_string(), "B".to_string()]]);
    let flow = doc
        .flow(&PageStyle::default(), PageViewport::default())
        .unwrap();
    assert_eq!(flow.lines().len(), 2, "two lines, one block");
}

#[test]
fn a_form_feed_starts_the_following_text_on_a_new_page() {
    let in_one = document(&[("paragraph", "A\u{c}B")]);
    assert_eq!(
        pages_of(&in_one, &tiny_style()),
        vec![vec!["A\u{c}".to_string()], vec!["B".to_string()]]
    );
    let across = document(&[("paragraph", "A\u{c}"), ("paragraph", "B")]);
    assert_eq!(
        pages_of(&across, &tiny_style()),
        vec![vec!["A\u{c}".to_string()], vec!["B".to_string()]]
    );
    // A break with nothing after it does not add a blank page.
    let last = document(&[("paragraph", "A\u{c}")]);
    assert_eq!(pages_of(&last, &tiny_style()).len(), 1);
    // Two breaks in a row leave one blank page between.
    let twice = document(&[("paragraph", "A\u{c}\u{c}B")]);
    assert_eq!(pages_of(&twice, &tiny_style()).len(), 3);
}

#[test]
fn a_heading_never_ends_a_page_without_its_text() {
    // Two lines and a gap use 30 of 50 points; the heading (13.6) fits, the
    // paragraph after it does not, so the heading moves with it.
    let doc = document(&[
        ("paragraph", "x\nx"),
        ("heading5", "Heading"),
        ("paragraph", "y"),
    ]);
    let pages = pages_of(&doc, &tiny_style());
    assert_eq!(pages.len(), 2, "{pages:?}");
    assert_eq!(pages[1], vec!["Heading".to_string(), "y".to_string()]);
}

#[test]
fn a_first_line_alone_at_the_bottom_of_a_page_moves_to_the_next() {
    // Three lines and a gap use 40 points: one line of a four-line paragraph
    // would fit, and must not be left alone.
    let doc = document(&[("paragraph", "x\nx\nx"), ("paragraph", "a\nb\nc\nd")]);
    let pages = pages_of(&doc, &tiny_style());
    assert_eq!(pages[0].len(), 3, "{pages:?}");
    assert_eq!(pages[1].len(), 4, "{pages:?}");
}

#[test]
fn a_last_line_alone_at_the_top_of_a_page_pulls_a_line_with_it() {
    // One line and a gap leave three lines for a four-line paragraph; the
    // fourth must not stand alone, so two go to each page.
    let doc = document(&[("paragraph", "x"), ("paragraph", "a\nb\nc\nd")]);
    let pages = pages_of(&doc, &tiny_style());
    assert_eq!(pages[0].len(), 3, "{pages:?}");
    assert_eq!(pages[1].len(), 2, "{pages:?}");
}

#[test]
fn short_paragraphs_still_split_freely() {
    // Two lines have no widow or orphan control: one may be left behind.
    let doc = document(&[("paragraph", "x\nx\nx"), ("paragraph", "a\nb")]);
    let pages = pages_of(&doc, &tiny_style());
    assert_eq!(pages[0].len(), 4, "{pages:?}");
    assert_eq!(pages[1], vec!["b".to_string()]);
}

fn page_texts(pdf: &[u8]) -> Vec<String> {
    loom_pdf::inspect::page_text(pdf).expect("readable pdf")
}

#[test]
fn pdf_honours_page_and_line_breaks() {
    let doc = document(&[("paragraph", "Before\u{c}"), ("paragraph", "After")]);
    let pages = page_texts(&export_pdf(&doc));
    assert_eq!(pages.len(), 2, "{pages:?}");
    assert!(pages[0].contains("Before") && !pages[0].contains("After"));
    assert!(pages[1].contains("After"));
    let soft = document(&[("paragraph", "One\u{b}Two")]);
    let pdf = export_pdf(&soft);
    let text = page_texts(&pdf).join("\n");
    assert!(text.contains("One") && text.contains("Two"), "{text}");
    assert!(!text.contains('\u{b}'));
    let content = loom_pdf::inspect::readable_content(&pdf).unwrap();
    assert!(!content.contains('\u{b}') && !content.contains('\u{c}'));
}

fn document_xml(doc: &WriterDocument) -> String {
    let bytes = crate::export_document_as_docx(doc).unwrap();
    let archive = loom_package::zip::PackageArchive::from_bytes(&bytes).unwrap();
    String::from_utf8(archive.get("word/document.xml").unwrap().to_vec()).unwrap()
}

#[test]
fn docx_writes_and_reads_back_soft_and_page_breaks() {
    let doc = document(&[("paragraph", "One\u{b}Two"), ("paragraph", "Three\u{c}")]);
    let xml = document_xml(&doc);
    assert!(xml.contains("<w:br/>"), "{xml}");
    assert!(xml.contains("<w:br w:type=\"page\"/>"), "{xml}");
    let back = import_docx(&crate::export_document_as_docx(&doc).unwrap()).unwrap();
    let texts: Vec<_> = back
        .blocks
        .iter()
        .map(|b| b.text.as_str().to_string())
        .collect();
    assert_eq!(texts, vec!["One\u{b}Two", "Three\u{c}"]);
    let _ = PageSetup::default();
}
