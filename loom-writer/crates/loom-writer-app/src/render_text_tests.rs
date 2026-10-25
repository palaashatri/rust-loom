//! The page draws each block from its typed runs: plain words plus the weight,
//! slant and rules of their character style. No markup language sits between
//! the document and the screen, so what the user types is drawn as typed.

use super::actions_tests::text_document;
use super::projection::render_text::{row_text, Align};
use super::*;
use i_slint_backend_testing::ElementHandle;

fn rows_of(document: &WriterDocument) -> Vec<WriterRenderBlock> {
    projection::project(document, PageViewport::default(), None).rows
}

fn texts(row: &WriterRenderBlock) -> Vec<String> {
    row.segments.iter().map(|s| s.text.to_string()).collect()
}

fn drawn_text(rows: &[WriterRenderBlock]) -> String {
    rows.iter().flat_map(texts).collect()
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
