//! The page draws each block from its typed runs: plain words plus the weight,
//! slant and rules of their character style. No markup language sits between
//! the document and the screen, so what the user types is drawn as typed.

use super::actions_tests::text_document;
use super::projection::render_text::{row_text, row_text_with_spaces, Align};
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_text::{CharacterStyle, FontWeight, ParagraphDirection, StyleRun};
use loom_writer_core::fonts::{
    FallbackPolicy, FontCatalog, Paragraph, ScanConfig, StyledSpan, TextDirection,
};
use std::sync::Arc;

fn rows_of(document: &WriterDocument) -> Vec<WriterRenderBlock> {
    projection::project(document, PageViewport::default(), None).rows
}

fn texts(row: &WriterRenderBlock) -> Vec<String> {
    row.segments.iter().map(|s| s.text.to_string()).collect()
}

fn drawn_text(rows: &[WriterRenderBlock]) -> String {
    rows.iter().flat_map(texts).collect()
}

struct ResetScriptCatalog;

impl Drop for ResetScriptCatalog {
    fn drop(&mut self) {
        loom_writer_core::fonts::reset_font_catalog();
    }
}

fn deterministic_script_catalog() -> Arc<FontCatalog> {
    let mut config = ScanConfig::with_dirs(Vec::new());
    config.policy = FallbackPolicy::new(["Noto Naskh Arabic", "Noto Sans Hebrew"]);
    Arc::new(FontCatalog::scan(&config))
}

#[test]
fn positioned_render_runs_follow_shaped_mixed_direction_geometry() {
    let _serial = loom_writer_core::test_fonts::serial();
    let catalog = deterministic_script_catalog();
    loom_writer_core::fonts::install_font_catalog(catalog.clone());
    let _reset = ResetScriptCatalog;

    let text = "مرحبا office שלום";
    let arabic_end = "مرحبا".len();
    let latin_start = arabic_end + 1;
    let latin_end = latin_start + "office".len();
    let latin_size = 19.0;
    let mut block = RichBlock::new(1, "paragraph", text);
    block.style.direction = ParagraphDirection::RightToLeft;
    block.style.alignment = loom_text::Alignment::Left;
    block.runs.push(StyleRun {
        start: latin_start,
        end: latin_end,
        style: CharacterStyle {
            font_size: latin_size,
            explicit_size: true,
            weight: FontWeight::Bold,
            color: Some("#CC3300FF".to_owned()),
            ..CharacterStyle::default()
        },
    });

    let regular = catalog.resolve("Inter", 400, false).expect("bundled Inter");
    let bold = catalog
        .resolve("Inter", 700, false)
        .expect("bundled Inter Bold");
    let base_size = PageStyle::default().body_font_size_pt;
    let spans = [
        StyledSpan::new(0..latin_start, &regular, base_size),
        StyledSpan::new(latin_start..latin_end, &bold, latin_size),
        StyledSpan::new(latin_end..text.len(), &regular, base_size),
    ];
    let paragraph = Paragraph::new(text, &spans, TextDirection::RightToLeft);
    let shaped = catalog.layout_paragraph_line(&paragraph, 0..text.len());

    let mut document = WriterDocument::new("mixed-render", "Mixed render");
    document.blocks.push(block);
    let page_style = document.page.page_style();
    let visible_width = 260.0;
    let projection = projection::project_with_viewport_scale(
        &document,
        PageViewport::default(),
        visible_width,
        Some(page_style.height_pt),
        1.5,
    );
    let row = projection
        .rows
        .iter()
        .find(|row| drawn_text(std::slice::from_ref(row)) == text)
        .expect("the projected row keeps the logical paragraph text");
    assert_eq!(
        drawn_text(std::slice::from_ref(row)),
        text,
        "the editor retains the source string for input and accessibility"
    );
    assert!(
        row.positioned,
        "mixed style runs stay positioned as one line"
    );
    assert!(
        row.glyph_tiles.row_count() > 0,
        "the page paints shaped glyph tiles"
    );
    let painted = row.glyph_tiles.iter().any(|tile| {
        tile.source
            .to_rgba8()
            .is_some_and(|pixels| pixels.as_bytes().chunks_exact(4).any(|pixel| pixel[3] != 0))
    });
    assert!(
        painted,
        "the Arabic, Latin and Hebrew glyphs have painted pixels"
    );
    let source_pixels: Vec<_> = row
        .glyph_tiles
        .iter()
        .filter_map(|tile| tile.source.to_rgba8())
        .flat_map(|pixels| {
            pixels
                .as_bytes()
                .chunks_exact(4)
                .filter(|pixel| pixel[3] != 0)
                .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
                .collect::<Vec<_>>()
        })
        .collect();
    assert!(
        source_pixels
            .iter()
            .any(|pixel| pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0),
        "default document glyphs are painted with stable black ink"
    );
    assert!(
        source_pixels
            .iter()
            .any(|pixel| pixel[0] > pixel[1] && pixel[3] != 0),
        "explicit foreground colors remain in the same color-bearing image"
    );
    let visible_content_right = visible_width - page_style.margin_left_pt;
    assert!(
        row.glyph_tiles
            .iter()
            .all(|tile| tile.x + tile.width <= visible_content_right + 1.0),
        "glyph images are clipped to the visible page canvas"
    );
    assert!(
        !row.unsupported,
        "the deterministic outline fonts rasterize"
    );
    let flowed = document
        .flow(
            &page_style,
            projection::layout_viewport(&page_style, PageViewport::default()),
        )
        .expect("the projected document has core flow geometry");
    let flow_line = flowed.lines().first().expect("one document line");
    let writer_shape = flow_line
        .shaped_line
        .as_ref()
        .expect("the app consumes the core shaped line");
    for stop in shaped.caret_positions() {
        assert!(
            (writer_shape.caret_x(stop.offset, stop.affinity) - stop.x).abs() < 0.05,
            "caret at source byte {} follows the independent paragraph shape",
            stop.offset
        );
    }
}

