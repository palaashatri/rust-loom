use std::ops::Range;
use std::rc::Rc;

use slint::{ComponentHandle, SharedString};

use crate::history::HistoryKind;
use crate::{apply_with_history, GuiState, WriterApp, WriterReviewState};
use loom_writer_core::{
    PageViewport, RevisionAnchor, RevisionKind, RevisionPayload, TextSelection, WriterDocument,
};

pub(crate) fn register(app: &WriterApp, state: &Rc<GuiState>) {
    let global = app.global::<WriterReviewState>();
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_toggle_recording(move || {
            let Some(app) = app_ref.upgrade() else { return };
            toggle_recording(&app, &state);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_set_author(move |author| {
            let Some(app) = app_ref.upgrade() else { return };
            set_author(&app, &state, author.to_string());
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_navigate(move |delta| {
            let Some(app) = app_ref.upgrade() else { return };
            navigate(&app, &state, delta);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_resolve_current(move |accept| {
            let Some(app) = app_ref.upgrade() else { return };
            resolve_current(&app, &state, accept);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_resolve_all(move |accept| {
            let Some(app) = app_ref.upgrade() else { return };
            resolve_all(&app, &state, accept);
        });
    }
}

pub(crate) fn sync_state(app: &WriterApp, document: &WriterDocument, viewport: PageViewport) {
    let review = app.global::<WriterReviewState>();
    let revisions = document.tracked_changes.revisions();
    let count = revisions.len();
    let index = if count == 0 {
        0
    } else {
        (review.get_current_index().max(0) as usize).min(count - 1)
    };
    review.set_recording(document.tracked_changes.recording());
    review.set_author(document.tracked_changes.author().into());
    review.set_revision_count(count.min(i32::MAX as usize) as i32);
    review.set_current_index(index as i32);

    let Some(revision) = revisions.get(index) else {
        review.set_current_kind(SharedString::new());
        review.set_current_author(SharedString::new());
        review.set_current_before(SharedString::new());
        review.set_current_after(SharedString::new());
        review.set_current_before_style(SharedString::new());
        review.set_current_after_style(SharedString::new());
        review.set_current_location("No pending changes".into());
        return;
    };

    review.set_current_kind(kind_label(revision.kind).into());
    review.set_current_author(revision.author.as_str().into());
    let before = revision
        .before
        .as_ref()
        .map_or("", |payload| payload.text.as_str());
    review.set_current_before(
        if revision.kind == RevisionKind::Deletion {
            before.to_owned()
        } else {
            excerpt(before)
        }
        .into(),
    );
    review.set_current_after(
        excerpt(
            revision
                .after
                .as_ref()
                .map_or("", |payload| payload.text.as_str()),
        )
        .into(),
    );
    let paragraph_format = document
        .tracked_changes
        .paragraph_format_change(revision.id);
    review.set_current_before_style(
        payload_style(
            revision.before.as_ref(),
            paragraph_format.map(|change| change.before),
        )
        .into(),
    );
    review.set_current_after_style(
        payload_style(
            revision.after.as_ref(),
            paragraph_format.map(|change| change.after),
        )
        .into(),
    );
    review.set_current_location(
        location(
            document,
            &revision.anchor,
            viewport,
            review.get_panel_open(),
        )
        .into(),
    );
}

fn toggle_recording(app: &WriterApp, state: &GuiState) {
    let mut next = state.current.borrow().clone();
    let recording = !next.tracked_changes.recording();
    next.tracked_changes.set_recording(recording);
    if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
        return;
    }
    app.set_status_right(if recording {
        "Tracked change recording on".into()
    } else {
        "Tracked change recording off".into()
    });
}

fn set_author(app: &WriterApp, state: &GuiState, author: String) {
    let mut next = state.current.borrow().clone();
    if next.tracked_changes.author() == author.as_str() {
        return;
    }
    if let Err(error) = next.tracked_changes.set_author(author) {
        app.set_status_right(format!("Tracked-change author was not updated: {error}").into());
        sync_state(app, &state.current.borrow(), *state.viewport.borrow());
        return;
    }
    if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
        return;
    }
    app.set_status_right("Tracked-change author updated".into());
}

fn navigate(app: &WriterApp, state: &GuiState, delta: i32) {
    let (selection, index, count) = {
        let document = state.current.borrow();
        let revisions = document.tracked_changes.revisions();
        let count = revisions.len();
        if count == 0 {
            app.set_status_right("No unresolved tracked changes".into());
            return;
        }
        let current = app.global::<WriterReviewState>().get_current_index().max(0);
        let index = (current + delta).rem_euclid(count as i32) as usize;
        let Some(selection) = selection_for_anchor(&document, &revisions[index].anchor) else {
            app.set_status_right("Tracked change position is unavailable".into());
            return;
        };
        (selection, index, count)
    };

    app.global::<WriterReviewState>()
        .set_current_index(index as i32);
    app.invoke_selection_changed(
        selection.anchor.min(i32::MAX as usize) as i32,
        selection.focus.min(i32::MAX as usize) as i32,
    );
    crate::reveal_caret(app, state);
    sync_state(app, &state.current.borrow(), *state.viewport.borrow());
    app.set_status_right(format!("Tracked change {} of {count}", index + 1).into());
}

fn resolve_current(app: &WriterApp, state: &GuiState, accept: bool) {
    let index = app.global::<WriterReviewState>().get_current_index().max(0) as usize;
    let Some(id) = state
        .current
        .borrow()
        .tracked_changes
        .revisions()
        .get(index)
        .map(|revision| revision.id)
    else {
        app.set_status_right("No unresolved tracked changes".into());
        return;
    };
    resolve_ids(app, state, vec![id], accept);
}

fn resolve_all(app: &WriterApp, state: &GuiState, accept: bool) {
    let mut next = state.current.borrow().clone();
    if next.tracked_changes.revisions().is_empty() {
        app.set_status_right("No unresolved tracked changes".into());
        return;
    }
    let count = match if accept {
        next.accept_all_revisions()
    } else {
        next.reject_all_revisions()
    } {
        Ok(count) => count,
        Err(error) => {
            app.set_status_right(format!("Tracked changes could not be resolved: {error}").into());
            return;
        }
    };
    if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
        return;
    }
    navigate(app, state, 0);
    app.set_status_right(
        format!(
            "{} {count} tracked change{}",
            if accept { "Accepted" } else { "Rejected" },
            if count == 1 { "" } else { "s" }
        )
        .into(),
    );
}

fn resolve_ids(app: &WriterApp, state: &GuiState, ids: Vec<u64>, accept: bool) {
    let mut next = state.current.borrow().clone();
    let count = ids.len();
    for id in ids {
        let result = if accept {
            next.accept_revision(id)
        } else {
            next.reject_revision(id)
        };
        match result {
            Ok(true) => {}
            Ok(false) => {
                app.set_status_right("Tracked change is no longer available".into());
                return;
            }
            Err(error) => {
                app.set_status_right(
                    format!("Tracked change could not be resolved: {error}").into(),
                );
                return;
            }
        }
    }
    if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
        return;
    }
    navigate(app, state, 0);
    app.set_status_right(
        format!(
            "{} {count} tracked change{}",
            if accept { "Accepted" } else { "Rejected" },
            if count == 1 { "" } else { "s" }
        )
        .into(),
    );
}

fn selection_for_anchor(
    document: &WriterDocument,
    anchor: &RevisionAnchor,
) -> Option<TextSelection> {
    let (range, _, _) = anchor_global_range(document, anchor)?;
    Some(if range.is_empty() {
        TextSelection::caret(range.start)
    } else {
        TextSelection::range(range.start, range.end)
    })
}

fn location(
    document: &WriterDocument,
    anchor: &RevisionAnchor,
    viewport: PageViewport,
    include_page: bool,
) -> String {
    let Some((range, start_index, end_index)) = anchor_global_range(document, anchor) else {
        return "Position unavailable".to_owned();
    };
    let selection = if range.is_empty() {
        TextSelection::caret(range.start)
    } else {
        TextSelection::range(range.start, range.end)
    };
    let style = document.page.page_style();
    let layout_viewport = PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: crate::normalize_page_zoom(viewport.zoom, 1.0),
        scroll_x: crate::normalize_page_scroll(viewport.scroll_x),
        scroll_y: crate::normalize_page_scroll(viewport.scroll_y),
    };
    let page = if include_page {
        document
            .layout(&style, layout_viewport)
            .ok()
            .and_then(|layout| {
                document
                    .selection_rectangles_for(&layout, &style, &selection)
                    .first()
                    .map(|rect| rect.page_index + 1)
            })
    } else {
        None
    };
    let position = if start_index == end_index {
        if anchor.start == anchor.end {
            format!(
                "block #{} · byte {}",
                document.blocks[start_index].id, anchor.start
            )
        } else {
            format!(
                "block #{} · bytes {}–{}",
                document.blocks[start_index].id, anchor.start, anchor.end
            )
        }
    } else {
        format!(
            "block #{} byte {}–block #{} byte {}",
            document.blocks[start_index].id,
            anchor.start,
            document.blocks[end_index].id,
            anchor.end
        )
    };
    match page {
        Some(page) => format!("Page {page} · {position}"),
        None if include_page => format!("Page unavailable · {position}"),
        None => position,
    }
}

pub(crate) fn anchor_global_range(
    document: &WriterDocument,
    anchor: &RevisionAnchor,
) -> Option<(Range<usize>, usize, usize)> {
    let start_index = document
        .blocks
        .iter()
        .position(|block| block.id == anchor.block_id)?;
    let end_block_id = anchor.end_block_id.unwrap_or(anchor.block_id);
    let end_index = document
        .blocks
        .iter()
        .position(|block| block.id == end_block_id)?;
    if end_index < start_index {
        return None;
    }
    let start_text = document.blocks[start_index].text.as_str();
    let end_text = document.blocks[end_index].text.as_str();
    if anchor.start > start_text.len()
        || !start_text.is_char_boundary(anchor.start)
        || anchor.end > end_text.len()
        || !end_text.is_char_boundary(anchor.end)
    {
        return None;
    }
    let block_start = |index: usize| {
        document.blocks[..index]
            .iter()
            .fold(0usize, |total, block| {
                total
                    .saturating_add(block.text.as_str().len())
                    .saturating_add(1)
            })
    };
    let start = block_start(start_index).checked_add(anchor.start)?;
    let end = block_start(end_index).checked_add(anchor.end)?;
    (start <= end).then_some((start..end, start_index, end_index))
}

fn kind_label(kind: RevisionKind) -> &'static str {
    match kind {
        RevisionKind::Insertion => "Insertion",
        RevisionKind::Deletion => "Deletion",
        RevisionKind::Formatting => "Formatting",
    }
}

pub(crate) fn excerpt(text: &str) -> String {
    let mut chars = text.chars();
    let prefix: String = chars.by_ref().take(120).collect();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}

fn payload_style(
    payload: Option<&RevisionPayload>,
    paragraph_format: Option<loom_writer_core::RevisionParagraphFormat<'_>>,
) -> String {
    let mut styles = Vec::new();
    if let Some(payload) = payload {
        styles.extend(payload.runs.iter().take(3).map(|run| {
            let style = &run.style;
            let mut values = vec![format!(
                "{} pt, weight {}",
                style.font_size,
                style.weight.numeric()
            )];
            if !style.font_family.is_empty() {
                values.push(style.font_family.clone());
            }
            if style.italic {
                values.push("italic".to_owned());
            }
            if style.underline {
                values.push("underline".to_owned());
            }
            if style.strikethrough {
                values.push("strikethrough".to_owned());
            }
            if style.superscript {
                values.push("superscript".to_owned());
            }
            if style.subscript {
                values.push("subscript".to_owned());
            }
            if let Some(color) = &style.color {
                values.push(format!("color {color}"));
            }
            format!("{}–{} bytes: {}", run.start, run.end, values.join(", "))
        }));
        if payload.runs.len() > 3 {
            styles.push("…".to_owned());
        } else if payload.runs.is_empty() {
            styles.push("inherited character formatting".to_owned());
        }
    }
    if let Some(paragraph_format) = paragraph_format {
        let style = paragraph_format.style;
        styles.push(format!(
            "paragraph kind {}, alignment {:?}, direction {:?}, spacing before {:.2} pt / after {:.2} pt, line spacing {:.2}×, indents left {:.2} pt / right {:.2} pt / first line {:.2} pt, break {:?}, list level {}",
            paragraph_format.kind,
            style.alignment,
            style.direction,
            style.space_before,
            style.space_after,
            style.line_spacing,
            style.left_indent,
            style.right_indent,
            style.first_line_indent,
            style.break_rule,
            style.list_level,
        ));
    }
    if styles.is_empty() {
        String::new()
    } else {
        styles.join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions_tests::{test_state, text_document};
    use crate::{apply_state, wire_writer_shared_callbacks};

    fn launched(document: WriterDocument) -> (WriterApp, Rc<GuiState>) {
        let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
        let (app, state) = test_state(document, dialogs);
        wire_writer_shared_callbacks(&app, &state, None);
        apply_state(&app, &state);
        (app, state)
    }

    fn replacement_document() -> WriterDocument {
        let before = text_document("cat");
        let mut next = before.clone();
        next.tracked_changes.set_recording(true);
        next.replace_editor_text_at("dog", Some(3))
            .expect("replace text");
        next.record_user_edit_from(&before, Some(3))
            .expect("record replacement");
        next
    }

    #[test]
    fn review_menu_opens_the_panel_and_recording_toggle_is_undoable() {
        let (app, state) = launched(text_document("text"));
        let review = app.global::<WriterReviewState>();
        assert!(!review.get_panel_open());
        assert!(crate::toolbar_commands::dispatch(
            &app,
            "writer.review.open"
        ));
        assert!(review.get_panel_open());

        review.invoke_toggle_recording();
        assert!(state.current.borrow().tracked_changes.recording());
        app.invoke_undo();
        assert!(!state.current.borrow().tracked_changes.recording());
        app.invoke_redo();
        assert!(state.current.borrow().tracked_changes.recording());
    }

    #[test]
    fn review_navigation_selects_each_exact_anchor_and_shows_deleted_text() {
        let (app, state) = launched(replacement_document());
        let review = app.global::<WriterReviewState>();
        assert!(crate::toolbar_commands::dispatch(
            &app,
            "writer.review.open"
        ));
        assert!(review.get_panel_open());
        assert_eq!(review.get_revision_count(), 2);
        assert_eq!(review.get_current_kind().as_str(), "Deletion");
        assert_eq!(state.current.borrow().selection(), TextSelection::caret(0));
        assert!(review.get_current_location().as_str().contains("Page 1"));
        assert!(review.get_current_before().as_str().contains("cat"));

        review.invoke_navigate(1);
        assert_eq!(review.get_current_kind().as_str(), "Insertion");
        assert_eq!(
            state.current.borrow().selection(),
            TextSelection::range(0, 3)
        );
        assert_eq!(
            (app.get_selection_anchor(), app.get_selection_focus()),
            (0, 3)
        );
        assert!(review.get_current_location().as_str().contains("Page 1"));
        assert!(review.get_current_location().as_str().contains("bytes 0–3"));

        review.invoke_navigate(1);
        assert_eq!(review.get_current_kind().as_str(), "Deletion");
        assert_eq!(state.current.borrow().selection(), TextSelection::caret(0));
        assert!(review.get_current_before().as_str().contains("cat"));
        assert!(review.get_current_location().as_str().contains("byte 0"));
    }

    #[test]
    fn resolving_current_changes_uses_one_undoable_history_entry() {
        let (app, state) = launched(replacement_document());
        let review = app.global::<WriterReviewState>();
        review.invoke_navigate(1);
        review.invoke_resolve_current(false);
        assert!(state.current.borrow().editor_text().is_empty());
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 1);
        app.invoke_undo();
        assert_eq!(state.current.borrow().editor_text(), "dog");
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 2);

        review.invoke_navigate(1);
        review.invoke_resolve_current(true);
        assert_eq!(state.current.borrow().editor_text(), "dog");
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 1);
        app.invoke_undo();
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 2);
    }

    #[test]
    fn resolving_all_accept_or_reject_is_one_undoable_transaction() {
        let (app, state) = launched(replacement_document());
        let review = app.global::<WriterReviewState>();
        review.invoke_resolve_all(true);
        assert_eq!(state.current.borrow().editor_text(), "dog");
        assert!(state
            .current
            .borrow()
            .tracked_changes
            .revisions()
            .is_empty());
        assert_eq!(state.history.borrow().undo_len(), 1);
        app.invoke_undo();
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 2);
        app.invoke_redo();
        assert!(state
            .current
            .borrow()
            .tracked_changes
            .revisions()
            .is_empty());
        app.invoke_undo();
        review.set_author("New author after undo".into());
        review.invoke_set_author(review.get_author());
        assert!(!app.get_can_redo());

        let (app, state) = launched(replacement_document());
        app.global::<WriterReviewState>().invoke_resolve_all(false);
        assert_eq!(state.current.borrow().editor_text(), "cat");
        assert!(state
            .current
            .borrow()
            .tracked_changes
            .revisions()
            .is_empty());
        assert_eq!(state.history.borrow().undo_len(), 1);
        app.invoke_undo();
        assert_eq!(state.current.borrow().editor_text(), "dog");
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 2);
        app.invoke_redo();
        assert_eq!(state.current.borrow().editor_text(), "cat");
        assert!(state
            .current
            .borrow()
            .tracked_changes
            .revisions()
            .is_empty());
    }

    #[test]
    fn bulk_resolution_failure_keeps_the_current_document_and_history_unchanged() {
        let mut invalid = replacement_document();
        invalid
            .replace_editor_text("bad")
            .expect("make revision payload stale");
        let (app, state) = launched(invalid);
        let before = state.current.borrow().clone();
        let history_before = state.history.borrow().undo_len();

        app.global::<WriterReviewState>().invoke_resolve_all(true);

        assert_eq!(*state.current.borrow(), before);
        assert_eq!(state.history.borrow().undo_len(), history_before);
        assert!(app
            .get_status_right()
            .as_str()
            .contains("Tracked changes could not be resolved:"));
    }

    #[test]
    fn overlapping_single_resolutions_require_the_dependency_order() {
        let (app, state) = launched(replacement_document());
        let review = app.global::<WriterReviewState>();
        let before = state.current.borrow().clone();
        assert_eq!(before.tracked_changes.revisions().len(), 2);

        // The older deletion cannot be rejected while its newer replacement
        // insertion is still unresolved. Failure must not commit any state.
        review.invoke_resolve_current(false);
        assert_eq!(*state.current.borrow(), before);
        assert_eq!(state.history.borrow().undo_len(), 0);
        assert!(app
            .get_status_right()
            .as_str()
            .contains("newer overlapping"));

        // Reject the newer insertion first, then reject the deletion to restore
        // the original text. Each user resolution is its own history step.
        review.invoke_navigate(1);
        review.invoke_resolve_current(false);
        assert_eq!(state.current.borrow().editor_text(), "");
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 1);
        review.invoke_resolve_current(false);
        assert_eq!(state.current.borrow().editor_text(), "cat");
        assert!(state
            .current
            .borrow()
            .tracked_changes
            .revisions()
            .is_empty());
        assert_eq!(state.history.borrow().undo_len(), 2);

        app.invoke_undo();
        assert_eq!(state.current.borrow().editor_text(), "");
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 1);
        app.invoke_undo();
        assert_eq!(*state.current.borrow(), before);

        // Accepting a newer replacement before its deletion is also blocked;
        // accepting the older deletion first then accepts the insertion.
        let (app, state) = launched(replacement_document());
        let review = app.global::<WriterReviewState>();
        let before = state.current.borrow().clone();
        review.invoke_navigate(1);
        review.invoke_resolve_current(true);
        assert_eq!(*state.current.borrow(), before);
        assert_eq!(state.history.borrow().undo_len(), 0);

        review.invoke_navigate(-1);
        review.invoke_resolve_current(true);
        assert_eq!(state.current.borrow().editor_text(), "dog");
        assert_eq!(state.current.borrow().tracked_changes.revisions().len(), 1);
        review.invoke_resolve_current(true);
        assert_eq!(state.current.borrow().editor_text(), "dog");
        assert!(state
            .current
            .borrow()
            .tracked_changes
            .revisions()
            .is_empty());
        assert_eq!(state.history.borrow().undo_len(), 2);
    }

    #[test]
    fn formatting_review_includes_before_and_after_style_details() {
        let mut before = loom_text::CharacterStyle::default();
        let mut after = before.clone();
        after.italic = true;
        before.underline = true;
        let payload = |style| RevisionPayload {
            text: "sample".to_owned(),
            runs: vec![loom_text::StyleRun {
                start: 0,
                end: 6,
                style,
            }],
        };

        assert!(payload_style(Some(&payload(before)), None).contains("underline"));
        assert!(payload_style(Some(&payload(after)), None).contains("italic"));
        assert_eq!(kind_label(RevisionKind::Formatting), "Formatting");
    }

    #[test]
    fn paragraph_format_review_includes_kind_alignment_direction_and_spacing() {
        let mut before = text_document("sample");
        before.tracked_changes.set_recording(true);
        let mut after = before.clone();
        let block = &mut after.blocks[0];
        block.kind = "heading2".into();
        block.style.alignment = loom_text::Alignment::Center;
        block.style.direction = loom_text::ParagraphDirection::RightToLeft;
        block.style.space_before = 12.0;
        block.style.space_after = 4.0;
        block.style.line_spacing = 1.3;
        block.style.first_line_indent = 3.0;
        after
            .record_user_edit_from(&before, Some(6))
            .expect("record paragraph formatting");

        let (app, _) = launched(after);
        let review = app.global::<WriterReviewState>();
        let before_style = review.get_current_before_style().to_string();
        let after_style = review.get_current_after_style().to_string();
        assert!(before_style.contains("paragraph kind paragraph"));
        assert!(before_style.contains("alignment Start"));
        assert!(before_style.contains("direction Auto"));
        assert!(after_style.contains("paragraph kind heading2"));
        assert!(after_style.contains("alignment Center"));
        assert!(after_style.contains("direction RightToLeft"));
        assert!(after_style.contains("spacing before 12.00 pt / after 4.00 pt"));
        assert!(after_style.contains("line spacing 1.30×"));
        assert!(after_style.contains("first line 3.00 pt"));
    }

    #[test]
    fn author_identity_is_editable_undoable_and_persistent() {
        let (app, state) = launched(text_document("text"));
        let review = app.global::<WriterReviewState>();
        assert_eq!(
            review.get_author().as_str(),
            state.current.borrow().tracked_changes.author()
        );

        review.set_author("A. Writer".into());
        review.invoke_set_author(review.get_author());
        assert_eq!(state.current.borrow().tracked_changes.author(), "A. Writer");
        app.invoke_undo();
        assert_eq!(state.current.borrow().tracked_changes.author(), "Anonymous");
        app.invoke_redo();
        assert_eq!(state.current.borrow().tracked_changes.author(), "A. Writer");

        review.set_author("   ".into());
        review.invoke_set_author(review.get_author());
        assert_eq!(state.current.borrow().tracked_changes.author(), "A. Writer");
        assert_eq!(review.get_author().as_str(), "A. Writer");

        let bytes = loom_writer_core::save_document(&state.current.borrow()).expect("save");
        let reopened = loom_writer_core::load_document(&bytes).expect("reopen");
        assert_eq!(reopened.tracked_changes.author(), "A. Writer");
    }

    #[test]
    fn author_changes_apply_to_new_typing_and_native_reopen_keeps_unresolved_revisions() {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT_FILE: AtomicU64 = AtomicU64::new(0);
        let mut document = text_document("ab");
        document.set_selection(TextSelection::caret(2));
        let (app, state) = launched(document);
        let review = app.global::<WriterReviewState>();
        review.set_author("A. Writer".into());
        review.invoke_set_author(review.get_author());
        review.invoke_toggle_recording();

        app.invoke_document_edited("abX".into(), 3, 3);
        review.set_author("B. Writer".into());
        review.invoke_set_author(review.get_author());
        app.invoke_document_edited("abXY".into(), 4, 4);

        let pending = state.current.borrow().tracked_changes.revisions().to_vec();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].author, "A. Writer");
        assert_eq!(pending[1].author, "B. Writer");
        assert_eq!(pending[0].after.as_ref().unwrap().text, "X");
        assert_eq!(pending[1].after.as_ref().unwrap().text, "Y");

        review.set_author("   ".into());
        review.invoke_set_author(review.get_author());
        assert_eq!(review.get_author().as_str(), "B. Writer");
        assert_eq!(state.current.borrow().tracked_changes.author(), "B. Writer");
        assert_eq!(state.current.borrow().tracked_changes.revisions(), pending);

        let path = std::env::temp_dir().join(format!(
            "loom-writer-revisions-{}-{}.loomdoc",
            std::process::id(),
            NEXT_FILE.fetch_add(1, Ordering::Relaxed)
        ));
        crate::save_flow::save_file(&path, &state.current.borrow()).expect("save native document");
        let reopened = crate::save_flow::load_file(&path).expect("reopen native document");
        std::fs::remove_file(path).expect("remove native document fixture");
        assert_eq!(reopened.tracked_changes.author(), "B. Writer");
        assert!(reopened.tracked_changes.recording());
        assert_eq!(reopened.tracked_changes.revisions(), pending);
    }

    #[test]
    fn deleted_text_remains_full_in_review_state() {
        let deleted = format!("Deleted paragraph: {}", "removed words ".repeat(24));
        let mut document = text_document("replacement");
        let block_id = document.blocks[0].id;
        document.tracked_changes.set_recording(true);
        document
            .record_revision(
                RevisionKind::Deletion,
                block_id,
                0..0,
                Some(RevisionPayload::plain(deleted.clone())),
                None,
            )
            .expect("deletion")
            .expect("recording is on");
        let (app, _) = launched(document);
        let review = app.global::<WriterReviewState>();
        assert!(deleted.len() > 120);
        assert_eq!(review.get_current_before().as_str(), deleted);
    }
}
