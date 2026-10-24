//! Typed table blocks: storage format 2, migration of Markdown table text, the
//! editor's flat text and its input rules. These tests describe the contract;
//! the Markdown storage they replace is no longer read as a table.

use loom_objects::table::{CellStyleOverride, HorizontalAlign, Table, TextRun};

use super::{flat_table_text, TableTextMap, TABLE_CELL_SEPARATOR, TABLE_ROW_SEPARATOR};
use crate::{load_document, save_document, RichBlock, WriterDocument};

/// A table block as the previous Writer stored it: Markdown with emphasis,
/// an escaped star and pipe, a stray double star, and the metadata line.
const LEGACY: &str = "| **Item** | Qty | Note |\n| :--- | ---: | :---: |\n| *Apple* | 3 | a \\| b |\n| x\\*y | 2 ** | prefix |\n<!-- loom-table: style=table-style-light-1 banded=0 first_col=1 -->";

fn legacy_json(text: &str, block_id: u64) -> String {
    format!(
        "{{\"id\":\"doc\",\"title\":\"Doc\",\"blocks\":[{{\"id\":{block_id},\"kind\":\"table\",\"text\":{}}}]}}",
        loom_package::manifest::json::escape(text)
    )
}

fn two_by_two(id: &str) -> Table {
    let mut table = Table::new(2, 2);
    table.id = id.to_string();
    table.set_cell_text(0, 0, "A");
    table.set_cell_text(0, 1, "B");
    table.set_cell_text(1, 0, "C");
    table.set_cell_text(1, 1, "D");
    table
}

fn document_with(table: Table) -> WriterDocument {
    let mut document = WriterDocument::new("doc", "Doc");
    document.push(RichBlock::table_block(1, table));
    document
}

#[test]
fn table_block_saves_typed_table_and_no_markdown() {
    let mut table = two_by_two("t1");
    table.columns[0].width_pt = 120.0;
    table.set_cell_rich_text(0, 0, vec![TextRun::bold("A")]);
    table.cell_mut(1, 1).expect("cell").style_override = Some(CellStyleOverride {
        background_color: Some("#E8F0FE".to_string()),
        horizontal_align: Some(HorizontalAlign::Center),
        ..Default::default()
    });
    let document = document_with(table.clone());
    let json = document.to_content_json();
    assert!(
        json.contains("\"format\":2"),
        "content must declare format 2: {json}"
    );
    assert!(
        json.contains("\"table\":{"),
        "a table must be stored as an object under \"table\": {json}"
    );
    for markup in ["|", "---", "**", "loom-table"] {
        assert!(
            !json.contains(markup),
            "typed storage must not write {markup:?}: {json}"
        );
    }
    let bytes = save_document(&document).expect("save");
    let loaded = load_document(&bytes).expect("load");
    assert_eq!(loaded.blocks[0].typed_table(), Some(&table));
}

#[test]
fn legacy_markdown_table_migrates_losslessly() {
    let document = WriterDocument::from_content_json(&legacy_json(LEGACY, 7))
        .expect("a legacy document opens");
    let block = &document.blocks[0];
    let table = block
        .typed_table()
        .expect("a legacy Markdown table migrates to a typed table");
    let expected = [
        ["Item", "Qty", "Note"],
        ["Apple", "3", "a | b"],
        ["x*y", "2 **", "prefix"],
    ];
    for (row, cells) in expected.iter().enumerate() {
        for (column, text) in cells.iter().enumerate() {
            assert_eq!(
                table.cell_text(row, column).as_deref(),
                Some(*text),
                "cell ({row},{column})"
            );
        }
    }
    assert_eq!(
        table.cell(0, 0).expect("cell").content.runs,
        vec![TextRun::bold("Item")]
    );
    let italic_apple = TextRun {
        italic: true,
        ..TextRun::plain("Apple")
    };
    assert_eq!(
        table.cell(1, 0).expect("cell").content.runs,
        vec![italic_apple]
    );
    let align = |row, column| {
        table
            .cell(row, column)
            .and_then(|cell| cell.style_override.as_ref())
            .and_then(|style| style.horizontal_align)
    };
    assert_eq!(align(1, 0), Some(HorizontalAlign::Left));
    assert_eq!(align(1, 1), Some(HorizontalAlign::Right));
    assert_eq!(align(1, 2), Some(HorizontalAlign::Center));
    assert_eq!(table.style_id, "table-style-light-1");
    assert!(!table.banded_rows);
    assert!(table.first_column);
    assert!(table.header_row);
    let widths: Vec<f32> = table.columns.iter().map(|c| c.width_pt).collect();
    assert_eq!(widths, vec![80.0, 80.0, 80.0]);
    assert_eq!(block.text.as_str(), flat_table_text(table));
}