/// A document that exercises every look the page can draw: a centred heading
/// with one bold word, a paragraph with bold, italic, underline and
/// strikethrough, a right-aligned line with a bold word, and a soft break.
fn styled_document() -> WriterDocument {
    let mut document = WriterDocument::new("test", "Test");
    let mut heading = RichBlock::new(1, "heading1", "Quarterly Report");
    heading.style.alignment = loom_text::Alignment::Center;
    document.blocks = vec![
        heading,
        RichBlock::new(
            2,
            "paragraph",
            "Bold italic underline strike plain and * stars.",
        ),
        RichBlock::new(3, "paragraph", "Right aligned with a bold word"),
        RichBlock::new(4, "paragraph", "One\u{b}Two after a soft break"),
        RichBlock::new(5, "paragraph", "Last line"),
    ];
    document.blocks[2].style.alignment = loom_text::Alignment::Right;
    let start = |index: usize| -> usize {
        document.blocks[..index]
            .iter()
            .map(|block| block.text.as_str().len() + 1)
            .sum()
    };
    let (second, third, fourth) = (start(1), start(2), start(3));
    let ranges = [(0, 9), (second, second + 4), (third + 21, third + 25)];
    for (from, to) in ranges {
        set_selection_bold(&mut document, DocumentSelection::range(from, to), true);
    }
    set_selection_italic(
        &mut document,
        DocumentSelection::range(second + 5, second + 11),
        true,
    );
    set_selection_underline(
        &mut document,
        DocumentSelection::range(second + 12, second + 21),
        true,
    );
    set_selection_strikethrough(
        &mut document,
        DocumentSelection::range(second + 22, second + 28),
        true,
    );
    set_selection_italic(
        &mut document,
        DocumentSelection::range(fourth + 4, fourth + 7),
        true,
    );
    document
}

/// Set `LOOM_RENDER_TEXT_DUMP` to a directory to keep the render as a PNG.
#[test]
fn dump_the_styled_page_when_asked() {
    let Ok(dir) = std::env::var("LOOM_RENDER_TEXT_DUMP") else {
        return;
    };
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    apply_document(&app, &styled_document());
    let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    image
        .save(format!("{dir}/styled-page.png"))
        .expect("write png");
}

#[test]
fn bold_and_italic_runs_reach_the_page_without_markdown_syntax() {
    let mut document = text_document("Write bold and italic words");
    set_selection_bold(&mut document, DocumentSelection::range(6, 10), true);
    set_selection_italic(&mut document, DocumentSelection::range(15, 21), true);

    let rows = rows_of(&document);

    assert_eq!(rows.len(), 1);
    for row in &rows {
        for text in texts(row) {
            assert!(
                !text.contains('*') && !text.contains('_') && !text.contains('<'),
                "a segment carries markup syntax: {text:?}"
            );
        }
    }
    assert_eq!(drawn_text(&rows), "Write bold and italic words");
}

#[test]
fn a_bold_run_is_a_bold_segment_holding_the_plain_word() {
    let mut document = text_document("Write bold and italic words");
    set_selection_bold(&mut document, DocumentSelection::range(6, 10), true);
    set_selection_italic(&mut document, DocumentSelection::range(15, 21), true);

    let rows = rows_of(&document);
    let segments: Vec<_> = rows[0].segments.iter().collect();

    let bold = segments
        .iter()
        .find(|segment| segment.text.as_str() == "bold")
        .expect("the bold word is a segment of its own");
    assert!(bold.weight >= 700 && !bold.italic, "{bold:?}");
    let italic = segments
        .iter()
        .find(|segment| segment.text.as_str() == "italic")
        .expect("the italic word is a segment of its own");
    assert!(italic.weight < 700 && italic.italic, "{italic:?}");
    let plain: Vec<_> = segments
        .iter()
        .filter(|segment| segment.weight < 700 && !segment.italic)
        .map(|segment| segment.text.to_string())
        .collect();
    assert_eq!(plain, ["Write ", " and ", " words"]);
}

