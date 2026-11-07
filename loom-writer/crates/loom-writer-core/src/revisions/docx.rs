use std::collections::{BTreeMap, HashSet};

use super::{
    RevisionBlockFormatChange, RevisionBoundaryKind, RevisionKind, RevisionPayload, RevisionRecord,
    TrackedChanges,
};
use crate::{RichBlock, WriterDocument};

impl WriterDocument {
    pub(crate) fn import_docx_revisions(
        &mut self,
        mut revisions: Vec<RevisionRecord>,
        mut block_formats: BTreeMap<u64, RevisionBlockFormatChange>,
        recording: bool,
        author: Option<String>,
    ) -> Result<(), String> {
        if revisions.len() > super::MAX_REVISIONS || block_formats.len() > super::MAX_REVISIONS {
            return Err("DOCX has too many tracked changes".into());
        }
        if revisions.iter().filter(|revision| revision.id == 0).count() > 1 {
            return Err("DOCX reuses revision ID 0 for separate changes".into());
        }
        let mut ids = HashSet::with_capacity(revisions.len());
        for revision in revisions.iter().filter(|revision| revision.id != 0) {
            if !ids.insert(revision.id) {
                return Err("DOCX reuses a revision ID for separate changes".into());
            }
        }
        if let Some(revision) = revisions.iter_mut().find(|revision| revision.id == 0) {
            let replacement = ids
                .iter()
                .copied()
                .max()
                .unwrap_or(0)
                .checked_add(1)
                .ok_or_else(|| "DOCX revision IDs exceed the supported range".to_string())?;
            revision.id = replacement;
            ids.insert(replacement);
            if let Some(change) = block_formats.remove(&0) {
                block_formats.insert(replacement, change);
            }
        }
        if block_formats
            .keys()
            .any(|revision_id| !ids.contains(revision_id))
        {
            return Err("DOCX paragraph-format revision ID collides or has no revision".into());
        }
        let next_id = ids
            .iter()
            .copied()
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| "DOCX revision IDs exceed the supported range".to_string())?;
        let mut tracked_changes = TrackedChanges {
            recording,
            author: "Anonymous".into(),
            next_id,
            revisions,
            block_formats,
        };
        if let Some(author) = author {
            tracked_changes
                .set_author(author)
                .map_err(|error| format!("invalid DOCX tracked-change author: {error}"))?;
        }
        tracked_changes.validate_for_document(&self.blocks)?;
        self.tracked_changes = tracked_changes;
        Ok(())
    }
}

