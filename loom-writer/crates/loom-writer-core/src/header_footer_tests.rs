use super::{
    load_document, save_document, HeaderFooterConfig, HeaderFooterLineKind, PageNumberFormat,
    WriterDocument, HEADER_FOOTER_FONT_SIZE_PT, HEADER_FOOTER_LINE_HEIGHT_EM,
    WRITER_CONTENT_FORMAT,
};

fn current_format_content(settings: &str) -> String {
    format!(
        r#"{{"format":{WRITER_CONTENT_FORMAT},"id":"headers","title":"Headers","blocks":[],"header_footer":{settings},"footnotes":[],"tracked_changes":{{"recording":false,"author":"Anonymous","next_id":1,"revisions":[]}}}}"#
    )
}

#[test]
fn legacy_content_defaults_header_footer_settings() {
    let document =
        WriterDocument::from_content_json(r#"{"format":3,"id":"old","title":"Old","blocks":[]}"#)
            .expect("format 3 remains readable");

    assert_eq!(document.header_footer, HeaderFooterConfig::default());
}

#[test]
fn current_format_requires_header_footer_settings() {
    let result = WriterDocument::from_content_json(&format!(
        r#"{{"format":{WRITER_CONTENT_FORMAT},"id":"new","title":"New","blocks":[]}}"#
    ));

    assert!(
        result.is_err(),
        "current format must include its settings object"
    );
}

#[test]
fn header_footer_config_survives_package_save_and_reopen() {
    let mut document = WriterDocument::new("headers", "Headers");
    document.header_footer = HeaderFooterConfig {
        header_text: "Report — {page} of {total}".into(),
        footer_text: "Private · {page}".into(),
        alignment: "right".into(),
        page_number_format: PageNumberFormat::RomanLower,
        different_first_page: true,
    };

    let content = document.to_content_json();
    assert!(content.contains(&format!("\"format\":{WRITER_CONTENT_FORMAT}")));
    assert!(content.contains("\"header_footer\""));
    let reopened =
        load_document(&save_document(&document).expect("save settings")).expect("reopen settings");

    assert_eq!(reopened.header_footer, document.header_footer);
    assert!(document.same_content_as(&reopened));
    assert_eq!(
        reopened.header_footer.format_header(2, 8).as_deref(),
        Some("Report — ii of 8")
    );
    assert_eq!(
        reopened.header_footer.format_footer(2, 8).as_deref(),
        Some("Private · ii")
    );
    assert_eq!(reopened.header_footer.format_header(1, 8), None);
    assert_eq!(reopened.header_footer.effective_alignment(), "right");
}

#[test]
fn header_footer_changes_are_content_but_selection_is_view_state() {
    let original = WriterDocument::new("headers", "Headers");
    let mut moved_caret = original.clone();
    moved_caret.set_selection(super::TextSelection::caret(3));
    assert!(original.same_content_as(&moved_caret));

    let mut edited = original.clone();
    edited.header_footer.footer_text = "{page}".into();
    assert!(!original.same_content_as(&edited));
}

#[test]
fn page_number_formats_cover_later_pages() {
    assert_eq!(PageNumberFormat::Arabic.format(3), "3");
    assert_eq!(PageNumberFormat::RomanUpper.format(4), "IV");
    assert_eq!(PageNumberFormat::RomanUpper.format(27), "XXVII");
    assert_eq!(PageNumberFormat::RomanUpper.format(3_999), "MMMCMXCIX");
    assert_eq!(PageNumberFormat::RomanUpper.format(4_000), "4000");
    assert_eq!(PageNumberFormat::RomanLower.format(5), "v");
    assert_eq!(PageNumberFormat::Alphabetical.format(2), "B");
    assert_eq!(PageNumberFormat::Alphabetical.format(26), "Z");
    assert_eq!(PageNumberFormat::Alphabetical.format(27), "AA");
    assert_eq!(PageNumberFormat::Alphabetical.format(52), "AZ");
    assert_eq!(PageNumberFormat::Alphabetical.format(53), "BA");
    assert_eq!(PageNumberFormat::Alphabetical.format(0), "0");

    let config = HeaderFooterConfig::default();
    assert_eq!(config.effective_alignment(), "left");
}

#[test]
fn malformed_header_footer_types_names_and_duplicate_keys_are_rejected() {
    let malformed = [
        r#"{"header_text":12}"#,
        r#"{"page_number_format":"binary"}"#,
        r#"{"different_first_page":"true"}"#,
        r#"{"alignment":"justify"}"#,
        r#"{"header_text":"first","header_text":"last"}"#,
        r#"{"unknown_setting":true}"#,
    ];
    for settings in malformed {
        assert!(
            WriterDocument::from_content_json(&current_format_content(settings)).is_err(),
            "accepted malformed settings: {settings}"
        );
    }
}

#[test]
fn header_footer_text_and_config_limits_are_enforced() {
    let too_long = "x".repeat(super::header_footer::MAX_HEADER_FOOTER_TEXT_BYTES + 1);
    let content = format!(
        r#"{{"format":{WRITER_CONTENT_FORMAT},"id":"headers","title":"Headers","blocks":[],"header_footer":{{"header_text":"{too_long}"}}}}"#
    );
    assert!(WriterDocument::from_content_json(&content).is_err());

    let oversized_extension = "x".repeat(33 * 1_024);
    let settings = format!(r#"{{"extension":"{oversized_extension}"}}"#);
    assert!(WriterDocument::from_content_json(&current_format_content(&settings)).is_err());

    let mut invalid = WriterDocument::new("headers", "Headers");
    invalid.header_footer.header_text = too_long;
    assert!(save_document(&invalid).is_err());
}

#[test]
fn configured_headers_and_footers_require_nonoverlapping_page_margins() {
    let mut header = WriterDocument::new("headers", "Headers");
    header.header_footer.header_text = "Page {page}".into();
    header
        .page
        .set_custom_geometry_points(612.0, 792.0, 23.9, 72.0, 72.0, 72.0)
        .unwrap();

    assert!(save_document(&header).is_err());
    assert!(crate::export_pdf(&header).is_err());
    assert!(crate::export_docx(&header).is_err());

    header.header_footer.header_text.clear();
    header.header_footer.footer_text = "Page {page}".into();
    header
        .page
        .set_custom_geometry_points(612.0, 792.0, 72.0, 23.9, 72.0, 72.0)
        .unwrap();
    assert!(save_document(&header).is_err());
    assert!(crate::export_pdf(&header).is_err());
    assert!(crate::export_docx(&header).is_err());

    header.header_footer.footer_text.clear();
    assert!(save_document(&header).is_ok());
}

#[test]
fn header_footer_geometry_centers_the_text_line_and_bounds_mixed_script_width() {
    let mut document = WriterDocument::new("headers", "Headers");
    document.header_footer.alignment = "right".into();
    let style = document.page.page_style();
    let body_width = style.width_pt - style.margin_left_pt - style.margin_right_pt;
    let font = loom_pdf::TextStyle {
        size_pt: HEADER_FOOTER_FONT_SIZE_PT,
        ..Default::default()
    };
    let mut near_width = String::from("Loom שלום مرحبا ");
    while crate::text_metrics::pdf_text_width(&format!("{near_width}M"), &font) < body_width - 2.0 {
        near_width.push('M');
    }

    let header = document
        .header_footer
        .line_geometry(&near_width, &style, HeaderFooterLineKind::Header)
        .expect("near-width mixed-script header fits");
    let footer = document
        .header_footer
        .line_geometry(&near_width, &style, HeaderFooterLineKind::Footer)
        .expect("near-width mixed-script footer fits");
    assert!(header.text_width_pt <= header.width_pt);
    assert!(header.width_pt - header.text_width_pt < 12.0);
    assert!(
        (header.text_x_pt + header.text_width_pt - (style.width_pt - style.margin_right_pt)).abs()
            < 0.02
    );
    assert_eq!(header.font_size_pt, HEADER_FOOTER_FONT_SIZE_PT);
    assert!(header.height_pt >= HEADER_FOOTER_FONT_SIZE_PT * HEADER_FOOTER_LINE_HEIGHT_EM);
    assert!((header.top_pt + header.height_pt / 2.0 - style.margin_top_pt / 2.0).abs() < 0.02);
    assert!(
        (footer.top_pt + footer.height_pt / 2.0 - (style.height_pt - style.margin_bottom_pt / 2.0))
            .abs()
            < 0.02
    );
    let header_baseline_from_top = style.height_pt - header.pdf_baseline_y_pt;
    assert!(header_baseline_from_top > header.top_pt);
    assert!(header_baseline_from_top < header.top_pt + header.height_pt);

    let over_width = format!("{near_width}MMMM");
    assert!(document
        .header_footer
        .line_geometry(&over_width, &style, HeaderFooterLineKind::Header)
        .is_err());
}

#[test]
fn header_footer_line_breaks_are_rejected_by_persistence_and_exports() {
    for line_break in ["\n", "\r", "\r\n", "\u{0085}", "\u{2028}"] {
        let mut document = WriterDocument::new("headers", "Headers");
        document.header_footer.header_text = format!("first{line_break}second");

        assert!(document.header_footer.validate().is_err());
        assert!(save_document(&document).is_err());
        assert!(crate::export_pdf(&document).is_err());
        assert!(crate::export_docx(&document).is_err());
        assert!(WriterDocument::from_content_json(&document.to_content_json()).is_err());
    }
}
