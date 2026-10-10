use super::actions_tests::{test_state, text_document};
use super::*;

#[test]
fn list_style_kind_mutation_undoes_and_persists() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("First\nSecond\nThird"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    let app_ref = app.as_weak();
    // Apply a bulleted list to the whole document the way the inspector does.
    let _ = &app_ref;
    {
        let state = state.clone();
        let app = app_ref.upgrade().unwrap();
        let mut next = state.current.borrow().clone();
        let end = next.editor_text().len();
        let sel = TextSelection::range(0, end);
        let selection = DocumentSelection::range(sel.anchor, sel.focus);
        set_selection_list_style(&mut next, selection, 1);
        next.set_selection(sel);
        apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
    }
    {
        let current = state.current.borrow();
        assert!(
            current
                .blocks
                .iter()
                .all(|block| block.kind == "list-bulleted"),
            "all blocks should become bulleted list items"
        );
    }
    // Undo returns every block to a plain paragraph. The registry gates
    // undo on history state, so refresh enablement after the mutation.
    {
        let mut registry = state.registry.lock().unwrap();
        let history = state.history.borrow();
        sync_writer_registry_enablement(&mut registry, &state.current.borrow(), &history);
    }
    app.invoke_undo();
    {
        let current = state.current.borrow();
        assert!(
            current.blocks.iter().all(|block| block.kind == "paragraph"),
            "undo should restore plain paragraphs"
        );
    }
}

#[test]
fn numbered_lists_render_sequential_markers_and_markdown() {
    let mut document = text_document("One\nTwo\nplain\nThree");
    for index in [0usize, 1, 3] {
        document.blocks[index].kind = "list-numbered".to_string();
    }
    let (rows, _, _) = writer_render_projection(&document, PageViewport::default());
    let markers: Vec<String> = rows
        .iter()
        .filter(|row| !row.marker.is_empty())
        .map(|row| row.marker.to_string())
        .collect();
    assert_eq!(
        markers,
        vec!["1.", "2.", "1."],
        "numbering restarts after the plain paragraph"
    );

    let markdown = document.to_markdown();
    assert!(markdown.contains("1. One"));
    assert!(markdown.contains("2. Two"));
    assert!(markdown.contains("1. Three"));
    assert!(!markdown.contains("3. plain"));

    let mut bulleted = text_document("Alpha");
    bulleted.blocks[0].kind = "list-bulleted".to_string();
    assert!(bulleted.to_markdown().contains("- Alpha"));
}

#[test]
fn list_documents_round_trip_through_package() {
    let mut document = text_document("Item");
    document.blocks[0].kind = "list-bulleted".to_string();
    let bytes = loom_writer_core::save_document(&document).expect("save");
    let reopened = loom_writer_core::load_document(&bytes).expect("load");
    assert_eq!(reopened.blocks[0].kind, "list-bulleted");
}

#[test]
fn comments_add_resolve_delete_undo_and_round_trip() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("Hello anchored world"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);

    // Select "anchored" and add a comment on it.
    let mut next = state.current.borrow().clone();
    let block_len = next.blocks[0].text.len_bytes();
    next.set_selection(TextSelection::range(6, 6 + block_len.min(14)));
    apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
    app.invoke_add_comment(SharedString::from("Nice phrase"));
    {
        let current = state.current.borrow();
        assert_eq!(current.comments.len(), 1);
        assert_eq!(current.comments[0].body, "Nice phrase");
        assert_eq!(current.comments[0].block_id, current.blocks[0].id);
        assert_eq!(current.comments[0].start, 6);
    }
    assert_eq!(app.get_comment_entries().iter().count(), 1);

    // Resolve, then delete.
    let id = state.current.borrow().comments[0].id.clone();
    app.invoke_set_comment_resolved(SharedString::from(id.clone()), true);
    assert!(state.current.borrow().comments[0].resolved);
    app.invoke_delete_comment(SharedString::from(id));
    assert!(state.current.borrow().comments.is_empty());

    // Re-add and undo: the comment add participates in history.
    app.invoke_add_comment(SharedString::from("Second"));
    app.invoke_undo();
    assert!(
        state.current.borrow().comments.is_empty(),
        "undo should remove the comment"
    );

    // A fresh comment survives the package round-trip.
    app.invoke_add_comment(SharedString::from("Persisted"));
    let bytes = loom_writer_core::save_document(&state.current.borrow()).expect("save");
    let reopened = loom_writer_core::load_document(&bytes).expect("load");
    assert_eq!(reopened.comments.len(), 1);
    assert_eq!(reopened.comments[0].body, "Persisted");
}