impl WriterDocument {
    pub(crate) fn merge_docx_paragraph_revision(
        existing: &mut RevisionRecord,
        next: RevisionRecord,
        previous_block: &RichBlock,
        current_block: &RichBlock,
    ) -> Result<(), String> {
        if existing.id != next.id || existing.author != next.author || existing.kind != next.kind {
            return Err("DOCX revision ID has conflicting or reused metadata".into());
        }
        let is_text_paragraph = |block: &RichBlock| {
            block.typed_table().is_none()
                && !block.is_image()
                && block.kind != crate::CHART_BLOCK_KIND
                && block.kind != crate::TABLE_BLOCK_KIND
        };
        if !is_text_paragraph(previous_block)
            || !is_text_paragraph(current_block)
            || previous_block.text.is_empty()
            || current_block.text.is_empty()
        {
            return Err("DOCX revision ID spans unsupported blocks".into());
        }
        let previous_end_block = existing
            .anchor
            .end_block_id
            .unwrap_or(existing.anchor.block_id);
        if previous_end_block != previous_block.id
            || existing.anchor.end != previous_block.text.len_bytes()
            || next.anchor.block_id != current_block.id
            || next.anchor.end_block_id.is_some()
        {
            return Err("DOCX revision ID spans disjoint text ranges".into());
        }
        match existing.kind {
            RevisionKind::Insertion | RevisionKind::Formatting => {
                if next.anchor.start != 0
                    || next.anchor.end == 0
                    || next.anchor.end > current_block.text.len_bytes()
                {
                    return Err("DOCX revision ID spans disjoint text ranges".into());
                }
                if existing.kind == RevisionKind::Insertion
                    && existing.boundary_kind != RevisionBoundaryKind::ParagraphMark
                {
                    return Err(
                        "cross-paragraph DOCX insertion is missing its paragraph-mark revision"
                            .into(),
                    );
                }
                let current_prefix = current_block
                    .text
                    .as_str()
                    .get(..next.anchor.end)
                    .ok_or_else(|| "DOCX revision anchor is outside its paragraph".to_string())?;
                let previous_text = if existing.anchor.end_block_id.is_some() {
                    previous_block.text.as_str()
                } else {
                    previous_block
                        .text
                        .as_str()
                        .get(existing.anchor.start..)
                        .ok_or_else(|| {
                            "DOCX revision anchor is outside its paragraph".to_string()
                        })?
                };
                for payload in [&existing.before, &existing.after].into_iter().flatten() {
                    let fragment = payload.text.strip_suffix('\n').unwrap_or(&payload.text);
                    if !fragment.ends_with(previous_text) {
                        return Err("DOCX revision fragments are not paragraph-contiguous".into());
                    }
                }
                for payload in [&next.before, &next.after].into_iter().flatten() {
                    if !payload.text.starts_with(current_prefix) {
                        return Err("DOCX revision fragments are not paragraph-contiguous".into());
                    }
                }
            }
            RevisionKind::Deletion => {
                if existing.boundary_kind != RevisionBoundaryKind::ParagraphMark
                    || existing.anchor.start != previous_block.text.len_bytes()
                    || next.anchor.start != 0
                    || next.anchor.end != 0
                    || existing.after.is_some()
                    || next.after.is_some()
                {
                    return Err(
                        "cross-paragraph DOCX deletion requires one adjacent paragraph mark".into(),
                    );
                }
            }
        }
        match (&mut existing.before, &next.before) {
            (Some(target), Some(piece)) => Self::append_docx_paragraph_payload(target, piece)?,
            (None, None) => {}
            _ => return Err("DOCX revision payload is inconsistent".into()),
        }
        match (&mut existing.after, &next.after) {
            (Some(target), Some(piece)) => Self::append_docx_paragraph_payload(target, piece)?,
            (None, None) => {}
            _ => return Err("DOCX revision payload is inconsistent".into()),
        }
        existing.anchor.end = next.anchor.end;
        existing.anchor.end_block_id = Some(current_block.id);
        if existing.kind == RevisionKind::Formatting {
            existing.boundary_kind = RevisionBoundaryKind::ParagraphMark;
        }
        Ok(())
    }

