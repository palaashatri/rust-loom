use super::{export_docx, import_docx, PackageArchive};
use crate::{
    RevisionAnchor, RevisionBoundaryKind, RevisionKind, RevisionPayload, RevisionRecord, RichBlock,
    TrackedChanges, WriterDocument,
};
use loom_text::{Alignment, CharacterStyle, FontWeight, ParagraphDirection, StyleRun};

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

fn import_document_xml(xml: &str) -> Result<WriterDocument, String> {
    let mut archive = PackageArchive::new();
    archive
        .add("word/document.xml", xml.as_bytes().to_vec())
        .map_err(|error| error.to_string())?;
    import_docx(&archive.to_bytes().map_err(|error| error.to_string())?)
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
            std::collections::BTreeMap::new(),
            true,
            None,
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

#[test]
fn paragraph_heading_and_empty_paragraph_format_revisions_round_trip() {
    let mut before = WriterDocument::new("paragraph-format", "Paragraph format revisions");
    before
        .blocks
        .push(RichBlock::new(1, "paragraph", "body text"));
    before
        .blocks
        .push(RichBlock::new(2, "paragraph", "heading text"));
    before.blocks.push(RichBlock::new(3, "paragraph", ""));
    before
        .tracked_changes
        .set_author("Paragraph Reviewer")
        .unwrap();
    before.tracked_changes.set_recording(true);

    let mut changed = before.clone();
    changed.blocks[0].style.alignment = Alignment::Center;
    changed.blocks[0].style.space_before = 4.0;
    changed.blocks[1].kind = "heading2".into();
    changed.blocks[1].style.direction = ParagraphDirection::RightToLeft;
    changed.blocks[1].style.line_spacing = 1.5;
    changed.blocks[2].kind = "heading1".into();
    changed.blocks[2].style.alignment = Alignment::Right;
    changed.record_user_edit_from(&before, None).unwrap();

    let exported = export_docx(&changed).expect("export whole-paragraph format revisions");
    let xml = document_xml(&exported.bytes);
    assert_eq!(xml.matches("<w:pPrChange ").count(), 3);
    assert_eq!(
        xml.matches("<w:rPrChange ").count(),
        0,
        "paragraph properties are not written as inline run-property changes"
    );

    let reopened = import_docx(&exported.bytes).expect("reopen paragraph-format revisions");
    assert_eq!(reopened.blocks[0].style.alignment, Alignment::Center);
    assert_eq!(reopened.blocks[0].style.space_before, 4.0);
    assert_eq!(reopened.blocks[1].kind, "heading2");
    assert_eq!(
        reopened.blocks[1].style.direction,
        ParagraphDirection::RightToLeft
    );
    assert_eq!(reopened.blocks[1].style.line_spacing, 1.5);
    assert_eq!(reopened.blocks[2].kind, "heading1");
    assert_eq!(reopened.blocks[2].text.as_str(), "");
    assert_eq!(reopened.blocks[2].style.alignment, Alignment::Right);
    assert_revisions_round_trip(&changed, &reopened);

    for expected in changed.tracked_changes.revisions() {
        let expected_format = changed
            .tracked_changes
            .paragraph_format_change(expected.id)
            .expect("revision carries paragraph before/after formats");
        let actual_format = reopened
            .tracked_changes
            .paragraph_format_change(expected.id)
            .expect("paragraph formats survive DOCX import");
        assert_eq!(actual_format.before.kind, expected_format.before.kind);
        assert_eq!(actual_format.before.style, expected_format.before.style);
        assert_eq!(actual_format.after.kind, expected_format.after.kind);
        assert_eq!(actual_format.after.style, expected_format.after.style);
    }
}

#[test]
fn inline_and_paragraph_format_revisions_keep_separate_ooxml_properties() {
    let mut before = WriterDocument::new("mixed-format", "Inline and paragraph formats");
    before.blocks.push(RichBlock::new(1, "paragraph", "word"));
    before
        .tracked_changes
        .set_author("Formatting Reviewer")
        .unwrap();
    before.tracked_changes.set_recording(true);

    let mut changed = before.clone();
    changed.blocks[0].kind = "heading1".into();
    changed.blocks[0].style.alignment = Alignment::Center;
    changed.blocks[0].runs.push(StyleRun {
        start: 0,
        end: 4,
        style: CharacterStyle {
            weight: FontWeight::Bold,
            ..CharacterStyle::default()
        },
    });
    changed.record_user_edit_from(&before, None).unwrap();

    let paragraph_revision = changed
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| {
            changed
                .tracked_changes
                .paragraph_format_change(revision.id)
                .is_some()
        })
        .expect("paragraph format change was recorded");
    let inline_revision = changed
        .tracked_changes
        .revisions()
        .iter()
        .find(|revision| revision.id != paragraph_revision.id)
        .expect("inline format change was recorded");
    assert_ne!(paragraph_revision.id, inline_revision.id);

    let exported = export_docx(&changed).expect("export independent format revisions");
    let xml = document_xml(&exported.bytes);
    assert_eq!(xml.matches("<w:pPrChange ").count(), 1);
    assert_eq!(xml.matches("<w:rPrChange ").count(), 1);
    assert!(xml.contains(&format!("w:id=\"{}\"", paragraph_revision.id)));
    assert!(xml.contains(&format!("w:id=\"{}\"", inline_revision.id)));

    let reopened = import_docx(&exported.bytes).expect("reopen independent format revisions");
    assert_revisions_round_trip(&changed, &reopened);
    assert!(reopened
        .tracked_changes
        .paragraph_format_change(paragraph_revision.id)
        .is_some());
    assert!(reopened
        .tracked_changes
        .paragraph_format_change(inline_revision.id)
        .is_none());
}

