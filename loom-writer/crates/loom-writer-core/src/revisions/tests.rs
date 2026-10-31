use super::*;
use crate::WriterDocument;
use crate::{load_document, save_document};

fn document(text: &str) -> WriterDocument {
    let mut document = WriterDocument::new("tracked", "Tracked changes");
    document.push(RichBlock::new(1, "paragraph", text));
    document
}

fn style(color: Option<&str>) -> loom_text::CharacterStyle {
    loom_text::CharacterStyle {
        color: color.map(str::to_owned),
        ..Default::default()
    }
}

fn payload(text: &str, runs: Vec<StyleRun>) -> RevisionPayload {
    RevisionPayload {
        text: text.into(),
        runs,
    }
}

#[test]
fn tracked_changes_round_trip_persist_settings_ids_and_rich_payloads() {
    let mut document = document("a!bc");
    document.tracked_changes.set_author("Reviewer").unwrap();
    document.tracked_changes.set_recording(true);
    document.blocks[0].runs.push(StyleRun {
        start: 1,
        end: 2,
        style: style(Some("#336699")),
    });
    let id = document
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..2,
            None,
            Some(payload(
                "!",
                vec![StyleRun {
                    start: 0,
                    end: 1,
                    style: style(Some("#336699")),
                }],
            )),
        )
        .unwrap()
        .unwrap();
    assert_eq!(id, 1);
    let bytes = save_document(&document).unwrap();
    let loaded = load_document(&bytes).unwrap();
    assert_eq!(loaded, document);
    assert_eq!(loaded.tracked_changes.author(), "Reviewer");
    assert!(loaded.tracked_changes.recording());
    assert_eq!(loaded.tracked_changes.revisions()[0].id, 1);
}

#[test]
fn older_content_defaults_tracked_changes_without_changing_existing_documents() {
    let original = document("legacy");
    let current_json = original.to_content_json();
    let tracked_state = format!(
        ",\"tracked_changes\":{}",
        TrackedChanges::default().write_content_json()
    );
    let missing_state = current_json.replace(&tracked_state, "");
    assert!(WriterDocument::from_content_json(&missing_state).is_err());

    let json = current_json
        .replace(
            &format!("\"format\":{}", crate::WRITER_CONTENT_FORMAT),
            "\"format\":6",
        )
        .replace(",\"appearance\":\"white\"", "")
        .replace(&tracked_state, "");
    let loaded = WriterDocument::from_content_json(&json).unwrap();
    assert_eq!(loaded.tracked_changes, TrackedChanges::default());
    assert!(loaded.same_content_as(&original));
}

#[test]
fn recording_allocates_monotonic_ids_and_disabled_recording_does_not_advance() {
    let mut document = document("a!b");
    let first_payload = payload("!", Vec::new());
    assert_eq!(
        document
            .record_revision(
                RevisionKind::Insertion,
                1,
                1..2,
                None,
                Some(first_payload.clone())
            )
            .unwrap(),
        None
    );
    assert_eq!(document.tracked_changes.next_revision_id(), 1);
    document.tracked_changes.set_recording(true);
    assert_eq!(
        document
            .record_revision(
                RevisionKind::Insertion,
                1,
                1..2,
                None,
                Some(first_payload.clone())
            )
            .unwrap(),
        Some(1)
    );
    assert!(document.accept_revision(1).unwrap());
    assert_eq!(
        document
            .record_revision(RevisionKind::Insertion, 1, 1..2, None, Some(first_payload))
            .unwrap(),
        Some(2)
    );
    assert_eq!(document.tracked_changes.next_revision_id(), 3);
}

#[test]
fn recording_an_insertion_requires_payload_to_match_current_inline_style() {
    let mut document = document("a");
    let italic = loom_text::CharacterStyle {
        italic: true,
        ..Default::default()
    };
    document.blocks[0].runs = vec![StyleRun {
        start: 0,
        end: 1,
        style: italic.clone(),
    }];
    document.tracked_changes.set_recording(true);

    assert!(document
        .record_revision(
            RevisionKind::Insertion,
            1,
            0..1,
            None,
            Some(RevisionPayload::plain("a")),
        )
        .is_err());
    assert!(document.tracked_changes.revisions().is_empty());
    assert_eq!(document.tracked_changes.next_revision_id(), 1);

    assert_eq!(
        document
            .record_revision(
                RevisionKind::Insertion,
                1,
                0..1,
                None,
                Some(payload(
                    "a",
                    vec![StyleRun {
                        start: 0,
                        end: 1,
                        style: italic,
                    }],
                )),
            )
            .unwrap(),
        Some(1)
    );
}

