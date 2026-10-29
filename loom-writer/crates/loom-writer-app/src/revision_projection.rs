use std::collections::HashMap;
use std::ops::Range;

use loom_writer_core::{
    DocumentFlow, PageRect, PageStyle, RevisionKind, TextSelection, WriterDocument,
};
use slint::SharedString;

use crate::WriterRevisionMark;

const MAX_DELETION_CALLOUTS_PER_ANCHOR: usize = 3;
const DELETION_CALLOUT_STACK_STEP_PT: f32 = 14.0;

// shortcut: deletion text needs a 72 pt right or 60 pt left margin and at most three callouts per anchor; upgrade when layout reserves revision callout lanes.
pub(super) fn project(
    document: &WriterDocument,
    style: &PageStyle,
    flow: &DocumentFlow,
    base_page: PageRect,
    pages: &Range<usize>,
) -> Vec<WriterRevisionMark> {
    let content_width = (style.width_pt - style.margin_left_pt - style.margin_right_pt).max(1.0);
    let mut marks = Vec::new();
    let mut deletion_stacks = HashMap::new();
    for revision in document.tracked_changes.revisions() {
        let Some((range, _, _)) =
            crate::revision_actions::anchor_global_range(document, &revision.anchor)
        else {
            continue;
        };
        let selection = if range.is_empty() {
            TextSelection::caret(range.start)
        } else {
            TextSelection::range(range.start, range.end)
        };
        let rectangles = flow.selection_rects(document, &selection, pages.clone());
        for rect in
            super::ui_selection_rects(style, flow.zoom().max(f32::EPSILON), base_page, &rectangles)
        {
            let kind = match revision.kind {
                RevisionKind::Insertion => 0,
                RevisionKind::Deletion => 1,
                RevisionKind::Formatting => 2,
            };
            if kind == 1 {
                let stack = deletion_stacks
                    .entry((range.start, rect.page_index))
                    .or_insert(0usize);
                let stack_index = *stack;
                *stack = stack.saturating_add(1);
                let margin = if style.margin_right_pt >= 72.0 {
                    Some((
                        content_width + 4.0,
                        style.margin_right_pt / 2.0 - 12.0,
                        false,
                    ))
                } else if style.margin_left_pt >= 60.0 {
                    Some((
                        -style.margin_left_pt + 4.0,
                        style.margin_left_pt - 36.0,
                        true,
                    ))
                } else {
                    None
                };
                let fallback_x = if style.margin_left_pt >= 4.0 {
                    -4.0
                } else {
                    rect.x
                };
                let (x, mut width, right_aligned) = margin.unwrap_or((fallback_x, 2.0, false));
                let height = rect.height.max(12.0);
                let page_top = rect.page_index.max(0) as f32
                    * (style.height_pt + loom_writer_core::PAGE_GAP_PT)
                    - style.margin_top_pt;
                let page_bottom = page_top + style.height_pt;
                let offset = stack_index as f32 * DELETION_CALLOUT_STACK_STEP_PT;
                let y = if stack_index >= MAX_DELETION_CALLOUTS_PER_ANCHOR {
                    width = 0.0;
                    rect.y
                } else if rect.y + offset + height <= page_bottom {
                    rect.y + offset
                } else if rect.y - offset >= page_top {
                    rect.y - offset
                } else {
                    width = 0.0;
                    rect.y
                };
                let deleted = revision
                    .before
                    .as_ref()
                    .map_or("", |payload| payload.text.as_str());
                let accessible_text = if width > 0.0 {
                    format!("Deleted by {}: {deleted}", revision.author)
                } else {
                    String::new()
                };
                marks.push(WriterRevisionMark {
                    kind,
                    x,
                    y,
                    width,
                    height,
                    label: format!("− “{}”", crate::revision_actions::excerpt(deleted)).into(),
                    accessible_text: accessible_text.into(),
                    right_aligned,
                });
                continue;
            }
            let accessible_text = match revision.kind {
                RevisionKind::Insertion => format!("Insertion by {}", revision.author),
                RevisionKind::Formatting => format!("Formatting change by {}", revision.author),
                RevisionKind::Deletion => String::new(),
            };
            marks.push(WriterRevisionMark {
                kind,
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
                label: SharedString::new(),
                accessible_text: accessible_text.into(),
                right_aligned: false,
            });
        }
    }
    marks
}

#[cfg(test)]
mod tests {
    use super::*;
    use loom_writer_core::{PageViewport, RevisionPayload};

    fn document(text: &str) -> WriterDocument {
        let mut document = WriterDocument::new("revision-marks", "Revision marks");
        document.replace_paragraphs(text);
        document
    }

    fn formatting(text: &str, italic: bool) -> RevisionPayload {
        let mut style = loom_text::CharacterStyle::default();
        style.italic = italic;
        RevisionPayload {
            text: text.to_owned(),
            runs: vec![loom_text::StyleRun {
                start: 0,
                end: text.len(),
                style,
            }],
        }
    }

