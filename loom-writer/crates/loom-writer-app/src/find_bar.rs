//! Find and replace.
//!
//! Ctrl+F opens a bar over the page; typing selects the first match from the
//! caret, Enter and the buttons step through the rest (wrapping), and the
//! selection is the highlight. Ctrl+H adds a replace row. Matching ignores
//! case. Replacing is a document edit, so it is undoable and recovered like
//! any other.

use std::collections::HashMap;
use std::rc::Rc;

use loom_writer_core::{TextSelection, WriterDocument};
use slint::{ComponentHandle, SharedString};

use crate::{apply_with_history, FindBar, GuiState, HistoryKind, WriterApp};

/// Longest selection copied into the find field when the bar opens.
const PREFILL_LIMIT: usize = 80;

/// Where each match of `query` lies in the editor text, as UTF-8 byte ranges.
pub(crate) fn editor_matches(document: &WriterDocument, query: &str) -> Vec<(usize, usize)> {
    let mut starts = HashMap::new();
    let mut offset = 0;
    for block in &document.blocks {
        starts.insert(block.id, offset);
        offset += block.text.as_str().len() + 1;
    }
    document
        .find_all(query, false)
        .into_iter()
        .filter_map(|hit| {
            let base = starts.get(&hit.block_id)?;
            Some((base + hit.start, base + hit.end))
        })
        .collect()
}

/// The match to move to from a selection spanning `low..high`: the first one
/// at or after its end going forward, the last one ending at or before its
/// start going back, wrapping around the document. `None` when there are no
/// matches.
pub(crate) fn step_target(
    matches: &[(usize, usize)],
    (low, high): (usize, usize),
    forward: bool,
) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    if forward {
        Some(
            matches
                .iter()
                .position(|&(start, _)| start >= high)
                .unwrap_or(0),
        )
    } else {
        Some(
            matches
                .iter()
                .rposition(|&(_, end)| end <= low)
                .unwrap_or(matches.len() - 1),
        )
    }
}

/// The first match at or after the caret, wrapping; used while typing so the
/// highlight stays put as long as the current match still fits.
pub(crate) fn first_from(matches: &[(usize, usize)], low: usize) -> Option<usize> {
    if matches.is_empty() {
        return None;
    }
    Some(
        matches
            .iter()
            .position(|&(start, _)| start >= low)
            .unwrap_or(0),
    )
}

/// "3 of 12" when a match is selected, "12 matches" otherwise.
pub(crate) fn status_text(
    matches: &[(usize, usize)],
    query: &str,
    (low, high): (usize, usize),
) -> String {
    if query.is_empty() {
        return String::new();
    }
    if matches.is_empty() {
        return "No matches".to_string();
    }
    match matches.iter().position(|&range| range == (low, high)) {
        Some(index) => format!("{} of {}", index + 1, matches.len()),
        None => match matches.len() {
            1 => "1 match".to_string(),
            count => format!("{count} matches"),
        },
    }
}

fn selection_range(document: &WriterDocument) -> (usize, usize) {
    let selection = document.selection();
    (
        selection.anchor.min(selection.focus),
        selection.anchor.max(selection.focus),
    )
}

fn select(app: &WriterApp, (start, end): (usize, usize)) {
    app.invoke_selection_changed(
        start.min(i32::MAX as usize) as i32,
        end.min(i32::MAX as usize) as i32,
    );
}

fn publish_status(app: &WriterApp, state: &GuiState) {
    let bar = app.global::<FindBar>();
    let query = bar.get_query();
    let document = state.current.borrow();
    let matches = editor_matches(&document, query.as_str());
    let status = status_text(&matches, query.as_str(), selection_range(&document));
    bar.set_status(SharedString::from(status));
}

fn step(app: &WriterApp, state: &GuiState, forward: bool) {
    let query = app.global::<FindBar>().get_query();
    let target = {
        let document = state.current.borrow();
        let matches = editor_matches(&document, query.as_str());
        step_target(&matches, selection_range(&document), forward).map(|index| matches[index])
    };
    if let Some(range) = target {
        select(app, range);
    }
    publish_status(app, state);
}

fn open(app: &WriterApp, state: &GuiState, replace: bool) {
    let bar = app.global::<FindBar>();
    bar.set_show_replace(replace);
    {
        let document = state.current.borrow();
        let text = document.editor_text();
        let (low, high) = selection_range(&document);
        if let Some(selected) = text.get(low..high) {
            if !selected.is_empty()
                && !selected.contains('\n')
                && selected.chars().count() <= PREFILL_LIMIT
            {
                bar.set_query(SharedString::from(selected));
            }
        }
    }
    bar.set_open(true);
    bar.set_focus_tick(bar.get_focus_tick() + 1);
    publish_status(app, state);
}

