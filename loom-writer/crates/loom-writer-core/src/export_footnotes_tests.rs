use crate::{FootnoteEntry, RichBlock, WriterDocument};

fn page_text(pdf: &[u8]) -> Vec<String> {
    loom_pdf::inspect::page_text(pdf).expect("readable PDF")
}

fn assert_black_page_fills(pdf: &[u8], page_count: usize) {
    let operators = loom_pdf::inspect::readable_content(pdf).expect("read PDF drawing operators");
    let fills = operators.matches("0.000 0.000 0.000 rg 0.00 0.00").count();
    assert_eq!(
        fills, page_count,
        "every PDF page gets the black fill: {operators}"
    );
}

#[test]
fn pdf_draws_inline_marker_and_note_body_from_shared_plan() {
    let mut doc = WriterDocument::new("pdf-footnote", "PDF footnote");
    doc.blocks
        .push(RichBlock::new(7, "paragraph", "A source sentence."));
    doc.footnotes.push(FootnoteEntry {
        id: 41,
        block_id: 7,
        offset: 2,
        body: "The PDF footnote body.".into(),
    });

    let pages = page_text(&crate::export_pdf(&doc).expect("export footnote PDF"));
    assert_eq!(pages.len(), 1);
    assert!(pages[0].contains("A source sentence."), "{pages:?}");
    assert!(
        pages[0].contains("1"),
        "the inline reference is drawn: {pages:?}"
    );
    assert!(pages[0].contains("The PDF footnote body."), "{pages:?}");
}

#[test]
fn pdf_draws_reference_inside_a_typed_table_cell() {
    let mut table = crate::loom_objects::table::Table::new(1, 1);
    table.set_cell_text(0, 0, "Cell text");
    let mut doc = WriterDocument::new("pdf-table-footnote", "PDF table footnote");
    doc.blocks.push(RichBlock::table_block(7, table));
    let cell = crate::TableTextMap::new(doc.blocks[0].typed_table().unwrap())
        .cell_range(0, 0)
        .unwrap();
    doc.footnotes.push(FootnoteEntry {
        id: 41,
        block_id: 7,
        offset: cell.start + "Cell".len(),
        body: "The table footnote body.".into(),
    });

    let pages = page_text(&crate::export_pdf(&doc).expect("export table footnote PDF"));
    assert_eq!(pages.len(), 1);
    assert!(pages[0].contains("Cell text"), "{pages:?}");
    assert!(
        pages[0].contains("1"),
        "the table reference is drawn: {pages:?}"
    );
    assert!(pages[0].contains("The table footnote body."), "{pages:?}");
}

#[test]
fn black_pdf_fills_blank_overflow_and_footnote_pages() {
    let mut blank = WriterDocument::new("black-blank-pdf", "Black blank page");
    blank.page.appearance = crate::PageAppearance::Black;
    let blank_pdf = crate::export_pdf(&blank).expect("export blank black PDF");
    assert_eq!(page_text(&blank_pdf).len(), 1);
    assert_black_page_fills(&blank_pdf, 1);

    let mut overflow = WriterDocument::new("black-overflow-pdf", "Black overflow");
    overflow.page.appearance = crate::PageAppearance::Black;
    overflow.blocks.push(RichBlock::new(
        7,
        "paragraph",
        &"Overflow content. ".repeat(1_500),
    ));
    let overflow_pdf = crate::export_pdf(&overflow).expect("export overflow black PDF");
    let overflow_pages = page_text(&overflow_pdf).len();
    assert!(overflow_pages > 1, "fixture reaches overflow pages");
    assert_black_page_fills(&overflow_pdf, overflow_pages);
    assert!(
        loom_pdf::inspect::readable_content(&overflow_pdf)
            .expect("read overflow ink")
            .contains("1.000 1.000 1.000 rg"),
        "automatic text ink is white on black pages"
    );

    let mut footnote = WriterDocument::new("black-footnote-pdf", "Black footnote");
    footnote.page.appearance = crate::PageAppearance::Black;
    footnote
        .blocks
        .push(RichBlock::new(7, "paragraph", "Footnote source."));
    footnote.footnotes.push(FootnoteEntry {
        id: 41,
        block_id: 7,
        offset: 0,
        body: "Footnote overflow. ".repeat(600),
    });
    let footnote_pdf = crate::export_pdf(&footnote).expect("export footnote black PDF");
    let footnote_pages = page_text(&footnote_pdf).len();
    assert!(
        footnote_pages > 1,
        "fixture reaches footnote overflow pages"
    );
    assert_black_page_fills(&footnote_pdf, footnote_pages);
}