#[test]
fn legacy_migration_is_deterministic() {
    let first = WriterDocument::from_content_json(&legacy_json(LEGACY, 7)).expect("opens");
    let second = WriterDocument::from_content_json(&legacy_json(LEGACY, 7)).expect("opens");
    assert_eq!(
        first.blocks[0].typed_table(),
        second.blocks[0].typed_table()
    );
    assert_eq!(
        first.blocks[0].typed_table().expect("typed").id,
        "table-7",
        "the migrated id comes from the block, never from the clock"
    );
    let saved = save_document(&first).expect("save");
    let reloaded = load_document(&saved).expect("reload");
    assert_eq!(
        reloaded.blocks[0].typed_table(),
        first.blocks[0].typed_table()
    );
    assert_eq!(
        reloaded.blocks[0].text.as_str(),
        first.blocks[0].text.as_str()
    );
    assert!(
        reloaded.to_content_json().contains("\"format\":2"),
        "the next save writes format 2, so conversion happens once"
    );
}

#[test]
fn legacy_migration_remaps_comments_and_selection() {
    let apple = LEGACY.find("*Apple*").expect("fixture") + 1;
    let json = format!(
        "{{\"id\":\"doc\",\"title\":\"Doc\",\"blocks\":[{{\"id\":7,\"kind\":\"table\",\"text\":{}}}],\"comments\":[{{\"id\":\"c1\",\"author\":\"Ana\",\"block_id\":7,\"start\":{apple},\"end\":{},\"body\":\"check\",\"resolved\":false,\"orphaned\":false}}],\"selection\":{{\"anchor\":{apple},\"focus\":{apple},\"affinity\":\"downstream\"}}}}",
        loom_package::manifest::json::escape(LEGACY),
        apple + 5
    );
    let document = WriterDocument::from_content_json(&json).expect("opens");
    let flat = document.blocks[0].text.as_str().to_string();
    let apple_at = flat.find("Apple").expect("Apple is in the flat text");
    let thread = &document.comments[0];
    assert_eq!((thread.start, thread.end), (apple_at, apple_at + 5));
    assert_eq!(&flat[thread.start..thread.end], "Apple");
    assert_eq!(document.selection.anchor, apple_at);
}

#[test]
fn newer_format_and_untyped_flat_table_are_refused() {
    let flat = flat_table_text(&two_by_two("x"));
    let block = format!(
        "{{\"id\":7,\"kind\":\"table\",\"text\":{}}}",
        loom_package::manifest::json::escape(&flat)
    );
    let newer = format!(
        "{{\"format\":3,\"id\":\"doc\",\"title\":\"Doc\",\"blocks\":[{}]}}",
        legacy_block(LEGACY)
    );
    assert!(
        WriterDocument::from_content_json(&newer).is_err(),
        "a format above 2 must not open"
    );
    let untyped_v1 = format!("{{\"id\":\"doc\",\"title\":\"Doc\",\"blocks\":[{block}]}}");
    assert!(
        WriterDocument::from_content_json(&untyped_v1).is_err(),
        "flat text without a typed table must not open"
    );
    let untyped_v2 =
        format!("{{\"format\":2,\"id\":\"doc\",\"title\":\"Doc\",\"blocks\":[{block}]}}");
    assert!(
        WriterDocument::from_content_json(&untyped_v2).is_err(),
        "format 2 flat text without a \"table\" object must not open"
    );
}

