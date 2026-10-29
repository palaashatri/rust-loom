use loom_package::zip::PackageArchive;
use roxmltree::Document;

use super::super::tests::rich_document;
use super::super::{export_docx, import_docx};
use crate::{HeaderFooterConfig, PageNumberFormat};

fn parts(bytes: &[u8]) -> PackageArchive {
    PackageArchive::from_bytes(bytes).expect("open DOCX package")
}

fn part_text(bytes: &[u8], path: &str) -> String {
    std::str::from_utf8(parts(bytes).get(path).expect("part exists"))
        .expect("part UTF-8")
        .to_owned()
}

fn replace_part(bytes: &[u8], path: &str, value: &str) -> Vec<u8> {
    let original = parts(bytes);
    let mut rewritten = PackageArchive::new();
    for entry in original.paths() {
        let value = if entry == path {
            value.as_bytes().to_vec()
        } else {
            original.get(entry).expect("part exists").to_vec()
        };
        rewritten.add(entry, value).expect("copy DOCX part");
    }
    rewritten.to_bytes().expect("write DOCX package")
}

#[test]
fn exported_parts_and_fields_round_trip_canonical_settings() {
    let mut document = rich_document();
    document.header_footer = HeaderFooterConfig {
        header_text: "Quarterly report — {page} of {total}".into(),
        footer_text: "Internal · {page}".into(),
        alignment: "right".into(),
        page_number_format: PageNumberFormat::RomanLower,
        different_first_page: true,
    };

    let exported = export_docx(&document).expect("export header/footer");
    let package = parts(&exported.bytes);
    let content_types =
        std::str::from_utf8(package.get("[Content_Types].xml").expect("content types"))
            .expect("content types UTF-8");
    let relationships = std::str::from_utf8(
        package
            .get("word/_rels/document.xml.rels")
            .expect("document relationships"),
    )
    .expect("relationships UTF-8");
    let document_xml =
        std::str::from_utf8(package.get("word/document.xml").expect("document part"))
            .expect("document UTF-8");
    let header_xml = std::str::from_utf8(package.get("word/header1.xml").expect("header part"))
        .expect("header UTF-8");
    let footer_xml = std::str::from_utf8(package.get("word/footer1.xml").expect("footer part"))
        .expect("footer UTF-8");

    for (name, source) in [
        ("[Content_Types].xml", content_types),
        ("word/_rels/document.xml.rels", relationships),
        ("word/document.xml", document_xml),
        ("word/header1.xml", header_xml),
        ("word/footer1.xml", footer_xml),
    ] {
        Document::parse(source).unwrap_or_else(|error| panic!("{name}: {error}"));
    }
    assert!(content_types.contains("/word/header1.xml"));
    assert!(content_types.contains("/word/header2.xml"));
    assert!(content_types.contains("/word/footer1.xml"));
    assert!(content_types.contains("/word/footer2.xml"));
    assert!(relationships.contains("rIdHeaderDefault"));
    assert!(relationships.contains("rIdHeaderFirst"));
    assert!(relationships.contains("rIdFooterDefault"));
    assert!(relationships.contains("rIdFooterFirst"));
    assert!(document_xml.contains("w:type=\"default\" r:id=\"rIdHeaderDefault\""));
    assert!(document_xml.contains("w:type=\"first\" r:id=\"rIdHeaderFirst\""));
    assert!(document_xml.contains("w:fmt=\"lowerRoman\""));
    assert!(document_xml.contains("<w:titlePg/>"));
    assert!(
        document_xml.find("<w:headerReference").unwrap() < document_xml.find("<w:pgSz").unwrap()
    );
    assert!(document_xml.find("<w:pgMar").unwrap() < document_xml.find("<w:pgNumType").unwrap());
    assert!(header_xml.contains(" PAGE "));
    assert!(header_xml.contains(" NUMPAGES "));
    assert!(
        header_xml.contains("<w:instrText xml:space=\"preserve\"> PAGE \\* roman </w:instrText>")
    );
    assert!(
        footer_xml.contains("<w:instrText xml:space=\"preserve\"> PAGE \\* roman </w:instrText>")
    );
    assert!(header_xml.contains("<w:jc w:val=\"right\"/>"));
    assert!(package.get("word/header2.xml").is_some());
    assert!(package.get("word/footer2.xml").is_some());
    Document::parse(
        std::str::from_utf8(package.get("word/header2.xml").expect("first page header"))
            .expect("first header UTF-8"),
    )
    .expect("first page header XML");
    Document::parse(
        std::str::from_utf8(package.get("word/footer2.xml").expect("first page footer"))
            .expect("first footer UTF-8"),
    )
    .expect("first page footer XML");

    let imported = import_docx(&exported.bytes).expect("import exported headers/footers");
    assert_eq!(imported.header_footer, document.header_footer);
}