    /// Rejects pending revisions whose DOCX text and paragraph-mark ranges cannot be
    /// represented without changing the unresolved payload.
    pub(crate) fn validate_docx_revision_export(&self) -> Result<(), String> {
        self.tracked_changes.validate_for_document(&self.blocks)?;
        let revisions = self.tracked_changes.revisions();
        let mut ranges = Vec::with_capacity(revisions.len());
        let mut paragraph_format_ids = HashSet::new();
        let mut paragraph_format_blocks = HashSet::new();
        let mut block_starts = Vec::with_capacity(self.blocks.len());
        let mut block_start = 0usize;
        for (index, block) in self.blocks.iter().enumerate() {
            block_starts.push(block_start);
            block_start += block.text.len_bytes();
            if index + 1 < self.blocks.len() {
                block_start += 1;
            }
        }

        for revision in revisions {
            let start_index = self
                .blocks
                .iter()
                .position(|block| block.id == revision.anchor.block_id)
                .ok_or_else(|| "tracked change refers to a missing paragraph".to_string())?;
            let end_block_id = revision
                .anchor
                .end_block_id
                .unwrap_or(revision.anchor.block_id);
            let end_index = self
                .blocks
                .iter()
                .position(|block| block.id == end_block_id)
                .ok_or_else(|| "tracked change refers to a missing paragraph".to_string())?;
            let start_block = &self.blocks[start_index];
            let end_block = &self.blocks[end_index];
            let cross_paragraph = start_index != end_index;
            if let Some(change) = self.tracked_changes.paragraph_format_change(revision.id) {
                if cross_paragraph || !paragraph_format_blocks.insert(start_block.id) {
                    return Err("DOCX supports one paragraph-format revision per paragraph".into());
                }
                Self::validate_docx_paragraph_format(change.before.kind, change.before.style)?;
                Self::validate_docx_paragraph_format(change.after.kind, change.after.style)?;
                paragraph_format_ids.insert(revision.id);
            }
            let is_text_paragraph = |block: &RichBlock| {
                block.typed_table().is_none()
                    && block.kind != crate::TABLE_BLOCK_KIND
                    && !block.is_image()
                    && block.kind != crate::CHART_BLOCK_KIND
            };
            let payload_texts: Vec<&str> = [&revision.before, &revision.after]
                .into_iter()
                .flatten()
                .map(|payload| payload.text.as_str())
                .collect();
            let has_paragraph_mark = payload_texts.iter().any(|text| text.contains('\n'));
            let has_soft_break = payload_texts.iter().any(|text| text.contains('\u{b}'));
            match revision.boundary_kind {
                RevisionBoundaryKind::Unknown if has_paragraph_mark || has_soft_break => {
                    return Err(
                        "DOCX export rejects legacy revisions with unknown boundary ownership"
                            .into(),
                    );
                }
                RevisionBoundaryKind::Text if has_paragraph_mark || has_soft_break => {
                    return Err("tracked revision boundary kind does not match its payload".into());
                }
                RevisionBoundaryKind::ParagraphMark if !has_paragraph_mark || has_soft_break => {
                    return Err(
                        "tracked paragraph-mark revision has ambiguous boundary payload".into(),
                    );
                }
                RevisionBoundaryKind::SoftBreak if !has_soft_break || has_paragraph_mark => {
                    return Err("tracked soft-break revision has ambiguous boundary payload".into());
                }
                RevisionBoundaryKind::Mixed => {
                    return Err(
                        "DOCX export does not support mixed paragraph and soft breaks".into(),
                    );
                }
                _ => {}
            }

            let table_revision = start_block.typed_table().is_some();
            if table_revision {
                if cross_paragraph
                    || revision.anchor.end_block_id.is_some()
                    || revision.boundary_kind == RevisionBoundaryKind::ParagraphMark
                    || !super::anchors::table_range_is_one_cell(
                        start_block,
                        revision.anchor.start..revision.anchor.end,
                    )
                {
                    return Err(
                        "DOCX export supports tracked table text only within one cell".into(),
                    );
                }
            } else if !is_text_paragraph(start_block) {
                return Err("DOCX export does not support tracked changes in objects".into());
            }

            if cross_paragraph {
                if !matches!(
                    revision.kind,
                    RevisionKind::Insertion | RevisionKind::Formatting
                ) || !is_text_paragraph(end_block)
                    || end_index != start_index + 1
                    || revision.anchor.start > start_block.text.len_bytes()
                    || revision.anchor.end > end_block.text.len_bytes()
                    || start_block.text.is_empty()
                    || end_block.text.is_empty()
                    || revision.boundary_kind != RevisionBoundaryKind::ParagraphMark
                    || payload_texts
                        .iter()
                        .any(|text| text.matches('\n').count() != 1)
                {
                    return Err(
                        "DOCX supports one adjacent paragraph-mark boundary in text insertions and formatting"
                            .into(),
                    );
                }
            } else if !table_revision && !is_text_paragraph(end_block) {
                return Err("DOCX export does not support tracked changes in objects".into());
            }

            if revision.kind == RevisionKind::Insertion
                && revision.boundary_kind == RevisionBoundaryKind::ParagraphMark
                && !cross_paragraph
            {
                return Err("DOCX paragraph-mark insertion must span adjacent paragraphs".into());
            }
            if revision.kind == RevisionKind::Deletion
                && revision.boundary_kind == RevisionBoundaryKind::ParagraphMark
            {
                let before = revision.before.as_ref().ok_or_else(|| {
                    "paragraph-mark deletion has no unresolved payload".to_string()
                })?;
                if cross_paragraph
                    || start_block.kind != "paragraph"
                    || before.text.matches('\n').count() != 1
                    || revision.anchor.start != revision.anchor.end
                    || revision.anchor.start == 0
                    || revision.anchor.start >= start_block.text.len_bytes()
                    || self.tracked_changes.revisions().iter().any(|other| {
                        other.id != revision.id && other.anchor.block_id == revision.anchor.block_id
                    })
                    || self
                        .comments
                        .iter()
                        .any(|comment| comment.block_id == revision.anchor.block_id)
                    || self
                        .footnotes
                        .iter()
                        .any(|footnote| footnote.block_id == revision.anchor.block_id)
                {
                    return Err(
                        "DOCX paragraph-mark deletion requires one unambiguous text boundary without other anchors"
                            .into(),
                    );
                }
            }

            if revision.boundary_kind == RevisionBoundaryKind::Unknown && cross_paragraph {
                return Err(
                    "DOCX export rejects legacy revisions with unknown boundary ownership".into(),
                );
            }
            if revision.kind == RevisionKind::Deletion
                && revision.boundary_kind == RevisionBoundaryKind::ParagraphMark
            {
                let before = revision
                    .before
                    .as_ref()
                    .expect("validated deletion payload");
                if before.text.matches('\n').count() != 1 {
                    return Err("DOCX supports only one paragraph mark per deletion".into());
                }
            }

            for payload in [&revision.before, &revision.after].into_iter().flatten() {
                for run in &payload.runs {
                    Self::validate_docx_revision_style(&run.style)?;
                }
            }

            let (global_start, global_end) =
                super::anchors::anchor_global_range(&self.blocks, &revision.anchor)
                    .map_err(|error| error.to_string())?;
            for footnote in &self.footnotes {
                let Some(block_index) = self
                    .blocks
                    .iter()
                    .position(|block| block.id == footnote.block_id)
                else {
                    continue;
                };
                let position = block_starts[block_index] + footnote.offset;
                if global_start < position && position < global_end {
                    return Err(
                        "DOCX export does not support revisions containing footnote references"
                            .into(),
                    );
                }
            }
            ranges.push((revision.id, revision.kind, global_start, global_end));
        }

        for (index, (id, kind, start, end)) in ranges.iter().copied().enumerate() {
            for (other_id, other_kind, other_start, other_end) in
                ranges[index + 1..].iter().copied()
            {
                let paragraph_format_overlap =
                    paragraph_format_ids.contains(&id) || paragraph_format_ids.contains(&other_id);
                let replacement_pair = (kind == RevisionKind::Deletion
                    && other_kind == RevisionKind::Insertion
                    && start == end
                    && start == other_start)
                    || (other_kind == RevisionKind::Deletion
                        && kind == RevisionKind::Insertion
                        && other_start == other_end
                        && other_start == start);
                let overlaps = start < other_end && other_start < end;
                let deletion_point_inside = (kind == RevisionKind::Deletion
                    && start == end
                    && other_start <= start
                    && start < other_end)
                    || (other_kind == RevisionKind::Deletion
                        && other_start == other_end
                        && start <= other_start
                        && other_start < end);
                if (overlaps || deletion_point_inside)
                    && !replacement_pair
                    && !paragraph_format_overlap
                    && (matches!(kind, RevisionKind::Insertion | RevisionKind::Formatting)
                        || matches!(
                            other_kind,
                            RevisionKind::Insertion | RevisionKind::Formatting
                        ))
                {
                    return Err("DOCX export cannot represent overlapping revisions".into());
                }
                if kind == other_kind
                    && matches!(kind, RevisionKind::Insertion | RevisionKind::Formatting)
                    && overlaps
                    && !paragraph_format_overlap
                {
                    return Err(
                        "DOCX export cannot represent overlapping revisions of one kind".into(),
                    );
                }
            }
        }
        Ok(())
    }

