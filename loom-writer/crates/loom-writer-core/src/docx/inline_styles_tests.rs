use loom_objects::chart::{Chart, ChartSeries, ChartType};
use loom_package::zip::{ArchiveError, PackageArchive};
use loom_text::{CharacterStyle, StyleRun};
use roxmltree::Document;

use crate::{docx::export_docx, import_docx, RichBlock, WriterDocument};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";

fn add_styled_block(
    doc: &mut WriterDocument,
    id: u64,
    kind: &str,
    text: &str,
    style: CharacterStyle,
) {
    let mut block = RichBlock::new(id, kind, text);
    block.runs.push(StyleRun {
        start: 0,
        end: text.len(),
        style,
    });
    doc.push(block);
}

fn run_style<'a>(doc: &'a WriterDocument, text: &str) -> Option<&'a CharacterStyle> {
    let block = doc
        .blocks
        .iter()
        .find(|block| block.text.as_str() == text)?;
    block.runs.iter().find_map(|run| {
        (block.text.as_str().get(run.start..run.end) == Some(text)).then_some(&run.style)
    })
}

fn exported_document_xml(doc: &WriterDocument) -> String {
    let export = export_docx(doc).expect("DOCX exports");
    let archive = PackageArchive::from_bytes(&export.bytes).expect("export ZIP reads");
    String::from_utf8(
        archive
            .get("word/document.xml")
            .expect("document part exists")
            .to_vec(),
    )
    .expect("document XML is UTF-8")
}

fn property_values(xml: &str, element: &str, attribute: &str) -> Vec<String> {
    let parsed = Document::parse(xml).expect("document XML parses");
    parsed
        .descendants()
        .filter(|node| {
            node.is_element()
                && node.tag_name().name() == element
                && node.tag_name().namespace() == Some(W)
        })
        .filter_map(|node| node.attribute((W, attribute)).map(str::to_owned))
        .collect()
}

fn import_body(body: &str) -> WriterDocument {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <w:document xmlns:w=\"{W}\"><w:body>{body}</w:body></w:document>"
    );
    let mut archive = PackageArchive::new();
    archive
        .add("word/document.xml", xml.into_bytes())
        .expect("add document part");
    import_docx(&archive.to_bytes().expect("package test document")).expect("DOCX imports")
}

fn plain_run_paragraph(text: &str, properties: &str) -> String {
    let run_properties = if properties.is_empty() {
        String::new()
    } else {
        format!("<w:rPr>{properties}</w:rPr>")
    };
    format!("<w:p><w:r>{run_properties}<w:t>{text}</w:t></w:r></w:p>")
}

#[test]
fn opaque_rgb_colors_survive_export_and_import() {
    let mut source = WriterDocument::new("colors", "Colors");
    for (id, text, color) in [(1, "short", "#336699"), (2, "alpha-ff", "#D4E5F6FF")] {
        let style = CharacterStyle {
            color: Some(color.to_string()),
            ..CharacterStyle::default()
        };
        add_styled_block(&mut source, id, "paragraph", text, style);
    }

    let xml = exported_document_xml(&source);
    assert_eq!(
        property_values(&xml, "color", "val"),
        vec!["336699".to_string(), "D4E5F6".to_string()],
        "Word color values omit the leading hash and opaque alpha byte"
    );

    let export = export_docx(&source).expect("DOCX exports");
    let imported = import_docx(&export.bytes).expect("DOCX imports");
    assert_eq!(
        run_style(&imported, "short").unwrap().color.as_deref(),
        Some("#336699FF")
    );
    assert_eq!(
        run_style(&imported, "alpha-ff").unwrap().color.as_deref(),
        Some("#D4E5F6FF")
    );
}

#[test]
fn automatic_color_round_trips_and_unstyled_runs_stay_unstyled() {
    let mut source = WriterDocument::new("automatic", "Automatic color");
    let automatic = CharacterStyle {
        color: None,
        ..CharacterStyle::default()
    };
    add_styled_block(&mut source, 1, "paragraph", "automatic", automatic);

    let export = export_docx(&source).expect("DOCX exports");
    let xml = exported_document_xml(&source);
    assert_eq!(
        property_values(&xml, "color", "val"),
        vec!["auto".to_string()]
    );
    let imported = import_docx(&export.bytes).expect("DOCX imports");
    assert_eq!(run_style(&imported, "automatic").unwrap().color, None);

    let plain = import_body(&plain_run_paragraph("plain", ""));
    assert!(
        plain.blocks[0].runs.is_empty(),
        "a run without w:rPr keeps the existing inherited/default behavior"
    );
}