#[test]
fn absent_header_footer_parts_import_as_defaults() {
    let document = rich_document();
    let exported = export_docx(&document).expect("export default document");
    let imported = import_docx(&exported.bytes).expect("import default document");
    assert_eq!(imported.header_footer, HeaderFooterConfig::default());
}

#[test]
fn importing_header_footer_below_minimum_margin_is_rejected() {
    let mut document = rich_document();
    document.header_footer.header_text = "Page {page}".into();
    let exported = export_docx(&document).expect("export valid header");
    let package = parts(&exported.bytes);
    let xml = std::str::from_utf8(package.get("word/document.xml").expect("document part"))
        .expect("document XML");
    let low_margin = xml.replace("w:top=\"1440\"", "w:top=\"478\"");
    assert_ne!(
        low_margin, xml,
        "fixture must contain the default top margin"
    );
    let bytes = replace_part(&exported.bytes, "word/document.xml", &low_margin);

    let error = import_docx(&bytes).expect_err("reject header that overlaps the body");
    assert!(error.contains("top margin"), "{error}");
}

#[test]
fn page_and_total_page_fields_use_each_supported_docx_format() {
    let mut document = rich_document();
    document.header_footer.header_text = "Page {page} of {total}".into();
    for (format, instruction) in [
        (PageNumberFormat::Arabic, "ARABIC"),
        (PageNumberFormat::RomanUpper, "ROMAN"),
        (PageNumberFormat::RomanLower, "roman"),
        (PageNumberFormat::Alphabetical, "ALPHABETIC"),
    ] {
        document.header_footer.page_number_format = format;
        let exported = export_docx(&document).expect("export page-number format");
        let archive = parts(&exported.bytes);
        let header = std::str::from_utf8(archive.get("word/header1.xml").expect("header part"))
            .expect("header UTF-8");
        for field in ["PAGE", "NUMPAGES"] {
            assert!(
                header.contains(&format!(
                    "<w:instrText xml:space=\"preserve\"> {field} \\* {instruction} </w:instrText>"
                )),
                "{field} did not use {instruction}"
            );
        }
        let imported = import_docx(&exported.bytes).expect("import page-number fields");
        assert_eq!(imported.header_footer, document.header_footer);
    }
}

#[test]
fn unsupported_first_page_content_and_external_relationships_are_rejected() {
    let mut document = rich_document();
    document.header_footer = HeaderFooterConfig {
        header_text: "Header".into(),
        different_first_page: true,
        ..HeaderFooterConfig::default()
    };
    let exported = export_docx(&document).expect("export first-page header");
    let original = part_text(&exported.bytes, "word/header2.xml");
    let visible_first_page = original.replace("<w:p/>", "<w:p><w:r><w:t>Special</w:t></w:r></w:p>");
    let fixture = replace_part(&exported.bytes, "word/header2.xml", &visible_first_page);
    assert!(import_docx(&fixture)
        .unwrap_err()
        .contains("custom first-page header/footer content"));

    let rels = part_text(&exported.bytes, "word/_rels/document.xml.rels");
    let external = rels.replace(
        "Target=\"header1.xml\"",
        "Target=\"https://example.invalid/header.xml\" TargetMode=\"External\"",
    );
    let fixture = replace_part(&exported.bytes, "word/_rels/document.xml.rels", &external);
    assert!(import_docx(&fixture)
        .unwrap_err()
        .contains("external header/footer relationships"));
}

#[test]
fn malformed_or_oversized_header_footer_parts_are_refused() {
    let mut document = rich_document();
    document.header_footer.footer_text = "A footer".into();
    let exported = export_docx(&document).expect("export footer");

    let malformed = replace_part(
        &exported.bytes,
        "word/footer1.xml",
        "<w:ftr><w:p><w:r><w:t>unfinished",
    );
    let malformed_error = import_docx(&malformed).unwrap_err();
    assert!(
        malformed_error.contains("malformed header/footer XML")
            || malformed_error.contains("complete root element"),
        "unexpected malformed-part error: {malformed_error}"
    );

    let oversized = format!("<w:ftr>{}</w:ftr>", " ".repeat(32 * 1_024));
    let oversized = replace_part(&exported.bytes, "word/footer1.xml", &oversized);
    assert!(import_docx(&oversized)
        .unwrap_err()
        .contains("header/footer part exceeds the 32768-byte import limit"));
}