#[test]
fn what_the_user_types_is_drawn_as_typed_never_parsed() {
    // Every one of these means something to a Markdown parser. As plain
    // characters they must come out exactly as typed, in a single regular
    // segment that the text item aligns and wraps itself.
    let typed = "**not bold** _nor this_ # no heading 1. no list <u>no</u> [a](b) `c` & ~~d~~ \\";
    let rows = rows_of(&text_document(typed));

    assert_eq!(rows.len(), 1);
    let segments: Vec<_> = rows[0].segments.iter().collect();
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].text.as_str(), typed);
    assert!(segments[0].weight < 700 && !segments[0].italic);
    assert!(!rows[0].positioned);
}

#[test]
fn a_typed_asterisk_inside_a_bold_run_stays_one_asterisk() {
    let mut document = text_document("a *star* here");
    set_selection_bold(&mut document, DocumentSelection::range(2, 8), true);

    let rows = rows_of(&document);

    assert_eq!(drawn_text(&rows), "a *star* here");
    let bold: Vec<_> = rows[0]
        .segments
        .iter()
        .filter(|segment| segment.weight >= 700)
        .map(|segment| segment.text.to_string())
        .collect();
    assert_eq!(bold, ["*star*"]);
}

#[test]
fn a_soft_break_renders_as_a_line_break_and_is_not_drawn() {
    let mut document = WriterDocument::new("test", "Test");
    document.blocks = vec![RichBlock::new(1, "paragraph", "One\u{b}Two")];

    let rows = rows_of(&document);

    assert_eq!(rows.len(), 2, "the break starts a second row");
    assert_eq!(texts(&rows[0]), ["One"]);
    assert_eq!(texts(&rows[1]), ["Two"]);
    assert!(rows[1].y > rows[0].y, "the next line is lower");
    assert!(
        !drawn_text(&rows).contains(['\u{b}', '\u{c}', '\n']),
        "a break is not a character the page draws"
    );
}

#[test]
fn a_page_break_renders_as_a_new_page_and_is_not_drawn() {
    let mut document = WriterDocument::new("test", "Test");
    document.blocks = vec![
        RichBlock::new(1, "paragraph", "Before\u{c}"),
        RichBlock::new(2, "paragraph", "After"),
    ];

    let rows = rows_of(&document);

    assert_eq!(rows.len(), 2);
    assert_eq!(texts(&rows[0]), ["Before"]);
    assert_eq!(texts(&rows[1]), ["After"]);
    assert!(
        rows[1].y >= PageStyle::default().height_pt,
        "the text after a page break is on the next page: {}",
        rows[1].y
    );
    assert!(!drawn_text(&rows).contains(['\u{b}', '\u{c}', '\n']));
}

#[test]
fn a_break_inside_a_row_the_page_wraps_itself_starts_a_new_line() {
    // Table cells and the fallback rows are not broken by the page layout, so
    // a break in their text must still end the line.
    let mut block = RichBlock::new(1, "paragraph", "One\u{b}Two");
    block.style.alignment = loom_text::Alignment::Left;
    let row = row_text(&block, 12.0, 300.0, Align::Block);

    let lines: Vec<_> = row
        .segments
        .iter()
        .map(|segment| (segment.text.as_str(), segment.y))
        .collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], ("One", 0.0));
    assert_eq!(lines[1].0, "Two");
    assert!(lines[1].1 > 0.0, "the second line sits lower");
    assert!(row.positioned);
}

#[test]
fn segment_positions_are_the_ones_the_layout_highlights() {
    let document = styled_document();
    let style = document.page.page_style();
    let viewport = projection::layout_viewport(&style, PageViewport::default());
    let flow = document.flow(&style, viewport).expect("layout");
    let zoom = flow.zoom();
    let base = flow.page_bounds()[0];
    let highlight = |from: usize, to: usize| -> f32 {
        let found = flow.selection_rects(&document, &TextSelection::range(from, to), 0..1);
        (found[0].rect.x - base.x - style.margin_left_pt * zoom) / zoom
    };
    let rows = rows_of(&document);

    // Heading (centred): a heading is bold throughout, so its text is one look
    // and the text item centres it, rather than the row placing its segments.
    let heading = &rows[0];
    assert_eq!(texts(heading), ["Quarterly Report"]);
    assert!(
        !heading.positioned,
        "a single-look heading is aligned by its text item"
    );
    // Right-aligned line: "...with a " plain, "bold" bold, " word" plain.
    let right = rows
        .iter()
        .find(|row| texts(row).iter().any(|text| text == "bold"))
        .expect("the right-aligned row");
    let third = document.editor_text().find("Right aligned").expect("block");
    let bold = right
        .segments
        .iter()
        .find(|segment| segment.text.as_str() == "bold")
        .expect("bold segment");
    let expected = highlight(third + 21, third + 22);
    assert!(
        (right.x + bold.x - expected).abs() < 0.05,
        "bold starts at {} but the highlight starts at {expected}",
        right.x + bold.x
    );
}

