//! Writer's anchored footnote editor actions.
use crate::{apply_with_history, GuiState, HistoryKind, WriterApp, WriterFootnoteState};
use loom_writer_core::{FootnoteEntry, WriterDocument};
use slint::{ComponentHandle, SharedString};

pub(crate) fn register(app: &WriterApp, state: &std::rc::Rc<GuiState>) {
    let global = app.global::<WriterFootnoteState>();
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_open_editor(move || {
            let Some(app) = app_ref.upgrade() else { return };
            let document = state.current.borrow();
            let note = selection_anchor(&document)
                .ok()
                .and_then(|(block_id, offset)| {
                    document
                        .footnotes
                        .iter()
                        .find(|note| note.block_id == block_id && note.offset == offset)
                        .cloned()
                });
            open_editor(&app, note);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_reference_selected(move |id| {
            let Some(app) = app_ref.upgrade() else { return };
            let note = id.parse::<u64>().ok().and_then(|id| {
                state
                    .current
                    .borrow()
                    .footnotes
                    .iter()
                    .find(|note| note.id == id)
                    .cloned()
            });
            if note.is_some() {
                open_editor(&app, note);
            } else {
                let global = app.global::<WriterFootnoteState>();
                global.set_selected_id(SharedString::new());
                global.set_draft_body(SharedString::new());
                global.set_error("The footnote is no longer available".into());
                global.set_dialog_open(false);
                app.set_status_right("The footnote is no longer available".into());
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_apply(move || {
            let Some(app) = app_ref.upgrade() else { return };
            let global = app.global::<WriterFootnoteState>();
            if !global.get_dialog_open() {
                return;
            }
            let body = global.get_draft_body().to_string();
            if body.trim().is_empty() {
                global.set_error("A footnote needs a body".into());
                return;
            }

            let selected_id = global.get_selected_id().to_string();
            let mut next = state.current.borrow().clone();
            let result = if selected_id.is_empty() {
                selection_anchor(&next)
                    .and_then(|(block_id, offset)| next.insert_footnote(block_id, offset, &body))
            } else {
                selected_id
                    .parse::<u64>()
                    .map_err(|_| "The selected footnote ID is invalid".to_string())
                    .and_then(|id| next.update_footnote_body(id, &body).map(|()| id))
            };
            match result {
                Ok(_) => {
                    if !apply_with_history(&app, &state, next, HistoryKind::DocumentAction) {
                        global.set_error(app.get_status_left());
                        return;
                    }
                    global.set_error(SharedString::new());
                    global.set_dialog_open(false);
                    app.set_status_right(
                        if selected_id.is_empty() {
                            "Footnote added"
                        } else {
                            "Footnote updated"
                        }
                        .into(),
                    );
                }
                Err(error) => global.set_error(error.into()),
            }
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        global.on_remove(move || {
            let Some(app) = app_ref.upgrade() else { return };
            let global = app.global::<WriterFootnoteState>();
            if !global.get_dialog_open() {
                return;
            }
            let id = match global.get_selected_id().parse::<u64>() {
                Ok(id) => id,
                Err(_) => {
                    global.set_error("Select a footnote to remove".into());
                    return;
                }
            };
            let mut next = state.current.borrow().clone();
            if !next.remove_footnote(id) {
                global.set_error("The footnote is no longer available".into());
                return;
            }
            if !apply_with_history(&app, &state, next, HistoryKind::DocumentAction) {
                global.set_error(app.get_status_left());
                return;
            }
            global.set_error(SharedString::new());
            global.set_dialog_open(false);
            app.set_status_right("Footnote removed".into());
        });
    }
    {
        let app_ref = app.as_weak();
        global.on_cancel(move || {
            if let Some(app) = app_ref.upgrade() {
                let global = app.global::<WriterFootnoteState>();
                global.set_selected_id(SharedString::new());
                global.set_draft_body(SharedString::new());
                global.set_error(SharedString::new());
                global.set_dialog_open(false);
            }
        });
    }
}

fn open_editor(app: &WriterApp, note: Option<FootnoteEntry>) {
    let global = app.global::<WriterFootnoteState>();
    if let Some(note) = note {
        global.set_selected_id(note.id.to_string().into());
        global.set_draft_body(note.body.into());
    } else {
        global.set_selected_id(SharedString::new());
        global.set_draft_body(SharedString::new());
    }
    global.set_error(SharedString::new());
    global.set_dialog_open(true);
}

/// Resolve the focused editor byte offset to a stable block-local UTF-8 anchor.
fn selection_anchor(document: &WriterDocument) -> Result<(u64, usize), String> {
    let offset = document.selection().focus;
    let block = crate::block_containing_offset(document, offset)
        .ok_or_else(|| "Add a paragraph before inserting a footnote".to_string())?;
    let index = document
        .blocks
        .iter()
        .position(|candidate| std::ptr::eq(candidate, block))
        .ok_or_else(|| "The footnote anchor is no longer available".to_string())?;
    let block_start = document.blocks[..index]
        .iter()
        .map(|previous| previous.text.as_str().len() + 1)
        .sum::<usize>();
    let local_offset = offset
        .checked_sub(block_start)
        .ok_or_else(|| "The footnote anchor is outside the document".to_string())?
        .min(block.text.as_str().len());
    if !block.text.as_str().is_char_boundary(local_offset) {
        return Err("The footnote anchor is not on a UTF-8 character boundary".into());
    }
    Ok((block.id, local_offset))
}

#[cfg(test)]
mod tests {
    use std::{rc::Rc, sync::Arc};

    use loom_desktop::{MenuBarService as _, NativeMenuBar, ScriptedFileDialogs};
    use slint::{ComponentHandle, Model as _};

    use crate::{
        actions_tests::test_state, footnote_actions, local_menu, palette_catalog, toolbar_commands,
        WriterApp,
    };
    use loom_writer_core::{FootnoteEntry, PageViewport, RichBlock, TextSelection, WriterDocument};

    fn document() -> (WriterDocument, u64) {
        let mut document = WriterDocument::new("footnotes", "Footnotes");
        let block_id = document.next_id();
        document.push(RichBlock::new(block_id, "paragraph", "éB"));
        document.set_selection(TextSelection::caret(2));
        (document, block_id)
    }

    fn launched(document: WriterDocument) -> (WriterApp, Rc<crate::GuiState>) {
        test_state(document, Rc::new(ScriptedFileDialogs::new([], [])))
    }

    #[test]
    fn add_uses_a_stable_id_utf8_anchor_and_one_undoable_commit() {
        let (document, block_id) = document();
        let (app, state) = launched(document);
        crate::wire_writer_shared_callbacks(&app, &state, None);
        let global = app.global::<crate::WriterFootnoteState>();
        let recovery_writes = crate::recovery::WRITES.with(|writes| writes.get());
        global.invoke_open_editor();
        global.set_draft_body("A note".into());
        global.invoke_apply();

        assert_eq!(state.current.borrow().footnotes.len(), 1);
        let note = state.current.borrow().footnotes[0].clone();
        assert_eq!(note.id, 1);
        assert_eq!(note.block_id, block_id);
        assert_eq!(note.offset, 2);
        assert_eq!(state.history.borrow().undo_len(), 1);
        assert_eq!(
            crate::recovery::WRITES.with(|writes| writes.get()),
            recovery_writes + 1,
            "adding a footnote records one complete recovery snapshot"
        );
        assert!(app.get_document_dirty());

        app.invoke_undo();
        assert!(state.current.borrow().footnotes.is_empty());
        assert!(!app.get_document_dirty());
        assert!(state.history.borrow().can_redo());
        app.invoke_redo();
        assert_eq!(state.current.borrow().footnotes[0].id, note.id);
        assert_eq!(state.current.borrow().footnotes[0].offset, 2);
        assert!(app.get_document_dirty());
        assert_eq!(
            crate::recovery::WRITES.with(|writes| writes.get()),
            recovery_writes + 2,
            "redo returns the footnote through the normal recovery path"
        );
    }

    #[test]
    fn cancel_and_invalid_drafts_do_not_commit() {
        let (document, _) = document();
        let (app, state) = launched(document);
        footnote_actions::register(&app, &state);
        let global = app.global::<crate::WriterFootnoteState>();
        let recovery_writes = crate::recovery::WRITES.with(|writes| writes.get());
        global.invoke_open_editor();
        global.set_draft_body("Draft".into());
        global.invoke_cancel();
        assert_eq!(state.history.borrow().undo_len(), 0);
        assert!(state.current.borrow().footnotes.is_empty());

        global.invoke_open_editor();
        global.set_draft_body("  ".into());
        global.invoke_apply();
        assert!(global.get_dialog_open());
        assert!(!global.get_error().is_empty());
        assert_eq!(state.history.borrow().undo_len(), 0);
        assert!(state.current.borrow().footnotes.is_empty());
        assert_eq!(
            crate::recovery::WRITES.with(|writes| writes.get()),
            recovery_writes,
            "cancel and invalid body validation do not write recovery"
        );
    }

    #[test]
    fn stale_reference_cannot_fall_through_to_add() {
        let (document, _) = document();
        let (app, state) = launched(document);
        footnote_actions::register(&app, &state);
        let global = app.global::<crate::WriterFootnoteState>();
        global.set_dialog_open(true);
        global.set_selected_id("41".into());
        global.set_draft_body("Obsolete edit".into());

        global.invoke_reference_selected("404".into());
        assert!(!global.get_dialog_open());
        assert!(global.get_selected_id().is_empty());
        assert!(global.get_draft_body().is_empty());
        assert!(!global.get_error().is_empty());

        global.set_draft_body("Must not become a new note".into());
        global.invoke_apply();
        assert!(state.current.borrow().footnotes.is_empty());
        assert_eq!(state.history.borrow().undo_len(), 0);
    }

    #[test]
    fn remove_after_cancel_does_not_remove_the_footnote() {
        let (mut document, block_id) = document();
        document.footnotes.push(FootnoteEntry {
            id: 41,
            block_id,
            offset: 2,
            body: "Keep this note".into(),
        });
        let (app, state) = launched(document);
        footnote_actions::register(&app, &state);
        let global = app.global::<crate::WriterFootnoteState>();

        global.invoke_reference_selected("41".into());
        global.set_draft_body("Discard this edit".into());
        global.invoke_cancel();
        assert!(!global.get_dialog_open());
        assert!(global.get_selected_id().is_empty());
        assert!(global.get_draft_body().is_empty());

        // Model a delayed Remove activation after the editor was cancelled.
        global.set_selected_id("41".into());
        global.invoke_remove();
        assert_eq!(state.current.borrow().footnotes.len(), 1);
        assert_eq!(state.current.borrow().footnotes[0].body, "Keep this note");
        assert_eq!(state.history.borrow().undo_len(), 0);
    }

    #[test]
    fn reference_opens_edit_and_remove_preserve_history_semantics() {
        let (mut document, block_id) = document();
        document.footnotes.push(FootnoteEntry {
            id: 41,
            block_id,
            offset: 2,
            body: "Old body".into(),
        });
        let (app, state) = launched(document);
        footnote_actions::register(&app, &state);
        let global = app.global::<crate::WriterFootnoteState>();

        global.invoke_reference_selected("41".into());
        assert_eq!(global.get_draft_body(), "Old body");
        global.set_draft_body("Discard this edit".into());
        global.invoke_cancel();
        assert_eq!(state.current.borrow().footnotes[0].body, "Old body");
        assert_eq!(state.history.borrow().undo_len(), 0);

        global.invoke_reference_selected("41".into());
        global.set_draft_body("Updated body".into());
        global.invoke_apply();
        assert_eq!(state.current.borrow().footnotes[0].id, 41);
        assert_eq!(state.current.borrow().footnotes[0].offset, 2);
        assert_eq!(state.current.borrow().footnotes[0].body, "Updated body");
        assert_eq!(state.history.borrow().undo_len(), 1);

        global.invoke_reference_selected("41".into());
        global.invoke_remove();
        assert!(state.current.borrow().footnotes.is_empty());
        assert_eq!(state.history.borrow().undo_len(), 2);
        let undone = state.history.borrow_mut().undo().expect("undo removal");
        assert_eq!(undone.footnotes[0].id, 41);
        assert_eq!(undone.footnotes[0].body, "Updated body");
    }

    #[test]
    fn toolbar_palette_and_local_menu_open_the_same_footnote_editor() {
        let (document, _) = document();
        let (app, state) = launched(document);
        footnote_actions::register(&app, &state);
        toolbar_commands::wire(&app);

        app.invoke_toolbar_command("writer.footnotes".into());
        let global = app.global::<crate::WriterFootnoteState>();
        assert!(global.get_dialog_open());
        global.invoke_cancel();

        assert!(palette_catalog::dispatch_palette_action(
            &app,
            palette_catalog::PaletteAction::Command("writer.footnotes"),
        ));
        assert!(global.get_dialog_open());
        global.invoke_cancel();
        assert!(palette_catalog::master_palette(&app)
            .iter()
            .any(|command| command.id == "writer.footnotes"));

        let menu_service = Arc::new(NativeMenuBar::new());
        menu_service
            .install_menu_bar(&local_menu::writer_menu_bar())
            .expect("install Writer menu");
        local_menu::wire_action(&app, menu_service.clone());
        local_menu::sync(&app, &menu_service).expect("project Writer menu");
        let items = app.get_local_menu_items();
        assert!((0..items.row_count()).any(|index| {
            items
                .row_data(index)
                .is_some_and(|item| item.command_id == "writer.footnotes")
        }));

        let app_ref = app.as_weak();
        menu_service
            .register_action_sink(Arc::new(move |action| {
                let app = app_ref.upgrade().ok_or_else(|| {
                    loom_desktop::DesktopError::InvalidRequest("Writer window is closed".into())
                })?;
                crate::dispatch_command(&app, &action.id);
                Ok(())
            }))
            .expect("register test menu sink");
        app.invoke_local_menu_action("writer.footnotes".into());
        assert!(global.get_dialog_open());
    }

    #[test]
    fn projection_exposes_references_body_and_continuation_pages() {
        let (mut document, block_id) = document();
        let body = "footnote continuation text ".repeat(600);
        document
            .insert_footnote(block_id, 2, &body)
            .expect("insert footnote");
        let projection = crate::projection::project(&document, PageViewport::default(), None);

        assert_eq!(projection.footnote_references.len(), 1);
        assert!(projection
            .footnote_lines
            .iter()
            .any(|line| line.text.contains("footnote continuation text")));
        assert!(projection
            .footnote_lines
            .iter()
            .any(|line| line.text.contains("(continued).")));
        assert!(projection.page_count > 1);
        assert_eq!(projection.pages.start, 0);
        assert_eq!(projection.pages.end, projection.page_count as usize);
        let page_height = document.page.page_style().height_pt;
        assert!(projection
            .footnote_lines
            .iter()
            .any(|line| line.y > page_height));
    }

    #[test]
    fn footnote_projection_separates_ordinal_body_and_uses_core_line_height() {
        let (mut document, block_id) = document();
        document
            .insert_footnote(block_id, 2, "A short projected note.")
            .expect("insert footnote");
        let projection = crate::projection::project(&document, PageViewport::default(), None);
        let line = projection
            .footnote_lines
            .iter()
            .find(|line| !line.number.is_empty())
            .expect("first note line has a separate ordinal");
        assert_eq!(line.number.to_string(), "1.");
        assert!(line.number_x < line.text_x);
        assert!(line.number_width > 0.0 && line.text_width > 0.0);
        assert_eq!(line.text.to_string(), "A short projected note.");
        let reference = projection
            .footnote_references
            .first()
            .expect("projected inline reference");
        assert!(reference.width > 0.0);

        let style = document.page.page_style();
        let flow = document
            .flow(
                &style,
                crate::projection::layout_viewport(&style, PageViewport::default()),
            )
            .expect("core footnote line geometry");
        let core_line = flow.footnote_lines().first().expect("core note line");
        let expected_height = core_line.number_bounds.height / flow.zoom();
        assert!((line.line_height - expected_height).abs() < 0.01);
    }
}
