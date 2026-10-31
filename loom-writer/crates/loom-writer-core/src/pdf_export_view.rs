//! PDF exports with an explicit tracked-change view.
use crate::{
    export_pdf, RevisionKind, RevisionPayload, RevisionRecord, RichBlock, WriterDocument,
    WriterError,
};

/// The tracked-change view included in a PDF export.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfRevisionView {
    /// Export the current document contents, which include insertions and omit deletions.
    Final,
    /// Export the current contents followed by a complete unresolved-change appendix.
    Review,
}

impl PdfRevisionView {
    /// A stable, user-facing name for the selected export view.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Final => "Final",
            Self::Review => "Review",
        }
    }
}

/// Export a PDF with the selected tracked-change view.
///
/// The existing export_pdf remains the compatibility path and does not add a view label or
/// revision appendix. This explicit API labels the PDF itself. Review exports include the
/// current document followed by a paginated appendix that preserves every unresolved
/// insertion, deletion, and formatting revision.
pub fn export_pdf_with_view(
    document: &WriterDocument,
    view: PdfRevisionView,
) -> Result<Vec<u8>, WriterError> {
    let mut export_document = document.clone();
    let mut next_block_id = next_block_id(&export_document)?;

    if view == PdfRevisionView::Review {
        document
            .tracked_changes
            .validate_for_document(&document.blocks)
            .map_err(WriterError::Invalid)?;
    }

    push_export_block(
        &mut export_document,
        &mut next_block_id,
        "paragraph",
        format!("PDF view: {}", view.name()),
    )?;

    if view == PdfRevisionView::Review {
        append_review_appendix(
            &mut export_document,
            &mut next_block_id,
            document.tracked_changes.revisions(),
        )?;
    }

    export_pdf(&export_document)
}

fn next_block_id(document: &WriterDocument) -> Result<Option<u64>, WriterError> {
    document
        .blocks
        .iter()
        .map(|block| block.id)
        .max()
        .unwrap_or(0)
        .checked_add(1)
        .map(Some)
        .ok_or_else(|| WriterError::Invalid("PDF export could not allocate a view label".into()))
}

fn push_export_block(
    document: &mut WriterDocument,
    next_id: &mut Option<u64>,
    kind: &str,
    text: String,
) -> Result<(), WriterError> {
    let id = next_id
        .take()
        .ok_or_else(|| WriterError::Invalid("PDF review block ID space is exhausted".into()))?;
    *next_id = id.checked_add(1);
    document.push(RichBlock::new(id, kind, &text));
    Ok(())
}

fn append_review_appendix(
    document: &mut WriterDocument,
    next_id: &mut Option<u64>,
    revisions: &[RevisionRecord],
) -> Result<(), WriterError> {
    push_export_block(
        document,
        next_id,
        "heading2",
        format!("Unresolved tracked changes ({})", revisions.len()),
    )?;
    if revisions.is_empty() {
        return push_export_block(
            document,
            next_id,
            "paragraph",
            "No unresolved tracked changes.".into(),
        );
    }

    for revision in revisions {
        let kind = revision_kind_name(revision.kind);
        push_export_block(
            document,
            next_id,
            "heading3",
            format!(
                "Change #{} - {} - by {}",
                revision.id, kind, revision.author
            ),
        )?;
        let end = match revision.anchor.end_block_id {
            Some(block_id) => format!("through block {}, byte {}", block_id, revision.anchor.end),
            None => format!("byte {}", revision.anchor.end),
        };
        push_export_block(
            document,
            next_id,
            "paragraph",
            format!(
                "Anchor: block {}, byte {} to {}",
                revision.anchor.block_id, revision.anchor.start, end
            ),
        )?;

        match revision.kind {
            RevisionKind::Insertion => {
                let after = required_payload(revision, revision.after.as_ref())?;
                append_payload(document, next_id, "Inserted text", after)?;
            }
            RevisionKind::Deletion => {
                let before = required_payload(revision, revision.before.as_ref())?;
                append_payload(document, next_id, "Deleted text", before)?;
            }
            RevisionKind::Formatting => {
                let before = required_payload(revision, revision.before.as_ref())?;
                let after = required_payload(revision, revision.after.as_ref())?;
                push_export_block(
                    document,
                    next_id,
                    "paragraph",
                    format!("Formatted text: {}", after.text),
                )?;
                append_formatting(document, next_id, "Before", before)?;
                append_formatting(document, next_id, "After", after)?;
            }
        }
    }

    Ok(())
}

fn required_payload<'a>(
    revision: &RevisionRecord,
    payload: Option<&'a RevisionPayload>,
) -> Result<&'a RevisionPayload, WriterError> {
    payload.ok_or_else(|| {
        WriterError::Invalid(format!(
            "tracked change {} is missing its {} payload",
            revision.id,
            revision_kind_name(revision.kind)
        ))
    })
}