#[test]
fn comment_projection_keeps_the_same_thread_id_across_wrapped_utf8_ranges() {
    let text = "word 🌍 ".repeat(700);
    let mut document = text_document(&text);
    let block_id = document.blocks[0].id;
    let start = text.find("🌍").expect("emoji anchor");
    let end = text.rfind("🌍").expect("last emoji anchor") + "🌍".len();
    let comment_id = document
        .add_comment_thread(block_id, start, end, "Review this long passage")
        .expect("add comment");
    document.set_selection(TextSelection::range(0, 2));

    let (rows, selection_rects, comment_rects) =
        writer_render_projection(&document, PageViewport::default());

    assert!(!rows.is_empty());
    assert_eq!(
        selection_rects.len(),
        1,
        "active selection stays independent"
    );
    assert!(
        comment_rects.len() > 1,
        "comment range follows wrapped lines"
    );
    assert!(
        comment_rects.iter().any(|rect| rect.page_index > 0),
        "comment range crosses into later pages"
    );
    assert!(comment_rects
        .iter()
        .all(|rect| rect.comment_id == comment_id));
    assert_eq!(comment_rects.iter().filter(|rect| rect.marker).count(), 1);
    assert_eq!(
        comment_selection_range(&document, &comment_id),
        Some((start, end)),
        "the inspector action resolves to the same UTF-8 byte range as the page highlight"
    );
    assert_eq!(document.selection(), TextSelection::range(0, 2));
}

#[test]
fn navigating_to_comment_selects_the_anchor_and_opens_the_compact_inspector() {
    let text = "word 🌍 ".repeat(700);
    let mut document = text_document(&text);
    let block_id = document.blocks[0].id;
    let start = text.rfind("🌍").expect("late emoji anchor");
    let end = start + "🌍".len();
    let comment_id = document
        .add_comment_thread(block_id, start, end, "Review this phrase")
        .expect("add comment");
    let (app, state) = test_state(
        document,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None])),
    );
    app.set_compact_inspector_layout(true);
    app.set_inspector_available(true);
    wire_writer_shared_callbacks(&app, &state, None);

    app.invoke_navigate_comment(SharedString::from(comment_id.clone()));

    assert!(
        app.get_show_inspector(),
        "comment review opens the inspector"
    );
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::range(start, end),
        "the page selection matches the UTF-8 comment anchor"
    );
    assert!(
        state.viewport.borrow().scroll_y > 0.0,
        "a comment on a later page scrolls into view"
    );
    assert_eq!(
        app.get_page_scroll_y(),
        state.viewport.borrow().scroll_y,
        "the visible page and controller share the same scroll position"
    );
    assert!(app
        .get_comment_rects()
        .iter()
        .all(|rect| rect.comment_id == comment_id));
}