    fn append_docx_paragraph_payload(
        target: &mut RevisionPayload,
        next: &RevisionPayload,
    ) -> Result<(), String> {
        let separator = !target.text.ends_with('\n');
        let shift = target.text.len() + usize::from(separator);
        if separator {
            target.text.push('\n');
        }
        target.text.push_str(&next.text);
        target.runs.extend(next.runs.iter().map(|run| {
            let mut run = run.clone();
            run.start += shift;
            run.end += shift;
            run
        }));
        target
            .validate()
            .map_err(|error| format!("DOCX tracked-change payload is invalid: {error}"))
    }

    fn validate_docx_paragraph_format(
        kind: &str,
        style: &loom_text::ParagraphStyle,
    ) -> Result<(), String> {
        let is_heading = kind
            .strip_prefix("heading")
            .is_some_and(|level| matches!(level, "1" | "2" | "3" | "4" | "5" | "6"));
        if kind != "paragraph" && !is_heading {
            return Err(
                "DOCX tracked paragraph formatting supports paragraph and heading1-heading6 only"
                    .into(),
            );
        }
        if style.list_level != 0 {
            return Err("DOCX tracked paragraph formatting cannot preserve list levels".into());
        }
        if style.break_rule != loom_text::LineBreakRule::Word {
            return Err(
                "DOCX tracked paragraph formatting cannot preserve this line-break rule".into(),
            );
        }

        let exact_twips = |value: f32, property: &str, allow_negative: bool| {
            let twips = value * 20.0;
            if !value.is_finite()
                || (!allow_negative && value < 0.0)
                || !twips.is_finite()
                || twips.abs() > 31_680.0
                || twips.round() != twips
                || value != twips.round() / 20.0
            {
                return Err(format!(
                    "DOCX tracked paragraph {property} is not exactly representable in twips"
                ));
            }
            Ok(())
        };
        exact_twips(style.space_before, "space before", false)?;
        exact_twips(style.space_after, "space after", false)?;
        exact_twips(style.left_indent, "left indent", false)?;
        exact_twips(style.right_indent, "right indent", false)?;
        exact_twips(style.first_line_indent, "first-line indent", true)?;

        let line = style.line_spacing * 240.0;
        if !style.line_spacing.is_finite()
            || !line.is_finite()
            || line <= 0.0
            || line > 31_680.0
            || line.round() != line
            || style.line_spacing != line.round() / 240.0
        {
            return Err("DOCX tracked paragraph line spacing is not exactly representable".into());
        }
        Ok(())
    }

