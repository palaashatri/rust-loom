use crate::{HeaderFooterConfig, PageNumberFormat, RichBlock, WriterDocument};

#[test]
fn pdf_repeats_page_fields_and_suppresses_the_first_page() {
    let mut document = WriterDocument::new("headers", "Headers");
    document.push(RichBlock::new(
        document.next_id(),
        "paragraph",
        "First body page\u{000c}Second body page",
    ));
    document.header_footer = HeaderFooterConfig {
        header_text: "Section {page}".into(),
        footer_text: "Page {page} of {total}".into(),
        alignment: "center".into(),
        page_number_format: PageNumberFormat::RomanUpper,
        different_first_page: true,
    };

    let pdf = crate::export_pdf(&document).expect("PDF export with header and footer");
    let pages = loom_pdf::inspect::page_text(&pdf).expect("extract PDF pages");
    assert_eq!(pages.len(), 2, "explicit page break creates two pages");
    assert!(pages[0].contains("First body page"));
    assert!(!pages[0].contains("Section I"));
    assert!(!pages[0].contains("Page I of 2"));
    assert!(pages[1].contains("Second body page"));
    assert!(pages[1].contains("Section II"));
    assert!(pages[1].contains("Page II of 2"));

    let style = document.page.page_style();
    let header = document
        .header_footer
        .line_geometry("Section II", &style, crate::HeaderFooterLineKind::Header)
        .expect("header line geometry");
    let footer = document
        .header_footer
        .line_geometry("Page II of 2", &style, crate::HeaderFooterLineKind::Footer)
        .expect("footer line geometry");
    let readable = loom_pdf::inspect::readable_content(&pdf).expect("read PDF positions");
    for geometry in [header, footer] {
        let position = format!(
            "{:.2} {:.2} Td",
            geometry.text_x_pt, geometry.pdf_baseline_y_pt
        );
        assert!(readable.contains(&position), "PDF text uses {position}");
    }
}

#[test]
fn pdf_refuses_header_or_footer_text_that_cannot_fit_the_page() {
    let mut document = WriterDocument::new("long-header", "Long header");
    document.header_footer.footer_text = "x".repeat(2_048);
    let error = crate::export_pdf(&document).expect_err("oversized footer text must not clip");
    assert!(error
        .to_string()
        .contains("does not fit between the page margins"));
}
