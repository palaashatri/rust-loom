//! Undo history regressions: the byte budget must keep a supported document's
//! recent steps undoable, and typing must group by word, Enter, caret jumps and
//! pauses. The typing tests drive the real key path through the window.

use super::actions_tests::text_document;
use super::keyboard_flow_tests::launched;
use super::*;
use slint::platform::{Key, WindowEvent};

/// Roughly 400 pages of prose: 4,000 paragraphs of about 330 bytes each.
fn four_hundred_page_document() -> WriterDocument {
    let text: String = (0..4_000)
        .map(|i| format!("Paragraph {i:04} {}\n", "word ".repeat(62)))
        .collect();
    text_document(&text)
}

/// `doc` with `inserted` typed at byte `caret` of its editor text.
fn typed_at(doc: &WriterDocument, caret: usize, inserted: &str) -> WriterDocument {
    let mut text = doc.editor_text();
    text.insert_str(caret, inserted);
    let mut next = doc.clone();
    next.replace_editor_text_at(&text, Some(caret + inserted.len()))
        .expect("edit the document");
    next
}

#[test]
fn a_400_page_document_keeps_at_least_twenty_undo_steps() {
    let mut history = EditorHistory::new();
    let mut doc = four_hundred_page_document();
    let mut states = vec![doc.clone()];
    for step in 0..20usize {
        let next = typed_at(&doc, step * 60_000, "x");
        history.record(doc, next.clone(), HistoryKind::DocumentAction, step as u64);
        doc = next;
        states.push(doc.clone());
    }
    assert_eq!(
        history.undo_len(),
        20,
        "all twenty edits of a 400-page document must stay undoable"
    );
    eprintln!(
        "400-page history: 20 entries retain {} bytes (budget {} bytes)",
        history.total_bytes(),
        super::history::HISTORY_MAX_BYTES
    );
    assert!(history.total_bytes() < super::history::HISTORY_MAX_BYTES);
    for expected in states.iter().rev().skip(1) {
        let undone = history.undo().expect("an undo step remains");
        assert_eq!(&undone, expected);
    }
}

#[test]
fn an_in_place_edit_adds_only_the_changed_text_to_its_entry() {
    let doc = four_hundred_page_document();
    let edited = typed_at(&doc, 60_000, "x");
    let unchanged = super::history::retained_bytes(&doc, &doc);
    let changed = super::history::retained_bytes(&doc, &edited);
    assert!(
        changed - unchanged < 4096,
        "only the edited paragraph should be new, but the edit added {} bytes",
        changed - unchanged
    );
}

fn increase_string_capacity(value: &mut String) -> usize {
    let before = value.capacity();
    value.reserve_exact(8192);
    value.capacity() - before
}

#[test]
fn history_charges_owned_style_capacity_in_both_snapshots() {
    let mut original = text_document("styled");
    original.blocks[0].runs.push(loom_text::StyleRun {
        start: 0,
        end: 6,
        style: loom_text::CharacterStyle {
            font_family: "Inter".into(),
            color: Some("#112233FF".into()),
            background: Some("#445566FF".into()),
            ..Default::default()
        },
    });
    let original = original.clone();
    let mut larger = original.clone();
    let style = &mut larger.blocks[0].runs[0].style;
    let extra = increase_string_capacity(&mut style.font_family)
        + increase_string_capacity(style.color.as_mut().unwrap())
        + increase_string_capacity(style.background.as_mut().unwrap());

    let before = super::history::retained_bytes(&original, &original);
    let after = super::history::retained_bytes(&larger, &larger);
    assert!(
        after >= before + 2 * extra,
        "both owned style copies must be charged for reserved storage: {before} -> {after}, extra {extra}"
    );
}