    fn validate_docx_revision_style(style: &loom_text::CharacterStyle) -> Result<(), String> {
        let default_weight = loom_text::CharacterStyle::default().weight.numeric();
        if ![default_weight, loom_text::FontWeight::Bold.numeric()]
            .contains(&style.weight.numeric())
        {
            return Err("DOCX tracked formatting cannot represent this font weight".into());
        }
        if style.superscript && style.subscript {
            return Err(
                "DOCX tracked formatting cannot represent simultaneous super/subscript".into(),
            );
        }
        if style.has_own_size() {
            let size = style.font_size;
            if !size.is_finite()
                || !(1.0..=1638.0).contains(&size)
                || (size * 2.0).round() / 2.0 != size
            {
                return Err(
                    "DOCX tracked formatting requires a supported half-point font size".into(),
                );
            }
        }
        if style
            .color
            .as_deref()
            .is_some_and(|color| !Self::docx_color_is_supported(color))
        {
            return Err("DOCX tracked formatting requires an opaque RGB color".into());
        }
        Ok(())
    }

    fn docx_color_is_supported(color: &str) -> bool {
        let Some(hex) = color.strip_prefix('#') else {
            return false;
        };
        if !hex.is_ascii() {
            return false;
        }
        match hex.len() {
            6 => hex.bytes().all(|byte| byte.is_ascii_hexdigit()),
            8 => {
                hex[..6].bytes().all(|byte| byte.is_ascii_hexdigit())
                    && hex[6..].eq_ignore_ascii_case("FF")
            }
            _ => false,
        }
    }
}