#[test]
fn compact_comment_review_keeps_anchor_for_formatting_and_escape() {
    let text = "Keep selection after opening review";
    let mut document = text_document(text);
    let block_id = document.blocks[0].id;
    let start = text.find("selection").expect("selected word");
    let end = start + "selection".len();
    let comment_id = document
        .add_comment_thread(block_id, start, end, "Keep this phrase")
        .expect("add anchored comment");
    let (app, state) = test_state(
        document,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None])),
    );
    app.set_compact_inspector_layout(true);
    app.set_inspector_available(true);
    wire_writer_shared_callbacks(&app, &state, None);
    wire_writer_inspector_toggle(&app, &state, None);

    app.window().set_size(slint::PhysicalSize::new(1024, 720));
    let _ = loom_test_support::snapshot_component(&app, 1024.0, 720.0, 1.0)
        .expect("show Writer before comment navigation");
    app.invoke_navigate_comment(SharedString::from(comment_id.clone()));
    assert!(app.get_show_inspector(), "comment navigation opens review");
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::range(start, end)
    );

    // A format action in the focused inspector must still use the document's
    // selected comment anchor rather than collapsing the selection.
    app.invoke_toggle_bold();
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::range(start, end)
    );
    assert!(
        formatting_state_for_selection(
            &state.current.borrow(),
            DocumentSelection::range(start, end),
        )
        .bold
    );

    // Escape must work immediately after comment navigation opens the review.
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });
    assert!(!app.get_show_inspector(), "Escape closes compact review");

    // Hiding the inspector returns focus to Format; Space on that focused
    // toolbar button reopens it through the production callback.
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Space.into(),
        });
    assert!(
        app.get_show_inspector(),
        "focus returns to the Format trigger"
    );
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::range(start, end)
    );

    app.invoke_add_comment(SharedString::from("Follow-up comment"));
    {
        let current = state.current.borrow();
        assert_eq!(current.comments.len(), 2);
        assert_eq!(current.comments[1].body, "Follow-up comment");
        assert_eq!(current.comments[1].block_id, block_id);
        assert_eq!(
            (current.comments[1].start, current.comments[1].end),
            (start, end)
        );
    }
    app.invoke_navigate_comment(SharedString::from(comment_id));
    assert_eq!(
        state.current.borrow().selection(),
        TextSelection::range(start, end),
        "navigation returns to the same anchored phrase after adding a comment"
    );
}

#[test]
fn comment_anchor_rebases_with_edit_and_is_restored_by_undo_redo() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let mut document = text_document("Hello world");
    let block_id = document.blocks[0].id;
    document
        .add_comment_thread(block_id, 6, 11, "Review this word")
        .expect("add comment");
    let (app, state) = test_state(document, dialogs);
    wire_writer_shared_callbacks(&app, &state, None);

    let mut next = state.current.borrow().clone();
    next.set_selection(TextSelection::caret(0));
    next.replace_selection_text("New ").expect("insert text");
    apply_with_history(&app, &state, next, HistoryKind::DocumentAction);
    let edited = state.current.borrow().clone();
    let comment = &edited.comments[0];
    let block = edited.get(comment.block_id).expect("edited block");
    assert_eq!(&block.text.as_str()[comment.start..comment.end], "world");

    app.invoke_undo();
    let undone = state.current.borrow().clone();
    let comment = &undone.comments[0];
    let block = undone.get(comment.block_id).expect("undone block");
    assert_eq!(&block.text.as_str()[comment.start..comment.end], "world");
    assert_eq!((comment.start, comment.end), (6, 11));

    app.invoke_redo();
    let redone = state.current.borrow().clone();
    let comment = &redone.comments[0];
    let block = redone.get(comment.block_id).expect("redone block");
    assert_eq!(&block.text.as_str()[comment.start..comment.end], "world");
    assert_eq!((comment.start, comment.end), (10, 15));
}

#[test]
fn comment_on_caret_anchors_whole_block_and_rejects_empty_body() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("Whole block"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    app.invoke_add_comment(SharedString::from("   "));
    assert!(state.current.borrow().comments.is_empty());
    app.invoke_add_comment(SharedString::from("On the block"));
    let current = state.current.borrow();
    assert_eq!(current.comments.len(), 1);
    assert_eq!(current.comments[0].start, 0);
    assert_eq!(current.comments[0].end, current.blocks[0].text.len_bytes());
}

#[test]
fn insert_table_is_undoable_and_persists() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("First"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);

    app.invoke_insert_table();
    {
        let current = state.current.borrow();
        assert_eq!(current.blocks.len(), 2);
        assert_eq!(current.blocks[1].kind, loom_writer_core::TABLE_BLOCK_KIND);
    }
    // Undo removes the table block.
    app.invoke_undo();
    assert_eq!(state.current.borrow().blocks.len(), 1);

    // Re-insert and verify the package round-trip preserves the table.
    app.invoke_insert_table();
    let bytes = loom_writer_core::save_document(&state.current.borrow()).expect("save");
    let reopened = loom_writer_core::load_document(&bytes).expect("load");
    assert!(reopened
        .blocks
        .iter()
        .any(|block| block.kind == loom_writer_core::TABLE_BLOCK_KIND));
    let table = reopened
        .table_from_block(
            reopened
                .blocks
                .iter()
                .find(|block| block.kind == loom_writer_core::TABLE_BLOCK_KIND)
                .expect("table block")
                .id,
        )
        .expect("table parses");
    assert_eq!(table.rows.len(), loom_writer_core::INSERT_ROWS);
}

