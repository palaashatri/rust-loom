//! Find highlights, F3 stepping and "Match case".

use super::actions_tests::{test_state, text_document};
use super::*;

fn setup(text: &str) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document(text), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    let document = state.current.borrow().clone();
    refresh_writer_render_projection(&app, &document, *state.viewport.borrow());
    (app, state)
}

fn selected(state: &GuiState) -> String {
    let document = state.current.borrow();
    let selection = document.selection();
    let (low, high) = (
        selection.anchor.min(selection.focus),
        selection.anchor.max(selection.focus),
    );
    document.editor_text()[low..high].to_string()
}

fn highlight_count(app: &WriterApp) -> usize {
    app.global::<FindBar>().get_match_rects().row_count()
}

fn query(app: &WriterApp, text: &str) {
    let bar = app.global::<FindBar>();
    bar.set_query(text.into());
    bar.invoke_query_changed(text.into());
}

#[test]
fn every_other_match_is_highlighted_while_the_bar_is_open_and_only_then() {
    let (app, state) = setup("cat one\ncat two\nCat three\nno match here");
    assert_eq!(highlight_count(&app), 0, "nothing before the bar opens");
    let bar = app.global::<FindBar>();
    bar.invoke_open_requested(false);
    query(&app, "cat");
    // Three matches; the current one is the selection, the other two are soft.
    assert_eq!(highlight_count(&app), 2);
    let rects: Vec<_> = bar.get_match_rects().iter().collect();
    assert!(rects.iter().all(|r| r.width > 0.0 && r.height > 0.0));
    assert!(
        rects[1].y > rects[0].y,
        "the later match is lower on the page"
    );
    // Stepping moves which match is current but keeps the others highlighted.
    bar.invoke_find_next();
    assert_eq!(highlight_count(&app), 2);
    let after: Vec<_> = bar.get_match_rects().iter().collect();
    assert_ne!(
        rects[0].y, after[0].y,
        "the highlighted set follows the current match"
    );
    // No matches: no highlights.
    query(&app, "zebra");
    assert_eq!(highlight_count(&app), 0);
    // Closing the bar removes them.
    query(&app, "cat");
    assert_eq!(highlight_count(&app), 2);
    bar.invoke_close();
    assert_eq!(highlight_count(&app), 0);
    let _ = state;
}

#[test]
fn highlights_follow_document_edits_while_the_bar_is_open() {
    let (app, state) = setup("cat and cat");
    app.global::<FindBar>().invoke_open_requested(false);
    query(&app, "cat");
    assert_eq!(highlight_count(&app), 1);
    let mut next = state.current.borrow().clone();
    next.replace_paragraphs("cat and cat and cat");
    apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
    assert_eq!(highlight_count(&app), 2, "a new match gets its highlight");
}

#[test]
fn f3_and_shift_f3_step_matches_with_the_bar_closed_using_the_last_query() {
    let (app, state) = setup("one cat two cat three cat");
    let bar = app.global::<FindBar>();
    bar.invoke_open_requested(false);
    query(&app, "cat");
    bar.invoke_close();
    assert!(!bar.get_open());
    let position = |state: &GuiState| state.current.borrow().selection().anchor;
    let first = position(&state);
    bar.invoke_find_next();
    let second = position(&state);
    assert!(second > first);
    bar.invoke_find_next();
    assert!(position(&state) > second);
    bar.invoke_find_previous();
    assert_eq!(position(&state), second, "Shift+F3 steps back");
    assert_eq!(selected(&state), "cat");
    assert_eq!(highlight_count(&app), 0, "closed bar shows no highlights");
}

#[test]
fn the_f3_key_reaches_find_from_the_window() {
    let (app, state) = setup("a cat, another cat");
    let bar = app.global::<FindBar>();
    // Pre-set the query before opening the bar (simulates it being set by a previous find)
    bar.set_query("cat".into());
    // Open the find bar with the pre-set query
    bar.invoke_open_requested(false);
    assert_eq!(
        bar.get_status().as_str(),
        "1 of 2",
        "opening with pre-set query finds matches"
    );
    assert_eq!(
        selected(&state),
        "cat",
        "opening with pre-set query selects the first match"
    );
    // F3 should find the next match
    bar.invoke_find_next();
    assert_eq!(selected(&state), "cat", "F3 finds the next match");
    let first = state.current.borrow().selection().anchor;
    // Shift+F3 should find the previous match
    bar.invoke_find_previous();
    assert_ne!(
        state.current.borrow().selection().anchor,
        first,
        "Shift+F3 steps back"
    );
}

#[test]
fn match_case_switches_between_case_sensitive_and_insensitive_matching() {
    let (app, state) = setup("Cat cat CAT");
    let bar = app.global::<FindBar>();
    bar.invoke_open_requested(false);
    query(&app, "cat");
    assert!(
        bar.get_status().as_str().ends_with("of 3"),
        "{}",
        bar.get_status()
    );
    bar.set_match_case(true);
    bar.invoke_query_changed(bar.get_query());
    assert_eq!(selected(&state), "cat");
    assert_eq!(bar.get_status().as_str(), "1 of 1");
    assert_eq!(highlight_count(&app), 0);
    bar.set_match_case(false);
    bar.invoke_query_changed(bar.get_query());
    assert_eq!(highlight_count(&app), 2);
}