#[test]
fn reject_insertion_splices_only_its_range_and_keeps_later_text() {
    let mut document = document("a!b");
    document.tracked_changes.set_recording(true);
    document
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..2,
            None,
            Some(payload("!", Vec::new())),
        )
        .unwrap();
    document.blocks[0].text = crate::Text::from_str("a!b later");
    assert!(document.reject_revision(1).unwrap());
    assert_eq!(document.blocks[0].text.as_str(), "ab later");
    assert!(document.tracked_changes.revisions().is_empty());
}

#[test]
fn reject_deletion_restores_its_rich_payload() {
    let mut document = document("ab");
    document.tracked_changes.set_recording(true);
    document
        .record_revision(
            RevisionKind::Deletion,
            1,
            1..1,
            Some(payload(
                "X",
                vec![StyleRun {
                    start: 0,
                    end: 1,
                    style: style(Some("#123456")),
                }],
            )),
            None,
        )
        .unwrap();
    document.blocks[0].text = crate::Text::from_str("ab later");
    assert!(document.reject_revision(1).unwrap());
    assert_eq!(document.blocks[0].text.as_str(), "aXb later");
    assert_eq!(document.blocks[0].runs[0].start, 1);
    assert_eq!(document.blocks[0].runs[0].end, 2);
    assert_eq!(
        document.blocks[0].runs[0].style.color.as_deref(),
        Some("#123456")
    );
}

#[test]
fn reject_format_restores_only_the_recorded_style_range() {
    let mut document = document("abcdef");
    document.blocks[0].runs.push(StyleRun {
        start: 2,
        end: 4,
        style: style(Some("#123456")),
    });
    document.tracked_changes.set_recording(true);
    document
        .record_revision(
            RevisionKind::Formatting,
            1,
            2..4,
            Some(payload("cd", Vec::new())),
            Some(payload(
                "cd",
                vec![StyleRun {
                    start: 0,
                    end: 2,
                    style: style(Some("#123456")),
                }],
            )),
        )
        .unwrap();
    document.blocks[0].text = crate::Text::from_str("abcdef after");
    assert!(document.reject_revision(1).unwrap());
    assert_eq!(document.blocks[0].text.as_str(), "abcdef after");
    assert!(document.blocks[0].runs.is_empty());
}

#[test]
fn rejecting_format_keeps_later_non_overlapping_revision_valid() {
    let mut document = document("abcdef");
    document.blocks[0].runs.push(StyleRun {
        start: 0,
        end: 1,
        style: style(Some("#123456")),
    });
    document.tracked_changes.set_recording(true);
    let format_id = document
        .record_revision(
            RevisionKind::Formatting,
            1,
            0..1,
            Some(payload("a", Vec::new())),
            Some(payload(
                "a",
                vec![StyleRun {
                    start: 0,
                    end: 1,
                    style: style(Some("#123456")),
                }],
            )),
        )
        .unwrap()
        .unwrap();
    let later_id = document
        .record_revision(
            RevisionKind::Insertion,
            1,
            3..4,
            None,
            Some(payload("d", Vec::new())),
        )
        .unwrap()
        .unwrap();

    assert!(document.reject_revision(format_id).unwrap());
    assert_eq!(document.blocks[0].text.as_str(), "abcdef");
    assert!(document.blocks[0].runs.is_empty());
    assert_eq!(document.tracked_changes.revisions().len(), 1);
    assert_eq!(document.tracked_changes.revisions()[0].id, later_id);
    document
        .tracked_changes
        .validate_for_document(&document.blocks)
        .expect("the later revision remains valid after style-only rejection");
}

#[test]
fn rejection_conflict_keeps_document_and_revision_unchanged() {
    let mut document = document("a!b");
    document.tracked_changes.set_recording(true);
    document
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..2,
            None,
            Some(payload("!", Vec::new())),
        )
        .unwrap();
    document.blocks[0].text = crate::Text::from_str("a?b");
    let before = document.clone();
    assert!(document.reject_revision(1).is_err());
    assert_eq!(document, before);
}

