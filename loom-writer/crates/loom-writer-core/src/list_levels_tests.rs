//! Nested lists through the document model and every format that carries
//! them: the layout the editor draws, the saved file, Word, Markdown and PDF.

use loom_package::zip::PackageArchive;

use crate::{
    export_docx, import_docx, load_document, save_document, PageStyle, PageViewport, RichBlock,
    WriterDocument,
};

fn block(id: u64, kind: &str, text: &str, level: u8) -> RichBlock {
    let mut block = RichBlock::new(id, kind, text);
    block.style.list_level = level;
    block
}

fn doc_of(blocks: Vec<RichBlock>) -> WriterDocument {
    let mut doc = WriterDocument::new("lists", "Lists");
    doc.blocks = blocks;
    doc
}

fn viewport() -> PageViewport {
    PageViewport {
        width: 900.0,
        height: 900.0,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    }
}

fn levels(doc: &WriterDocument) -> Vec<u8> {
    doc.blocks.iter().map(|b| b.style.list_level).collect()
}

fn offset_of(doc: &WriterDocument, index: usize) -> usize {
    doc.blocks[..index]
        .iter()
        .map(|b| b.text.len_bytes() + 1)
        .sum()
}

fn document_xml(doc: &WriterDocument) -> String {
    let export = export_docx(doc).expect("docx exports");
    let archive = PackageArchive::from_bytes(&export.bytes).expect("zip reads back");
    String::from_utf8(archive.get("word/document.xml").unwrap().to_vec()).unwrap()
}

/// The `w:numId` of the paragraph whose text is `text`.
fn num_id_of(xml: &str, text: &str) -> Option<String> {
    let at = xml.find(&format!(">{text}<"))?;
    let start = xml[..at].rfind("<w:p>")?;
    let paragraph = &xml[start..at];
    let pos = paragraph.find("<w:numId w:val=\"")? + "<w:numId w:val=\"".len();
    let rest = &paragraph[pos..];
    Some(rest[..rest.find('"')?].to_string())
}

#[test]
fn a_nested_level_is_indented_eighteen_points_per_level_in_the_layout() {
    let style = PageStyle::default();
    let doc = doc_of(vec![
        block(1, "list-bulleted", "top", 0),
        block(2, "list-bulleted", "nested", 1),
        block(3, "list-bulleted", "deeper", 2),
    ]);
    let flow = doc.flow(&style, viewport()).unwrap();
    let x: Vec<f32> = flow.lines().iter().map(|line| line.bounds.x).collect();
    assert_eq!(x.len(), 3);
    assert!((x[1] - x[0] - 18.0).abs() < 0.01, "level 1: {x:?}");
    assert!((x[2] - x[0] - 36.0).abs() < 0.01, "level 2: {x:?}");
}

#[test]
fn a_level_on_a_paragraph_that_is_not_a_list_item_is_not_drawn() {
    let style = PageStyle::default();
    let plain = doc_of(vec![block(1, "paragraph", "text", 3)]);
    assert_eq!(plain.blocks[0].effective_list_level(), 0);
    assert_eq!(plain.blocks[0].list_indent_pt(), 0.0);
    let base = doc_of(vec![block(1, "paragraph", "text", 0)]);
    let x = |doc: &WriterDocument| doc.flow(&style, viewport()).unwrap().lines()[0].bounds.x;
    assert_eq!(x(&plain), x(&base));
}

#[test]
fn shifting_changes_only_the_list_items_the_selection_touches() {
    let mut doc = doc_of(vec![
        block(1, "list-bulleted", "alpha", 0),
        block(2, "paragraph", "middle", 0),
        block(3, "list-numbered", "omega", 0),
    ]);
    let end = offset_of(&doc, 2) + doc.blocks[2].text.len_bytes();
    let shift = doc.shift_list_levels(0, end, true);
    assert_eq!((shift.items, shift.changed), (2, 2));
    assert_eq!(levels(&doc), [1, 0, 1]);
}