#[test]
fn table_and_comments_reachable_through_keyboard_command_paths() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("Body"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);

    // The command registry is the single keyboard surface: palette search
    // must find the table command and dispatch must insert a table.
    {
        let registry = state.registry.lock().unwrap();
        let hits = registry.search("table");
        assert!(
            hits.iter()
                .any(|(spec, _)| spec.id.as_str() == "writer.table.insert"),
            "palette search should find the Insert Table command"
        );
    }
    assert!(dispatch_command(&app, "writer.table.insert"));
    assert_eq!(
        state.current.borrow().blocks[1].kind,
        loom_writer_core::TABLE_BLOCK_KIND
    );

    // Undo through the same registry surface removes it again.
    assert!(dispatch_command(&app, "edit.undo"));
    assert_eq!(state.current.borrow().blocks.len(), 1);
}

#[test]
fn inspector_commands_expose_a11y_labels() {
    // The foundation contract requires every icon-only action to carry an
    // accessible label; the Writer toolbar's icon buttons all set one.
    let source = include_str!("../ui/toolbar.slint");
    for icon_button in source.split("LoomIconButton {").skip(1) {
        let block: String = icon_button
            .lines()
            .take_while(|line| !line.contains('}'))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            block.contains("label:") || block.contains("accessible-label:"),
            "toolbar icon button missing accessible label: {block}"
        );
    }
}

#[test]
fn closing_a_dirty_document_asks_before_the_window_goes_away() {
    let (app, state) = test_state(
        text_document("saved"),
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
    );
    wire_close_guard(&app, &state);
    *state.current.borrow_mut() = text_document("edited since the last save");

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);

    assert!(
        app.get_save_changes_open(),
        "the unsaved-changes dialog must open"
    );
    assert_eq!(
        state.pending_replacement.get(),
        Some(PendingReplacement::CloseWindow)
    );
}

#[test]
fn closing_a_clean_document_does_not_prompt() {
    let (app, state) = test_state(
        text_document("saved"),
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
    );
    wire_close_guard(&app, &state);

    app.window()
        .dispatch_event(slint::platform::WindowEvent::CloseRequested);

    assert!(!app.get_save_changes_open());
    assert_eq!(state.pending_replacement.get(), None);
}

#[test]
fn adjacent_runs_that_render_alike_merge_into_one_span() {
    // Regression: runs that differ only in font size or family (attributes the
    // page markup does not express) were each wrapped separately, so delimiters
    // met as `****` / `~~~~` and the page showed literal asterisks and tildes.
    let mut document = text_document("Write, format, and export.");
    set_selection_bold(&mut document, DocumentSelection::range(0, 13), true);
    set_selection_strikethrough(&mut document, DocumentSelection::range(0, 13), true);
    let mut first = document.blocks[0].runs[0].clone();
    first.end = 6;
    let mut second = document.blocks[0].runs[0].clone();
    second.start = 6;
    second.style.font_size += 2.0;
    document.blocks[0].runs = vec![first, second];

    let markup = writer_render_markup(&document.blocks[0]);

    assert!(
        !markup.contains("****") && !markup.contains("~~~~"),
        "spans that render alike must merge, got {markup:?}"
    );
    assert!(
        markup.starts_with("~~**Write, format**~~"),
        "one merged bold+strike span expected, got {markup:?}"
    );
}

#[test]
fn styled_spans_keep_edge_whitespace_outside_their_delimiters() {
    // Regression: a styled span ending in a space produced `**text **`, which
    // CommonMark cannot close, so the page showed literal asterisks.
    let mut document = text_document("open .loomdoc packages");
    set_selection_bold(&mut document, DocumentSelection::range(0, 5), true);
    set_selection_italic(&mut document, DocumentSelection::range(0, 5), true);
    set_selection_bold(&mut document, DocumentSelection::range(5, 13), true);

    let markup = writer_render_markup(&document.blocks[0]);

    assert_eq!(markup, "***open*** **.loomdoc** packages");
}