fn query_changed(app: &WriterApp, state: &GuiState, text: &str) {
    let target = {
        let document = state.current.borrow();
        let matches = editor_matches(&document, text);
        first_from(&matches, selection_range(&document).0).map(|index| matches[index])
    };
    if let Some(range) = target {
        select(app, range);
    }
    publish_status(app, state);
}

fn replace_current(app: &WriterApp, state: &GuiState) {
    let bar = app.global::<FindBar>();
    let (query, replacement) = (bar.get_query(), bar.get_replacement());
    let selected_match = {
        let document = state.current.borrow();
        let range = selection_range(&document);
        editor_matches(&document, query.as_str())
            .into_iter()
            .find(|&candidate| candidate == range)
    };
    // The first press only selects the next match, as other editors do.
    let Some((start, end)) = selected_match else {
        step(app, state, true);
        return;
    };
    let mut next = state.current.borrow().clone();
    let text = next.editor_text();
    let edited = format!("{}{}{}", &text[..start], replacement, &text[end..]);
    if let Err(error) = next.replace_editor_text(&edited) {
        app.set_status_left(SharedString::from(format!("Replace failed: {error}")));
        return;
    }
    let caret = start + replacement.len();
    next.set_selection(TextSelection::range(caret, caret));
    apply_with_history(app, state, next, HistoryKind::DocumentAction);
    step(app, state, true);
}

fn replace_all(app: &WriterApp, state: &GuiState) {
    let bar = app.global::<FindBar>();
    let (query, replacement) = (bar.get_query(), bar.get_replacement());
    let mut next = state.current.borrow().clone();
    let count = next.replace_all(query.as_str(), replacement.as_str(), false);
    if count == 0 {
        bar.set_status(SharedString::from("No matches"));
        return;
    }
    apply_with_history(app, state, next, HistoryKind::DocumentAction);
    bar.set_status(SharedString::from(format!("{count} replaced")));
}

