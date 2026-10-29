use super::{export_docx, import_docx, PackageArchive};
use crate::{
    RevisionAnchor, RevisionBoundaryKind, RevisionKind, RevisionPayload, RevisionRecord, RichBlock,
    TrackedChanges, WriterDocument,
};

fn payload(text: &str) -> RevisionPayload {
    RevisionPayload {
        text: text.to_string(),
        runs: Vec::new(),
    }
}

fn document_xml(bytes: &[u8]) -> String {
    let archive = PackageArchive::from_bytes(bytes).expect("DOCX package reads");
    String::from_utf8(
        archive
            .get("word/document.xml")
            .expect("DOCX document part exists")
            .to_vec(),
    )
    .expect("DOCX document part is UTF-8")
}

fn assert_revisions_round_trip(expected: &WriterDocument, actual: &WriterDocument) {
    assert_eq!(
        actual.tracked_changes.recording(),
        expected.tracked_changes.recording()
    );
    assert_eq!(
        actual.tracked_changes.revisions().len(),
        expected.tracked_changes.revisions().len()
    );
    for revision in expected.tracked_changes.revisions() {
        let reopened = actual
            .tracked_changes
            .revisions()
            .iter()
            .find(|candidate| candidate.id == revision.id)
            .expect("revision ID survives DOCX round trip");
        assert_eq!(reopened.author, revision.author);
        assert_eq!(reopened.kind, revision.kind);
        assert_eq!(reopened.boundary_kind, revision.boundary_kind);
        assert_eq!(reopened.before, revision.before);
        assert_eq!(reopened.after, revision.after);
    }
}

#[test]
fn cross_paragraph_text_revisions_round_trip_without_losing_deleted_payload() {
    let mut before = WriterDocument::new("cross", "Cross paragraph revisions");
    before
        .blocks
        .push(RichBlock::new(1, "paragraph", "leftAoldBright"));
    before
        .tracked_changes
        .set_author("Cross Paragraph Reviewer")
        .unwrap();
    before.tracked_changes.set_recording(true);

    let mut changed = before.clone();
    changed
        .replace_editor_text_at("leftX\nYright", None)
        .unwrap();
    changed.record_user_edit_from(&before, None).unwrap();

    let insertion = changed
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| revision.kind == RevisionKind::Insertion)
        .expect("cross-paragraph insertion was recorded");
    assert_eq!(insertion.after.as_ref().unwrap().text, "X\nY");
    assert_eq!(insertion.boundary_kind, RevisionBoundaryKind::ParagraphMark);
    assert!(insertion.anchor.end_block_id.is_some());
    let deletion = changed
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| revision.kind == RevisionKind::Deletion)
        .expect("replaced text remains unresolved");
    assert_eq!(deletion.before.as_ref().unwrap().text, "AoldB");

    let exported = export_docx(&changed).expect("export supported paragraph-spanning text");
    let xml = document_xml(&exported.bytes);
    assert_eq!(xml.matches("<w:ins ").count(), 3);
    assert_eq!(
        xml.matches(&format!("w:id=\"{}\"", insertion.id)).count(),
        3,
        "the same stable revision ID marks both text fragments and its paragraph mark"
    );
    assert!(xml.contains("<w:rPr><w:ins "));
    assert!(xml.contains("w:author=\"Cross Paragraph Reviewer\""));
    assert!(xml.contains("<w:delText>AoldB</w:delText>"));

    let reopened = import_docx(&exported.bytes).expect("reopen tracked DOCX");
    assert_eq!(reopened.editor_text(), changed.editor_text());
    assert_eq!(reopened.blocks.len(), changed.blocks.len());
    assert_revisions_round_trip(&changed, &reopened);
}