/// After a successful Save or Save As the window must stop showing the unsaved
/// marker; it used to stay lit until the next edit.
#[test]
fn successful_save_clears_the_unsaved_marker() {
    let path = std::env::temp_dir().join(format!(
        "loom-writer-dirty-after-save-{}.loomdoc",
        std::process::id()
    ));
    let dialogs: Rc<dyn FileDialogService> = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [],
        [Some(path.clone())],
    ));
    let (app, state) = test_state(text_document("before"), dialogs);
    state
        .current
        .borrow_mut()
        .replace_paragraphs("after the edit");
    apply_state(&app, &state);
    assert!(app.get_document_dirty(), "an edited document is dirty");

    assert!(save_current_document(&app, &state, true).expect("save succeeds"));

    assert!(!app.get_document_dirty(), "a saved document is not dirty");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_recovered_draft_reads_as_unsaved_but_a_fresh_start_does_not() {
    // Nothing recovered: the blank start is both the document and the baseline.
    let (document, saved) = startup_documents(None, None, None).expect("fresh start");
    assert!(document_content_equal(&document, &saved));

    // Recovered text that differs from the baseline is unsaved work.
    let mut draft = sample_document();
    draft.replace_paragraphs("Words that only exist in a recovered draft");
    let (document, saved) = startup_documents(Some(untitled_draft(draft.clone())), None, None)
        .expect("recovered start");
    assert!(
        document_content_equal(&document, &draft),
        "the draft is what opens"
    );
    assert!(
        !document_content_equal(&document, &saved),
        "a recovered draft must read as unsaved, so closing asks first"
    );

    // A recovered draft identical to the baseline has nothing to lose.
    let (document, saved) =
        startup_documents(Some(untitled_draft(blank_startup_document())), None, None)
            .expect("identical draft");
    assert!(document_content_equal(&document, &saved));

    // The baseline follows the requested template, not always the sample.
    let (_, report_saved) =
        startup_documents(Some(untitled_draft(draft)), None, Some(TemplateId::Report))
            .expect("template baseline");
    assert!(document_content_equal(
        &report_saved,
        &template_document(TemplateId::Report)
    ));
}

/// A recovered draft that was never tied to a file.
fn untitled_draft(document: WriterDocument) -> crate::recovery_draft::RecoveredDraft {
    crate::recovery_draft::RecoveredDraft {
        document,
        source: None,
    }
}

#[test]
fn a_recovered_draft_is_compared_with_the_file_it_came_from() {
    let dir = std::env::temp_dir().join(format!("loom-draft-baseline-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create test directory");
    let path = dir.join("Essay.loomdoc");
    let mut saved = WriterDocument::new("essay", "Essay");
    saved.replace_paragraphs("Saved on disk");
    std::fs::write(
        &path,
        loom_writer_core::save_document(&saved).expect("serialize saved file"),
    )
    .expect("write saved file");
    let mut edited = saved.clone();
    edited.replace_paragraphs("Edited, never saved");
    let draft = crate::recovery_draft::RecoveredDraft {
        document: edited.clone(),
        source: Some(path.clone()),
    };

    // A plain launch compares the draft with its file, not with the template.
    let (document, baseline) =
        startup_documents(Some(draft.clone()), None, Some(TemplateId::Report))
            .expect("plain launch");
    assert!(document_content_equal(&document, &edited));
    assert!(
        document_content_equal(&baseline, &saved),
        "the baseline is the saved file"
    );

    // Launching with the same file gives the same comparison.
    let open = path.to_string_lossy().into_owned();
    let (_, baseline) =
        startup_documents(Some(draft.clone()), Some(&open), None).expect("same file");
    assert!(document_content_equal(&baseline, &saved));

    // A file that has since disappeared still restores the draft, which reads
    // as unsaved; saving recreates the file.
    std::fs::remove_file(&path).expect("remove the saved file");
    let (document, baseline) =
        startup_documents(Some(draft), Some(&open), None).expect("draft for a missing file");
    assert!(document_content_equal(&document, &edited));
    assert!(!document_content_equal(&document, &baseline));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_save_prompt_says_closing_only_when_the_window_is_closing() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("Saved text"), dialogs);
    state.current.borrow_mut().replace_paragraphs("Edited text");
    assert!(document_is_dirty(&state));

    assert!(request_document_replacement(
        &app,
        &state,
        PendingReplacement::NewDocument
    ));
    assert!(!app.get_save_changes_closing(), "New says replacing");

    assert!(request_document_replacement(
        &app,
        &state,
        PendingReplacement::CloseWindow
    ));
    assert!(app.get_save_changes_closing(), "closing says closing");
}