#[test]
fn later_overlapping_text_revisions_can_be_rejected_in_dependency_order() {
    let mut before = document("zabcw");
    before.tracked_changes.set_recording(true);
    before
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..4,
            None,
            Some(payload("abc", Vec::new())),
        )
        .unwrap();

    let mut candidate = before.clone();
    candidate.replace_editor_text_at("zaXcw", Some(3)).unwrap();
    candidate
        .record_user_edits_from(
            &before,
            &[RevisionTextEdit {
                old_start: 2,
                old_end: 3,
                new_start: 2,
                new_end: 3,
            }],
        )
        .unwrap();
    assert_eq!(candidate.tracked_changes.revisions().len(), 3);
    let latest_insertion = candidate
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| {
            revision.id > 1
                && revision.kind == RevisionKind::Insertion
                && revision
                    .after
                    .as_ref()
                    .is_some_and(|payload| payload.text == "X")
        })
        .unwrap()
        .id;

    let unresolved = candidate.clone();
    assert!(matches!(
        candidate.reject_revision(1),
        Err(WriterError::Invalid(message)) if message == REVISION_DEPENDENCY_CONFLICT
    ));
    assert_eq!(candidate, unresolved);
    assert!(matches!(
        candidate.accept_revision(latest_insertion),
        Err(WriterError::Invalid(message)) if message == REVISION_DEPENDENCY_CONFLICT
    ));
    assert_eq!(candidate, unresolved);

    assert!(candidate.reject_revision(latest_insertion).unwrap());
    assert_eq!(candidate.editor_text(), "zacw");
    assert!(candidate
        .tracked_changes
        .revisions()
        .iter()
        .any(|revision| revision.id == 1));
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();

    let deletion_id = candidate
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| revision.id > 1 && revision.kind == RevisionKind::Deletion)
        .unwrap()
        .id;
    assert!(candidate.reject_revision(deletion_id).unwrap());
    assert_eq!(candidate.editor_text(), "zabcw");
    assert!(candidate.reject_revision(1).unwrap());
    assert_eq!(candidate.editor_text(), "zw");
    assert!(candidate.tracked_changes.revisions().is_empty());
}

#[test]
fn accepting_an_overlapping_parent_commits_its_text_for_later_resolution() {
    let mut before = document("zabcw");
    before.tracked_changes.set_recording(true);
    before
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..4,
            None,
            Some(payload("abc", Vec::new())),
        )
        .unwrap();
    let mut candidate = before.clone();
    candidate.replace_editor_text_at("zaXcw", Some(3)).unwrap();
    candidate
        .record_user_edits_from(
            &before,
            &[RevisionTextEdit {
                old_start: 2,
                old_end: 3,
                new_start: 2,
                new_end: 3,
            }],
        )
        .unwrap();
    let visible_text = candidate.editor_text();

    assert!(candidate.accept_revision(1).unwrap());
    assert_eq!(candidate.editor_text(), visible_text);
    assert_eq!(
        candidate
            .tracked_changes
            .revisions()
            .iter()
            .map(|revision| revision.id)
            .collect::<Vec<_>>(),
        vec![2, 3]
    );

    assert!(candidate.reject_revision(3).unwrap());
    assert_eq!(candidate.editor_text(), "zacw");
    assert!(candidate.reject_revision(2).unwrap());
    assert_eq!(candidate.editor_text(), "zabcw");
    assert!(candidate.tracked_changes.revisions().is_empty());
}

#[test]
fn accepting_all_overlapping_revisions_keeps_current_text_and_next_id() {
    let mut before = document("zabcw");
    before.tracked_changes.set_recording(true);
    before
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..4,
            None,
            Some(payload("abc", Vec::new())),
        )
        .unwrap();
    let mut candidate = before.clone();
    candidate.replace_editor_text_at("zaXcw", Some(3)).unwrap();
    candidate
        .record_user_edits_from(
            &before,
            &[RevisionTextEdit {
                old_start: 2,
                old_end: 3,
                new_start: 2,
                new_end: 3,
            }],
        )
        .unwrap();
    let visible_text = candidate.editor_text();
    let next_id = candidate.tracked_changes.next_revision_id();

    assert_eq!(candidate.accept_all_revisions().unwrap(), 3);
    assert_eq!(candidate.editor_text(), visible_text);
    assert!(candidate.tracked_changes.revisions().is_empty());
    assert_eq!(candidate.tracked_changes.next_revision_id(), next_id);
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();
}

#[test]
fn rejecting_all_overlapping_revisions_restores_the_pre_change_text() {
    let mut before = document("zabcw");
    before.tracked_changes.set_recording(true);
    before
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..4,
            None,
            Some(payload("abc", Vec::new())),
        )
        .unwrap();
    let mut candidate = before.clone();
    candidate.replace_editor_text_at("zaXcw", Some(3)).unwrap();
    candidate
        .record_user_edits_from(
            &before,
            &[RevisionTextEdit {
                old_start: 2,
                old_end: 3,
                new_start: 2,
                new_end: 3,
            }],
        )
        .unwrap();
    let next_id = candidate.tracked_changes.next_revision_id();

    assert_eq!(candidate.reject_all_revisions().unwrap(), 3);
    assert_eq!(candidate.editor_text(), "zw");
    assert!(candidate.tracked_changes.revisions().is_empty());
    assert_eq!(candidate.tracked_changes.next_revision_id(), next_id);
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();
}

