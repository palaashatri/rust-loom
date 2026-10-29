use std::collections::{BTreeMap, HashSet};
use std::mem::size_of;

use loom_text::StyleRun;

use crate::{RichBlock, WriterError};

mod anchors;
mod docx;
mod edits;
mod json;
#[cfg(test)]
mod tests;
mod transactions;

use anchors::validate_record;
use json::{parse_revision, parse_u64, payload_bytes, payload_capacity, revision_json};

pub const MAX_REVISIONS: usize = 10_000;
pub const MAX_REVISION_AUTHOR_BYTES: usize = 256;
pub const MAX_REVISION_PAYLOAD_BYTES: usize = 1_048_576;
pub const MAX_REVISION_TOTAL_BYTES: usize = 16_777_216;
pub const MAX_REVISION_ENTRY_JSON_BYTES: usize = 2_097_152;
pub const MAX_TRACKED_CHANGES_JSON_BYTES: usize = 33_554_432;

const REVISION_DEPENDENCY_CONFLICT: &str =
    "resolve newer overlapping tracked changes before this revision";
const INVALID_REVISION: &str = "tracked change is malformed or conflicts with the current document";
const UNRECORDABLE_TABLE_TEXT_EDIT: &str =
    "tracked changes can record table text edits within one cell at a time";
const UNRECORDABLE_BLOCK_FORMAT: &str =
    "tracked changes cannot record paragraph, table layout, or object formatting";

/// One exact text replacement in a user transaction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionTextEdit {
    /// Byte range in the document before the transaction.
    pub old_start: usize,
    pub old_end: usize,
    /// Byte range in the document after all transaction edits.
    pub new_start: usize,
    pub new_end: usize,
}

/// The part of a block affected by a revision; offsets are UTF-8 byte boundaries.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RevisionAnchor {
    pub block_id: u64,
    pub start: usize,
    /// End offset in `block_id`, or in `end_block_id` when the range spans paragraphs.
    pub end: usize,
    /// Final block for a multi-block range; absent anchors retain the original single-block JSON shape.
    pub end_block_id: Option<u64>,
    /// SHA-256 of the UTF-8 text before the anchor start, guarding against stale offsets.
    pub prefix_sha256: [u8; 32],
}

/// Text and relative style runs retained for an insertion, deletion, or format change.
#[derive(Clone, Debug, PartialEq)]
pub struct RevisionPayload {
    pub text: String,
    pub runs: Vec<StyleRun>,
}

impl RevisionPayload {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            runs: Vec::new(),
        }
    }

    fn measured_bytes(&self) -> usize {
        self.text
            .len()
            .saturating_add(self.runs.iter().fold(0usize, |total, run| {
                total
                    .saturating_add(run.style.font_family.len())
                    .saturating_add(run.style.color.as_ref().map_or(0, String::len))
                    .saturating_add(run.style.background.as_ref().map_or(0, String::len))
                    .saturating_add(size_of::<StyleRun>())
            }))
    }

    fn validate(&self) -> Result<(), WriterError> {
        if self.text.len() > MAX_REVISION_PAYLOAD_BYTES
            || self.text.contains('\r')
            || self.runs.len() > MAX_REVISION_PAYLOAD_BYTES / size_of::<StyleRun>()
        {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        let mut previous_end = 0;
        for run in &self.runs {
            if run.start >= run.end
                || run.start < previous_end
                || run.end > self.text.len()
                || !self.text.is_char_boundary(run.start)
                || !self.text.is_char_boundary(run.end)
            {
                return Err(WriterError::Invalid(INVALID_REVISION.into()));
            }
            crate::style_json::validate_character_style(&run.style)
                .map_err(|error| WriterError::Invalid(error.into()))?;
            previous_end = run.end;
        }
        if self.measured_bytes() > MAX_REVISION_PAYLOAD_BYTES {
            return Err(WriterError::Invalid(INVALID_REVISION.into()));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionKind {
    Insertion,
    Deletion,
    Formatting,
}

/// Identifies non-text boundaries represented by a revision payload.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RevisionBoundaryKind {
    /// Older documents did not preserve whether a separator was a paragraph mark.
    Unknown,
    /// The revision covers ordinary text only.
    Text,
    /// The revision covers one or more paragraph marks between blocks.
    ParagraphMark,
    /// The revision covers a soft break inside a block.
    SoftBreak,
    /// The revision combines boundary characters with text or both boundary kinds.
    Mixed,
}

impl RevisionBoundaryKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Text => "text",
            Self::ParagraphMark => "paragraph_mark",
            Self::SoftBreak => "soft_break",
            Self::Mixed => "mixed",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "unknown" => Some(Self::Unknown),
            "text" => Some(Self::Text),
            "paragraph_mark" => Some(Self::ParagraphMark),
            "soft_break" => Some(Self::SoftBreak),
            "mixed" => Some(Self::Mixed),
            _ => None,
        }
    }
}