#[test]
fn underline_and_strikethrough_are_rules_under_and_through_their_words() {
    let document = styled_document();
    let rows = rows_of(&document);
    let paragraph = &rows[1];
    let segments: Vec<_> = paragraph.segments.iter().collect();

    let underlined = segments
        .iter()
        .find(|segment| segment.underline)
        .expect("underlined segment");
    let struck = segments
        .iter()
        .find(|segment| segment.strikethrough)
        .expect("struck segment");
    assert_eq!(underlined.text.as_str(), "underline");
    assert_eq!(struck.text.as_str(), "strike");
    let rules: Vec<_> = paragraph.rules.iter().collect();
    assert_eq!(rules.len(), 2, "one rule per underlined or struck run");
    let (under, through) = (&rules[0], &rules[1]);
    assert!((under.x - underlined.x).abs() < 0.01);
    assert!((through.x - struck.x).abs() < 0.01);
    assert!(under.width > 20.0 && through.width > 10.0);
    // The underline hangs below the strikethrough, both inside the row.
    assert!(through.y < under.y);
    assert!(under.y + under.height <= paragraph.height);
    assert!(under.height >= 0.6);
}

#[test]
fn a_plain_row_is_one_segment_the_text_item_aligns_itself() {
    let mut block = RichBlock::new(1, "paragraph", "Plain words only");
    block.style.alignment = loom_text::Alignment::Center;
    let row = row_text(&block, 12.0, 400.0, Align::Line(80.0));

    assert!(!row.positioned);
    assert!(row.rules.is_empty());
    assert_eq!(row.segments.len(), 1);
    assert_eq!(row.segments[0].text.as_str(), "Plain words only");
    assert_eq!(row.segments[0].x, 0.0);
}

#[test]
fn text_of_one_look_with_a_trailing_break_is_still_one_segment() {
    let mut block = RichBlock::new(1, "paragraph", "Line end\u{b}");
    block.runs.push(loom_text::StyleRun {
        start: 0,
        end: 8,
        style: loom_text::CharacterStyle {
            weight: loom_text::FontWeight::Bold,
            ..Default::default()
        },
    });
    let row = row_text(&block, 12.0, 400.0, Align::Line(50.0));

    assert!(
        !row.positioned,
        "the break alone does not make a second look"
    );
    assert_eq!(row.segments.len(), 1);
    assert_eq!(row.segments[0].text.as_str(), "Line end");
    assert!(row.segments[0].weight >= 700);
}

#[test]
fn mixed_looks_wrap_to_the_row_width_in_text_the_page_wraps_itself() {
    let text = "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda";
    let mut block = RichBlock::new(1, "paragraph", text);
    block.runs.push(loom_text::StyleRun {
        start: 6,
        end: 10,
        style: loom_text::CharacterStyle {
            weight: loom_text::FontWeight::Bold,
            ..Default::default()
        },
    });
    let width = 120.0;
    let row = row_text(&block, 12.0, width, Align::Block);

    let mut lines: Vec<f32> = row.segments.iter().map(|segment| segment.y).collect();
    lines.dedup();
    assert!(
        lines.len() >= 3,
        "the text wraps over several lines: {lines:?}"
    );
    assert!(lines.windows(2).all(|pair| pair[1] > pair[0]));
    let mut again = String::new();
    for segment in &row.segments {
        again.push_str(segment.text.as_str());
    }
    // Wrapping only moves segments down: every character is drawn once, and
    // the spaces a line breaks at hang off its end.
    assert_eq!(again, text);
    for segment in &row.segments {
        let advance = loom_writer_core::text_advance(segment.text.trim_end(), 0, &[], 12.0);
        assert!(
            segment.x + advance <= width + 6.0,
            "{:?} runs past the row: {} + {advance}",
            segment.text,
            segment.x
        );
    }
}

#[test]
fn a_centred_or_right_aligned_row_is_shifted_by_its_own_width() {
    let mut block = RichBlock::new(1, "paragraph", "Bold and plain");
    block.runs.push(loom_text::StyleRun {
        start: 0,
        end: 4,
        style: loom_text::CharacterStyle {
            weight: loom_text::FontWeight::Bold,
            ..Default::default()
        },
    });
    let width = 300.0;
    let whole = loom_writer_core::text_advance("Bold and plain", 0, &block.runs, 12.0);

    block.style.alignment = loom_text::Alignment::Left;
    let left = row_text(&block, 12.0, width, Align::Block);
    block.style.alignment = loom_text::Alignment::Center;
    let centre = row_text(&block, 12.0, width, Align::Block);
    block.style.alignment = loom_text::Alignment::Right;
    let right = row_text(&block, 12.0, width, Align::Block);

    assert_eq!(left.segments[0].x, 0.0);
    assert!((centre.segments[0].x - (width - whole) / 2.0).abs() < 0.01);
    assert!((right.segments[0].x - (width - whole)).abs() < 0.01);
    assert!(left.segments[1].x > 0.0, "the plain part follows the bold");
}