#[test]
fn table_cell_text_revision_round_trips_in_its_cell() {
    let mut before = WriterDocument::new("table", "Table revisions");
    let block_id = before.insert_table_block(0, 1, 1).unwrap();
    before.edit_table_text(block_id, "cell").unwrap();
    before.tracked_changes.set_author("Table Reviewer").unwrap();
    before.tracked_changes.set_recording(true);

    let mut changed = before.clone();
    changed.edit_table_text(block_id, "Xcell").unwrap();
    changed.set_selection(crate::TextSelection::caret(1));
    changed.record_user_edit_from(&before, Some(1)).unwrap();
    let insertion = changed
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| revision.kind == RevisionKind::Insertion)
        .expect("table-cell insertion was recorded");
    assert_eq!(insertion.after.as_ref().unwrap().text, "X");

    let exported = export_docx(&changed).expect("export tracked table-cell text");
    let xml = document_xml(&exported.bytes);
    assert!(xml.contains("<w:tbl>"));
    assert_eq!(xml.matches("<w:ins ").count(), 1);
    assert!(xml.contains("w:author=\"Table Reviewer\""));

    let reopened = import_docx(&exported.bytes).expect("reopen tracked table DOCX");
    assert_eq!(reopened.editor_text(), changed.editor_text());
    assert!(reopened.blocks[0].typed_table().is_some());
    assert_revisions_round_trip(&changed, &reopened);
}

#[test]
fn paragraph_mark_only_insertion_round_trips_as_a_ppr_revision() {
    let mut before = WriterDocument::new("paragraph-mark", "Paragraph mark");
    before.blocks.push(RichBlock::new(1, "paragraph", "alpha"));
    before.tracked_changes.set_recording(true);
    let mut changed = before.clone();
    changed
        .replace_editor_text_at("alpha\nomega", None)
        .unwrap();
    changed.record_user_edit_from(&before, None).unwrap();

    let revision = changed.tracked_changes.revisions()[0].clone();
    assert_eq!(revision.after.as_ref().unwrap().text, "\n");
    assert_eq!(revision.boundary_kind, RevisionBoundaryKind::ParagraphMark);
    let exported = export_docx(&changed).expect("paragraph-mark insertion is supported");
    let xml = document_xml(&exported.bytes);
    assert!(xml.contains("<w:rPr><w:ins "));
    assert!(xml.contains(&format!("w:id=\"{}\" ", revision.id)));
    assert!(xml.contains("w:author="));

    let reopened = import_docx(&exported.bytes).expect("reopen paragraph-mark insertion");
    assert_eq!(reopened.editor_text(), changed.editor_text());
    assert_revisions_round_trip(&changed, &reopened);

    let mut legacy = changed.clone();
    let legacy_json = legacy
        .tracked_changes
        .write_content_json()
        .replace(r#","boundary_kind":"paragraph_mark""#, "");
    legacy.tracked_changes = TrackedChanges::from_content_json_value(&legacy_json).unwrap();
    let error = export_docx(&legacy)
        .expect_err("legacy paragraph ownership is not guessed")
        .to_string();
    assert!(error.contains("unknown boundary ownership"));
}

#[test]
fn paragraph_mark_deletion_round_trips_and_collapses_the_imported_paragraphs() {
    let mut before = WriterDocument::new("deleted-paragraph-mark", "Deleted paragraph mark");
    before.blocks.push(RichBlock::new(1, "paragraph", "left"));
    before.blocks.push(RichBlock::new(2, "paragraph", "right"));
    before.tracked_changes.set_recording(true);
    let mut changed = before.clone();
    changed
        .replace_editor_text_at("leftright", Some(4))
        .unwrap();
    changed.record_user_edit_from(&before, Some(4)).unwrap();

    let revision = changed.tracked_changes.revisions()[0].clone();
    assert_eq!(revision.before.as_ref().unwrap().text, "\n");
    assert_eq!(revision.boundary_kind, RevisionBoundaryKind::ParagraphMark);
    let exported = export_docx(&changed).expect("paragraph-mark deletion is supported");
    let xml = document_xml(&exported.bytes);
    assert!(xml.contains("<w:rPr><w:del "));
    assert!(xml.contains(&format!("w:id=\"{}\" ", revision.id)));

    let reopened = import_docx(&exported.bytes).expect("reopen paragraph-mark deletion");
    assert_eq!(reopened.editor_text(), "leftright");
    assert_eq!(reopened.blocks.len(), 1);
    assert_revisions_round_trip(&changed, &reopened);
}

#[test]
fn cross_paragraph_deletion_round_trips_text_around_its_paragraph_mark() {
    let mut before = WriterDocument::new("deleted-range", "Deleted range");
    before.blocks.push(RichBlock::new(1, "paragraph", "leftX"));
    before.blocks.push(RichBlock::new(2, "paragraph", "Yright"));
    before
        .tracked_changes
        .set_author("Cross Paragraph Reviewer")
        .unwrap();
    before.tracked_changes.set_recording(true);

    let mut changed = before.clone();
    changed
        .replace_editor_text_at("leftright", Some(4))
        .unwrap();
    changed.record_user_edit_from(&before, Some(4)).unwrap();

    let revision = changed
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| revision.kind == RevisionKind::Deletion)
        .expect("paragraph-spanning deletion remains unresolved");
    assert_eq!(revision.before.as_ref().unwrap().text, "X\nY");
    assert_eq!(revision.boundary_kind, RevisionBoundaryKind::ParagraphMark);

    let exported = export_docx(&changed).expect("export paragraph-spanning deletion");
    let xml = document_xml(&exported.bytes);
    assert_eq!(
        xml.matches(&format!("w:id=\"{}\"", revision.id)).count(),
        3,
        "the same stable revision ID marks both deleted text fragments and the paragraph mark"
    );
    assert!(xml.contains("w:author=\"Cross Paragraph Reviewer\""));
    assert!(xml.contains("<w:delText>X</w:delText>"));
    assert!(xml.contains("<w:delText>Y</w:delText>"));

    let reopened = import_docx(&exported.bytes).expect("reopen paragraph-spanning deletion");
    assert_eq!(reopened.editor_text(), changed.editor_text());
    assert_eq!(reopened.blocks.len(), 1);
    assert_revisions_round_trip(&changed, &reopened);
}