#[test]
fn tracked_paragraph_format_revisions_inside_tables_fail_closed() {
    let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:tbl><w:tr><w:tc><w:p><w:pPr><w:jc w:val="right"/><w:pPrChange w:id="1" w:author="Reviewer"><w:pPr><w:jc w:val="center"/></w:pPr></w:pPrChange></w:pPr><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr></w:tbl></w:body></w:document>"#;
    let error = import_document_xml(xml)
        .expect_err("tracked paragraph formatting inside DOCX tables must fail closed");
    assert!(error.contains("tracked paragraph formatting inside DOCX tables is unsupported"));
}

#[test]
fn paragraph_property_import_separates_live_and_previous_ppr_and_rejects_bad_changes() {
    let xml = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:jc w:val="right"/><w:pPrChange w:id="7" w:author="Paragraph Reviewer"><w:pPr><w:jc w:val="center"/></w:pPr></w:pPrChange></w:pPr><w:r><w:t>text</w:t></w:r></w:p></w:body></w:document>"#;
    let imported = import_document_xml(xml).expect("import live and previous paragraph styles");
    assert_eq!(imported.blocks[0].style.alignment, Alignment::Right);
    let revision = &imported.tracked_changes.revisions()[0];
    let format = imported
        .tracked_changes
        .paragraph_format_change(revision.id)
        .expect("pPrChange carries paragraph snapshots");
    assert_eq!(format.before.style.alignment, Alignment::Center);
    assert_eq!(format.after.style.alignment, Alignment::Right);
    let reopened = import_docx(
        &export_docx(&imported)
            .expect("export imported paragraph change")
            .bytes,
    )
    .expect("reopen paragraph change");
    assert_eq!(reopened.blocks[0].style.alignment, Alignment::Right);
    assert_eq!(
        reopened
            .tracked_changes
            .paragraph_format_change(revision.id)
            .unwrap()
            .before
            .style
            .alignment,
        Alignment::Center
    );

    let make_document = |ppr: &str| {
        format!(
            r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p>{ppr}<w:r><w:t>text</w:t></w:r></w:p></w:body></w:document>"#
        )
    };
    let cases = [
        (
            r#"<w:pPr><w:pPrChange w:id="1" w:author="Reviewer"/></w:pPr>"#,
            "missing its previous w:pPr",
        ),
        (
            r#"<w:pPr><w:pPrChange w:id="1" w:author="Reviewer"><w:pPr><w:pPrChange w:id="2" w:author="Nested"><w:pPr/></w:pPrChange></w:pPr></w:pPrChange></w:pPr>"#,
            "outside a paragraph's direct w:pPr",
        ),
        (
            r#"<w:pPr><w:pPrChange w:id="1" w:author="Reviewer"><w:pPr/></w:pPrChange><w:pPrChange w:id="2" w:author="Reviewer"><w:pPr/></w:pPrChange></w:pPr>"#,
            "multiple w:pPrChange revisions",
        ),
        (
            r#"<w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr><w:pPrChange w:id="1" w:author="Reviewer"><w:pPr/></w:pPrChange></w:pPr>"#,
            "list mappings are unsupported",
        ),
        (
            r#"<w:pPr><w:pPrChange w:id="1" w:author="Reviewer"><w:pPr><w:keepNext/></w:pPr></w:pPrChange></w:pPr>"#,
            "w:keepNext is not supported",
        ),
        (
            r#"<w:pPr><w:pPrChange w:id="invalid" w:author="Reviewer"><w:pPr/></w:pPrChange></w:pPr>"#,
            "invalid w:id",
        ),
        (
            r#"<w:pPr><w:jc w:val="distribute"/><w:pPrChange w:id="1" w:author="Reviewer"><w:pPr/></w:pPrChange></w:pPr>"#,
            "alignment is not supported",
        ),
    ];
    for (ppr, expected_error) in cases {
        let error = import_document_xml(&make_document(ppr))
            .expect_err("malformed or unsupported paragraph-property changes are rejected");
        assert!(
            error.contains(expected_error),
            "expected {expected_error:?} in error {error:?}"
        );
    }

    let collision = r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:pPr><w:jc w:val="right"/><w:pPrChange w:id="4" w:author="Paragraph Reviewer"><w:pPr><w:jc w:val="left"/></w:pPr></w:pPrChange></w:pPr><w:r><w:rPr><w:b/><w:rPrChange w:id="4" w:author="Inline Reviewer"><w:rPr/></w:rPrChange></w:rPr><w:t>text</w:t></w:r></w:p></w:body></w:document>"#;
    let error = import_document_xml(collision)
        .expect_err("one OOXML ID cannot identify both inline and paragraph changes");
    assert!(error.contains("collides with an inline revision"));
}