#[test]
fn failing_reject_all_keeps_document_and_revision_ids_unchanged() {
    let mut before = document("zabcw");
    before.tracked_changes.set_recording(true);
    before
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..4,
            None,
            Some(payload("abc", Vec::new())),
        )
        .unwrap();
    let mut candidate = before.clone();
    candidate.replace_editor_text_at("zaXcw", Some(3)).unwrap();
    candidate
        .record_user_edits_from(
            &before,
            &[RevisionTextEdit {
                old_start: 2,
                old_end: 3,
                new_start: 2,
                new_end: 3,
            }],
        )
        .unwrap();
    candidate.blocks[0].text = crate::Text::from_str("stale");
    let uncommitted = candidate.clone();
    let revision_ids = candidate
        .tracked_changes
        .revisions()
        .iter()
        .map(|revision| revision.id)
        .collect::<Vec<_>>();

    assert!(candidate.reject_all_revisions().is_err());
    assert_eq!(candidate, uncommitted);
    assert_eq!(
        candidate
            .tracked_changes
            .revisions()
            .iter()
            .map(|revision| revision.id)
            .collect::<Vec<_>>(),
        revision_ids
    );
}

#[test]
fn deletion_reject_refuses_a_shifted_prefix_without_mutating() {
    let mut document = document("ab");
    document.tracked_changes.set_recording(true);
    document
        .record_revision(
            RevisionKind::Deletion,
            1,
            1..1,
            Some(payload("X", Vec::new())),
            None,
        )
        .unwrap();
    document.blocks[0].text = crate::Text::from_str("xab");
    let before = document.clone();
    assert!(document.reject_revision(1).is_err());
    assert_eq!(document, before);
}

#[test]
fn malformed_ids_utf8_ranges_authors_and_duplicate_fields_are_rejected() {
    let mut source_document = document("a!b?c");
    source_document.tracked_changes.set_recording(true);
    source_document
        .record_revision(
            RevisionKind::Insertion,
            1,
            1..2,
            None,
            Some(payload("!", Vec::new())),
        )
        .unwrap();
    source_document
        .record_revision(
            RevisionKind::Insertion,
            1,
            3..4,
            None,
            Some(payload("?", Vec::new())),
        )
        .unwrap();
    let json = source_document.to_content_json();

    let duplicate_id = json.replacen("\"id\":2,\"author\"", "\"id\":1,\"author\"", 1);
    assert_ne!(duplicate_id, json);
    assert!(WriterDocument::from_content_json(&duplicate_id).is_err());

    let zero_id = json.replacen("\"id\":1,\"author\"", "\"id\":0,\"author\"", 1);
    assert_ne!(zero_id, json);
    assert!(WriterDocument::from_content_json(&zero_id).is_err());

    let duplicate_next_id = json.replace("\"next_id\":3", "\"next_id\":3,\"next_id\":3");
    assert_ne!(duplicate_next_id, json);
    assert!(WriterDocument::from_content_json(&duplicate_next_id).is_err());

    let state = source_document.tracked_changes.write_content_json();
    let state_field = format!(",\"tracked_changes\":{state}");
    let duplicate_state = json.replacen(
        &state_field,
        &format!("{state_field},\"tracked_changes\":{state}"),
        1,
    );
    assert_ne!(duplicate_state, json);
    assert!(WriterDocument::from_content_json(&duplicate_state).is_err());

    let mut multibyte = document("é!");
    multibyte.tracked_changes.set_recording(true);
    assert!(
        multibyte
            .record_revision(
                RevisionKind::Insertion,
                1,
                1..2,
                None,
                Some(payload("!", Vec::new())),
            )
            .is_err(),
        "anchors inside a UTF-8 code point are rejected"
    );
    assert!(multibyte.tracked_changes.set_author(" ").is_err());
    assert_eq!(multibyte.tracked_changes.author(), "Anonymous");
    assert_eq!(multibyte.tracked_changes.next_revision_id(), 1);
    assert!(multibyte.tracked_changes.revisions().is_empty());
}

#[test]
fn committed_text_edits_rebase_anchors_while_recording_is_disabled() {
    let mut document = document("hello world");
    document.tracked_changes.set_recording(true);
    let block_id = document.blocks[0].id;
    document
        .record_revision(
            RevisionKind::Insertion,
            block_id,
            6..11,
            None,
            Some(payload("world", Vec::new())),
        )
        .unwrap();
    let revision_id = document.tracked_changes.revisions[0].id;
    document.tracked_changes.set_recording(false);

    let mut prefixed = document.clone();
    prefixed
        .replace_editor_text_at("new hello world", Some(4))
        .unwrap();
    prefixed.record_user_edit_from(&document, Some(4)).unwrap();
    let revision = &prefixed.tracked_changes.revisions[0];
    assert_eq!(revision.id, revision_id);
    assert_eq!(revision.anchor.start, 10);
    assert_eq!(revision.anchor.end, 15);
    assert_eq!(revision.after.as_ref().unwrap().text, "world");
    let next_id = prefixed.tracked_changes.next_revision_id();

    let before_delete = prefixed.clone();
    let mut deleted = before_delete.clone();
    deleted
        .replace_editor_text_at("hello world", Some(0))
        .unwrap();
    deleted
        .record_user_edit_from(&before_delete, Some(0))
        .unwrap();
    let revision = &deleted.tracked_changes.revisions[0];
    assert_eq!(revision.anchor.start, 6);
    assert_eq!(revision.anchor.end, 11);
    assert_eq!(deleted.tracked_changes.next_revision_id(), next_id);
    deleted
        .tracked_changes
        .validate_for_document(&deleted.blocks)
        .unwrap();
}