pub(crate) fn wire(app: &WriterApp, state: &Rc<GuiState>) {
    let bar = app.global::<FindBar>();
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        bar.on_open_requested(move |replace| {
            if let Some(app) = app_ref.upgrade() {
                open(&app, &state, replace);
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        bar.on_query_changed(move |text| {
            if let Some(app) = app_ref.upgrade() {
                query_changed(&app, &state, text.as_str());
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        bar.on_find_next(move || {
            if let Some(app) = app_ref.upgrade() {
                step(&app, &state, true);
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        bar.on_find_previous(move || {
            if let Some(app) = app_ref.upgrade() {
                step(&app, &state, false);
            }
        });
    }
    {
        let app_ref = app.as_weak();
        bar.on_close(move || {
            if let Some(app) = app_ref.upgrade() {
                let bar = app.global::<FindBar>();
                bar.set_open(false);
                bar.set_page_focus_tick(bar.get_page_focus_tick() + 1);
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        bar.on_replace_current(move || {
            if let Some(app) = app_ref.upgrade() {
                replace_current(&app, &state);
            }
        });
    }
    {
        let (state, app_ref) = (state.clone(), app.as_weak());
        bar.on_replace_all(move || {
            if let Some(app) = app_ref.upgrade() {
                replace_all(&app, &state);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document(text: &str) -> WriterDocument {
        let mut document = WriterDocument::new("find", "Find");
        document.replace_paragraphs(text);
        document
    }

    #[test]
    fn matches_are_offsets_in_the_editor_text_across_paragraphs() {
        let doc = document("The cat sat.\nA CAT naps.\nNo felines");
        let text = doc.editor_text();
        let hits = editor_matches(&doc, "cat");
        assert_eq!(hits.len(), 2);
        assert_eq!(&text[hits[0].0..hits[0].1], "cat");
        assert_eq!(&text[hits[1].0..hits[1].1], "CAT");
        assert!(editor_matches(&doc, "").is_empty());
    }

    #[test]
    fn stepping_goes_forward_and_back_and_wraps() {
        let matches = [(0, 3), (10, 13), (20, 23)];
        // From a caret before everything, then from each match's end.
        assert_eq!(step_target(&matches, (0, 0), true), Some(0));
        assert_eq!(step_target(&matches, (0, 3), true), Some(1));
        assert_eq!(step_target(&matches, (20, 23), true), Some(0), "wraps");
        assert_eq!(step_target(&matches, (20, 23), false), Some(1));
        assert_eq!(step_target(&matches, (0, 3), false), Some(2), "wraps");
        assert_eq!(step_target(&[], (0, 0), true), None);
    }

    #[test]
    fn typing_keeps_the_match_under_the_caret() {
        let matches = [(0, 3), (10, 13)];
        assert_eq!(first_from(&matches, 0), Some(0));
        assert_eq!(first_from(&matches, 4), Some(1));
        assert_eq!(first_from(&matches, 14), Some(0));
    }

    #[test]
    fn the_status_reads_position_count_or_nothing() {
        let matches = [(0, 3), (10, 13)];
        assert_eq!(status_text(&matches, "cat", (10, 13)), "2 of 2");
        assert_eq!(status_text(&matches, "cat", (5, 5)), "2 matches");
        assert_eq!(status_text(&matches[..1], "cat", (5, 5)), "1 match");
        assert_eq!(status_text(&[], "cat", (0, 0)), "No matches");
        assert_eq!(status_text(&[], "", (0, 0)), "");
    }
}

#[cfg(test)]
mod app_tests {
    use super::*;
    use crate::actions_tests::{test_state, text_document};

    fn setup(text: &str) -> (WriterApp, Rc<GuiState>) {
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = test_state(text_document(text), dialogs);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        (app, state)
    }

    fn selected(state: &GuiState) -> String {
        let document = state.current.borrow();
        let (low, high) = selection_range(&document);
        document.editor_text()[low..high].to_string()
    }

    #[test]
    fn opening_finding_and_stepping_select_matches_and_wrap() {
        let (app, state) = setup("The cat sat.\nAnother Cat here.");
        let bar = app.global::<FindBar>();
        bar.invoke_open_requested(false);
        assert!(bar.get_open());
        bar.set_query("cat".into());
        bar.invoke_query_changed("cat".into());
        assert_eq!(selected(&state), "cat");
        assert_eq!(bar.get_status().as_str(), "1 of 2");
        bar.invoke_find_next();
        assert_eq!(selected(&state), "Cat");
        assert_eq!(bar.get_status().as_str(), "2 of 2");
        bar.invoke_find_next();
        assert_eq!(bar.get_status().as_str(), "1 of 2", "wraps to the first");
        bar.invoke_find_previous();
        assert_eq!(bar.get_status().as_str(), "2 of 2", "wraps to the last");
        bar.set_query("zebra".into());
        bar.invoke_query_changed("zebra".into());
        assert_eq!(bar.get_status().as_str(), "No matches");
        bar.invoke_close();
        assert!(!bar.get_open());
    }

    #[test]
    fn opening_with_a_selection_searches_for_it() {
        let (app, state) = setup("alpha beta alpha");
        app.invoke_selection_changed(0, 5);
        app.global::<FindBar>().invoke_open_requested(false);
        assert_eq!(app.global::<FindBar>().get_query().as_str(), "alpha");
        assert_eq!(app.global::<FindBar>().get_status().as_str(), "1 of 2");
        let _ = state;
    }

    #[test]
    fn replace_all_is_one_undoable_edit() {
        let (app, state) = setup("The cat sat.\nAnother Cat here.");
        let bar = app.global::<FindBar>();
        bar.set_query("cat".into());
        bar.set_replacement("dog".into());
        bar.invoke_replace_all();
        assert_eq!(
            state.current.borrow().editor_text(),
            "The dog sat.\nAnother dog here."
        );
        assert_eq!(bar.get_status().as_str(), "2 replaced");
        app.invoke_undo();
        assert_eq!(
            state.current.borrow().editor_text(),
            "The cat sat.\nAnother Cat here."
        );
    }

    #[test]
    fn replace_selects_first_then_replaces_the_selected_match() {
        let (app, state) = setup("one cat two cat");
        let bar = app.global::<FindBar>();
        bar.set_query("cat".into());
        bar.set_replacement("owl".into());
        // Nothing selected yet: the first press only finds the next match.
        bar.invoke_replace_current();
        assert_eq!(selected(&state), "cat");
        assert_eq!(state.current.borrow().editor_text(), "one cat two cat");
        // The second replaces it and moves on to the next.
        bar.invoke_replace_current();
        assert_eq!(state.current.borrow().editor_text(), "one owl two cat");
        assert_eq!(selected(&state), "cat");
        bar.invoke_replace_current();
        assert_eq!(state.current.borrow().editor_text(), "one owl two owl");
    }
}