#[test]
fn a_caret_drawn_at_an_offset_is_hit_at_the_same_offset() {
    use loom_text::{CharacterStyle, FontWeight, StyleRun};

    // Headings, wrapped body text and a bold run: the cases where layout,
    // selection geometry and hit-testing used to disagree about glyph widths.
    let mut document = WriterDocument::new("hit", "Hit");
    document.replace_paragraphs(
        "Heading words here\nWrite, format, and export documents. Your files stay on your computer as open loomdoc packages, and nothing is uploaded.\nSecond paragraph with more text, punctuation; and numbers 12345.",
    );
    document.blocks[0].kind = "heading1".into();
    document.blocks[1].runs.push(StyleRun {
        start: 7,
        end: 36,
        style: CharacterStyle {
            weight: FontWeight::Bold,
            ..Default::default()
        },
    });

    let style = document.page.page_style();
    let viewport = PageViewport {
        width: style.width_pt,
        height: style.height_pt,
        zoom: 1.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
    };
    let length = document.editor_text().len();
    let mut checked = 0;
    for offset in 0..=length {
        if !document.editor_text().is_char_boundary(offset) {
            continue;
        }
        document.set_selection(TextSelection::caret(offset));
        let layout = document.layout(&style, viewport).expect("layout");
        let base = layout.page_bounds[0];
        let caret = layout
            .selection_rects
            .iter()
            .find(|rect| rect.start == rect.end)
            .expect("a caret rectangle");
        let x = caret.rect.x - base.x - style.margin_left_pt;
        let y = caret.rect.y - base.y - style.margin_top_pt + caret.rect.height / 2.0;
        assert_eq!(
            writer_pointer_offset(&document, viewport, x, y),
            Some(offset),
            "a click on the caret drawn at offset {offset} must land on offset {offset}"
        );
        checked += 1;
    }
    assert!(
        checked > 150,
        "the whole text was exercised ({checked} offsets)"
    );
}

#[test]
fn double_and_triple_clicks_on_the_page_select_a_word_and_a_paragraph() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("Alpha beta gamma\nSecond paragraph"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);

    // Where the page draws the middle of "beta" (editor offset 8), as the
    // pointer sees it: relative to the content area.
    let (x, y) = {
        let mut document = state.current.borrow().clone();
        let style = document.page.page_style();
        let viewport = PageViewport {
            width: style.width_pt,
            height: style.height_pt,
            zoom: 1.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
        };
        document.set_selection(TextSelection::caret(8));
        let layout = document.layout(&style, viewport).expect("layout");
        let base = layout.page_bounds[0];
        let caret = layout
            .selection_rects
            .iter()
            .find(|rect| rect.start == rect.end)
            .expect("caret rectangle");
        (
            caret.rect.x - base.x - style.margin_left_pt,
            caret.rect.y - base.y - style.margin_top_pt + caret.rect.height / 2.0,
        )
    };
    let selection = || {
        let current = state.current.borrow();
        let selection = current.selection();
        (
            selection.anchor.min(selection.focus),
            selection.anchor.max(selection.focus),
        )
    };

    app.invoke_pointer_pressed(x, y, false);
    app.invoke_pointer_released(x, y);
    assert_eq!(selection(), (8, 8), "one click places the caret");

    app.invoke_pointer_pressed(x, y, false);
    app.invoke_pointer_released(x, y);
    assert_eq!(
        selection(),
        (6, 10),
        "a second click selects the word \"beta\""
    );

    app.invoke_pointer_pressed(x, y, false);
    app.invoke_pointer_released(x, y);
    assert_eq!(selection(), (0, 16), "a third click selects the paragraph");
}