#[test]
fn a_larger_run_is_a_segment_drawn_at_its_own_size() {
    let mut document = text_document("Write a large word here");
    set_selection_font_size(&mut document, DocumentSelection::range(8, 13), 24.0);

    let rows = rows_of(&document);
    let segments: Vec<_> = rows[0].segments.iter().collect();

    let large = segments
        .iter()
        .find(|segment| segment.text.as_str() == "large")
        .expect("the larger word is a segment of its own");
    assert_eq!(large.size, 24.0, "{large:?}");
    let plain = segments
        .iter()
        .find(|segment| segment.text.as_str() == "Write a ")
        .expect("the words before it are a segment of their own");
    assert_eq!(
        plain.size,
        loom_writer_core::PageStyle::default().font_size_for_kind("paragraph"),
        "text outside the chosen run keeps the block size: {plain:?}"
    );
}

#[test]
fn a_bold_word_draws_heavier_than_the_same_word_plain() {
    fn dark_pixels(document: &WriterDocument) -> usize {
        set_platform();
        let app = WriterApp::new().expect("create WriterApp");
        apply_document(&app, document);
        let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
        let (width, _) = image.dimensions();
        // The page column, below the toolbar and above the status bar.
        image
            .enumerate_pixels()
            .filter(|(x, y, pixel)| {
                (400..width.saturating_sub(400)).contains(x)
                    && (150..400).contains(y)
                    && pixel.0[0] < 100
            })
            .count()
    }
    let plain = text_document("Heavy heavy heavy heavy");
    let mut bold = plain.clone();
    set_selection_bold(&mut bold, DocumentSelection::range(0, 23), true);

    assert!(
        dark_pixels(&bold) > dark_pixels(&plain),
        "bold text must put more ink on the page than regular text"
    );
}

#[test]
fn the_page_exposes_each_drawn_segment_to_assistive_technology() {
    set_platform();
    let app = WriterApp::new().expect("create WriterApp");
    let mut document = text_document("Read bold words");
    set_selection_bold(&mut document, DocumentSelection::range(5, 9), true);
    apply_document(&app, &document);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");

    for label in ["Read ", "bold", " words"] {
        let found = ElementHandle::find_by_accessible_label(&app, label).count();
        assert!(found >= 1, "no text exposed for segment {label:?}");
    }
}

#[test]
fn enlarging_body_text_to_12_pt_reads_back_as_12_pt() {
    let mut document = text_document("Body text here");
    set_selection_font_size(&mut document, DocumentSelection::range(0, 4), 12.0);

    let state = formatting_state_for_selection(&document, DocumentSelection::range(0, 4));
    assert_eq!(
        state.font_size_pt, 12,
        "a chosen 12 pt inside 11 pt body text reads back as 12 pt: {state:?}"
    );
}

#[test]
fn dump_the_sized_page_when_asked() {
    let Ok(dir) = std::env::var("LOOM_RENDER_TEXT_DUMP") else {
        return;
    };
    set_platform();
    let mut document = text_document("A small word, a LARGE word, and a huge heading in the body.");
    set_selection_font_size(&mut document, DocumentSelection::range(16, 21), 24.0);
    set_selection_font_size(&mut document, DocumentSelection::range(34, 38), 36.0);
    let app = WriterApp::new().expect("create WriterApp");
    apply_document(&app, &document);
    let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    image
        .save(format!("{dir}/sized-page.png"))
        .expect("write png");
}

#[test]
fn dump_the_pdf_comparison_page_when_asked() {
    let Ok(dir) = std::env::var("LOOM_RENDER_TEXT_DUMP") else {
        return;
    };
    set_platform();
    let mut document = WriterDocument::new("dump", "Dump");
    let heading = RichBlock::new(document.next_id(), "heading1", "Quarterly report");
    document.push(heading);
    let mut body = RichBlock::new(
        document.next_id(),
        "paragraph",
        "A large word, a huge heading, and plain text.",
    );
    body.runs = vec![loom_text::StyleRun {
        start: 2,
        end: 7,
        style: loom_text::CharacterStyle {
            font_size: 24.0,
            explicit_size: true,
            ..Default::default()
        },
    }];
    document.push(body);
    let table_id = document.next_id();
    document.push(loom_writer_core::markdown_table_block(
        table_id,
        "| Region | Sessions | Change |\n| --- | --- | --- |\n| North | 1,204 | +8.1% |\n| South | 987 | +5.4% |",
    ));
    let app = WriterApp::new().expect("create WriterApp");
    apply_document(&app, &document);
    let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    image
        .save(format!("{dir}/comparison-editor.png"))
        .expect("write png");
}

#[test]
fn a_heading_row_is_bold_on_the_page_as_it_is_in_the_pdf() {
    let mut document = WriterDocument::new("heading", "Heading");
    document.push(RichBlock::new(
        document.next_id(),
        "heading1",
        "Quarterly report",
    ));
    let rows = rows_of(&document);
    let segment = rows[0]
        .segments
        .row_data(0)
        .expect("the heading draws a segment");
    assert!(
        segment.weight >= 700,
        "a heading is bold on the page, as the PDF draws it: {segment:?}"
    );
}