#[test]
fn text_transaction_rejects_a_stale_anchor_without_committing_metadata() {
    let mut document = document("cat");
    document.tracked_changes.set_recording(true);
    let block_id = document.blocks[0].id;
    document
        .record_revision(
            RevisionKind::Insertion,
            block_id,
            0..3,
            None,
            Some(payload("cat", Vec::new())),
        )
        .unwrap();
    document.tracked_changes.revisions[0].anchor.prefix_sha256 = [7; 32];

    let before = document.clone();
    let mut candidate = before.clone();
    candidate.replace_editor_text_at("cats", Some(4)).unwrap();
    let uncommitted = candidate.clone();
    assert!(candidate.record_user_edit_from(&before, Some(4)).is_err());
    assert_eq!(candidate, uncommitted);
    assert_eq!(document, before);
}

#[test]
fn paragraph_break_edits_use_backward_compatible_multiblock_anchors() {
    let mut before = document("alphaomega");
    before.tracked_changes.set_recording(true);
    let first_id = before.blocks[0].id;
    before
        .record_revision(
            RevisionKind::Insertion,
            first_id,
            5..10,
            None,
            Some(payload("omega", Vec::new())),
        )
        .unwrap();

    let mut candidate = before.clone();
    candidate
        .replace_editor_text_at("alpha\nomega", Some(6))
        .unwrap();
    candidate.record_user_edit_from(&before, Some(6)).unwrap();

    let moved = candidate
        .tracked_changes
        .revisions
        .iter()
        .find(|revision| revision.id == 1)
        .unwrap();
    assert_eq!(moved.anchor.start, 0);
    assert_eq!(moved.anchor.end, 5);
    assert_eq!(moved.anchor.end_block_id, None);
    assert_eq!(moved.after.as_ref().unwrap().text, "omega");

    let paragraph_revision = candidate
        .tracked_changes
        .revisions
        .iter()
        .find(|revision| revision.id == 2)
        .unwrap();
    assert_eq!(paragraph_revision.kind, RevisionKind::Insertion);
    assert_eq!(
        paragraph_revision.boundary_kind,
        RevisionBoundaryKind::ParagraphMark
    );
    assert_eq!(paragraph_revision.after.as_ref().unwrap().text, "\n");
    assert!(paragraph_revision.anchor.end_block_id.is_some());
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();
    let bytes = crate::save_document(&candidate).unwrap();
    assert_eq!(crate::load_document(&bytes).unwrap(), candidate);

    assert!(candidate.reject_revision(2).unwrap());
    assert_eq!(candidate.editor_text(), "alphaomega");
    let moved = &candidate.tracked_changes.revisions[0];
    assert_eq!(moved.anchor.block_id, first_id);
    assert_eq!(moved.anchor.start, 5);
    assert_eq!(moved.anchor.end, 10);
    assert_eq!(candidate.tracked_changes.next_revision_id(), 3);
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();
}

#[test]
fn tracked_edits_reject_multi_cell_table_hunks_atomically() {
    let mut before = document("");
    let block_id = before.insert_table_block(0, 1, 2).unwrap();
    before
        .edit_table_text(
            block_id,
            &format!("left{}right", crate::tables::TABLE_CELL_SEPARATOR),
        )
        .unwrap();
    before.tracked_changes.set_recording(true);
    let old_text = before.editor_text();

    let mut candidate = before.clone();
    candidate
        .edit_table_text(
            block_id,
            &format!("north{}south", crate::tables::TABLE_CELL_SEPARATOR),
        )
        .unwrap();
    let new_text = candidate.editor_text();
    let edit = RevisionTextEdit {
        old_start: 0,
        old_end: old_text.len(),
        new_start: 0,
        new_end: new_text.len(),
    };
    let uncommitted = candidate.clone();

    let error = candidate
        .record_user_edits_from(&before, &[edit])
        .unwrap_err();
    assert!(matches!(
        error,
        WriterError::Invalid(message) if message == UNRECORDABLE_TABLE_TEXT_EDIT
    ));
    assert_eq!(candidate, uncommitted);
    assert_eq!(before.editor_text(), old_text);
    assert!(candidate.tracked_changes.revisions.is_empty());
    assert_eq!(candidate.tracked_changes.next_revision_id(), 1);
}