#[test]
fn malformed_or_translucent_colors_fail_export_without_changing_the_source() {
    for color in ["red", "#12XZ56", "#3366997F"] {
        let mut source = WriterDocument::new("invalid-color", "Invalid color");
        let style = CharacterStyle {
            color: Some(color.to_string()),
            ..CharacterStyle::default()
        };
        add_styled_block(&mut source, 1, "paragraph", "text", style);
        let before = source.clone();

        let result = export_docx(&source);
        let message = match result {
            Err(ArchiveError::Corrupt(message)) => message,
            Err(error) => panic!(
                "unrepresentable color {color:?} must fail with a corrupt-data error: {error}"
            ),
            Ok(_) => panic!("export accepted unrepresentable color {color:?}"),
        };
        assert!(
            message.to_ascii_lowercase().contains("color"),
            "corrupt-data error must identify the color: {message}"
        );
        assert_eq!(source, before, "failed export leaves the source unchanged");
    }
}

#[test]
fn malformed_docx_color_is_reported_instead_of_dropped() {
    let body = plain_run_paragraph("styled", r#"<w:color w:val="not-a-color"/>"#);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n         <w:document xmlns:w=\"{W}\"><w:body>{body}</w:body></w:document>"
    );
    let mut archive = PackageArchive::new();
    archive
        .add("word/document.xml", xml.into_bytes())
        .expect("add document part");
    let bytes = archive.to_bytes().expect("package test document");

    let Err(error) = import_docx(&bytes) else {
        panic!("malformed Word color must not be silently dropped");
    };
    assert!(error.to_ascii_lowercase().contains("color"), "{error}");
}

#[test]
fn malformed_table_cell_color_is_reported_instead_of_dropped() {
    let paragraph = plain_run_paragraph("styled", r#"<w:color w:val="not-a-color"/>"#);
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n         <w:document xmlns:w=\"{W}\"><w:body><w:tbl><w:tr><w:tc>{paragraph}</w:tc></w:tr></w:tbl></w:body></w:document>"
    );
    let mut archive = PackageArchive::new();
    archive
        .add("word/document.xml", xml.into_bytes())
        .expect("add document part");
    let bytes = archive.to_bytes().expect("package test document");

    let Err(error) = import_docx(&bytes) else {
        panic!("malformed table-cell Word color must not be silently dropped");
    };
    assert!(error.to_ascii_lowercase().contains("color"), "{error}");
}

#[test]
fn invalid_color_diagnostics_stay_concise_for_oversized_metadata() {
    let long_color = format!("#{}", "g".repeat(16 * 1024));
    let mut messages = Vec::new();

    for color in ["red", long_color.as_str()] {
        let mut source = WriterDocument::new("invalid-color-diagnostic", "Invalid color");
        let style = CharacterStyle {
            color: Some(color.to_string()),
            ..CharacterStyle::default()
        };
        add_styled_block(&mut source, 7, "paragraph", "text", style);
        let before = source.clone();

        let message = match export_docx(&source) {
            Err(ArchiveError::Corrupt(message)) => message,
            Err(error) => panic!("invalid inline color returned the wrong error: {error}"),
            Ok(_) => panic!("invalid inline color was accepted"),
        };
        let normalized = message.to_ascii_lowercase();
        assert!(normalized.contains("color"), "diagnostic must name color");
        assert!(
            normalized.contains("opaque"),
            "diagnostic must explain opacity"
        );
        assert!(
            message.len() <= 160,
            "diagnostic must stay concise, got {} bytes",
            message.len()
        );
        assert_eq!(source, before, "failed export leaves the source unchanged");
        messages.push(message);
    }

    assert!(
        messages[1].len() <= messages[0].len() + 32,
        "long invalid metadata expanded the diagnostic: short={} bytes, long={} bytes",
        messages[0].len(),
        messages[1].len()
    );
}

#[test]
fn imported_superscript_and_subscript_vertical_alignment_are_preserved() {
    let body = format!(
        "{}{}",
        plain_run_paragraph("super", "<w:vertAlign w:val=\"superscript\"/>"),
        plain_run_paragraph("sub", "<w:vertAlign w:val=\"subscript\"/>")
    );
    let imported = import_body(&body);

    let superscript = run_style(&imported, "super").expect("superscript style run");
    assert!(superscript.superscript);
    assert!(!superscript.subscript);
    let subscript = run_style(&imported, "sub").expect("subscript style run");
    assert!(subscript.subscript);
    assert!(!subscript.superscript);
}