#[test]
fn the_style_border_reaches_the_table_cell_on_the_page() {
    // Medium 1, the default style, sets a 1.5 pt navy rule under the header row.
    let mut document = WriterDocument::new("style-border", "Style border");
    let id = document.next_id();
    document.push(loom_writer_core::markdown_table_block(
        id,
        "| A | B |\n| --- | --- |\n| C | D |",
    ));
    let projection = projection::project(&document, PageViewport::default(), None);
    let header = projection
        .table_cell_rects
        .first()
        .expect("a table cell rectangle");
    assert!(
        header.edge_bottom.visible,
        "the header cell draws its style's bottom border: {:?}",
        header.edge_bottom
    );
    assert_eq!(
        header.edge_bottom.color,
        slint::Color::from_rgb_u8(0x1B, 0x36, 0x5D),
        "the border is the style's navy"
    );
    assert!((header.edge_bottom.width - 1.5).abs() < 0.01);
}

#[test]
fn a_table_row_grows_for_a_cell_with_a_line_break() {
    let row_height = |markdown: &str| {
        let mut document = WriterDocument::new("row-height", "Row height");
        let id = document.next_id();
        document.push(loom_writer_core::markdown_table_block(id, markdown));
        let projection = projection::project(&document, PageViewport::default(), None);
        projection
            .table_cell_rects
            .first()
            .expect("a table cell rectangle")
            .height
    };
    let one_line = row_height("| ab | x |\n| --- | --- |\n| c | d |");
    let mut document = WriterDocument::new("row-height", "Row height");
    let id = document.next_id();
    let mut table = loom_objects::Table::new(1, 1);
    table.set_cell_text(0, 0, "ab\u{b}cd");
    document.push(RichBlock::table_block(id, table));
    let projection = projection::project(&document, PageViewport::default(), None);
    let two_lines = projection
        .table_cell_rects
        .first()
        .expect("a table cell rectangle")
        .height;
    assert!(
        two_lines > one_line + 10.0,
        "a cell with a line break makes its row taller: {one_line} and {two_lines}"
    );
}

#[test]
fn typed_table_rows_grow_for_wrapped_run_metrics_and_keep_asymmetric_heights() {
    let _serial = loom_writer_core::test_fonts::serial();
    let catalog = deterministic_script_catalog();
    loom_writer_core::fonts::install_font_catalog(catalog);
    let _reset = ResetScriptCatalog;

    let large_text = "word ".repeat(12);
    let mut table = loom_objects::Table::new(2, 1);
    table.header_row = false;
    table.set_cell_rich_text(
        0,
        0,
        vec![loom_objects::table::TextRun {
            text: large_text.clone(),
            font_size_pt: Some(24.0),
            ..loom_objects::table::TextRun::plain("")
        }],
    );
    table.set_cell_text(1, 0, "short");
    let mut document = WriterDocument::new("typed-row-height", "Typed row height");
    document
        .insert_table_object(0, &table)
        .expect("insert typed table");

    let projection = projection::project_with_viewport_scale(
        &document,
        PageViewport::default(),
        1024.0,
        Some(PageStyle::default().height_pt),
        1.0,
    );
    assert_eq!(projection.table_cell_rects.len(), 2);
    let large_height = projection.table_cell_rects[0].height;
    let small_height = projection.table_cell_rects[1].height;
    assert!(
        large_height > small_height + 20.0,
        "row height follows its large wrapped run while a short row stays compact: {large_height} vs {small_height}"
    );
    let large_row = projection
        .rows
        .iter()
        .find(|row| drawn_text(std::slice::from_ref(row)) == large_text)
        .expect("the large cell remains a logical text row");
    assert!(
        !large_row.unsupported,
        "offered large-font wrapping fits the automatically measured row"
    );
    assert!(
        large_row.glyph_tiles.row_count() > 0,
        "the wrapped cell paints shaped glyphs rather than the short-row fallback"
    );
}

