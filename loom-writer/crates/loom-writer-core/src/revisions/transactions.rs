use std::ops::Range;

use crate::WriterDocument;

use super::anchors::*;
use super::edits::*;
use super::*;

impl WriterDocument {
    pub fn record_revision(
        &mut self,
        kind: RevisionKind,
        block_id: u64,
        range: Range<usize>,
        before: Option<RevisionPayload>,
        after: Option<RevisionPayload>,
    ) -> Result<Option<u64>, WriterError> {
        if !self.tracked_changes.recording {
            return Ok(None);
        }
        if kind == RevisionKind::Formatting && range.is_empty() {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        let block_index = unique_block_index(&self.blocks, block_id)?;
        let prefix = self.blocks[block_index]
            .text
            .as_str()
            .get(..range.start)
            .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
        let payload = before.as_ref().or(after.as_ref());
        let boundary_kind = match payload.map(|payload| payload.text.as_str()) {
            Some(text) if text.contains('\u{b}') && text.contains('\n') => {
                RevisionBoundaryKind::Mixed
            }
            Some(text) if text.contains('\u{b}') => RevisionBoundaryKind::SoftBreak,
            Some(text) if text.contains('\n') => RevisionBoundaryKind::Unknown,
            _ => RevisionBoundaryKind::Text,
        };
        self.record_revision_at_anchor(
            kind,
            RevisionAnchor {
                block_id,
                start: range.start,
                end: range.end,
                end_block_id: None,
                prefix_sha256: crate::zip::sha256(prefix.as_bytes()),
            },
            boundary_kind,
            before,
            after,
            None,
        )
    }

    pub(super) fn record_revision_at_anchor(
        &mut self,
        kind: RevisionKind,
        anchor: RevisionAnchor,
        boundary_kind: RevisionBoundaryKind,
        before: Option<RevisionPayload>,
        after: Option<RevisionPayload>,
        block_format_change: Option<RevisionBlockFormatChange>,
    ) -> Result<Option<u64>, WriterError> {
        if !self.tracked_changes.recording {
            return Ok(None);
        }
        if kind == RevisionKind::Formatting
            && anchor.start == anchor.end
            && block_format_change.is_none()
        {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        let revision = RevisionRecord {
            id: self.tracked_changes.next_id,
            author: self.tracked_changes.author.clone(),
            anchor,
            kind,
            boundary_kind,
            before,
            after,
        };
        let next_id = revision
            .id
            .checked_add(1)
            .filter(|id| *id != 0)
            .ok_or_else(|| WriterError::Invalid("revision ID space is exhausted".into()))?;
        validate_record(&revision, &self.blocks)?;
        if self.tracked_changes.revisions.len() >= MAX_REVISIONS {
            return Err(WriterError::Invalid(
                "document revision limit reached".into(),
            ));
        }
        let next_total = self.tracked_changes.revisions.iter().fold(
            revision
                .author
                .len()
                .saturating_add(payload_bytes(&revision.before))
                .saturating_add(payload_bytes(&revision.after)),
            |total, prior| {
                total
                    .saturating_add(prior.author.len())
                    .saturating_add(payload_bytes(&prior.before))
                    .saturating_add(payload_bytes(&prior.after))
            },
        );
        if next_total > MAX_REVISION_TOTAL_BYTES {
            return Err(WriterError::Invalid(
                "tracked changes exceed the payload limit".into(),
            ));
        }
        let id = revision.id;
        self.tracked_changes.revisions.push(revision);
        if let Some(change) = block_format_change {
            self.tracked_changes.block_formats.insert(id, change);
        }
        self.tracked_changes.next_id = next_id;
        Ok(Some(id))
    }

    /// Rebase existing anchors and record one committed user edit atomically.
    pub fn record_user_edit_from(
        &mut self,
        before: &WriterDocument,
        caret: Option<usize>,
    ) -> Result<(), WriterError> {
        let old_text = before.editor_text();
        let new_text = self.editor_text();
        let edits = if old_text == new_text {
            Vec::new()
        } else {
            let edit = caret
                .and_then(|caret| {
                    crate::comments::TextEdit::from_caret(&old_text, &new_text, caret)
                })
                .unwrap_or_else(|| crate::comments::TextEdit::diff(&old_text, &new_text));
            vec![RevisionTextEdit {
                old_start: edit.old_start,
                old_end: edit.old_end,
                new_start: edit.new_start,
                new_end: edit.new_end,
            }]
        };
        self.record_user_edits_from(before, &edits)
    }

    pub fn record_user_edits_from(
        &mut self,
        before: &WriterDocument,
        edits: &[RevisionTextEdit],
    ) -> Result<(), WriterError> {
        if self.tracked_changes.revisions != before.tracked_changes.revisions {
            self.tracked_changes
                .validate_for_document(&self.blocks)
                .map_err(WriterError::Invalid)?;
            return Ok(());
        }
        before
            .tracked_changes
            .validate_for_document(&before.blocks)
            .map_err(WriterError::Invalid)?;

        let mut candidate = self.clone();
        validate_revision_text_edits(before, &candidate, edits)?;
        if candidate.tracked_changes.recording {
            validate_supported_block_format(before, &candidate)?;
        }
        candidate.rebase_revisions_through_hunks(before, edits)?;

        if candidate.tracked_changes.recording {
            if before.editor_text() != candidate.editor_text() {
                for edit in edits {
                    let old_range = edit.old_start..edit.old_end;
                    let new_range = edit.new_start..edit.new_end;
                    let recordable = (!range_touches_table(&before.blocks, old_range.clone())
                        || range_is_one_table_cell(&before.blocks, old_range))
                        && (!range_touches_table(&candidate.blocks, new_range.clone())
                            || range_is_one_table_cell(&candidate.blocks, new_range));
                    if !recordable {
                        return Err(WriterError::Invalid(UNRECORDABLE_TABLE_TEXT_EDIT.into()));
                    }
                    candidate.record_text_edit(
                        before,
                        crate::comments::TextEdit {
                            old_start: edit.old_start,
                            old_end: edit.old_end,
                            new_start: edit.new_start,
                            new_end: edit.new_end,
                        },
                    )?;
                }
                candidate.record_format_edits(before)?;
            } else {
                candidate.record_format_edits(before)?;
            }
        }
        candidate
            .tracked_changes
            .validate_for_document(&candidate.blocks)
            .map_err(WriterError::Invalid)?;
        *self = candidate;
        Ok(())
    }

    fn rebase_revisions_from(
        &mut self,
        before: &WriterDocument,
        edit: crate::comments::TextEdit,
    ) -> Result<(), WriterError> {
        let mut rebased = Vec::with_capacity(self.tracked_changes.revisions.len());
        for mut revision in self.tracked_changes.revisions.clone() {
            let (old_start, old_end) = if before.blocks.is_empty()
                && revision.kind == RevisionKind::Deletion
                && revision.anchor.end_block_id.is_none()
                && revision.anchor.start == 0
                && revision.anchor.end == 0
            {
                (0, 0)
            } else {
                anchor_global_range(&before.blocks, &revision.anchor)?
            };
            let is_block_format = self
                .tracked_changes
                .block_formats
                .contains_key(&revision.id);
            let (new_start, new_end) = if is_block_format {
                let block_index = unique_block_index(&self.blocks, revision.anchor.block_id)?;
                let block = &self.blocks[block_index];
                revision.anchor = RevisionAnchor {
                    block_id: block.id,
                    start: 0,
                    end: block.text.len_bytes(),
                    end_block_id: None,
                    prefix_sha256: crate::zip::sha256(&[]),
                };
                anchor_global_range(&self.blocks, &revision.anchor)?
            } else if old_start == old_end {
                let mapped = rebase_revision_offset(old_start, edit, true);
                (mapped, mapped)
            } else {
                let start = if edit.old_start == edit.old_end
                    && edit.old_start == old_start
                    && revision.kind != RevisionKind::Deletion
                {
                    edit.new_end
                } else {
                    rebase_revision_offset(old_start, edit, true)
                };
                let inserted_at_end = edit.old_start == edit.old_end && edit.old_end == old_end;
                (
                    start,
                    rebase_revision_offset(old_end, edit, inserted_at_end),
                )
            };
            if revision.kind != RevisionKind::Deletion && new_start == new_end && !is_block_format {
                continue;
            }
            if !is_block_format {
                revision.anchor = if self.blocks.is_empty() && new_start == new_end {
                    RevisionAnchor {
                        block_id: revision.anchor.block_id,
                        start: 0,
                        end: 0,
                        end_block_id: None,
                        prefix_sha256: crate::zip::sha256(&[]),
                    }
                } else {
                    anchor_from_global_range(&self.blocks, new_start, new_end)?
                };
            }
            match revision.kind {
                RevisionKind::Insertion => {
                    revision.after = Some(payload_for_range(&self.blocks, new_start..new_end)?);
                }
                RevisionKind::Formatting => {
                    let old_after = revision
                        .after
                        .as_ref()
                        .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
                    let new_after = payload_for_range(&self.blocks, new_start..new_end)?;
                    let mut before_payload = revision
                        .before
                        .clone()
                        .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
                    if is_block_format {
                        before_payload.runs = new_after.runs.clone();
                    } else {
                        before_payload.runs = crate::remap_style_runs(
                            &old_after.text,
                            &new_after.text,
                            &before_payload.runs,
                        );
                    }
                    before_payload.text = new_after.text.clone();
                    revision.before = Some(before_payload);
                    revision.after = Some(new_after);
                }
                RevisionKind::Deletion => {}
            }
            rebased.push(revision);
        }
        self.tracked_changes.revisions = rebased;
        Ok(())
    }

    fn rebase_revisions_through_hunks(
        &mut self,
        before: &WriterDocument,
        edits: &[RevisionTextEdit],
    ) -> Result<(), WriterError> {
        let mut rebased = Vec::with_capacity(self.tracked_changes.revisions.len());
        for mut revision in self.tracked_changes.revisions.clone() {
            let (old_start, old_end) = if before.blocks.is_empty()
                && revision.kind == RevisionKind::Deletion
                && revision.anchor.end_block_id.is_none()
                && revision.anchor.start == 0
                && revision.anchor.end == 0
            {
                (0, 0)
            } else {
                anchor_global_range(&before.blocks, &revision.anchor)?
            };
            let is_block_format = self
                .tracked_changes
                .block_formats
                .contains_key(&revision.id);
            let (new_start, new_end) = if is_block_format {
                let block_index = unique_block_index(&self.blocks, revision.anchor.block_id)?;
                let block = &self.blocks[block_index];
                revision.anchor = RevisionAnchor {
                    block_id: block.id,
                    start: 0,
                    end: block.text.len_bytes(),
                    end_block_id: None,
                    prefix_sha256: crate::zip::sha256(&[]),
                };
                anchor_global_range(&self.blocks, &revision.anchor)?
            } else if old_start == old_end {
                let mapped = rebase_revision_offset_through_hunks(old_start, edits, true);
                (mapped, mapped)
            } else {
                let insertion_at_start = revision.kind != RevisionKind::Deletion
                    && edits
                        .iter()
                        .any(|edit| edit.old_start == old_start && edit.old_end == old_start);
                let start =
                    rebase_revision_offset_through_hunks(old_start, edits, !insertion_at_start);
                // Text inserted exactly at the revision's end is a separate,
                // newer change; it must not widen this revision's payload.
                let insertion_at_end = edits
                    .iter()
                    .any(|edit| edit.old_start == old_end && edit.old_end == old_end);
                (
                    start,
                    rebase_revision_offset_through_hunks(old_end, edits, insertion_at_end),
                )
            };
            if revision.kind != RevisionKind::Deletion && new_start == new_end && !is_block_format {
                continue;
            }
            if !is_block_format {
                revision.anchor = if self.blocks.is_empty() && new_start == new_end {
                    RevisionAnchor {
                        block_id: revision.anchor.block_id,
                        start: 0,
                        end: 0,
                        end_block_id: None,
                        prefix_sha256: crate::zip::sha256(&[]),
                    }
                } else {
                    anchor_from_global_range(&self.blocks, new_start, new_end)?
                };
            }
            match revision.kind {
                RevisionKind::Insertion => {
                    revision.after = Some(payload_for_range(&self.blocks, new_start..new_end)?);
                }
                RevisionKind::Formatting => {
                    let new_after = payload_for_range(&self.blocks, new_start..new_end)?;
                    let mut before_payload = revision
                        .before
                        .clone()
                        .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
                    if is_block_format {
                        before_payload.runs = new_after.runs.clone();
                    } else {
                        before_payload.runs = remap_revision_runs_through_hunks(
                            old_start,
                            new_start,
                            edits,
                            &before_payload.runs,
                            new_after.text.len(),
                        );
                    }
                    before_payload.text = new_after.text.clone();
                    revision.before = Some(before_payload);
                    revision.after = Some(new_after);
                }
                RevisionKind::Deletion => {}
            }
            if revision.kind != RevisionKind::Deletion
                && revision.boundary_kind != RevisionBoundaryKind::Unknown
            {
                let payload = revision
                    .after
                    .as_ref()
                    .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
                revision.boundary_kind = Self::revision_boundary_kind_for_range(
                    &self.blocks,
                    new_start..new_end,
                    payload,
                );
            }
            rebased.push(revision);
        }
        self.tracked_changes.revisions = rebased;
        Ok(())
    }

    fn revision_boundary_kind_for_range(
        blocks: &[RichBlock],
        range: Range<usize>,
        payload: &RevisionPayload,
    ) -> RevisionBoundaryKind {
        let mut offset = 0usize;
        let mut paragraph_mark = false;
        for (index, block) in blocks.iter().enumerate() {
            let separator = offset + block.text.len_bytes();
            if index + 1 < blocks.len() && range.start <= separator && separator < range.end {
                paragraph_mark = true;
            }
            offset = separator + usize::from(index + 1 < blocks.len());
        }
        let soft_break = payload.text.contains('\u{b}');
        match (paragraph_mark, soft_break) {
            (true, true) => RevisionBoundaryKind::Mixed,
            (true, false) => RevisionBoundaryKind::ParagraphMark,
            (false, true) => RevisionBoundaryKind::SoftBreak,
            (false, false) if payload.text.contains('\n') => RevisionBoundaryKind::Unknown,
            (false, false) => RevisionBoundaryKind::Text,
        }
    }

    fn record_text_edit(
        &mut self,
        before: &WriterDocument,
        edit: crate::comments::TextEdit,
    ) -> Result<(), WriterError> {
        if edit.old_start < edit.old_end {
            let old_range = edit.old_start..edit.old_end;
            let old_payload = payload_for_range(&before.blocks, old_range.clone())?;
            let boundary_kind =
                Self::revision_boundary_kind_for_range(&before.blocks, old_range, &old_payload);
            let anchor = if self.blocks.is_empty() {
                let (index, _) = block_location_at(&before.blocks, edit.old_start)
                    .ok_or_else(|| WriterError::Invalid(INVALID_REVISION.into()))?;
                let block = &before.blocks[index];
                RevisionAnchor {
                    block_id: block.id,
                    start: 0,
                    end: 0,
                    end_block_id: None,
                    prefix_sha256: crate::zip::sha256(&[]),
                }
            } else {
                anchor_from_global_range(&self.blocks, edit.new_start, edit.new_start)?
            };
            self.record_revision_at_anchor(
                RevisionKind::Deletion,
                anchor,
                boundary_kind,
                Some(old_payload),
                None,
                None,
            )?;
        }
        if edit.new_start < edit.new_end {
            let new_range = edit.new_start..edit.new_end;
            let payload = payload_for_range(&self.blocks, new_range)?;
            // A leading hard line break creates the paragraph mark of the new
            // paragraph. Word treats that mark as its own revision, so keep it
            // separate from the text typed after the break.
            let paragraph_marks = payload
                .text
                .bytes()
                .take_while(|byte| *byte == b'\n')
                .count();
            let mut text_start = edit.new_start;
            for _ in 0..paragraph_marks {
                let anchor = anchor_from_global_range(&self.blocks, text_start, text_start + 1)?;
                self.record_revision_at_anchor(
                    RevisionKind::Insertion,
                    anchor,
                    RevisionBoundaryKind::ParagraphMark,
                    None,
                    Some(RevisionPayload::plain("\n")),
                    None,
                )?;
                text_start += 1;
            }
            if text_start < edit.new_end {
                let text_range = text_start..edit.new_end;
                let text_payload = payload_for_range(&self.blocks, text_range.clone())?;
                let text_boundary = Self::revision_boundary_kind_for_range(
                    &self.blocks,
                    text_range.clone(),
                    &text_payload,
                );
                let anchor = anchor_from_global_range(&self.blocks, text_start, edit.new_end)?;
                self.record_revision_at_anchor(
                    RevisionKind::Insertion,
                    anchor,
                    text_boundary,
                    None,
                    Some(text_payload),
                    None,
                )?;
            }
        }
        Ok(())
    }

    fn record_format_edits(&mut self, before: &WriterDocument) -> Result<(), WriterError> {
        for old_block in &before.blocks {
            let Some(index) = self
                .blocks
                .iter()
                .position(|block| block.id == old_block.id)
            else {
                continue;
            };
            let block_id = self.blocks[index].id;
            let block_text = self.blocks[index].text.as_str().to_owned();
            if old_block.text.as_str() != block_text {
                continue;
            }
            let before_runs = block_revision_runs(old_block);
            let after_runs = block_revision_runs(&self.blocks[index]);
            let ranges = style_change_ranges(&block_text, &before_runs, &after_runs);
            for range in ranges {
                if !table_range_is_one_cell(&self.blocks[index], range.clone()) {
                    return Err(WriterError::Invalid(INVALID_REVISION.into()));
                }
                let before_payload = RevisionPayload {
                    text: old_block.text.as_str()[range.clone()].to_owned(),
                    runs: runs_in_range(&before_runs, range.clone()),
                };
                let after_payload = RevisionPayload {
                    text: block_text[range.clone()].to_owned(),
                    runs: runs_in_range(&after_runs, range.clone()),
                };
                self.record_revision_at_anchor(
                    RevisionKind::Formatting,
                    RevisionAnchor {
                        block_id,
                        start: range.start,
                        end: range.end,
                        end_block_id: None,
                        prefix_sha256: crate::zip::sha256(
                            &self.blocks[index].text.as_str().as_bytes()[..range.start],
                        ),
                    },
                    RevisionBoundaryKind::Unknown,
                    Some(before_payload),
                    Some(after_payload),
                    None,
                )?;
            }
        }
        self.record_block_format_edits(before)
    }

    fn record_block_format_edits(&mut self, before: &WriterDocument) -> Result<(), WriterError> {
        super::edits::record_block_format_edits(self, before)
    }

    pub fn accept_revision(&mut self, id: u64) -> Result<bool, WriterError> {
        let Some(index) = self
            .tracked_changes
            .revisions
            .iter()
            .position(|revision| revision.id == id)
        else {
            return Ok(false);
        };
        let mut candidate = self.clone();
        candidate
            .tracked_changes
            .validate_for_document(&candidate.blocks)
            .map_err(WriterError::Invalid)?;
        let revision = candidate.tracked_changes.revisions[index].clone();
        if matches!(
            revision.kind,
            RevisionKind::Insertion | RevisionKind::Deletion
        ) {
            ensure_revision_resolution_order(
                &candidate.blocks,
                &candidate.tracked_changes.revisions,
                id,
                &revision,
                RevisionResolutionOrder::AcceptOldest,
            )?;
        }
        if candidate.tracked_changes.block_formats.contains_key(&id) {
            let block_id = revision.anchor.block_id;
            let has_older_block_format =
                candidate
                    .tracked_changes
                    .block_formats
                    .keys()
                    .any(|other_id| {
                        *other_id < id
                            && candidate.tracked_changes.revisions.iter().any(|other| {
                                other.id == *other_id && other.anchor.block_id == block_id
                            })
                    });
            if has_older_block_format {
                return Err(WriterError::Invalid(REVISION_DEPENDENCY_CONFLICT.into()));
            }
        }
        candidate.tracked_changes.revisions.remove(index);
        candidate.tracked_changes.block_formats.remove(&id);
        candidate
            .tracked_changes
            .validate_for_document(&candidate.blocks)
            .map_err(WriterError::Invalid)?;
        *self = candidate;
        Ok(true)
    }

    /// Accepts every pending revision while keeping the current document content.
    pub fn accept_all_revisions(&mut self) -> Result<usize, WriterError> {
        self.tracked_changes
            .validate_for_document(&self.blocks)
            .map_err(WriterError::Invalid)?;
        let accepted = self.tracked_changes.revisions.len();
        self.tracked_changes.revisions.clear();
        self.tracked_changes.block_formats.clear();
        Ok(accepted)
    }

    /// Rejects pending revisions newest-first as one atomic document transaction.
    pub fn reject_all_revisions(&mut self) -> Result<usize, WriterError> {
        let mut candidate = self.clone();
        candidate
            .tracked_changes
            .validate_for_document(&candidate.blocks)
            .map_err(WriterError::Invalid)?;
        let mut revision_ids = candidate
            .tracked_changes
            .revisions
            .iter()
            .map(|revision| revision.id)
            .collect::<Vec<_>>();
        revision_ids.sort_unstable_by(|left, right| right.cmp(left));
        let rejected = revision_ids.len();
        for id in revision_ids {
            candidate.reject_revision(id)?;
        }
        if !candidate.tracked_changes.revisions.is_empty() {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        candidate
            .tracked_changes
            .validate_for_document(&candidate.blocks)
            .map_err(WriterError::Invalid)?;
        *self = candidate;
        Ok(rejected)
    }

    pub fn reject_revision(&mut self, id: u64) -> Result<bool, WriterError> {
        let Some(index) = self
            .tracked_changes
            .revisions
            .iter()
            .position(|revision| revision.id == id)
        else {
            return Ok(false);
        };
        let mut candidate = self.clone();
        candidate
            .tracked_changes
            .validate_for_document(&candidate.blocks)
            .map_err(WriterError::Invalid)?;
        let revision = candidate.tracked_changes.revisions[index].clone();
        if matches!(
            revision.kind,
            RevisionKind::Insertion | RevisionKind::Deletion
        ) {
            ensure_revision_resolution_order(
                &candidate.blocks,
                &candidate.tracked_changes.revisions,
                id,
                &revision,
                RevisionResolutionOrder::RejectNewest,
            )?;
        }
        if candidate.tracked_changes.block_formats.contains_key(&id) {
            let block_id = revision.anchor.block_id;
            let has_newer_block_format =
                candidate
                    .tracked_changes
                    .block_formats
                    .keys()
                    .any(|other_id| {
                        *other_id > id
                            && candidate.tracked_changes.revisions.iter().any(|other| {
                                other.id == *other_id && other.anchor.block_id == block_id
                            })
                    });
            if has_newer_block_format {
                return Err(WriterError::Invalid(REVISION_DEPENDENCY_CONFLICT.into()));
            }
        }
        let before = candidate.clone();

        match revision.kind {
            RevisionKind::Insertion | RevisionKind::Deletion => {
                let replacement = if revision.kind == RevisionKind::Insertion {
                    RevisionPayload::plain("")
                } else {
                    revision.before.clone().expect("validated deletion payload")
                };
                candidate.splice_revision_text(&revision.anchor, &replacement)?;
                let edit = crate::comments::TextEdit::diff(
                    &before.editor_text(),
                    &candidate.editor_text(),
                );
                if edit.old_start != edit.old_end || edit.new_start != edit.new_end {
                    candidate.rebase_revisions_from(&before, edit)?;
                }
            }
            RevisionKind::Formatting => {
                let payload = revision.before.as_ref().expect("validated format payload");
                if let Some(change) = candidate.tracked_changes.block_formats.get(&id) {
                    let block_index =
                        unique_block_index(&candidate.blocks, revision.anchor.block_id)?;
                    candidate.blocks[block_index].kind = change.before.kind.clone();
                    candidate.blocks[block_index].style = change.before.style.clone();
                }
                let (start, end) = anchor_global_range(&candidate.blocks, &revision.anchor)?;
                candidate.apply_revision_payload_runs(start..end, &payload.runs)?;
            }
        }
        candidate
            .tracked_changes
            .revisions
            .retain(|pending| pending.id != id);
        candidate.tracked_changes.block_formats.remove(&id);
        candidate
            .tracked_changes
            .validate_for_document(&candidate.blocks)
            .map_err(WriterError::Invalid)?;
        *self = candidate;
        Ok(true)
    }

    fn splice_revision_text(
        &mut self,
        anchor: &RevisionAnchor,
        payload: &RevisionPayload,
    ) -> Result<(), WriterError> {
        let (global_start, global_end) = if self.blocks.is_empty() {
            (0, 0)
        } else {
            anchor_global_range(&self.blocks, anchor)?
        };
        let mut new_text = self.editor_text();
        new_text.replace_range(global_start..global_end, &payload.text);
        let new_end = global_start + payload.text.len();
        self.replace_editor_text_at(&new_text, Some(new_end))?;
        self.apply_revision_payload_runs(global_start..new_end, &payload.runs)
    }

    fn apply_revision_payload_runs(
        &mut self,
        range: Range<usize>,
        runs: &[StyleRun],
    ) -> Result<(), WriterError> {
        if range.is_empty() {
            return Ok(());
        }
        let Some((start_index, start_local)) = block_location_at(&self.blocks, range.start) else {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        };
        let Some((end_index, end_local)) = block_location_at(&self.blocks, range.end) else {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        };
        if start_index == end_index && self.blocks[start_index].typed_table().is_some() {
            if !table_range_is_one_cell(&self.blocks[start_index], start_local..end_local) {
                return Err(WriterError::Invalid(INVALID_REVISION.into()));
            }
            self.set_table_text_runs(range.clone(), reset_table_run_style)
                .map_err(WriterError::Invalid)?;
            for run in runs {
                self.set_table_text_runs(
                    range.start + run.start..range.start + run.end,
                    |target| apply_table_run_style(target, &run.style),
                )
                .map_err(WriterError::Invalid)?;
            }
            return Ok(());
        }
        for index in start_index..=end_index {
            let block_start = block_start_offset(&self.blocks, index);
            let block_end = block_start + self.blocks[index].text.len_bytes();
            let start = range.start.max(block_start);
            let end = range.end.min(block_end);
            if start >= end {
                continue;
            }
            let local = start - block_start..end - block_start;
            let replacement = runs_in_range(runs, start - range.start..end - range.start);
            let block = &mut self.blocks[index];
            if block.typed_table().is_some() {
                return Err(WriterError::Invalid(INVALID_REVISION.into()));
            }
            block.runs = replace_runs_in_range(&block.runs, local, &replacement);
        }
        Ok(())
    }
}