#[test]
fn cross_paragraph_formatting_round_trips_payload_styles_and_boundary() {
    let mut document = WriterDocument::new("formatting-boundary", "Formatting boundary");
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "alpha"));
    document
        .blocks
        .push(RichBlock::new(2, "paragraph", "omega"));
    let old_style = loom_text::CharacterStyle {
        color: Some("#336699".into()),
        ..Default::default()
    };
    let before = RevisionPayload {
        text: "alpha\nomega".into(),
        runs: vec![
            loom_text::StyleRun {
                start: 0,
                end: 5,
                style: old_style.clone(),
            },
            loom_text::StyleRun {
                start: 6,
                end: 11,
                style: old_style,
            },
        ],
    };
    let after = payload("alpha\nomega");
    document
        .import_docx_revisions(
            vec![RevisionRecord {
                id: 7,
                author: "Formatting Reviewer".into(),
                anchor: RevisionAnchor {
                    block_id: 1,
                    start: 0,
                    end: 5,
                    end_block_id: Some(2),
                    prefix_sha256: crate::zip::sha256(&[]),
                },
                kind: RevisionKind::Formatting,
                boundary_kind: RevisionBoundaryKind::ParagraphMark,
                before: Some(before),
                after: Some(after),
            }],
            true,
        )
        .expect("install cross-paragraph formatting");

    let exported = export_docx(&document).expect("cross-paragraph formatting is supported");
    let xml = document_xml(&exported.bytes);
    assert!(xml.matches("<w:rPrChange").count() >= 2);
    let reopened = import_docx(&exported.bytes).expect("reopen formatting revisions");
    assert_eq!(reopened.editor_text(), document.editor_text());
    assert_revisions_round_trip(&document, &reopened);
}

#[test]
fn overlapping_insertions_are_rejected_before_docx_output() {
    let mut document = WriterDocument::new("overlap", "Overlapping revisions");
    document
        .blocks
        .push(RichBlock::new(1, "paragraph", "abcdef"));
    document.tracked_changes.set_recording(true);
    document
        .record_revision(RevisionKind::Insertion, 1, 1..4, None, Some(payload("bcd")))
        .unwrap();
    document
        .record_revision(RevisionKind::Insertion, 1, 2..5, None, Some(payload("cde")))
        .unwrap();

    let error = export_docx(&document)
        .expect_err("ambiguous overlapping insertion markup must be rejected")
        .to_string();
    assert!(
        error.contains("overlapping revisions"),
        "unexpected error: {error}"
    );
}
