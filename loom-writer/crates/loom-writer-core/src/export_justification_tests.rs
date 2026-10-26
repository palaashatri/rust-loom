use crate::export::test_support::export_valid_pdf as export_pdf;
use crate::{RichBlock, WriterDocument};

#[test]
fn public_pdf_export_keeps_justified_words_on_their_logical_line() {
    let phrase = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu ";
    let text = phrase.repeat(4);
    let mut document = WriterDocument::new("pdf-justify-extract", "PDF justification");
    let mut paragraph = RichBlock::new(1, "paragraph", &text);
    paragraph.style.alignment = loom_text::Alignment::Justify;
    document.push(paragraph);

    let pdf = export_pdf(&document);
    let pages = loom_pdf::inspect::page_text(&pdf).expect("extract public PDF output");
    assert!(
        pages.iter().any(|page| page.contains("alpha beta")),
        "a justified visual line must extract with its word spacing intact: {pages:?}"
    );
}

#[test]
fn public_pdf_export_keeps_a_separator_between_list_marker_and_body() {
    use loom_text::{CharacterStyle, FontWeight, StyleRun};

    let mut document = WriterDocument::new("pdf-list-extract", "PDF list extraction");
    let mut item = RichBlock::new(1, "list-numbered", "Item bold");
    item.runs.push(StyleRun {
        start: 5,
        end: 9,
        style: CharacterStyle {
            weight: FontWeight::Bold,
            ..Default::default()
        },
    });
    document.push(item);

    let pdf = export_pdf(&document);
    let pages = loom_pdf::inspect::page_text(&pdf).expect("extract public PDF output");
    assert!(
        pages.iter().any(|page| page.contains("1. Item bold")),
        "the list marker and mixed-style body must remain ordered in extracted text: {pages:?}"
    );
}