fn legacy_block(text: &str) -> String {
    format!(
        "{{\"id\":7,\"kind\":\"table\",\"text\":{}}}",
        loom_package::manifest::json::escape(text)
    )
}

#[test]
fn editor_text_uses_unit_and_record_separators() {
    let document = document_with(two_by_two("t"));
    assert_eq!(TABLE_CELL_SEPARATOR, '\u{1f}');
    assert_eq!(TABLE_ROW_SEPARATOR, '\u{1e}');
    assert_eq!(document.editor_text(), "A\u{1f}B\u{1e}C\u{1f}D");
}

#[test]
fn every_table_offset_maps_to_exactly_one_cell() {
    let table = two_by_two("t");
    let flat = flat_table_text(&table);
    let map = TableTextMap::new(&table);
    assert_eq!(map.len(), flat.len());
    let cells: [(usize, usize, std::ops::Range<usize>); 4] =
        [(0, 0, 0..1), (0, 1, 2..3), (1, 0, 4..5), (1, 1, 6..7)];
    for offset in 0..=flat.len() {
        let hits: Vec<_> = cells
            .iter()
            .filter(|(_, _, range)| range.start <= offset && offset <= range.end)
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "offset {offset} must sit in exactly one cell"
        );
        assert_eq!(
            map.cell_at(offset),
            Some((hits[0].0, hits[0].1)),
            "offset {offset}"
        );
    }
    for (row, column, range) in cells {
        assert_eq!(map.cell_range(row, column), Some(range));
    }
}

#[test]
fn typing_in_a_cell_edits_only_its_runs() {
    let mut table = two_by_two("t");
    table.set_cell_rich_text(0, 0, vec![TextRun::bold("A")]);
    let mut document = document_with(table);
    let edited = document.editor_text().replacen("B", "xB", 1);
    document
        .replace_editor_text(&edited)
        .expect("typing in a cell is accepted");
    let table = document.blocks[0].typed_table().expect("still a table");
    assert_eq!(table.cell_text(0, 1).as_deref(), Some("xB"));
    assert_eq!(
        table.cell(0, 0).expect("cell").content.runs,
        vec![TextRun::bold("A")]
    );
    assert_eq!(table.cell_text(1, 1).as_deref(), Some("D"));
    assert_eq!(document.blocks[0].text.as_str(), flat_table_text(table));
}

#[test]
fn pasted_newlines_separators_and_pipes_stay_in_the_cell() {
    let mut document = document_with(two_by_two("t"));
    let edited = document.editor_text().replacen("A", "Aa\nb|c\u{1f}d", 1);
    document
        .replace_editor_text(&edited)
        .expect("a paste is accepted");
    let table = document.blocks[0].typed_table().expect("still a table");
    assert_eq!(table.cell_text(0, 0).as_deref(), Some("Aa b|cd"));
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.col_count(), 2);
    assert_eq!(table.cell_text(0, 1).as_deref(), Some("B"));
    assert_eq!(table.cell_text(1, 0).as_deref(), Some("C"));
}