#[test]
fn repeated_paragraph_format_revisions_fail_closed_for_one_ppr_change_limit() {
    let mut before = WriterDocument::new("repeated-format", "Repeated paragraph format");
    before.blocks.push(RichBlock::new(1, "paragraph", "text"));
    before.tracked_changes.set_recording(true);

    let mut first = before.clone();
    first.blocks[0].style.alignment = Alignment::Center;
    first.record_user_edit_from(&before, None).unwrap();

    let mut second = first.clone();
    second.blocks[0].style.alignment = Alignment::Right;
    second.record_user_edit_from(&first, None).unwrap();

    let error = export_docx(&second)
        .expect_err("multiple pPrChange revisions for one paragraph are unsupported")
        .to_string();
    assert!(error.contains("one paragraph-format revision per paragraph"));
}

#[test]
fn custom_author_property_preserves_future_author_and_is_validated() {
    let mut before = WriterDocument::new("author", "Tracked change author");
    before.blocks.push(RichBlock::new(1, "paragraph", "before"));
    before.tracked_changes.set_recording(true);
    before
        .tracked_changes
        .set_author("Revision Author")
        .unwrap();

    let mut changed = before.clone();
    changed.replace_editor_text_at("after", None).unwrap();
    changed.record_user_edit_from(&before, None).unwrap();
    changed
        .tracked_changes
        .set_author("Future & <Writer> — 李")
        .unwrap();

    let exported = export_docx(&changed).expect("export the future tracked-change author");
    let repeated = export_docx(&changed).expect("repeat deterministic author export");
    assert_eq!(exported.bytes, repeated.bytes);
    let archive = PackageArchive::from_bytes(&exported.bytes).expect("read exported package");
    let custom = std::str::from_utf8(archive.get("docProps/custom.xml").unwrap())
        .expect("custom properties are UTF-8");
    assert!(custom.contains("name=\"LoomTrackedChangesAuthor\""));
    assert!(custom.contains("Future &amp; &lt;Writer&gt; — 李"));
    let rels = std::str::from_utf8(archive.get("_rels/.rels").unwrap()).unwrap();
    assert!(rels.contains("Target=\"docProps/custom.xml\""));
    let content_types = std::str::from_utf8(archive.get("[Content_Types].xml").unwrap()).unwrap();
    assert!(content_types.contains("PartName=\"/docProps/custom.xml\""));

    let reopened = import_docx(&exported.bytes).expect("reopen explicit custom author");
    assert_eq!(reopened.tracked_changes.author(), "Future & <Writer> — 李");
    assert!(reopened
        .tracked_changes
        .revisions()
        .iter()
        .all(|revision| revision.author == "Revision Author"));

    let mut author_only = WriterDocument::new("author-only", "No revisions");
    author_only
        .tracked_changes
        .set_author("No Revision Author — 李")
        .unwrap();
    let no_revisions = export_docx(&author_only).expect("export author without revisions");
    let no_revisions = import_docx(&no_revisions.bytes).expect("reopen author without revisions");
    assert!(no_revisions.tracked_changes.revisions().is_empty());
    assert_eq!(
        no_revisions.tracked_changes.author(),
        "No Revision Author — 李"
    );

    let no_custom_author = import_document_xml(
        r#"<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:ins w:id="8" w:author="Last Revision Author"><w:r><w:t>text</w:t></w:r></w:ins></w:p></w:body></w:document>"#,
    )
    .expect("import a DOCX without an explicit author property");
    assert_eq!(no_custom_author.tracked_changes.author(), "Anonymous");

    let mut malformed = PackageArchive::new();
    malformed
        .add(
            "word/document.xml",
            b"<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:body/></w:document>".to_vec(),
        )
        .unwrap();
    malformed
        .add(
            "docProps/custom.xml",
            b"<Properties xmlns=\"http://schemas.openxmlformats.org/officeDocument/2006/custom-properties\" xmlns:vt=\"http://schemas.openxmlformats.org/officeDocument/2006/docPropsVTypes\"><property fmtid=\"{D5CDD505-2E9C-101B-9397-08002B2CF9AE}\" pid=\"2\" name=\"LoomTrackedChangesAuthor\"><vt:bool>true</vt:bool></property></Properties>".to_vec(),
        )
        .unwrap();
    let error = import_docx(&malformed.to_bytes().unwrap())
        .expect_err("a non-string custom author is rejected");
    assert!(error.contains("must be a string custom property"));
}

#[test]
fn docx_revision_import_installs_validated_paragraph_formats_atomically() {
    let mut document = WriterDocument::new("atomic-import", "Atomic revisions");
    document.blocks.push(RichBlock::new(1, "paragraph", "text"));
    document
        .tracked_changes
        .set_author("Existing Author")
        .unwrap();

    let change = crate::revisions::RevisionBlockFormatChange {
        before: crate::revisions::RevisionBlockFormat {
            kind: "paragraph".into(),
            style: loom_text::ParagraphStyle::default(),
        },
        after: crate::revisions::RevisionBlockFormat {
            kind: "heading1".into(),
            style: loom_text::ParagraphStyle::default(),
        },
    };
    let error = document
        .import_docx_revisions(
            Vec::new(),
            std::collections::BTreeMap::from([(7, change)]),
            true,
            Some("Imported Author".into()),
        )
        .expect_err("unmatched paragraph format IDs fail validation");
    assert!(error.contains("collides or has no revision"));
    assert_eq!(document.tracked_changes.author(), "Existing Author");
    assert!(!document.tracked_changes.recording());
    assert!(document.tracked_changes.revisions().is_empty());
}
