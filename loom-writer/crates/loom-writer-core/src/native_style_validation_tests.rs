use crate::{load_document, save_document, RichBlock, WriterDocument};
use loom_objects::table::{Table, TextRun};
use loom_text::{CharacterStyle, StyleRun};

fn document_with_colors(
    foreground: Option<&str>,
    background: Option<&str>,
    cell_foreground: Option<&str>,
) -> WriterDocument {
    let mut document = WriterDocument::new("native-colors", "Native colors");
    let mut block = RichBlock::new(1, "paragraph", "inline");
    block.runs.push(StyleRun {
        start: 0,
        end: "inline".len(),
        style: CharacterStyle {
            color: foreground.map(str::to_owned),
            background: background.map(str::to_owned),
            ..CharacterStyle::default()
        },
    });
    document.push(block);

    let mut table = Table::new(1, 1);
    let mut run = TextRun::plain("cell");
    run.color = cell_foreground.map(str::to_owned);
    table.set_cell_rich_text(0, 0, vec![run]);
    document.push(RichBlock::table_block(2, table));
    document
}

#[test]
fn valid_rgb_rgba_and_automatic_colors_round_trip_in_native_packages() {
    for (foreground, background, cell_foreground) in [
        (Some("#123456"), Some("#AABBCC80"), Some("#334455FF")),
        (Some("#A1B2C3FF"), Some("#ABCDEF"), Some("#102030")),
        (None, None, None),
    ] {
        let document = document_with_colors(foreground, background, cell_foreground);
        let package = save_document(&document).expect("save supported native colors");
        let loaded = load_document(&package).expect("load supported native colors");
        assert_eq!(loaded, document);
        assert_eq!(loaded.blocks[0].runs[0].style.color.as_deref(), foreground);
        assert_eq!(
            loaded.blocks[0].runs[0].style.background.as_deref(),
            background
        );
        assert_eq!(
            loaded.blocks[1]
                .typed_table()
                .unwrap()
                .cell(0, 0)
                .unwrap()
                .content
                .runs[0]
                .color
                .as_deref(),
            cell_foreground
        );
    }
}

#[test]
fn malformed_inline_and_typed_cell_colors_are_refused_before_save_or_load() {
    let invalid_values = [
        (Some("red"), None, None),
        (None, Some("#12XZ56"), None),
        (None, None, Some("#12345")),
    ];

    for (foreground, background, cell_foreground) in invalid_values {
        let document = document_with_colors(foreground, background, cell_foreground);
        let json = document.to_content_json();
        let load_result = WriterDocument::from_content_json(&json);
        assert!(
            load_result.is_err(),
            "malformed inline or typed-cell color must fail native parsing"
        );
        let save_result = save_document(&document);
        assert!(
            save_result.is_err(),
            "save must not emit a package that its own loader rejects"
        );
    }
}

#[test]
fn malformed_color_diagnostics_do_not_echo_untrusted_values() {
    let invalid = format!("#{}", "g".repeat(16 * 1024));
    let document = document_with_colors(Some(&invalid), None, None);

    let error = match WriterDocument::from_content_json(&document.to_content_json()) {
        Err(error) => error.to_string(),
        Ok(_) => panic!("native parser accepted malformed color metadata"),
    };

    assert!(
        error.len() < 160,
        "diagnostic must stay bounded: {} bytes",
        error.len()
    );
    assert!(
        !error.contains(&invalid),
        "diagnostic must not echo raw metadata"
    );
}

#[test]
fn wrong_json_types_for_inline_colors_are_rejected() {
    let document = document_with_colors(None, None, None);
    let content = document.to_content_json();
    for field in ["color", "background"] {
        let needle = format!("\"{field}\":null");
        for value in ["42", "false", "{}", "[]", "\"null\""] {
            let replacement = format!("\"{field}\":{value}");
            let malformed = content.replacen(&needle, &replacement, 1);
            assert_ne!(malformed, content, "the fixture changes the inline field");
            let error = WriterDocument::from_content_json(&malformed)
                .expect_err("invalid color types and strings must be rejected");
            assert!(error.to_string().to_ascii_lowercase().contains("color"));
        }
    }
}

#[test]
fn plus_signs_are_not_hexadecimal_color_digits() {
    for color in ["#+10000", "#00+100", "#0000+1", "#000000+1"] {
        let document = document_with_colors(Some(color), None, None);
        assert!(
            WriterDocument::from_content_json(&document.to_content_json()).is_err(),
            "{color} must not be accepted as hexadecimal"
        );
        assert!(save_document(&document).is_err());
    }
}

#[test]
fn legacy_content_versions_accept_supported_colors_and_missing_fields() {
    let old = r##"{"format":1,"id":"old","title":"Old","blocks":[{"id":1,"kind":"paragraph","text":"old"}]}"##;
    WriterDocument::from_content_json(old).expect("a legacy unstyled document loads");

    let document = document_with_colors(Some("#a1B2c3"), Some("#12345680"), Some("#AABBccFF"));
    let content = document
        .to_content_json()
        .replace("\"format\":3", "\"format\":2");
    let loaded = WriterDocument::from_content_json(&content).expect("format 2 colors load");
    assert_eq!(
        loaded.blocks[0].runs[0].style.color.as_deref(),
        Some("#a1B2c3")
    );
    assert_eq!(
        loaded.blocks[0].runs[0].style.background.as_deref(),
        Some("#12345680")
    );
    assert_eq!(
        loaded.blocks[1]
            .typed_table()
            .unwrap()
            .cell(0, 0)
            .unwrap()
            .content
            .runs[0]
            .color
            .as_deref(),
        Some("#AABBccFF")
    );
}

#[test]
fn malformed_colors_in_later_and_covered_table_cells_are_rejected() {
    let mut table = Table::new(2, 2);
    let mut run = TextRun::plain("bad");
    run.color = Some("#12ZZ56".into());
    table.set_cell_rich_text(1, 1, vec![run]);
    table.cell_mut(1, 1).unwrap().is_covered = true;
    let mut document = WriterDocument::new("later-cell", "Later cell");
    document.push(RichBlock::table_block(1, table));

    assert!(save_document(&document).is_err());
    assert!(WriterDocument::from_content_json(&document.to_content_json()).is_err());
}

#[test]
fn pdf_export_refuses_malformed_inline_and_table_colors() {
    for (foreground, background, cell_foreground) in [
        (Some("#12ZZ56"), None, None),
        (None, Some("#12ZZ56"), None),
        (None, None, Some("#12ZZ56")),
    ] {
        let document = document_with_colors(foreground, background, cell_foreground);
        let error = crate::export_pdf(&document).expect_err("malformed color must fail PDF export");
        assert!(error.to_string().contains("color"), "{error}");
    }
}