#[test]
fn history_charges_nested_table_allocations_and_shares_the_same_table() {
    use loom_objects::table::{Border, CellBorders, CellStyleOverride, Table, TextRun};

    let mut table = Table::new(1, 1);
    table.name = Some("Table".into());
    table.set_cell_rich_text(
        0,
        0,
        vec![TextRun {
            font_family: Some("Inter".into()),
            color: Some("#112233FF".into()),
            ..TextRun::plain("cell")
        }],
    );
    table.rows[0].cells[0].style_override = Some(CellStyleOverride {
        background_color: Some("#445566FF".into()),
        borders: Some(CellBorders {
            top: Some(Border::solid(1.0, "#000000FF")),
            bottom: Some(Border::solid(1.0, "#000000FF")),
            left: Some(Border::solid(1.0, "#000000FF")),
            right: Some(Border::solid(1.0, "#000000FF")),
        }),
        ..Default::default()
    });

    let baseline = table.clone();
    let mut larger = baseline.clone();
    let mut extra = increase_string_capacity(&mut larger.id)
        + increase_string_capacity(larger.name.as_mut().unwrap())
        + increase_string_capacity(&mut larger.style_id);
    let columns = larger.columns.capacity();
    larger.columns.reserve_exact(128);
    extra += (larger.columns.capacity() - columns)
        * std::mem::size_of::<loom_objects::table::TableColumn>();
    let rows = larger.rows.capacity();
    larger.rows.reserve_exact(128);
    extra += (larger.rows.capacity() - rows) * std::mem::size_of::<loom_objects::table::TableRow>();
    let cells = larger.rows[0].cells.capacity();
    larger.rows[0].cells.reserve_exact(128);
    extra += (larger.rows[0].cells.capacity() - cells)
        * std::mem::size_of::<loom_objects::table::TableCell>();
    let cell = &mut larger.rows[0].cells[0];
    let runs = cell.content.runs.capacity();
    cell.content.runs.reserve_exact(128);
    extra += (cell.content.runs.capacity() - runs) * std::mem::size_of::<TextRun>();
    extra += increase_string_capacity(cell.content.runs[0].font_family.as_mut().unwrap());
    extra += increase_string_capacity(cell.content.runs[0].color.as_mut().unwrap());
    let style = cell.style_override.as_mut().unwrap();
    extra += increase_string_capacity(style.background_color.as_mut().unwrap());
    let borders = style.borders.as_mut().unwrap();
    for border in [
        &mut borders.top,
        &mut borders.bottom,
        &mut borders.left,
        &mut borders.right,
    ] {
        extra += increase_string_capacity(&mut border.as_mut().unwrap().color);
    }

    let document = |table| {
        let mut doc = WriterDocument::new("table", "Table");
        doc.push(RichBlock::table_block(1, table));
        doc
    };
    let original = document(baseline);
    let larger = document(larger);
    let before = super::history::retained_bytes(&original, &original);
    let shared = super::history::retained_bytes(&larger, &larger);
    assert!(
        shared >= before + extra,
        "the table's nested allocations must be charged: {before} -> {shared}, extra {extra}"
    );
    let mut distinct = larger.clone();
    let shared_clone = super::history::retained_bytes(&larger, &distinct);
    let separate_table = std::sync::Arc::make_mut(distinct.blocks[0].table.as_mut().unwrap());
    separate_table.rows[0].cells[0].content.runs[0]
        .font_family
        .as_mut()
        .unwrap()
        .push_str(&"f".repeat(8192));
    let both = super::history::retained_bytes(&larger, &distinct);
    assert!(
        both > shared_clone + 8192,
        "a distinct table allocation must be charged in addition to the original"
    );
}

#[test]
fn history_charges_picture_name_and_shared_picture_bytes() {
    use std::sync::Arc;

    let mut original = text_document("image");
    original.blocks[0].image = Some(loom_writer_core::InlineImage {
        name: "image.png".into(),
        format: loom_writer_core::ImageFormat::Png,
        width_px: 1,
        height_px: 1,
        width_pt: 1.0,
        height_pt: 1.0,
        bytes: Arc::new(vec![7; 16 * 1024]),
    });
    let mut larger = original.clone();
    let extra = increase_string_capacity(&mut larger.blocks[0].image.as_mut().unwrap().name);
    let before = super::history::retained_bytes(&original, &original);
    let shared = super::history::retained_bytes(&larger, &larger);
    assert!(
        shared >= before + 2 * extra,
        "the two owned picture names must be charged"
    );
    let mut distinct = larger.clone();
    let shared_clone = super::history::retained_bytes(&larger, &distinct);
    std::sync::Arc::make_mut(&mut distinct.blocks[0].image.as_mut().unwrap().bytes);
    let both = super::history::retained_bytes(&larger, &distinct);
    assert!(
        both >= shared_clone + 16 * 1024,
        "independently allocated image bytes must be charged"
    );
}