#[test]
fn literal_entity_looking_font_family_round_trips_without_double_unescape() {
    let family = "Literal &lt; face &amp; family";
    let mut source = WriterDocument::new("entity-font", "Entity font");
    let style = CharacterStyle {
        font_family: family.to_string(),
        ..CharacterStyle::default()
    };
    add_styled_block(&mut source, 1, "paragraph", "Typeface", style);

    let exported = export_docx(&source).expect("DOCX exports");
    let imported = import_docx(&exported.bytes).expect("DOCX imports");
    let restored = run_style(&imported, "Typeface").expect("font family run");
    assert_eq!(restored.font_family, family);
}

#[test]
fn explicit_default_size_overrides_heading_size_in_export() {
    let mut source = WriterDocument::new("heading-size", "Heading size");
    let style = CharacterStyle {
        explicit_size: true,
        ..CharacterStyle::default()
    };
    add_styled_block(&mut source, 1, "heading1", "Exact", style);

    let export = export_docx(&source).expect("DOCX exports");
    let xml = exported_document_xml(&source);
    assert_eq!(
        property_values(&xml, "sz", "val"),
        vec!["24".to_string()],
        "an explicitly chosen 12 pt run must override Heading 1's larger size"
    );
    let imported = import_docx(&export.bytes).expect("DOCX imports");
    let restored = run_style(&imported, "Exact").expect("explicit-size run");
    assert_eq!(restored.font_size, 12.0);
    assert!(restored.explicit_size);
}

#[test]
fn inline_property_tags_are_exact_and_attributes_do_not_leak_from_siblings() {
    let cases = [
        ("size-suffix", "<w:szCs w:val=\"40\"/>"),
        ("size-missing-value", "<w:sz/><w:szCs w:val=\"40\"/>"),
        ("color-suffix", "<w:colorChange w:val=\"AABBCC\"/>"),
        (
            "foreign-value-prefix",
            "<w:color xmlns:xw=\"urn:decoy\" xw:val=\"AABBCC\"/>",
        ),
        (
            "color-missing-value",
            "<w:color/><w:colorChange w:val=\"AABBCC\"/>",
        ),
        ("vertical-suffix", "<w:vertAlignCs w:val=\"superscript\"/>"),
        (
            "vertical-missing-value",
            "<w:vertAlign/><w:vertAlignCs w:val=\"superscript\"/>",
        ),
        ("font-suffix", "<w:rFontsCs w:ascii=\"Leak\"/>"),
        (
            "font-missing-ascii",
            "<w:rFonts/><w:rFontsCs w:ascii=\"Leak\"/>",
        ),
    ];
    let body = cases
        .iter()
        .map(|(text, properties)| plain_run_paragraph(text, properties))
        .collect::<String>();
    let imported = import_body(&body);

    assert_eq!(imported.blocks.len(), cases.len());
    for (block, (text, _)) in imported.blocks.iter().zip(cases) {
        assert_eq!(block.text.as_str(), text);
        assert!(
            block.runs.is_empty(),
            "decoy or missing attribute changed the default style for {text:?}: {:?}",
            block.runs
        );
    }
}

#[test]
fn commented_inline_property_does_not_create_a_style_run() {
    let imported = import_body(&plain_run_paragraph(
        "commented-color",
        "<!--<w:color w:val=\"AABBCC\"/>-->",
    ));

    assert!(
        imported.blocks[0].runs.is_empty(),
        "a property-looking XML comment must not set run formatting"
    );
}