#[test]
fn false_on_off_properties_do_not_enable_first_or_even_pages() {
    let mut document = rich_document();
    document.header_footer.header_text = "Header".into();
    let exported = export_docx(&document).expect("export header");
    let document_xml = part_text(&exported.bytes, "word/document.xml");
    let rewritten = document_xml.replace(
        "<w:pgNumType w:fmt=\"arabic\"/>",
        "<w:pgNumType w:fmt=\"arabic\"/><w:titlePg w:val=\"0\"/>",
    );
    let fixture = replace_part(&exported.bytes, "word/document.xml", &rewritten);
    let settings = part_text(&fixture, "word/settings.xml");
    let settings = settings.replace(
        "<w:zoom w:percent=\"100\"/>",
        "<w:evenAndOddHeaders w:val=\"0\"/><w:zoom w:percent=\"100\"/>",
    );
    let fixture = replace_part(&fixture, "word/settings.xml", &settings);

    let imported = import_docx(&fixture).expect("false on/off properties are supported");
    assert!(!imported.header_footer.different_first_page);
}

#[test]
fn unsupported_header_footer_fields_are_refused_instead_of_flattened() {
    let mut document = rich_document();
    document.header_footer.header_text = "Header".into();
    let exported = export_docx(&document).expect("export header");
    let header = part_text(&exported.bytes, "word/header1.xml");
    let unsupported_field = header.replace(
        "<w:t xml:space=\"preserve\">Header</w:t>",
        "<w:fldSimple w:instr=\"DATE\"><w:r><w:t>2026</w:t></w:r></w:fldSimple>",
    );
    let fixture = replace_part(&exported.bytes, "word/header1.xml", &unsupported_field);
    assert!(import_docx(&fixture)
        .unwrap_err()
        .contains("unsupported header/footer field DATE"));
}

#[test]
fn wrong_root_and_package_escaping_relationships_are_refused() {
    let mut document = rich_document();
    document.header_footer.header_text = "Header".into();
    let exported = export_docx(&document).expect("export header");
    let wrong_root = replace_part(
        &exported.bytes,
        "word/header1.xml",
        "<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:p/></w:document>",
    );
    assert!(import_docx(&wrong_root)
        .unwrap_err()
        .contains("unexpected root element"));

    let rels = part_text(&exported.bytes, "word/_rels/document.xml.rels");
    let escaping = rels.replace("Target=\"header1.xml\"", "Target=\"../../outside.xml\"");
    let fixture = replace_part(&exported.bytes, "word/_rels/document.xml.rels", &escaping);
    assert!(import_docx(&fixture)
        .unwrap_err()
        .contains("escapes the package"));
}

#[test]
fn multiple_docx_sections_are_refused_when_the_model_has_one_header_footer_config() {
    let document = rich_document();
    let exported = export_docx(&document).expect("export document");
    let document_xml = part_text(&exported.bytes, "word/document.xml");
    let multiple_sections = document_xml.replacen(
        "<w:sectPr",
        "<w:p><w:pPr><w:sectPr/></w:pPr></w:p><w:sectPr",
        1,
    );
    assert_ne!(
        document_xml, multiple_sections,
        "fixture adds a section break"
    );
    let fixture = replace_part(&exported.bytes, "word/document.xml", &multiple_sections);

    assert!(import_docx(&fixture)
        .unwrap_err()
        .contains("multiple document sections are not supported"));
}

#[test]
fn imported_header_line_breaks_are_rejected_instead_of_becoming_multiline_text() {
    let mut document = rich_document();
    document.header_footer.header_text = "Header".into();
    let exported = export_docx(&document).expect("export single-line header");
    let header = part_text(&exported.bytes, "word/header1.xml");
    let multiline = header.replace(
        "<w:t xml:space=\"preserve\">Header</w:t>",
        "<w:t xml:space=\"preserve\">first\nsecond</w:t>",
    );
    assert_ne!(header, multiline, "fixture inserts a header line break");
    let fixture = replace_part(&exported.bytes, "word/header1.xml", &multiline);

    assert!(import_docx(&fixture).unwrap_err().contains("single line"));
}