#[test]
fn typed_table_geometry_cache_invalidates_on_run_style_only_changes() {
    let _serial = loom_writer_core::test_fonts::serial();
    let catalog = deterministic_script_catalog();
    loom_writer_core::fonts::install_font_catalog(catalog);
    let _reset = ResetScriptCatalog;

    let text = "word ".repeat(12);
    let mut table = loom_objects::Table::new(1, 1);
    table.header_row = false;
    table.set_cell_rich_text(
        0,
        0,
        vec![loom_objects::table::TextRun {
            text: text.clone(),
            font_size_pt: Some(24.0),
            ..loom_objects::table::TextRun::plain("")
        }],
    );
    let mut document = WriterDocument::new("typed-row-cache", "Typed row cache");
    document
        .insert_table_object(0, &table)
        .expect("insert typed table");
    let following_block_id = document.next_id();
    document.push(RichBlock::new(
        following_block_id,
        "paragraph",
        "after the table",
    ));
    let flat_text = document.blocks[0].text.as_str().to_owned();
    let row_height = |document: &WriterDocument| {
        projection::project(document, PageViewport::default(), None)
            .table_cell_rects
            .first()
            .expect("one cell rectangle")
            .height
    };
    let following_block_y = |document: &WriterDocument| {
        let style = document.page.page_style();
        document
            .flow(&style, PageViewport::default())
            .expect("the document flow is valid")
            .lines()
            .iter()
            .find(|line| line.block_index == 1)
            .expect("the paragraph after the table has a flow line")
            .bounds
            .y
    };

    let large_height = row_height(&document);
    let large_following_y = following_block_y(&document);
    let typed_table = document.blocks[0]
        .table
        .as_mut()
        .expect("table block retains its typed table");
    loom_objects::Table::set_cell_text(std::sync::Arc::make_mut(typed_table), 0, 0, text.clone());
    assert_eq!(
        document.blocks[0].text.as_str(),
        flat_text,
        "the style edit leaves the flat table text unchanged"
    );
    let default_height = row_height(&document);
    let default_following_y = following_block_y(&document);
    assert!(
        large_height > default_height + 20.0,
        "typed run size changes invalidate cached row geometry: {large_height} -> {default_height}"
    );
    assert!(
        large_following_y > default_following_y + 20.0,
        "DocumentFlow remeasures typed rows after a same-id, same-text run-style change: {large_following_y} -> {default_following_y}"
    );
}

#[test]
fn nonempty_table_shape_failure_is_warned_but_empty_softbreak_lines_are_valid() {
    let _serial = loom_writer_core::test_fonts::serial();
    let _reset = ResetScriptCatalog;
    let mut table = loom_objects::Table::new(1, 1);
    table.set_cell_text(0, 0, "word");
    let mut document = WriterDocument::new("table-shape-warning", "Table shape warning");
    document
        .insert_table_object(0, &table)
        .expect("insert typed table");

    loom_writer_core::fonts::install_font_catalog(Arc::new(FontCatalog::empty(
        FallbackPolicy::new(Vec::<String>::new()),
    )));
    let refused = projection::project_with_viewport_scale(
        &document,
        PageViewport::default(),
        1024.0,
        Some(PageStyle::default().height_pt),
        1.0,
    );
    let refused_row = refused
        .rows
        .iter()
        .find(|row| row.table_cell)
        .expect("the failed cell remains visible as logical text");
    assert!(refused_row.unsupported);
    assert!(refused_row
        .unsupported_label
        .as_str()
        .starts_with("Glyph rendering unavailable: could not shape nonempty table-cell text"));

    loom_writer_core::fonts::install_font_catalog(deterministic_script_catalog());
    let typed_table = document.blocks[0]
        .table
        .as_mut()
        .expect("table block retains its typed table");
    loom_objects::Table::set_cell_text(Arc::make_mut(typed_table), 0, 0, "word\u{b}");
    let empty_line = projection::project_with_viewport_scale(
        &document,
        PageViewport::default(),
        1024.0,
        Some(PageStyle::default().height_pt),
        1.0,
    );
    let empty_line_row = empty_line
        .rows
        .iter()
        .find(|row| row.table_cell)
        .expect("the cell with a trailing soft break is visible");
    assert!(
        !empty_line_row.unsupported,
        "a legitimate empty visual line is not treated as failed nonempty shaping"
    );
    assert!(empty_line_row.glyph_tiles.row_count() > 0);
}

#[test]
fn a_caret_after_a_cell_line_break_sits_on_the_second_line() {
    let caret_at = |offset: usize| {
        let mut document = WriterDocument::new("caret-line", "Caret line");
        let mut table = loom_objects::Table::new(1, 1);
        table.set_cell_text(0, 0, "ab\u{b}cd");
        document.push(RichBlock::table_block(document.next_id(), table));
        document.set_selection(loom_writer_core::TextSelection::caret(offset));
        let projection = projection::project(&document, PageViewport::default(), None);
        projection
            .selection_rects
            .iter()
            .find(|rect| rect.caret)
            .map(|rect| rect.y)
            .expect("the caret is drawn")
    };
    let first = caret_at(1);
    let second = caret_at(5);
    assert!(
        second > first + 10.0,
        "the caret after the break is on the second line: {first} and {second}"
    );
}

#[test]
fn a_press_on_the_second_line_of_a_cell_places_the_caret_on_that_line() {
    let mut document = WriterDocument::new("press-line", "Press line");
    let mut table = loom_objects::Table::new(1, 1);
    table.set_cell_text(0, 0, "ab\u{b}cd");
    document.push(RichBlock::table_block(document.next_id(), table));
    let viewport = PageViewport::default();
    let projection = projection::project(&document, viewport, None);
    let cell = projection
        .table_cell_rects
        .first()
        .expect("a table cell rectangle")
        .clone();
    // The second line's band starts one line pitch (11 pt * 1.25) below the first.
    let second_line = cell.y + 4.0 + 11.0 * 1.25 + 4.0;
    let offset = super::pointer_hit_test::writer_pointer_offset(
        &document,
        viewport,
        cell.x + 8.0,
        second_line,
    )
    .expect("a press inside the cell resolves to a document offset");
    assert_eq!(offset, 3, "the second line starts after the soft break");
}