    fn reviewed_document() -> WriterDocument {
        let mut document = document("new styled");
        let block_id = document.blocks[0].id;
        document.tracked_changes.set_recording(true);
        document
            .tracked_changes
            .set_author("M6 Reviewer")
            .expect("author");
        loom_writer_core::set_selection_italic(
            &mut document,
            loom_writer_core::TextSelection::range(4, 10),
            true,
        );
        document
            .record_revision(
                RevisionKind::Insertion,
                block_id,
                0..3,
                None,
                Some(formatting("new", false)),
            )
            .expect("insertion")
            .expect("recording is on");
        document
            .record_revision(
                RevisionKind::Deletion,
                block_id,
                4..4,
                Some(RevisionPayload::plain("old")),
                None,
            )
            .expect("deletion")
            .expect("recording is on");
        document
            .record_revision(
                RevisionKind::Formatting,
                block_id,
                4..10,
                Some(formatting("styled", false)),
                Some(formatting("styled", true)),
            )
            .expect("formatting")
            .expect("recording is on");
        document
    }

    #[test]
    fn projection_marks_insertions_deletions_and_formatting() {
        let document = reviewed_document();
        let marks = super::super::project(&document, PageViewport::default(), None).revision_marks;
        assert_eq!(marks.iter().filter(|mark| mark.kind == 0).count(), 1);
        assert_eq!(marks.iter().filter(|mark| mark.kind == 1).count(), 1);
        assert_eq!(marks.iter().filter(|mark| mark.kind == 2).count(), 1);
        let deletion = marks
            .iter()
            .find(|mark| mark.kind == 1)
            .expect("deletion mark");
        assert!(deletion.width >= 24.0);
        assert!(deletion.label.as_str().contains("old"));
        assert!(deletion
            .accessible_text
            .as_str()
            .contains("Deleted by M6 Reviewer: old"));
    }

    #[test]
    fn projected_markup_follows_a_rebased_anchor() {
        let mut document = document("abcd");
        let block_id = document.blocks[0].id;
        document.tracked_changes.set_recording(true);
        document
            .record_revision(
                RevisionKind::Insertion,
                block_id,
                2..3,
                None,
                Some(RevisionPayload::plain("c")),
            )
            .expect("revision")
            .expect("recording is on");
        let before_x =
            super::super::project(&document, PageViewport::default(), None).revision_marks[0].x;

        let before = document.clone();
        document.tracked_changes.set_recording(false);
        document
            .replace_editor_text_at("Yabcd", Some(1))
            .expect("prefix edit");
        document
            .record_user_edit_from(&before, Some(1))
            .expect("rebase anchor");

        assert_eq!(
            (
                document.tracked_changes.revisions()[0].anchor.start,
                document.tracked_changes.revisions()[0].anchor.end
            ),
            (3, 4)
        );
        let marks = super::super::project(&document, PageViewport::default(), None).revision_marks;
        assert_eq!(marks.len(), 1);
        assert!(marks[0].x > before_x);
    }

    #[test]
    fn narrow_margins_keep_the_deletion_anchor_mark() {
        let mut document = document("surviving text");
        document
            .page
            .set_custom_margins_points(24.0, 24.0, 24.0, 24.0)
            .expect("custom margins");
        let block_id = document.blocks[0].id;
        document.tracked_changes.set_recording(true);
        document
            .record_revision(
                RevisionKind::Deletion,
                block_id,
                0..0,
                Some(RevisionPayload::plain("deleted words")),
                None,
            )
            .expect("deletion")
            .expect("recording is on");

        let marks = super::super::project(&document, PageViewport::default(), None).revision_marks;
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].kind, 1);
        assert!(marks[0].width < 24.0);
        assert!(marks[0].x < 0.0);
        assert!(marks[0].label.as_str().contains("deleted words"));
    }

    #[test]
    fn repeated_deletions_stack_and_defer_excess_callouts_to_the_review_panel() {
        let mut document = document("surviving text");
        let block_id = document.blocks[0].id;
        document.tracked_changes.set_recording(true);
        for index in 0..4 {
            let deleted = format!("deleted entry {index}: {}", "removed ".repeat(24));
            document
                .record_revision(
                    RevisionKind::Deletion,
                    block_id,
                    0..0,
                    Some(RevisionPayload::plain(deleted)),
                    None,
                )
                .expect("deletion")
                .expect("recording is on");
        }

        let marks = super::super::project(&document, PageViewport::default(), None).revision_marks;
        assert_eq!(marks.len(), 4);
        assert!(marks[0].width >= 24.0);
        assert!(marks[1].width >= 24.0);
        assert!(marks[1].y >= marks[0].y + DELETION_CALLOUT_STACK_STEP_PT);
        assert!(marks[2].y >= marks[1].y + DELETION_CALLOUT_STACK_STEP_PT);
        assert_eq!(marks[3].width, 0.0);
        assert!(marks[0]
            .accessible_text
            .as_str()
            .contains("deleted entry 0"));
        assert!(marks[0].accessible_text.as_str().len() > 120);
        assert!(marks[0].accessible_text.as_str().ends_with("removed "));
        assert!(marks[3].accessible_text.as_str().is_empty());
    }
}
