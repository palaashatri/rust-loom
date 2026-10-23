//! Menu and palette commands for list levels and the two breaks, through the
//! same dispatch the menus use, and the palette rows that offer them.

use super::actions_tests::test_state;
use super::*;

fn session(blocks: Vec<RichBlock>) -> (WriterApp, Rc<GuiState>) {
    let mut document = WriterDocument::new("test", "Test");
    document.blocks = blocks;
    let (app, state) = test_state(
        document,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
    );
    wire_writer_shared_callbacks(&app, &state, None);
    (app, state)
}

fn levels(state: &GuiState) -> Vec<u8> {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .map(|b| b.style.list_level)
        .collect()
}

fn texts(state: &GuiState) -> Vec<String> {
    state
        .current
        .borrow()
        .blocks
        .iter()
        .map(|b| b.text.as_str().to_string())
        .collect()
}

fn caret(state: &GuiState, at: usize) {
    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::caret(at));
}

#[test]
fn the_increase_and_decrease_list_level_commands_change_the_caret_item() {
    let (app, state) = session(vec![RichBlock::new(1, "list-bulleted", "item")]);
    caret(&state, 2);
    assert!(dispatch_command(&app, "list.level.increase"));
    assert_eq!(levels(&state), [1]);
    assert!(dispatch_command(&app, "list.level.increase"));
    assert_eq!(levels(&state), [2]);
    assert!(dispatch_command(&app, "list.level.decrease"));
    assert_eq!(levels(&state), [1]);
    app.invoke_undo();
    assert_eq!(levels(&state), [2]);
}

#[test]
fn the_list_level_commands_do_nothing_outside_a_list_item() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "plain")]);
    caret(&state, 0);
    assert!(dispatch_command(&app, "list.level.increase"));
    assert_eq!(levels(&state), [0]);
    assert_eq!(texts(&state), ["plain"]);
    assert_eq!(
        state.current.borrow().blocks[0].kind,
        "paragraph",
        "a paragraph does not become a list"
    );
}

#[test]
fn the_list_level_commands_do_nothing_inside_a_table_cell() {
    let table = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    let (app, state) = session(vec![loom_writer_core::markdown_table_block(1, table)]);
    caret(&state, 2);
    let before = state.current.borrow().blocks.clone();
    assert!(dispatch_command(&app, "list.level.increase"));
    assert_eq!(
        state.current.borrow().blocks,
        before,
        "the table is untouched"
    );
}

#[test]
fn the_page_break_command_breaks_the_page_as_ctrl_enter_does() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "AB")]);
    caret(&state, 1);
    assert!(dispatch_command(&app, "insert.page_break"));
    assert_eq!(texts(&state), ["A\u{c}", "B"]);
    app.invoke_undo();
    assert_eq!(texts(&state), ["AB"]);
}

#[test]
fn the_line_break_command_breaks_the_line_as_shift_enter_does() {
    let (app, state) = session(vec![RichBlock::new(1, "paragraph", "AB")]);
    caret(&state, 1);
    assert!(dispatch_command(&app, "insert.line_break"));
    assert_eq!(texts(&state), ["A\u{b}B"]);
    assert_eq!(state.current.borrow().selection(), TextSelection::caret(2));
}

#[test]
fn the_palette_offers_each_structure_command_with_its_shortcut() {
    let (app, _state) = session(vec![RichBlock::new(1, "paragraph", "AB")]);
    let registry = build_writer_registry();
    let found = |query: &str, id: &str| {
        palette_catalog::ranked_matches(&registry, query)
            .iter()
            .any(|spec| spec.id.as_str() == id)
    };
    assert!(found("page break", "insert.page_break"));
    assert!(found("line break", "insert.line_break"));
    assert!(found("increase list level", "list.level.increase"));
    assert!(found("decrease list level", "list.level.decrease"));
    let rows = palette_catalog::master_palette(&app);
    let shortcut = |id: &str| {
        rows.iter()
            .find(|row| row.id == id)
            .map(|row| row.shortcut)
            .unwrap_or("missing")
    };
    assert_eq!(shortcut("insert.page_break"), "Ctrl+Enter");
    assert_eq!(shortcut("insert.line_break"), "Shift+Enter");
    assert_eq!(shortcut("list.level.increase"), "Tab");
    assert_eq!(shortcut("list.level.decrease"), "Shift+Tab");
}