fn append_payload(
    document: &mut WriterDocument,
    next_id: &mut Option<u64>,
    label: &str,
    payload: &RevisionPayload,
) -> Result<(), WriterError> {
    push_export_block(
        document,
        next_id,
        "paragraph",
        format!("{label}: {}", payload.text),
    )?;
    append_formatting(document, next_id, label, payload)
}

fn append_formatting(
    document: &mut WriterDocument,
    next_id: &mut Option<u64>,
    label: &str,
    payload: &RevisionPayload,
) -> Result<(), WriterError> {
    let description = if payload.runs.is_empty() {
        format!("{label} formatting runs: inherited formatting")
    } else {
        format!("{label} formatting runs: {:?}", payload.runs)
    };
    push_export_block(document, next_id, "paragraph", description)
}

const fn revision_kind_name(kind: RevisionKind) -> &'static str {
    match kind {
        RevisionKind::Insertion => "Insertion",
        RevisionKind::Deletion => "Deletion",
        RevisionKind::Formatting => "Formatting",
    }
}

#[cfg(test)]
mod tests {
    use super::{export_pdf_with_view, PdfRevisionView};
    use crate::{RevisionKind, RevisionPayload, RichBlock, WriterDocument};
    use loom_text::{CharacterStyle, StyleRun};

    fn revision_document() -> WriterDocument {
        let mut document = WriterDocument::new("tracked-pdf", "Tracked PDF");
        document.push(RichBlock::new(1, "paragraph", "new idea"));
        document.push(RichBlock::new(2, "paragraph", "keep"));
        document.push(RichBlock::new(3, "paragraph", "styled"));
        document.tracked_changes.set_recording(true);
        document
            .tracked_changes
            .set_author("Ada Editor")
            .expect("valid author");

        document
            .record_revision(
                RevisionKind::Insertion,
                1,
                0..3,
                None,
                Some(RevisionPayload {
                    text: "new".into(),
                    runs: Vec::new(),
                }),
            )
            .expect("record insertion");
        document
            .record_revision(
                RevisionKind::Deletion,
                2,
                4..4,
                Some(RevisionPayload {
                    text: "removed".into(),
                    runs: Vec::new(),
                }),
                None,
            )
            .expect("record deletion");

        let before_runs = vec![StyleRun {
            start: 0,
            end: 6,
            style: CharacterStyle::default(),
        }];
        let after_style = CharacterStyle {
            italic: true,
            ..CharacterStyle::default()
        };
        let after_runs = vec![StyleRun {
            start: 0,
            end: 6,
            style: after_style,
        }];
        document.blocks[2].runs = after_runs.clone();
        document
            .record_revision(
                RevisionKind::Formatting,
                3,
                0..6,
                Some(RevisionPayload {
                    text: "styled".into(),
                    runs: before_runs,
                }),
                Some(RevisionPayload {
                    text: "styled".into(),
                    runs: after_runs,
                }),
            )
            .expect("record formatting");
        document
    }

    fn pdf_text(bytes: &[u8]) -> String {
        loom_pdf::inspect::page_text(bytes)
            .expect("read generated PDF text")
            .join("\n")
    }

    #[test]
    fn selected_views_label_the_pdf_and_preserve_actual_revision_content() {
        let document = revision_document();
        let before_export = document.clone();

        assert_eq!(PdfRevisionView::Final.name(), "Final");
        assert_eq!(PdfRevisionView::Review.name(), "Review");

        let final_pdf =
            export_pdf_with_view(&document, PdfRevisionView::Final).expect("final-view PDF");
        let final_text = pdf_text(&final_pdf);
        assert!(final_text.contains("PDF view: Final"));
        assert!(final_text.contains("new idea"));
        assert!(final_text.contains("keep"));
        assert!(final_text.contains("styled"));
        assert!(!final_text.contains("removed"));
        assert!(!final_text.contains("Unresolved tracked changes"));

        let review_pdf =
            export_pdf_with_view(&document, PdfRevisionView::Review).expect("review-view PDF");
        let review_text = pdf_text(&review_pdf);
        assert!(review_text.contains("PDF view: Review"));
        assert!(review_text.contains("Unresolved tracked changes (3)"));
        assert!(review_text.contains("Ada Editor"));
        assert!(review_text.contains("Insertion"));
        assert!(review_text.contains("Inserted text: new"));
        assert!(review_text.contains("Deletion"));
        assert!(review_text.contains("Deleted text: removed"));
        assert!(review_text.contains("Formatting"));
        assert!(review_text.contains("Formatted text: styled"));
        assert!(review_text.contains("Before formatting runs"));
        assert!(review_text.contains("After formatting runs"));
        assert!(review_text.contains("italic: true"));

        assert_eq!(document, before_export);
    }

    #[test]
    fn review_view_identifies_documents_without_pending_changes() {
        let document = WriterDocument::new("plain-pdf", "Plain PDF");
        let bytes =
            export_pdf_with_view(&document, PdfRevisionView::Review).expect("review-view PDF");
        let text = pdf_text(&bytes);
        assert!(text.contains("PDF view: Review"));
        assert!(text.contains("No unresolved tracked changes."));
    }
}