#[test]
fn table_cell_edits_rebase_and_reject_inside_the_cell() {
    let mut before = WriterDocument::new("tracked", "Tracked changes");
    let block_id = before.insert_table_block(0, 1, 1).unwrap();
    before.edit_table_text(block_id, "cell").unwrap();
    before.tracked_changes.set_recording(true);
    before
        .record_revision(
            RevisionKind::Insertion,
            block_id,
            2..4,
            None,
            Some(payload("ll", Vec::new())),
        )
        .unwrap();

    let mut candidate = before.clone();
    candidate.edit_table_text(block_id, "Xcell").unwrap();
    candidate.set_selection(crate::TextSelection::caret(1));
    candidate.record_user_edit_from(&before, Some(1)).unwrap();
    assert_eq!(candidate.tracked_changes.revisions[0].anchor.start, 3);
    assert_eq!(candidate.tracked_changes.revisions[0].anchor.end, 5);
    assert_eq!(
        candidate.tracked_changes.revisions[0]
            .after
            .as_ref()
            .unwrap()
            .text,
        "ll"
    );
    assert_eq!(
        candidate.tracked_changes.revisions[1]
            .after
            .as_ref()
            .unwrap()
            .text,
        "X"
    );
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();

    assert!(candidate.reject_revision(2).unwrap());
    assert_eq!(candidate.editor_text(), "cell");
    assert_eq!(candidate.tracked_changes.revisions[0].anchor.start, 2);
    assert_eq!(candidate.tracked_changes.revisions[0].anchor.end, 4);
    assert_eq!(candidate.tracked_changes.next_revision_id(), 3);
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();
}

#[test]
fn tracked_paragraph_format_changes_are_recorded_across_blocks_and_resolved() {
    let mut before = document("first");
    before.blocks.push(RichBlock::new(2, "paragraph", "second"));
    before.tracked_changes.set_recording(true);
    let second_style = before.blocks[1].style.clone();

    let mut candidate = before.clone();
    candidate.blocks[0].style.alignment = loom_text::Alignment::Center;
    candidate.blocks[0].style.space_before = 4.0;
    candidate.blocks[1].kind = "heading2".into();
    candidate.blocks[1].style.alignment = loom_text::Alignment::Right;
    candidate.blocks[1].style.direction = loom_text::ParagraphDirection::RightToLeft;
    candidate.blocks[1].style.line_spacing = 1.5;
    candidate.record_user_edit_from(&before, None).unwrap();

    assert_eq!(candidate.tracked_changes.revisions().len(), 2);
    assert!(candidate
        .tracked_changes
        .revisions()
        .iter()
        .all(|revision| revision.kind == RevisionKind::Formatting));
    let bytes = crate::save_document(&candidate).unwrap();
    let mut reopened = crate::load_document(&bytes).unwrap();
    assert_eq!(reopened, candidate);

    let first_id = reopened.tracked_changes.revisions()[0].id;
    let second_id = reopened.tracked_changes.revisions()[1].id;
    assert!(reopened.accept_revision(first_id).unwrap());
    assert_eq!(
        reopened.blocks[0].style.alignment,
        loom_text::Alignment::Center
    );
    assert_eq!(reopened.blocks[0].style.space_before, 4.0);
    assert_eq!(reopened.blocks[1].kind, "heading2");
    assert_eq!(
        reopened.blocks[1].style.alignment,
        loom_text::Alignment::Right
    );
    assert_eq!(
        reopened.blocks[1].style.direction,
        loom_text::ParagraphDirection::RightToLeft
    );
    assert!(reopened.reject_revision(second_id).unwrap());
    assert_eq!(reopened.blocks[1].kind, "paragraph");
    assert_eq!(reopened.blocks[1].style, second_style);
    assert!(reopened.tracked_changes.revisions().is_empty());
}

#[test]
fn tracked_formatting_on_an_empty_paragraph_round_trips_and_rejects() {
    let mut before = document("");
    before.tracked_changes.set_recording(true);
    let mut candidate = before.clone();
    candidate.blocks[0].style.alignment = loom_text::Alignment::Center;
    candidate.blocks[0].style.direction = loom_text::ParagraphDirection::RightToLeft;
    candidate.blocks[0].style.space_after = 6.0;
    candidate.record_user_edit_from(&before, None).unwrap();

    let revision = candidate.tracked_changes.revisions()[0].id;
    let bytes = crate::save_document(&candidate).unwrap();
    let mut reopened = crate::load_document(&bytes).unwrap();
    assert_eq!(reopened, candidate);
    assert!(reopened.reject_revision(revision).unwrap());
    assert_eq!(reopened.blocks[0].style, before.blocks[0].style);
    assert!(reopened.tracked_changes.revisions().is_empty());
}