#[test]
fn inline_attributes_accept_xml_whitespace_quotes_and_order() {
    let cases = [
        ("single-quoted", "<w:color w:val = '112233'/>", "#112233FF"),
        (
            "whitespace-around-equals",
            "<w:color\n\tw:val\t=\n '445566'/>",
            "#445566FF",
        ),
        (
            "exact-value-after-foreign-decoy",
            "<w:color xmlns:xw=\"urn:decoy\" xw:val=\"AABBCC\" w:val=\"778899\"/>",
            "#778899FF",
        ),
        (
            "quoted-greater-than-before-value",
            "<w:color xmlns:xw=\"urn:decoy\" xw:note=\">\" w:val=\"ABCDEF\"/>",
            "#ABCDEFFF",
        ),
    ];

    for (text, properties, expected) in cases {
        let imported = import_body(&plain_run_paragraph(text, properties));
        let style = run_style(&imported, text).expect("style run survives import");
        assert_eq!(style.color.as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn malformed_duplicate_attribute_is_ignored() {
    let imported = import_body(&plain_run_paragraph(
        "duplicate-value",
        "<w:color w:val=\"AABBCC\" w:val=\"112233\"/>",
    ));

    assert!(
        imported.blocks[0].runs.is_empty(),
        "a malformed duplicate attribute must not supply a color"
    );
}

#[test]
fn malformed_opening_attribute_fragment_fails_closed() {
    for xml in [
        "<w:color w:val=\"AABBCC\"",
        "<w:color w:val=\"AABBCC>",
        "<w:color w:val AABBCC/>",
    ] {
        assert_eq!(
            super::inline_style::attribute_value(xml, "w:val"),
            None,
            "malformed opening fragment must not provide an attribute: {xml:?}"
        );
    }
}

#[test]
fn relationship_tag_slice_respects_quoted_greater_than() {
    let xml = "<Relationships><Relationship Id='r1' Target='charts/a>b.xml'/></Relationships>";
    let (start, tag) = super::inline_style::property_tag_with_start(xml, "Relationship")
        .expect("relationship opening tag");

    assert!(tag.starts_with("<Relationship"));
    assert_eq!(
        super::inline_style::attribute_value(tag, "Target").as_deref(),
        Some("charts/a>b.xml")
    );
    assert_eq!(xml.get(start..start + tag.len()), Some(tag));
}

fn rewritten_document_part(bytes: &[u8], replacement: impl FnOnce(&str) -> String) -> Vec<u8> {
    let archive = PackageArchive::from_bytes(bytes).expect("export package reads");
    let document = String::from_utf8(
        archive
            .get("word/document.xml")
            .expect("document part exists")
            .to_vec(),
    )
    .expect("document XML is UTF-8");
    let rewritten = replacement(&document);
    assert_ne!(rewritten, document, "fixture mutation must match");
    let mut modified = PackageArchive::new();
    for path in archive.paths() {
        let data = if path == "word/document.xml" {
            rewritten.as_bytes().to_vec()
        } else {
            archive
                .get(path)
                .expect("original package part exists")
                .to_vec()
        };
        modified.add(path, data).expect("copy test package part");
    }
    modified.to_bytes().expect("modified package serializes")
}

#[test]
fn nested_picture_description_ignores_foreign_prefix_decoy() {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(1, 1, image::Rgba([200, 40, 80, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("encode tiny PNG");
    let image = crate::InlineImage::from_bytes("Original &lt; caption &amp; literal", png)
        .expect("valid PNG");
    let mut source = WriterDocument::new("nested-picture", "Nested picture");
    source
        .insert_image_block(0, image)
        .expect("insert picture block");
    let exported = export_docx(&source).expect("DOCX exports");
    let baseline = import_docx(&exported.bytes).expect("baseline DOCX imports");
    assert!(
        baseline.blocks.iter().any(|block| block.image.is_some()),
        "unmodified exported picture should import first: {:?}",
        baseline.blocks
    );
    let rewritten = rewritten_document_part(&exported.bytes, |xml| {
        xml.replacen(
            "<wp:docPr ",
            "<wp:docPr xmlns:xr=\"urn:decoy\" xr:descr=\"Decoy caption\" ",
            1,
        )
    });

    let imported = import_docx(&rewritten).expect("DOCX imports");
    let picture = imported
        .blocks
        .iter()
        .find_map(|block| block.image.as_ref())
        .expect("nested picture is imported");
    assert_eq!(picture.name, "Original &lt; caption &amp; literal");
}

#[test]
fn nested_chart_relation_ignores_foreign_prefix_decoy() {
    let chart = Chart::from_data(
        "nested-chart",
        Some("Chart".to_string()),
        ChartType::Column,
        vec!["Q1".to_string()],
        vec![ChartSeries {
            name: "Sales".to_string(),
            values: vec![3.0],
            color: None,
        }],
    );
    let mut source = WriterDocument::new("nested-chart", "Nested chart");
    source
        .insert_chart_block(0, chart)
        .expect("insert chart block");
    let exported = export_docx(&source).expect("DOCX exports");
    let baseline = import_docx(&exported.bytes).expect("baseline DOCX imports");
    assert!(
        baseline.blocks.iter().any(|block| block.kind == "chart"),
        "unmodified exported chart should import first: {:?}",
        baseline.blocks
    );
    let rewritten = rewritten_document_part(&exported.bytes, |xml| {
        xml.replacen(
            "<c:chart ",
            "<c:chart xmlns:xr=\"urn:decoy\" xr:id=\"rIdMissing\" ",
            1,
        )
    });

    let imported = import_docx(&rewritten).expect("DOCX imports");
    assert!(
        imported.blocks.iter().any(|block| block.kind == "chart"),
        "nested chart with its real r:id must still import"
    );
}