#[test]
fn dump_the_two_line_cell_page_when_asked() {
    let Ok(dir) = std::env::var("LOOM_RENDER_TEXT_DUMP") else {
        return;
    };
    set_platform();
    let mut document = WriterDocument::new("two-line", "Two line");
    let table_id = document.next_id();
    let mut table = loom_objects::Table::new(2, 2);
    table.set_cell_text(0, 0, "Region");
    table.set_cell_text(0, 1, "Sessions");
    table.set_cell_text(1, 0, "North\u{b}east");
    table.set_cell_text(1, 1, "1,204");
    document.push(RichBlock::table_block(table_id, table));
    document.push(RichBlock::new(
        document.next_id(),
        "paragraph",
        "The paragraph after the table starts below its last row.",
    ));
    let app = WriterApp::new().expect("create WriterApp");
    apply_document(&app, &document);
    let image = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    image
        .save(format!("{dir}/two-line-cell.png"))
        .expect("write png");
}

#[test]
fn dump_the_two_line_loomdoc_when_asked() {
    let Ok(path) = std::env::var("LOOM_TWO_LINE_LOOMDOC") else {
        return;
    };
    let mut document = WriterDocument::new("two-line-live", "Two line live");
    let table_id = document.next_id();
    let mut table = loom_objects::Table::new(2, 2);
    table.set_cell_text(0, 0, "Region");
    table.set_cell_text(0, 1, "Sessions");
    table.set_cell_text(1, 0, "North\u{b}east");
    table.set_cell_text(1, 1, "1,204");
    document.push(RichBlock::table_block(table_id, table));
    let bytes = loom_writer_core::save_document(&document).expect("save");
    std::fs::write(path, bytes).expect("write loomdoc");
}

#[test]
fn mixed_style_row_places_the_next_word_after_a_justified_gap() {
    let text = format!("one two {}", "x".repeat(20));
    let mut block = RichBlock::new(1, "paragraph", &text);
    block.style.alignment = loom_text::Alignment::Justify;
    block.runs.push(loom_text::StyleRun {
        start: 4,
        end: 7,
        style: loom_text::CharacterStyle {
            weight: loom_text::FontWeight::Bold,
            ..Default::default()
        },
    });

    let first_two_words = loom_writer_core::text_advance("one two", 0, &block.runs, 12.0);
    let all_words = loom_writer_core::text_advance(text.as_str(), 0, &block.runs, 12.0);
    let width = first_two_words + (all_words - first_two_words) / 2.0;
    let natural_prefix = loom_writer_core::text_advance("one ", 0, &block.runs, 12.0);
    let expected_x = natural_prefix + (width - first_two_words);

    let row = row_text(&block, 12.0, width, Align::Block);
    let two = row
        .segments
        .iter()
        .find(|segment| segment.text.as_str() == "two")
        .expect("bold word on the first wrapped line");

    assert!(
        (two.x - expected_x).abs() < 0.1,
        "next word should follow the expanded space: got {}, expected {expected_x}",
        two.x
    );
}

#[test]
fn published_empty_geometry_keeps_nonjustified_rendering_unchanged() {
    let mut block = RichBlock::new(1, "paragraph", "Plain marked text stays aligned");
    block.runs.push(loom_text::StyleRun {
        start: 6,
        end: 12,
        style: loom_text::CharacterStyle {
            weight: loom_text::FontWeight::Bold,
            underline: true,
            ..Default::default()
        },
    });

    let output = |row: &super::projection::render_text::RowText| {
        (
            row.positioned,
            row.segments
                .iter()
                .map(|segment| {
                    (
                        segment.text.to_string(),
                        segment.x,
                        segment.y,
                        segment.size,
                        segment.weight,
                        segment.italic,
                        segment.underline,
                        segment.strikethrough,
                    )
                })
                .collect::<Vec<_>>(),
            row.rules
                .iter()
                .map(|rule| (rule.x, rule.y, rule.width, rule.height))
                .collect::<Vec<_>>(),
        )
    };

    for alignment in [
        loom_text::Alignment::Left,
        loom_text::Alignment::Center,
        loom_text::Alignment::Right,
    ] {
        block.style.alignment = alignment;
        let before = row_text(&block, 12.0, 240.0, Align::Block);
        let projected = row_text_with_spaces(&block, 12.0, 240.0, Align::Block, Some(&[]));
        assert_eq!(
            output(&before),
            output(&projected),
            "empty shared geometry leaves {alignment:?} drawing unchanged"
        );
    }
}