#[test]
fn paragraph_format_reject_preserves_an_invalidated_revision_atomically() {
    let mut before = document("paragraph");
    before.tracked_changes.set_recording(true);
    let mut candidate = before.clone();
    candidate.blocks[0].style.alignment = loom_text::Alignment::Center;
    candidate.record_user_edit_from(&before, None).unwrap();
    let revision_id = candidate.tracked_changes.revisions()[0].id;
    candidate.blocks[0].style.alignment = loom_text::Alignment::Right;
    let stale = candidate.clone();

    assert!(candidate.reject_revision(revision_id).is_err());
    assert_eq!(candidate, stale);
    assert_eq!(candidate.tracked_changes.revisions()[0].id, revision_id);
}

#[test]
fn committed_inline_formatting_is_recorded_and_rejected() {
    let before = document("abc");
    let mut candidate = before.clone();
    let bold = loom_text::CharacterStyle {
        weight: loom_text::FontWeight::Bold,
        ..Default::default()
    };
    candidate.blocks[0].runs.push(StyleRun {
        start: 0,
        end: 2,
        style: bold,
    });
    candidate.tracked_changes.set_recording(true);

    candidate.record_user_edit_from(&before, None).unwrap();
    let revision = &candidate.tracked_changes.revisions[0];
    assert_eq!(revision.kind, RevisionKind::Formatting);
    assert_eq!(revision.before.as_ref().unwrap().runs, Vec::new());
    assert_eq!(revision.after.as_ref().unwrap().runs.len(), 1);
    let revision_id = revision.id;
    let next_id = candidate.tracked_changes.next_revision_id();

    assert!(candidate.reject_revision(revision_id).unwrap());
    assert!(candidate.blocks[0].runs.is_empty());
    assert_eq!(candidate.tracked_changes.next_revision_id(), next_id);
}

#[test]
fn replace_all_hunks_preserve_anchors_between_separated_matches() {
    let mut before = document("cat and cat");
    before.tracked_changes.set_recording(true);
    let block_id = before.blocks[0].id;
    before
        .record_revision(
            RevisionKind::Insertion,
            block_id,
            4..7,
            None,
            Some(payload("and", Vec::new())),
        )
        .unwrap();

    let mut candidate = before.clone();
    let (replaced, edits) =
        candidate.replace_all_with_edits("cat", "tiger", crate::replace::FindOptions::default());
    assert_eq!(replaced, 2);
    assert_eq!(edits.len(), 2);
    candidate.record_user_edits_from(&before, &edits).unwrap();

    assert_eq!(candidate.editor_text(), "tiger and tiger");
    let between = &candidate.tracked_changes.revisions[0];
    assert_eq!(between.anchor.start, 6);
    assert_eq!(between.anchor.end, 9);
    assert_eq!(between.after.as_ref().unwrap().text, "and");
    assert_eq!(candidate.tracked_changes.revisions.len(), 5);
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();
}

#[test]
fn replace_all_hunks_rebase_without_recording() {
    let mut before = document("cat and cat");
    before.tracked_changes.set_recording(true);
    let block_id = before.blocks[0].id;
    before
        .record_revision(
            RevisionKind::Insertion,
            block_id,
            4..7,
            None,
            Some(payload("and", Vec::new())),
        )
        .unwrap();
    before.tracked_changes.set_recording(false);
    let next_id = before.tracked_changes.next_revision_id();

    let mut candidate = before.clone();
    let (replaced, edits) =
        candidate.replace_all_with_edits("cat", "tiger", crate::replace::FindOptions::default());
    assert_eq!(replaced, 2);
    candidate.record_user_edits_from(&before, &edits).unwrap();

    assert_eq!(candidate.tracked_changes.revisions.len(), 1);
    assert_eq!(candidate.tracked_changes.next_revision_id(), next_id);
    let between = &candidate.tracked_changes.revisions[0];
    assert_eq!(between.anchor.start, 6);
    assert_eq!(between.anchor.end, 9);
    assert_eq!(between.after.as_ref().unwrap().text, "and");
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();
}

#[test]
fn resolution_transactions_are_not_recorded_as_user_edits() {
    let mut before = document("cat");
    before.tracked_changes.set_recording(true);
    let block_id = before.blocks[0].id;
    before
        .record_revision(
            RevisionKind::Insertion,
            block_id,
            0..3,
            None,
            Some(payload("cat", Vec::new())),
        )
        .unwrap();
    let revision_id = before.tracked_changes.revisions[0].id;
    let next_id = before.tracked_changes.next_revision_id();

    let mut accepted = before.clone();
    assert!(accepted.accept_revision(revision_id).unwrap());
    accepted.record_user_edit_from(&before, None).unwrap();
    assert!(accepted.tracked_changes.revisions.is_empty());
    assert_eq!(accepted.tracked_changes.next_revision_id(), next_id);
    assert_eq!(accepted.editor_text(), "cat");

    let mut rejected = before.clone();
    assert!(rejected.reject_revision(revision_id).unwrap());
    let after_reject = rejected.clone();
    rejected.record_user_edit_from(&before, Some(0)).unwrap();
    assert_eq!(rejected, after_reject);
    assert!(rejected.tracked_changes.revisions.is_empty());
    assert_eq!(rejected.tracked_changes.next_revision_id(), next_id);
    assert!(rejected.editor_text().is_empty());
}