impl RevisionKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Insertion => "insertion",
            Self::Deletion => "deletion",
            Self::Formatting => "formatting",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value {
            "insertion" => Some(Self::Insertion),
            "deletion" => Some(Self::Deletion),
            "formatting" => Some(Self::Formatting),
            _ => None,
        }
    }
}

/// A stable pending edit. Payloads are range-relative and never snapshot a whole block.
#[derive(Clone, Debug, PartialEq)]
pub struct RevisionRecord {
    pub id: u64,
    pub author: String,
    pub anchor: RevisionAnchor,
    pub kind: RevisionKind,
    /// Boundary ownership is explicit so a soft break is never guessed to be a paragraph mark.
    pub boundary_kind: RevisionBoundaryKind,
    pub before: Option<RevisionPayload>,
    pub after: Option<RevisionPayload>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RevisionBlockFormat {
    pub(crate) kind: String,
    pub(crate) style: loom_text::ParagraphStyle,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RevisionBlockFormatChange {
    pub(crate) before: RevisionBlockFormat,
    pub(crate) after: RevisionBlockFormat,
}

/// A read-only view of one side of a tracked paragraph-format change.
#[derive(Clone, Copy, Debug)]
pub struct RevisionParagraphFormat<'a> {
    /// Stored paragraph or heading/list kind.
    pub kind: &'a str,
    /// Stored layout and spacing values.
    pub style: &'a loom_text::ParagraphStyle,
}

/// The before and after paragraph formats retained by a formatting revision.
#[derive(Clone, Copy, Debug)]
pub struct RevisionParagraphFormatChange<'a> {
    /// Format before the change was recorded.
    pub before: RevisionParagraphFormat<'a>,
    /// Format currently applied while the change remains unresolved.
    pub after: RevisionParagraphFormat<'a>,
}

impl RevisionBlockFormat {
    fn from_block(block: &RichBlock) -> Self {
        Self {
            kind: block.kind.clone(),
            style: block.style.clone(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        let style = &self.style;
        if self.kind.trim().is_empty()
            || self.kind.len() > MAX_REVISION_PAYLOAD_BYTES
            || self.kind.contains('\r')
            || self.kind.contains('\n')
            || [
                style.space_before,
                style.space_after,
                style.line_spacing,
                style.left_indent,
                style.right_indent,
                style.first_line_indent,
            ]
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err(INVALID_REVISION.into());
        }
        Ok(())
    }
}

/// Document-owned tracked-change settings and unresolved revisions.
#[derive(Clone, Debug, PartialEq)]
pub struct TrackedChanges {
    recording: bool,
    author: String,
    next_id: u64,
    revisions: Vec<RevisionRecord>,
    block_formats: BTreeMap<u64, RevisionBlockFormatChange>,
}

impl Default for TrackedChanges {
    fn default() -> Self {
        Self {
            recording: false,
            author: "Anonymous".into(),
            next_id: 1,
            revisions: Vec::new(),
            block_formats: BTreeMap::new(),
        }
    }
}

impl TrackedChanges {
    pub fn recording(&self) -> bool {
        self.recording
    }

    pub fn set_recording(&mut self, recording: bool) {
        self.recording = recording;
    }

    pub fn author(&self) -> &str {
        &self.author
    }

    pub fn set_author(&mut self, author: impl Into<String>) -> Result<(), WriterError> {
        let author = author.into();
        if author.trim().is_empty() || author.len() > MAX_REVISION_AUTHOR_BYTES {
            return Err(WriterError::Invalid(
                "revision author must be nonempty and within the byte limit".into(),
            ));
        }
        self.author = author;
        Ok(())
    }

    pub fn revisions(&self) -> &[RevisionRecord] {
        &self.revisions
    }