#[test]
fn an_edit_larger_than_the_budget_stays_undoable() {
    let mut history = EditorHistory::with_budget(16, 1_000);
    let before = four_hundred_page_document();
    let after = typed_at(&before, 10, "x");
    history.record(before.clone(), after, HistoryKind::DocumentAction, 0);
    assert!(
        history.can_undo(),
        "the newest edit is never dropped, even when it alone exceeds the budget"
    );
    assert_eq!(history.undo(), Some(before));
}

#[test]
fn typing_a_sentence_undoes_one_word_at_a_time() {
    let s = launched("");
    type_text(&s.app, "hello world again");
    assert_eq!(text(&s), "hello world again");

    ctrl(&s.app, "z");
    assert_eq!(text(&s), "hello world", "the last word is one undo step");
    ctrl(&s.app, "z");
    assert_eq!(text(&s), "hello", "the space starts a new step");
    ctrl(&s.app, "z");
    assert_eq!(text(&s), "", "the first word is its own step");
}

#[test]
fn punctuation_after_a_word_starts_a_new_undo_step() {
    let s = launched("");
    type_text(&s.app, "hello, world");
    ctrl(&s.app, "z");
    assert_eq!(
        text(&s),
        "hello",
        "the comma after a word starts a new step"
    );
}

#[test]
fn enter_and_a_caret_jump_each_start_a_new_undo_step() {
    let s = launched("");
    type_text(&s.app, "one");
    press(&s.app, Key::Return);
    type_text(&s.app, "two");
    ctrl(&s.app, "z");
    assert!(
        !text(&s).contains("two"),
        "typing after Enter is its own step"
    );
    ctrl(&s.app, "z");
    assert_eq!(text(&s).trim_end(), "one", "Enter is its own step");

    let jump = launched("");
    type_text(&jump.app, "abc");
    for _ in 0..3 {
        press(&jump.app, Key::LeftArrow);
    }
    type_text(&jump.app, "X");
    assert_eq!(text(&jump), "Xabc");
    ctrl(&jump.app, "z");
    assert_eq!(
        text(&jump),
        "abc",
        "typing after a caret jump is its own step"
    );
}

#[test]
fn a_pause_longer_than_the_coalescing_window_starts_a_new_step() {
    let s = launched("");
    type_text(&s.app, "ab");
    let window = super::history::TYPING_COALESCE_WINDOW_MS;
    std::thread::sleep(std::time::Duration::from_millis(window + 100));
    type_text(&s.app, "cd");
    ctrl(&s.app, "z");
    assert_eq!(text(&s), "ab", "a pause starts a new step");
}

fn text(s: &keyboard_flow_tests::Session) -> String {
    s.state.current.borrow().plain_text()
}

fn tap(app: &WriterApp, key: impl Into<SharedString>) {
    let key = key.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: key.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text: key });
    let _ = snapshot_component(app, 1280.0, 800.0, 1.0).expect("render");
}

fn press(app: &WriterApp, key: Key) {
    tap(app, SharedString::from(key));
}

fn ctrl(app: &WriterApp, letter: &str) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: SharedString::from(Key::Control),
    });
    tap(app, letter);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: SharedString::from(Key::Control),
    });
}

fn type_text(app: &WriterApp, text: &str) {
    for c in text.chars() {
        tap(app, c.to_string());
    }
}

#[test]
fn typing_in_a_table_cell_undoes_one_word_at_a_time() {
    let s = launched("");
    let mut doc = WriterDocument::new("table-undo", "Table");
    let id = doc.next_id();
    doc.push(loom_writer_core::markdown_table_block(
        id,
        "|  | B |\n| --- | --- |\n| C | D |",
    ));
    doc.set_selection(loom_writer_core::TextSelection::caret(0));
    apply_document(&s.app, &doc);
    *s.state.current.borrow_mut() = doc;
    let cell = |s: &keyboard_flow_tests::Session| {
        s.state
            .current
            .borrow()
            .table_from_block(id)
            .expect("the table")
            .rows[0][0]
            .clone()
    };

    type_text(&s.app, "hello world again");
    assert_eq!(cell(&s), "hello world again");
    ctrl(&s.app, "z");
    assert_eq!(cell(&s), "hello world", "the last word is one undo step");
    ctrl(&s.app, "z");
    assert_eq!(cell(&s), "hello", "the space starts a new step");
    ctrl(&s.app, "z");
    assert_eq!(cell(&s), "", "the first word is its own step");
}
