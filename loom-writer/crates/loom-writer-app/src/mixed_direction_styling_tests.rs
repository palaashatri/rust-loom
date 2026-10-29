use super::*;

const TEXT: &str = "Latin שלום مرحبا Z";

fn mixed_script_document() -> WriterDocument {
    let mut document = WriterDocument::new("mixed-script-styles", "Mixed script styles");
    let mut block = RichBlock::new(1, "paragraph", TEXT);
    block.style.direction = ParagraphDirection::RightToLeft;
    for (sample, style) in [
        (
            "Latin",
            CharacterStyle {
                weight: FontWeight::Bold,
                italic: true,
                ..CharacterStyle::default()
            },
        ),
        (
            "שלום",
            CharacterStyle {
                font_size: 17.0,
                explicit_size: true,
                color: Some("#145AC8FF".to_owned()),
                superscript: true,
                ..CharacterStyle::default()
            },
        ),
        (
            "مرحبا",
            CharacterStyle {
                font_size: 18.0,
                explicit_size: true,
                color: Some("#C33218FF".to_owned()),
                subscript: true,
                ..CharacterStyle::default()
            },
        ),
    ] {
        let start = TEXT
            .find(sample)
            .expect("the fixture contains each script run");
        block.runs.push(StyleRun {
            start,
            end: start + sample.len(),
            style,
        });
    }
    document.push(block);
    document
}

fn painted_pixels(row: &WriterRenderBlock) -> Vec<[u8; 4]> {
    row.glyph_tiles
        .iter()
        .filter_map(|tile| tile.source.to_rgba8())
        .flat_map(|image| {
            image
                .as_bytes()
                .chunks_exact(4)
                .filter(|pixel| pixel[3] != 0)
                .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
                .collect::<Vec<_>>()
        })
        .collect()
}

#[test]
fn mixed_arabic_hebrew_inline_styles_keep_their_paint_and_script_geometry() {
    let _serial = loom_writer_core::test_fonts::serial();
    let catalog = deterministic_script_catalog();
    loom_writer_core::fonts::install_font_catalog(catalog);
    let _reset = ResetScriptCatalog;
    let document = mixed_script_document();
    let style = document.page.page_style();
    let projection = projection::project_with_viewport_scale(
        &document,
        PageViewport::default(),
        1280.0,
        Some(800.0),
        1.5,
    );
    let row = projection
        .rows
        .iter()
        .find(|row| drawn_text(std::slice::from_ref(row)) == TEXT)
        .expect("the shaped row retains its logical source text");
    assert!(
        row.glyph_tiles.row_count() > 0,
        "the page paints shaped glyphs"
    );

    let flow = document
        .flow(
            &style,
            projection::layout_viewport(&style, PageViewport::default()),
        )
        .expect("the styled paragraph has page geometry");
    let shaped = flow.lines()[0]
        .shaped_line
        .as_ref()
        .expect("the page uses shared bidi shaped geometry");
    let hebrew = styled_run(shaped, "שלום");
    let arabic = styled_run(shaped, "مرحبا");
    assert_eq!(hebrew.rgba, [20, 90, 200, 255]);
    assert_eq!(arabic.rgba, [195, 50, 24, 255]);
    assert!((hebrew.size_pt - 17.0 * 0.7).abs() < 0.01);
    assert!((arabic.size_pt - 18.0 * 0.7).abs() < 0.01);
    assert!((hebrew.baseline_shift_pt - 17.0 * 0.35).abs() < 0.01);
    assert!((arabic.baseline_shift_pt + 18.0 * 0.35).abs() < 0.01);

    let pixels = painted_pixels(row);
    assert!(
        pixels
            .iter()
            .any(|pixel| pixel[2] > pixel[0].saturating_add(40)),
        "painted Hebrew retains its blue foreground"
    );
    assert!(
        pixels
            .iter()
            .any(|pixel| pixel[0] > pixel[2].saturating_add(40)),
        "painted Arabic retains its red foreground"
    );
}

fn styled_run<'a>(
    shaped: &'a loom_writer_core::WriterShapedVisualLine,
    sample: &str,
) -> &'a loom_writer_core::WriterPaintedRun {
    let start = TEXT
        .find(sample)
        .expect("the fixture contains the styled script");
    shaped
        .painted_runs()
        .iter()
        .find(|run| run.source_range.start == start)
        .expect("the script remains a separately styled shaped run")
}