#[test]
fn replacements_record_delete_and_insert_as_one_transaction() {
    let mut before = document("cat");
    before.tracked_changes.set_recording(true);
    let mut candidate = before.clone();
    candidate.replace_editor_text_at("dog", Some(3)).unwrap();
    candidate.record_user_edit_from(&before, Some(3)).unwrap();

    assert_eq!(candidate.tracked_changes.revisions.len(), 2);
    assert_eq!(
        candidate.tracked_changes.revisions[0].kind,
        RevisionKind::Deletion
    );
    assert_eq!(
        candidate.tracked_changes.revisions[0]
            .before
            .as_ref()
            .unwrap()
            .text,
        "cat"
    );
    assert_eq!(
        candidate.tracked_changes.revisions[1].kind,
        RevisionKind::Insertion
    );
    assert_eq!(
        candidate.tracked_changes.revisions[1]
            .after
            .as_ref()
            .unwrap()
            .text,
        "dog"
    );
    let next_id = candidate.tracked_changes.next_revision_id();
    candidate
        .tracked_changes
        .validate_for_document(&candidate.blocks)
        .unwrap();

    assert!(candidate.reject_revision(2).unwrap());
    assert!(candidate.blocks.is_empty());
    assert_eq!(candidate.tracked_changes.revisions.len(), 1);
    assert!(candidate.reject_revision(1).unwrap());
    assert_eq!(candidate.editor_text(), "cat");
    assert!(candidate.tracked_changes.revisions.is_empty());
    assert_eq!(candidate.tracked_changes.next_revision_id(), next_id);
}

#[test]
fn paragraph_mark_and_soft_break_deletions_keep_distinct_boundary_ownership() {
    let mut before = WriterDocument::new("boundary", "Boundary ownership");
    before.blocks.push(RichBlock::new(1, "paragraph", "left"));
    before.blocks.push(RichBlock::new(2, "paragraph", "right"));
    before.tracked_changes.set_recording(true);

    let mut paragraph_mark_deleted = before.clone();
    paragraph_mark_deleted
        .replace_editor_text_at("leftright", Some(4))
        .unwrap();
    paragraph_mark_deleted
        .record_user_edit_from(&before, Some(4))
        .unwrap();
    let paragraph_mark = paragraph_mark_deleted
        .tracked_changes
        .revisions()
        .first()
        .expect("paragraph-mark deletion is recorded");
    assert_eq!(paragraph_mark.before.as_ref().unwrap().text, "\n");
    assert_eq!(
        paragraph_mark.boundary_kind,
        RevisionBoundaryKind::ParagraphMark
    );

    let bytes = crate::save_document(&paragraph_mark_deleted).unwrap();
    let reopened = crate::load_document(&bytes).unwrap();
    assert_eq!(reopened, paragraph_mark_deleted);
    assert_eq!(
        reopened.tracked_changes.revisions()[0].boundary_kind,
        RevisionBoundaryKind::ParagraphMark
    );

    let current_json = paragraph_mark_deleted.tracked_changes.write_content_json();
    let legacy_json = current_json.replace(",\"boundary_kind\":\"paragraph_mark\"", "");
    assert_ne!(current_json, legacy_json);
    let legacy = TrackedChanges::from_content_json_value(&legacy_json).unwrap();
    assert_eq!(
        legacy.revisions()[0].boundary_kind,
        RevisionBoundaryKind::Unknown
    );
    assert!(legacy
        .write_content_json()
        .contains("\"boundary_kind\":\"unknown\""));

    let mut soft_break_before = WriterDocument::new("soft", "Soft break");
    soft_break_before
        .blocks
        .push(RichBlock::new(1, "paragraph", "left\u{b}right"));
    soft_break_before.tracked_changes.set_recording(true);
    let mut soft_break_deleted = soft_break_before.clone();
    soft_break_deleted
        .replace_editor_text_at("leftright", Some(4))
        .unwrap();
    soft_break_deleted
        .record_user_edit_from(&soft_break_before, Some(4))
        .unwrap();
    let soft_break = soft_break_deleted
        .tracked_changes
        .revisions()
        .first()
        .expect("soft-break deletion is recorded");
    assert_eq!(soft_break.before.as_ref().unwrap().text, "\u{b}");
    assert_eq!(soft_break.boundary_kind, RevisionBoundaryKind::SoftBreak);
    let bytes = crate::save_document(&soft_break_deleted).unwrap();
    assert_eq!(crate::load_document(&bytes).unwrap(), soft_break_deleted);
}