#[test]
fn pasted_tabs_become_spaces_and_line_endings_become_paragraphs() {
    let mut document = WriterDocument::new("paste", "Paste");
    document.replace_paragraphs("one\r\n\ttabbed\rthird");
    assert_eq!(document.editor_text(), "one\n    tabbed\nthird");
    assert_eq!(document.blocks.len(), 3);
    assert!(!document.editor_text().contains('\t'));
}

#[test]
fn tab_inserts_spaces_at_the_caret_replaces_a_selection_and_undoes() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("abcdef"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);

    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::caret(3));
    app.invoke_insert_tab();
    assert_eq!(state.current.borrow().editor_text(), "abc    def");
    let selection = state.current.borrow().selection();
    assert_eq!(
        (selection.anchor, selection.focus),
        (7, 7),
        "caret follows the spaces"
    );

    state
        .current
        .borrow_mut()
        .set_selection(TextSelection::range(0, 3));
    app.invoke_insert_tab();
    assert_eq!(
        state.current.borrow().editor_text(),
        "        def",
        "a selection is replaced"
    );

    app.invoke_undo();
    assert_eq!(state.current.borrow().editor_text(), "abc    def");
}

#[test]
fn the_page_scrolls_to_keep_the_caret_in_view() {
    let long = (1..=120)
        .map(|n| {
            format!("Paragraph number {n} lorem ipsum dolor sit amet, consectetur adipiscing elit.")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document(&long), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    // The view height is measured from the laid-out window.
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    assert!(
        app.get_page_view_height() > 300.0,
        "the page area has a measured height"
    );
    assert_eq!(state.viewport.borrow().scroll_y, 0.0);

    // Jump to the end of the document (Ctrl+End): the page must follow.
    let end = state.current.borrow().editor_text().len() as i32;
    app.invoke_selection_changed(end, end);
    let scrolled = state.viewport.borrow().scroll_y;
    assert!(
        scrolled > 1000.0,
        "the end of a 120-paragraph document is far down, got {scrolled}"
    );
    assert_eq!(
        app.get_page_scroll_y(),
        scrolled,
        "the view is told the same offset"
    );

    // Back to the start: the page returns to the top.
    app.invoke_selection_changed(0, 0);
    assert_eq!(state.viewport.borrow().scroll_y, 0.0);

    // A caret that is already visible does not move the page.
    app.invoke_selection_changed(5, 5);
    assert_eq!(state.viewport.borrow().scroll_y, 0.0);
}

#[test]
fn page_down_and_page_up_move_the_caret_a_screen_and_shift_extends() {
    let long = (1..=120)
        .map(|n| {
            format!("Paragraph number {n} lorem ipsum dolor sit amet, consectetur adipiscing elit.")
        })
        .collect::<Vec<_>>()
        .join("\n");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document(&long), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    let focus = || state.current.borrow().selection().focus;
    let anchor = || state.current.borrow().selection().anchor;

    app.invoke_selection_changed(0, 0);
    app.invoke_page_move(1, false);
    let first = focus();
    assert!(
        first > 200,
        "Page Down moves well past the first lines, got {first}"
    );
    assert_eq!(anchor(), first, "a plain move collapses the selection");
    assert!(
        state.viewport.borrow().scroll_y > 0.0,
        "and the page scrolls with it"
    );

    app.invoke_page_move(1, false);
    assert!(focus() > first, "a second Page Down goes further");

    app.invoke_page_move(-1, false);
    app.invoke_page_move(-1, false);
    assert_eq!(focus(), 0, "two Page Ups return to the start");
    assert_eq!(state.viewport.borrow().scroll_y, 0.0);

    // Shift keeps the anchor and extends the selection.
    app.invoke_page_move(1, true);
    assert_eq!(anchor(), 0);
    assert!(focus() > 200);

    // At the end of the document Page Down stays at the end.
    let end = state.current.borrow().editor_text().len();
    app.invoke_selection_changed(end as i32, end as i32);
    app.invoke_page_move(1, false);
    assert_eq!(focus(), end);
}

#[test]
fn first_launch_opens_a_blank_untitled_document_with_a_hint_outside_it() {
    let (document, saved) = startup_documents(None, None, None).expect("fresh start");
    assert_eq!(document.title, "Untitled");
    assert!(
        document.blocks.iter().all(|block| block.text.is_empty()),
        "the user's document must start without any product text"
    );
    assert!(document_content_equal(&document, &saved));

    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(document, dialogs);
    apply_state(&app, &state);
    apply_startup_hint(&app, &state.current.borrow(), false);
    assert_eq!(app.get_doc_title().as_str(), "Untitled");
    assert!(!app.get_doc_content().as_str().contains("Welcome"));
    assert!(app.get_status_left().as_str().contains("Ctrl+K"));
    assert!(!document_is_dirty(&state), "a fresh blank is not unsaved");

    // A recovered draft keeps its own status and still reads as unsaved.
    let mut draft = blank_startup_document();
    draft.replace_paragraphs("recovered words");
    let (recovered, baseline) =
        startup_documents(Some(untitled_draft(draft)), None, None).expect("recovered");
    assert!(!document_content_equal(&recovered, &baseline));
}

#[test]
fn the_quick_start_sample_is_only_reachable_on_request() {
    let sample = sample_document();
    assert!(sample.title.contains("Quick Start"));
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(blank_startup_document(), dialogs);
    open_quick_start_sample(&app, &state);
    assert!(state.current.borrow().title.contains("Quick Start"));
    assert!(app.get_doc_content().as_str().contains("Welcome"));
    assert!(state.save_path.borrow().is_none());
}

fn comment_rect_spans(document: &WriterDocument) -> Vec<(f32, f32, f32)> {
    let (_, _, rects) = writer_render_projection(document, PageViewport::default());
    rects.iter().map(|r| (r.x, r.y, r.width)).collect()
}

fn commented_reference(text: &str, start: usize, end: usize) -> WriterDocument {
    let mut document = text_document(text);
    let block_id = document.blocks[0].id;
    document
        .add_comment_thread(block_id, start, end, "reference")
        .expect("reference comment");
    document
}

#[test]
fn typing_through_the_input_callback_keeps_the_page_highlight_on_the_same_words() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(commented_reference("Hello world", 6, 11), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    let before = comment_rect_spans(&state.current.borrow());
    assert_eq!(before.len(), 1);

    // Type "New " before the commented range, as the native editor reports it.
    app.invoke_document_edited("New Hello world".into(), 4, 4);
    {
        let document = state.current.borrow();
        let comment = &document.comments[0];
        assert_eq!((comment.start, comment.end), (10, 15));
        let reference = commented_reference("New Hello world", 10, 15);
        assert_eq!(
            comment_rect_spans(&document),
            comment_rect_spans(&reference),
            "the highlight covers 'world', not shifted letters"
        );
    }

    // Typing a copy of the word right before it moves the anchor, too.
    app.invoke_document_edited("New Hello world world".into(), 16, 16);
    {
        let document = state.current.borrow();
        let comment = &document.comments[0];
        assert_eq!((comment.start, comment.end), (16, 21));
        assert_eq!(
            comment_rect_spans(&document),
            comment_rect_spans(&commented_reference("New Hello world world", 16, 21))
        );
    }

    // Backspacing the copy away restores the earlier anchor.
    app.invoke_document_edited("New Hello world".into(), 10, 10);
    let document = state.current.borrow();
    assert_eq!(
        (document.comments[0].start, document.comments[0].end),
        (10, 15)
    );
}

#[test]
fn deleting_the_commented_text_orphans_the_thread_and_undo_redo_restore_it() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(commented_reference("Hello world", 6, 11), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);

    app.invoke_document_edited("Hello ".into(), 6, 6);
    {
        let document = state.current.borrow();
        assert!(document.comments[0].orphaned);
        assert!(comment_rect_spans(&document).is_empty());
    }
    let entries = app.get_comment_entries();
    assert!(entries.iter().next().expect("orphan stays listed").orphaned);

    app.invoke_undo();
    {
        let document = state.current.borrow();
        let comment = &document.comments[0];
        assert!(!comment.orphaned);
        assert_eq!((comment.start, comment.end), (6, 11));
        assert_eq!(comment_rect_spans(&document).len(), 1);
    }
    app.invoke_redo();
    assert!(state.current.borrow().comments[0].orphaned);
}
