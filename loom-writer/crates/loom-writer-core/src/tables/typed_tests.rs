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
        json.contains(&format!("\"format\":{}", crate::WRITER_CONTENT_FORMAT)),
        "content must declare the current format: {json}"
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
        reloaded
            .to_content_json()
            .contains(&format!("\"format\":{}", crate::WRITER_CONTENT_FORMAT)),
        "the next save writes the current format, so conversion happens once"
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
        "{{\"format\":{},\"id\":\"doc\",\"title\":\"Doc\",\"blocks\":[{}]}}",
        crate::WRITER_CONTENT_FORMAT + 1,
        legacy_block(LEGACY)
    );
    assert!(
        WriterDocument::from_content_json(&newer).is_err(),
        "a future format must not open"
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
    let pdf = crate::export::test_support::export_valid_pdf(&document);
    let text = loom_pdf::inspect::page_text(&pdf)
        .expect("extracted PDF text")
        .join("\n");
    assert!(
        text.contains("Head"),
        "the PDF prints the bold cell's text: {text}"
    );
    assert!(!text.contains('*'), "no Markdown emphasis reaches the PDF");
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

#[test]
fn direct_table_text_changes_rebase_footnote_anchors() {
    let mut document = document_with(two_by_two("t"));
    let block_id = document.blocks[0].id;
    let before = document.blocks[0].text.as_str().to_owned();
    let anchor = before.find('D').expect("last cell text");
    document.footnotes.push(crate::FootnoteEntry {
        id: 41,
        block_id,
        offset: anchor,
        body: "table note".into(),
    });

    let updated = before.replace('C', "CC");
    document
        .edit_table_text(block_id, &updated)
        .expect("edit a table cell without changing the grid");
    assert_eq!(document.footnotes[0].id, 41);
    assert_eq!(document.footnotes[0].offset, anchor + 1);
    assert_eq!(document.blocks[0].text.as_str().find('D'), Some(anchor + 1));
    assert!(document.validate_footnotes().is_ok());
}

#[test]
fn direct_table_text_updates_preserve_anchors_in_unchanged_cells() {
    let mut table = Table::new(1, 3);
    table.id = "three-cells".into();
    table.set_cell_text(0, 0, "foo");
    table.set_cell_text(0, 1, "keep");
    table.set_cell_text(0, 2, "foo");
    let mut document = document_with(table);
    let block_id = document.blocks[0].id;
    let old_text = document.blocks[0].text.as_str().to_owned();
    let anchor = old_text.find("keep").unwrap() + 2;
    document.footnotes.push(crate::FootnoteEntry {
        id: 45,
        block_id,
        offset: anchor,
        body: "middle cell note".into(),
    });

    let updated = old_text.replace("foo", "x");
    document
        .edit_table_text(block_id, &updated)
        .expect("edit multiple table cells without changing the grid");
    let new_anchor = document.blocks[0].text.as_str().find("keep").unwrap() + 2;
    assert_eq!(new_anchor, anchor - 2);
    assert_eq!(document.footnotes[0].id, 45);
    assert_eq!(document.footnotes[0].offset, new_anchor);
    assert!(document.validate_footnotes().is_ok());
}

#[test]
fn inserting_a_table_row_rebases_footnotes_in_later_cells() {
    let mut document = document_with(two_by_two("t"));
    let block_id = document.blocks[0].id;
    let old_anchor = document.blocks[0].text.as_str().find('D').unwrap();
    document.footnotes.push(crate::FootnoteEntry {
        id: 43,
        block_id,
        offset: old_anchor,
        body: "later row note".into(),
    });

    document
        .insert_table_row_at_offset(0, true)
        .expect("insert a row after the first row");
    let new_anchor = document.blocks[0].text.as_str().find('D').unwrap();
    assert!(new_anchor > old_anchor);
    assert_eq!(document.footnotes[0].id, 43);
    assert_eq!(document.footnotes[0].offset, new_anchor);
    assert!(document.validate_footnotes().is_ok());
}

#[test]
fn inserting_a_table_column_rebases_footnotes_by_cell() {
    let mut document = document_with(two_by_two("t"));
    let block_id = document.blocks[0].id;
    let old_text = document.blocks[0].text.as_str().to_owned();
    for (id, value) in [(51, 'C'), (52, 'D')] {
        document.footnotes.push(crate::FootnoteEntry {
            id,
            block_id,
            offset: old_text.find(value).unwrap(),
            body: format!("note {value}"),
        });
    }

    document
        .insert_table_column_at_offset(old_text.find('A').unwrap(), true)
        .expect("insert a column after the first");

    let new_text = document.blocks[0].text.as_str();
    for (id, value) in [(51, 'C'), (52, 'D')] {
        let note = document
            .footnotes
            .iter()
            .find(|note| note.id == id)
            .unwrap();
        assert_eq!(note.offset, new_text.find(value).unwrap());
    }
    assert!(document.validate_footnotes().is_ok());
}

#[test]
fn deleting_a_table_column_drops_only_its_notes() {
    let mut document = document_with(two_by_two("t"));
    let block_id = document.blocks[0].id;
    let old_text = document.blocks[0].text.as_str().to_owned();
    for (id, value) in [(53, 'B'), (54, 'C'), (55, 'D')] {
        document.footnotes.push(crate::FootnoteEntry {
            id,
            block_id,
            offset: old_text.find(value).unwrap(),
            body: format!("note {value}"),
        });
    }

    document
        .delete_table_column_at_offset(old_text.find('A').unwrap())
        .expect("delete the first column");

    let new_text = document.blocks[0].text.as_str();
    for (id, value) in [(53, 'B'), (55, 'D')] {
        let note = document
            .footnotes
            .iter()
            .find(|note| note.id == id)
            .unwrap();
        assert_eq!(note.offset, new_text.find(value).unwrap());
    }
    assert!(document.footnotes.iter().all(|note| note.id != 54));
    assert!(document.validate_footnotes().is_ok());
}

#[test]
fn typing_in_a_table_rebases_a_footnote_at_the_caret() {
    let mut document = WriterDocument::new("table-note", "Table note");
    let block_id = document.insert_table_block(0, 2, 2).expect("insert table");
    document.footnotes.push(crate::FootnoteEntry {
        id: 44,
        block_id,
        offset: 0,
        body: "caret note".into(),
    });

    document
        .replace_selection_text("x")
        .expect("type in first table cell");
    assert_eq!(document.footnotes[0].id, 44);
    assert_eq!(document.footnotes[0].block_id, block_id);
    assert_eq!(document.footnotes[0].offset, 1);
    assert!(document.validate_footnotes().is_ok());
}

#[test]
fn table_replace_all_keeps_notes_between_changed_cells() {
    let mut table = Table::new(1, 3);
    table.id = "three-cells".into();
    table.set_cell_text(0, 0, "foo");
    table.set_cell_text(0, 1, "keep");
    table.set_cell_text(0, 2, "foo");
    let mut document = document_with(table);
    let block_id = document.blocks[0].id;
    let anchor = document.blocks[0].text.as_str().find("keep").unwrap() + 2;
    document.footnotes.push(crate::FootnoteEntry {
        id: 42,
        block_id,
        offset: anchor,
        body: "middle cell note".into(),
    });

    assert_eq!(document.replace_all("foo", "x", false), 2);
    let rebased_anchor = document.blocks[0].text.as_str().find("keep").unwrap() + 2;
    assert_eq!(rebased_anchor, anchor - 2);
    assert_eq!(document.footnotes[0].offset, anchor - 2);
    assert_eq!(document.footnotes[0].id, 42);
    assert!(document.validate_footnotes().is_ok());
}

#[test]
fn table_replace_all_keeps_invalid_table_unchanged() {
    let mut table = Table::new(1, 2);
    table.set_cell_text(0, 0, "foo");
    table.set_cell_text(0, 1, "foo");
    let mut document = document_with(table);
    let block_id = document.blocks[0].id;
    let typed_before = flat_table_text(document.blocks[0].typed_table().unwrap());
    let malformed = format!("{typed_before}{TABLE_CELL_SEPARATOR}tail");
    let anchor = malformed.find("foo").unwrap() + 1;
    document.blocks[0].text = crate::Text::from_str(&malformed);
    document.footnotes.push(crate::FootnoteEntry {
        id: 43,
        block_id,
        offset: anchor,
        body: "preserve on failure".into(),
    });

    assert_eq!(document.replace_all("foo", "x", false), 0);
    assert_eq!(document.blocks[0].text.as_str(), malformed.as_str());
    assert_eq!(
        flat_table_text(document.blocks[0].typed_table().unwrap()),
        typed_before
    );
    assert_eq!(document.footnotes.len(), 1);
    assert_eq!(document.footnotes[0].id, 43);
    assert_eq!(document.footnotes[0].block_id, block_id);
    assert_eq!(document.footnotes[0].offset, anchor);
}

#[test]
fn merging_table_cells_rebases_footnotes_with_their_text_and_split_keeps_them() {
    let mut document = document_with(two_by_two("merge-notes"));
    let block_id = document.blocks[0].id;
    let old_text = document.blocks[0].text.as_str().to_owned();
    for (id, marker) in [(71, "A"), (72, "B"), (73, "C"), (74, "D")] {
        document.footnotes.push(crate::FootnoteEntry {
            id,
            block_id,
            offset: old_text.find(marker).unwrap(),
            body: format!("note {marker}"),
        });
    }

    document
        .merge_cells_at_offset(old_text.find('A').unwrap(), 2, 2)
        .expect("merge the four cells");

    let merged_text = document.blocks[0].text.as_str().to_owned();
    for (id, marker) in [(71, "A"), (72, "B"), (73, "C"), (74, "D")] {
        let note = document
            .footnotes
            .iter()
            .find(|note| note.id == id)
            .unwrap();
        assert_eq!(note.offset, merged_text.find(marker).unwrap());
    }
    assert!(document.validate_footnotes().is_ok());

    document
        .split_cell_at_offset(merged_text.find('A').unwrap())
        .expect("split the merged cell");
    let split_text = document.blocks[0].text.as_str();
    for (id, marker) in [(71, "A"), (72, "B"), (73, "C"), (74, "D")] {
        let note = document
            .footnotes
            .iter()
            .find(|note| note.id == id)
            .unwrap();
        assert_eq!(note.offset, split_text.find(marker).unwrap());
    }
    assert!(document.validate_footnotes().is_ok());
}

#[test]
fn merging_table_cells_preserves_empty_cell_anchors_separately() {
    let mut table = Table::new(1, 3);
    table.id = "empty-merge".into();
    table.set_cell_text(0, 0, "x");
    table.set_cell_text(0, 2, "z");
    let mut document = document_with(table);
    let block_id = document.blocks[0].id;
    let old_map = TableTextMap::new(document.blocks[0].typed_table().unwrap());
    document.footnotes.extend([
        crate::FootnoteEntry {
            id: 75,
            block_id,
            offset: old_map.cell_range(0, 1).unwrap().start,
            body: "empty cell note".into(),
        },
        crate::FootnoteEntry {
            id: 76,
            block_id,
            offset: old_map.cell_range(0, 2).unwrap().start,
            body: "moved cell note".into(),
        },
    ]);

    document
        .merge_cells_at_offset(old_map.cell_range(0, 0).unwrap().start, 1, 3)
        .expect("merge the empty and populated cells");

    let merged_table = document.blocks[0].typed_table().unwrap();
    let merged_map = TableTextMap::new(merged_table);
    let merged_text = document.blocks[0].text.as_str();
    assert_eq!(
        document
            .footnotes
            .iter()
            .find(|note| note.id == 75)
            .unwrap()
            .offset,
        merged_map.cell_range(0, 1).unwrap().start
    );
    assert_eq!(
        document
            .footnotes
            .iter()
            .find(|note| note.id == 76)
            .unwrap()
            .offset,
        merged_text.find('z').unwrap()
    );
    assert!(document.validate_footnotes().is_ok());
}