    /// Returns the saved paragraph formats for a block-format revision.
    pub fn paragraph_format_change(
        &self,
        revision_id: u64,
    ) -> Option<RevisionParagraphFormatChange<'_>> {
        let change = self.block_formats.get(&revision_id)?;
        Some(RevisionParagraphFormatChange {
            before: RevisionParagraphFormat {
                kind: &change.before.kind,
                style: &change.before.style,
            },
            after: RevisionParagraphFormat {
                kind: &change.after.kind,
                style: &change.after.style,
            },
        })
    }

    pub fn next_revision_id(&self) -> u64 {
        self.next_id
    }

    pub fn heap_capacity_bytes(&self) -> usize {
        self.author.capacity().saturating_add(
            self.revisions.iter().fold(
                self.revisions
                    .capacity()
                    .saturating_mul(size_of::<RevisionRecord>()),
                |total, revision| {
                    total
                        .saturating_add(revision.author.capacity())
                        .saturating_add(payload_capacity(&revision.before))
                        .saturating_add(payload_capacity(&revision.after))
                },
            ),
        )
    }

    pub(crate) fn write_content_json(&self) -> String {
        let mut out = String::from("{\"recording\":");
        out.push_str(if self.recording { "true" } else { "false" });
        out.push_str(",\"author\":");
        out.push_str(&crate::pkg_json::escape(&self.author));
        out.push_str(&format!(",\"next_id\":{},\"revisions\":[", self.next_id));
        for (index, revision) in self.revisions.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&revision_json(revision));
        }
        out.push_str("],\"block_formats\":");
        out.push_str(&json::block_formats_json(&self.block_formats));
        out.push('}');
        out
    }

    pub(crate) fn from_content_json_value(raw: &str) -> Result<Self, WriterError> {
        if raw.len() > MAX_TRACKED_CHANGES_JSON_BYTES {
            return Err(WriterError::Invalid(
                "tracked changes exceed the JSON size limit".into(),
            ));
        }
        let mut state = Self::default();
        let mut recording_seen = false;
        let mut author_seen = false;
        let mut next_seen = false;
        let mut revisions_seen = false;
        let mut block_formats_seen = false;
        let fields = crate::ContentParser::new(raw).parse()?;
        for (key, value) in fields {
            match key.as_str() {
                "recording" => {
                    if recording_seen {
                        return Err(WriterError::Json("duplicate tracked changes field".into()));
                    }
                    recording_seen = true;
                    state.recording = match &value {
                        crate::JsonValue::Raw(value) if value == "true" => true,
                        crate::JsonValue::Raw(value) if value == "false" => false,
                        _ => return Err(WriterError::Json("bad tracked changes setting".into())),
                    };
                }
                "author" => {
                    if author_seen {
                        return Err(WriterError::Json("duplicate tracked changes field".into()));
                    }
                    author_seen = true;
                    let crate::JsonValue::String(author) = value else {
                        return Err(WriterError::Json("expected revision author string".into()));
                    };
                    state.set_author(author)?;
                }
                "next_id" => {
                    if next_seen {
                        return Err(WriterError::Json("duplicate tracked changes field".into()));
                    }
                    next_seen = true;
                    state.next_id = parse_u64(&value, "bad next revision ID")?;
                }
                "revisions" => {
                    if revisions_seen {
                        return Err(WriterError::Json("duplicate tracked changes field".into()));
                    }
                    revisions_seen = true;
                    let crate::JsonValue::Raw(raw) = value else {
                        return Err(WriterError::Json("expected revisions array".into()));
                    };
                    state.revisions = crate::ContentParser::new(&raw)
                        .parse_array_bounded(MAX_REVISIONS, MAX_REVISION_ENTRY_JSON_BYTES)?
                        .iter()
                        .map(|item| parse_revision(item))
                        .collect::<Result<_, _>>()?;
                }
                "block_formats" => {
                    if block_formats_seen {
                        return Err(WriterError::Json("duplicate tracked changes field".into()));
                    }
                    block_formats_seen = true;
                    let crate::JsonValue::Raw(raw) = value else {
                        return Err(WriterError::Json(
                            "expected revision block formats array".into(),
                        ));
                    };
                    state.block_formats = json::parse_block_formats(&raw)?;
                }
                _ => {}
            }
        }
        if !(recording_seen && author_seen && next_seen && revisions_seen) {
            return Err(WriterError::Invalid(
                "tracked changes settings are incomplete".into(),
            ));
        }
        state.validate().map_err(WriterError::Invalid)?;
        Ok(state)
    }

    pub(crate) fn validate_for_document(&self, blocks: &[RichBlock]) -> Result<(), String> {
        self.validate()?;
        let mut ids = HashSet::with_capacity(self.revisions.len());
        let mut maximum_id = 0;
        let mut total_bytes = 0usize;
        for revision in &self.revisions {
            if revision.id == 0 || !ids.insert(revision.id) {
                return Err(INVALID_REVISION.into());
            }
            maximum_id = maximum_id.max(revision.id);
            total_bytes = total_bytes
                .saturating_add(revision.author.len())
                .saturating_add(payload_bytes(&revision.before))
                .saturating_add(payload_bytes(&revision.after));
            validate_record(revision, blocks).map_err(|_| INVALID_REVISION.to_string())?;
        }

        let mut block_format_chains: BTreeMap<u64, Vec<(u64, &RevisionBlockFormatChange)>> =
            BTreeMap::new();
        for (revision_id, change) in &self.block_formats {
            change.before.validate()?;
            change.after.validate()?;
            if change.before == change.after {
                return Err(INVALID_REVISION.into());
            }
            let revision = self
                .revisions
                .iter()
                .find(|revision| revision.id == *revision_id)
                .filter(|revision| revision.kind == RevisionKind::Formatting)
                .ok_or_else(|| INVALID_REVISION.to_string())?;
            if revision.anchor.start != 0 || revision.anchor.end_block_id.is_some() {
                return Err(INVALID_REVISION.into());
            }
            let matching_blocks = blocks
                .iter()
                .filter(|block| block.id == revision.anchor.block_id)
                .collect::<Vec<_>>();
            let [block] = matching_blocks.as_slice() else {
                return Err(INVALID_REVISION.into());
            };
            if block.typed_table().is_some()
                || block.image.is_some()
                || block.chart.is_some()
                || revision.anchor.end != block.text.len_bytes()
                || revision
                    .after
                    .as_ref()
                    .map_or(true, |payload| payload.text != block.text.as_str())
            {
                return Err(INVALID_REVISION.into());
            }
            if revision
                .before
                .as_ref()
                .map_or(true, |payload| payload.text != block.text.as_str())
            {
                return Err(INVALID_REVISION.into());
            }
            block_format_chains
                .entry(block.id)
                .or_default()
                .push((*revision_id, change));
        }
        for revision in &self.revisions {
            if revision.kind == RevisionKind::Formatting
                && revision.anchor.start == revision.anchor.end
                && !self.block_formats.contains_key(&revision.id)
            {
                return Err(INVALID_REVISION.into());
            }
        }
        for (block_id, chain) in block_format_chains {
            for pair in chain.windows(2) {
                if pair[0].1.after != pair[1].1.before {
                    return Err(INVALID_REVISION.into());
                }
            }
            let block = blocks
                .iter()
                .find(|block| block.id == block_id)
                .ok_or_else(|| INVALID_REVISION.to_string())?;
            if chain.last().map_or(true, |(_, change)| {
                change.after != RevisionBlockFormat::from_block(block)
            }) {
                return Err(INVALID_REVISION.into());
            }
        }

        if maximum_id >= self.next_id || total_bytes > MAX_REVISION_TOTAL_BYTES {
            return Err(INVALID_REVISION.into());
        }
        if self.write_content_json().len() > MAX_TRACKED_CHANGES_JSON_BYTES {
            return Err("tracked changes exceed the JSON size limit".into());
        }
        Ok(())
    }

    fn validate(&self) -> Result<(), String> {
        if self.next_id == 0
            || self.author.trim().is_empty()
            || self.author.len() > MAX_REVISION_AUTHOR_BYTES
            || self.revisions.len() > MAX_REVISIONS
            || self.block_formats.len() > MAX_REVISIONS
        {
            return Err(INVALID_REVISION.into());
        }
        for revision_id in self.block_formats.keys() {
            if *revision_id == 0
                || !self.revisions.iter().any(|revision| {
                    revision.id == *revision_id && revision.kind == RevisionKind::Formatting
                })
            {
                return Err(INVALID_REVISION.into());
            }
        }
        Ok(())
    }
}