#[test]
fn a_caret_inside_an_item_changes_that_item_only() {
    let mut doc = doc_of(vec![
        block(1, "list-bulleted", "alpha", 0),
        block(2, "list-bulleted", "omega", 0),
    ]);
    let inside_omega = offset_of(&doc, 1) + 2;
    let shift = doc.shift_list_levels(inside_omega, inside_omega, true);
    assert_eq!((shift.items, shift.changed), (1, 1));
    assert_eq!(levels(&doc), [0, 1]);
}

#[test]
fn a_selection_without_list_items_is_not_a_list_change() {
    let mut doc = doc_of(vec![block(1, "paragraph", "plain", 0)]);
    let shift = doc.shift_list_levels(0, 5, true);
    assert_eq!(shift.items, 0, "nothing here can be indented as a list");
    assert_eq!(levels(&doc), [0]);
}

#[test]
fn levels_stop_at_the_first_and_the_deepest_level() {
    let mut doc = doc_of(vec![block(1, "list-bulleted", "item", 5)]);
    let deeper = doc.shift_list_levels(2, 2, true);
    assert_eq!((deeper.items, deeper.changed), (1, 0));
    let mut top = doc_of(vec![block(1, "list-bulleted", "item", 0)]);
    let shallower = top.shift_list_levels(2, 2, false);
    assert_eq!((shallower.items, shallower.changed), (1, 0));
    assert_eq!(levels(&top), [0]);
}

#[test]
fn numbers_restart_for_each_level_and_after_a_shallower_item() {
    let numbered = |text: &str, level| block(0, "list-numbered", text, level);
    let doc = doc_of(vec![
        numbered("a", 0),
        numbered("b", 1),
        numbered("c", 1),
        numbered("d", 0),
        numbered("e", 1),
    ]);
    assert_eq!(doc.list_item_numbers(), [1, 1, 2, 2, 1]);

    let with_bullet = doc_of(vec![
        numbered("one", 0),
        block(0, "list-bulleted", "x", 1),
        numbered("two", 0),
    ]);
    assert_eq!(with_bullet.list_item_numbers(), [1, 0, 2]);

    let interrupted = doc_of(vec![
        numbered("one", 0),
        block(0, "paragraph", "p", 0),
        numbered("two", 0),
    ]);
    assert_eq!(interrupted.list_item_numbers(), [1, 0, 1]);
}

#[test]
fn bullet_glyphs_and_numbers_vary_by_level() {
    let bullets: Vec<String> = (0..4)
        .map(|level| block(1, "list-bulleted", "x", level).list_marker(0))
        .collect();
    assert_eq!(bullets, ["\u{2022}", "\u{25e6}", "\u{25aa}", "\u{2022}"]);
    assert_eq!(block(1, "list-numbered", "x", 2).list_marker(3), "3.");
    assert_eq!(block(1, "paragraph", "x", 0).list_marker(3), "");
}

#[test]
fn backspace_on_a_nested_item_moves_it_up_before_removing_its_marker() {
    let mut doc = doc_of(vec![block(1, "list-bulleted", "item", 2)]);
    assert!(doc.unlist_block_at(0));
    assert_eq!(
        (doc.blocks[0].kind.as_str(), levels(&doc)),
        ("list-bulleted", vec![1])
    );
    assert!(doc.unlist_block_at(0));
    assert_eq!(levels(&doc), [0]);
    assert!(doc.unlist_block_at(0));
    assert_eq!(doc.blocks[0].kind, "paragraph");
    assert!(!doc.unlist_block_at(0), "a paragraph has no marker");
}

#[test]
fn enter_on_an_empty_nested_item_reduces_its_level_then_leaves_the_list() {
    let mut doc = doc_of(vec![block(1, "list-numbered", "", 1)]);
    assert!(doc.exit_empty_list_item_at(0));
    assert_eq!(
        (doc.blocks[0].kind.as_str(), levels(&doc)),
        ("list-numbered", vec![0])
    );
    assert!(doc.exit_empty_list_item_at(0));
    assert_eq!(doc.blocks[0].kind, "paragraph");
}