#[test]
fn cell_edge_and_cross_cell_deletes_keep_the_grid() {
    // Backspace at the start of the second cell removes one separator: nothing changes.
    let mut document = document_with(two_by_two("t"));
    let edited = document.editor_text().replacen("\u{1f}B", "B", 1);
    let _ = document.replace_editor_text(&edited);
    let table = document.blocks[0].typed_table().expect("still a table");
    assert_eq!(table.cell_text(0, 0).as_deref(), Some("A"));
    assert_eq!(table.cell_text(0, 1).as_deref(), Some("B"));
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.col_count(), 2);

    // A selection across two cells clears each cell's part and keeps the separator.
    let mut table = two_by_two("t");
    table.set_cell_text(0, 0, "Hello");
    table.set_cell_text(0, 1, "World");
    let mut document = document_with(table);
    let edited = document.editor_text().replacen("lo\u{1f}Wo", "", 1);
    document
        .replace_editor_text(&edited)
        .expect("the cross-cell delete is accepted");
    let table = document.blocks[0].typed_table().expect("still a table");
    assert_eq!(table.cell_text(0, 0).as_deref(), Some("Hel"));
    assert_eq!(table.cell_text(0, 1).as_deref(), Some("rld"));
    assert_eq!(table.row_count(), 2);
    assert_eq!(table.col_count(), 2);
}

#[test]
fn replace_selection_text_edits_the_cells_and_keeps_the_block_in_step() {
    use crate::TextSelection;
    let mut document = document_with(two_by_two("t"));
    let start = document.editor_text().find('B').expect("cell B");
    let mut selection = TextSelection::caret(start);
    selection.focus = start + 1;
    document.set_selection(selection);
    document
        .replace_selection_text("Z")
        .expect("typing over a cell is accepted");
    let table = document.blocks[0]
        .typed_table()
        .expect("still a typed table");
    assert_eq!(table.cell_text(0, 1).as_deref(), Some("Z"));
    assert_eq!(table.cell_text(1, 0).as_deref(), Some("C"));
    assert_eq!(
        document.blocks[0].text.as_str(),
        flat_table_text(table),
        "the block text is derived from the cells, never edited on its own"
    );
}

#[test]
fn a_lone_enter_in_a_cell_changes_nothing() {
    let mut document = document_with(two_by_two("t"));
    let before = document.editor_text();
    let edited = before.replacen("\u{1f}B", "\n\u{1f}B", 1);
    let changed = document.replace_editor_text(&edited).expect("accepted");
    assert!(!changed, "Enter inside a cell does nothing");
    assert_eq!(document.editor_text(), before);
}

#[test]
fn grid_changing_editor_text_is_refused() {
    let mut document = document_with(two_by_two("t"));
    let before_text = document.editor_text();
    let before_table = document.blocks[0].typed_table().cloned();
    let result = document.edit_table_text(1, "A\u{1f}B\u{1e}C\u{1e}D");
    assert!(result.is_err(), "a changed grid must be refused");
    assert_eq!(document.editor_text(), before_text);
    assert_eq!(document.blocks[0].typed_table().cloned(), before_table);
}

#[test]
fn exports_read_the_typed_table() {
    let mut table = two_by_two("t");
    table.set_cell_rich_text(0, 0, vec![TextRun::bold("Head")]);
    table.cell_mut(1, 1).expect("cell").style_override = Some(CellStyleOverride {
        horizontal_align: Some(HorizontalAlign::Center),
        ..Default::default()
    });
    let document = document_with(table);
    let markdown = document.to_markdown();
    assert!(
        markdown.contains("**Head**"),
        "bold is written as emphasis: {markdown}"
    );
    assert!(
        markdown.contains(":---:"),
        "the centred column is marked: {markdown}"
    );
    let pdf = crate::export_pdf(&document);
    let content = loom_pdf::inspect::readable_content(&pdf).expect("readable PDF");
    assert!(
        content.contains("Head"),
        "the PDF prints the bold cell's text"
    );
    assert!(
        !content.contains('*'),
        "no Markdown emphasis reaches the PDF"
    );
}

#[test]
fn layout_has_one_line_per_table_row() {
    let document = document_with(two_by_two("t"));
    let flow = document
        .flow(&crate::PageStyle::default(), crate::PageViewport::default())
        .expect("the table lays out");
    let lines = flow
        .lines()
        .iter()
        .filter(|line| line.block_index == 0)
        .count();
    assert_eq!(lines, 2, "a table is one line per row, never wrapped");
}