#[test]
fn splitting_a_nested_item_keeps_its_level_on_both_halves() {
    let mut doc = doc_of(vec![block(1, "list-bulleted", "abc", 2)]);
    let mut text = doc.editor_text();
    text.push('\n');
    doc.replace_editor_text_at(&text, Some(4)).unwrap();
    assert_eq!(doc.blocks.len(), 2);
    assert_eq!(levels(&doc), [2, 2]);
}

#[test]
fn a_file_saves_and_loads_each_level_and_omits_zero() {
    let doc = doc_of(vec![
        block(1, "list-bulleted", "top", 0),
        block(2, "list-bulleted", "deep", 5),
    ]);
    let json = doc.to_content_json();
    assert_eq!(
        json.matches("\"list_level\"").count(),
        1,
        "only the nested item: {json}"
    );
    assert!(json.contains("\"list_level\":5"));
    let back = WriterDocument::from_content_json(&json).unwrap();
    assert_eq!(levels(&back), [0, 5]);

    let bytes = save_document(&doc).unwrap();
    let reopened = load_document(&bytes).unwrap();
    assert_eq!(levels(&reopened), [0, 5]);
}

#[test]
fn a_document_without_levels_saves_without_the_key_and_loads_at_level_zero() {
    let doc = doc_of(vec![block(1, "list-numbered", "one", 0)]);
    let json = doc.to_content_json();
    assert!(!json.contains("list_level"), "unchanged file: {json}");
    let old_style_file = json.replace("\"list_level\":0,", "");
    let back = WriterDocument::from_content_json(&old_style_file).unwrap();
    assert_eq!(levels(&back), [0]);
}

#[test]
fn word_export_writes_the_level_and_import_reads_it_back() {
    let doc = doc_of(vec![
        block(1, "list-bulleted", "Top item", 0),
        block(2, "list-bulleted", "Nested item", 2),
    ]);
    let xml = document_xml(&doc);
    assert!(
        xml.contains("<w:ilvl w:val=\"2\"/>"),
        "level written: {xml}"
    );

    let bytes = export_docx(&doc).unwrap().bytes;
    let imported = import_docx(&bytes).unwrap();
    let nested = imported
        .blocks
        .iter()
        .find(|b| b.text.as_str() == "Nested item")
        .expect("nested item imported");
    assert_eq!(nested.kind, "list-bulleted");
    assert_eq!(nested.style.list_level, 2);
    let top = imported
        .blocks
        .iter()
        .find(|b| b.text.as_str() == "Top item")
        .unwrap();
    assert_eq!(top.style.list_level, 0);
}

#[test]
fn a_numbered_run_continues_across_a_deeper_bullet_in_word() {
    let doc = doc_of(vec![
        block(1, "list-numbered", "one", 0),
        block(2, "list-bulleted", "sub", 1),
        block(3, "list-numbered", "two", 0),
    ]);
    let xml = document_xml(&doc);
    assert_eq!(num_id_of(&xml, "one"), num_id_of(&xml, "two"));
}

#[test]
fn markdown_indents_each_nested_item_two_spaces_per_level() {
    let doc = doc_of(vec![
        block(1, "list-bulleted", "Top", 0),
        block(2, "list-bulleted", "Nested", 1),
        block(3, "list-numbered", "Numbered", 0),
        block(4, "list-numbered", "Sub", 1),
    ]);
    let md = doc.to_markdown();
    assert!(md.contains("- Top\n  - Nested\n"), "{md:?}");
    assert!(md.contains("1. Numbered\n  1. Sub\n"), "{md:?}");
}

/// The x position read from the PDF text run's text matrix.
fn run_x(content: &str, text: &str) -> f32 {
    drawn_runs(content)
        .into_iter()
        .find(|run| run.text == text)
        .unwrap_or_else(|| panic!("{text:?} is drawn in {content:?}"))
        .x
}

#[test]
fn pdf_draws_each_level_further_in_with_its_marker() {
    let style = PageStyle::default();
    let doc = doc_of(vec![
        block(1, "list-bulleted", "Top", 0),
        block(2, "list-bulleted", "Nested", 1),
        block(3, "list-bulleted", "Deeper", 2),
    ]);
    let pdf = crate::export_pdf(&doc);
    let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
    let margin = style.margin_left_pt;
    // Each item's text starts at the shared text start: its level's indent plus
    // the marker room, as the editor lays it out.
    let text_start = |index: usize| margin + doc.blocks[index].text_start_pt(&style);
    assert!(
        (run_x(&content, "Top") - text_start(0)).abs() < 0.01,
        "top text starts at {}",
        text_start(0)
    );
    assert!(
        (run_x(&content, "Nested") - text_start(1)).abs() < 0.01,
        "nested text starts at {}",
        text_start(1)
    );
    assert!(
        (run_x(&content, "Deeper") - text_start(2)).abs() < 0.01,
        "deeper text starts at {}",
        text_start(2)
    );
    // Each marker hangs in the room left of its text, its right edge 4 pt before it.
    let body = loom_pdf::TextStyle {
        size_pt: style.body_font_size_pt,
        ..Default::default()
    };
    for (index, glyph) in ["\u{2022}", "\u{25e6}", "\u{25aa}"].into_iter().enumerate() {
        let right = run_x(&content, glyph) + crate::text_metrics::pdf_text_width(glyph, &body);
        assert!(
            (right - (text_start(index) - 4.0)).abs() < 0.01,
            "marker {glyph:?} ends at {right}"
        );
    }
}

/// Every supported PDF text run in drawing order.
fn drawn_runs(content: &str) -> Vec<crate::export::test_support::DrawnTextRun> {
    crate::export::test_support::parse_drawn_text_runs(content).expect("semantic text runs")
}

#[test]
fn a_list_lines_text_starts_where_the_editor_starts_it() {
    let style = PageStyle::default();
    // Long enough to wrap onto several lines, so the continuation lines are checked too.
    let long = "wrapping ".repeat(60);
    let doc = doc_of(vec![
        block(1, "list-bulleted", &long, 0),
        block(2, "list-numbered", "Nested", 1),
    ]);
    let pdf = crate::export_pdf(&doc);
    let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
    let runs = drawn_runs(&content);
    let margin = style.margin_left_pt;
    let first = margin + doc.blocks[0].text_start_pt(&style);
    let nested = margin + doc.blocks[1].text_start_pt(&style);

    let lines: Vec<f32> = runs
        .iter()
        .filter(|run| run.text.starts_with("wrapping"))
        .map(|run| run.x)
        .collect();
    assert!(lines.len() > 1, "the item wraps: {runs:?}");
    for x in &lines {
        assert!(
            (x - first).abs() < 0.01,
            "a line starts at {x}, the editor's text start is {first}"
        );
    }
    let nested_x = runs
        .iter()
        .find(|run| run.text == "Nested")
        .map(|run| run.x)
        .expect("the nested item is drawn");
    assert!(
        (nested_x - nested).abs() < 0.01,
        "nested text at {nested_x}, the editor's text start is {nested}"
    );

    // The marker hangs in the room left of the text, its right edge 4 pt before it
    // (the editor's 4 px gap in writer_components.slint).
    let body = loom_pdf::TextStyle {
        size_pt: style.body_font_size_pt,
        ..Default::default()
    };
    for (glyph, start) in [("\u{2022}", first), ("1.", nested)] {
        let run = runs
            .iter()
            .find(|run| run.text == glyph)
            .unwrap_or_else(|| panic!("{glyph:?} is drawn in {runs:?}"));
        let right = run.x + crate::text_metrics::pdf_text_width(glyph, &body);
        assert!(
            (right - (start - 4.0)).abs() < 0.01,
            "{glyph:?} ends at {right}, 4 pt before {start}"
        );
    }
}
