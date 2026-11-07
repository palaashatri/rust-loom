use std::collections::HashSet;

use loom_package::manifest::json as pkg_json;

use crate::{
    comments::TextEdit, ContentParser, FootnoteEntry, JsonValue, RichBlock, WriterDocument,
    WriterError,
};

pub(crate) const MAX_FOOTNOTES_PER_DOCUMENT: usize = 4_096;
pub(crate) const MAX_FOOTNOTE_BODY_BYTES: usize = 64 * 1_024;
pub(crate) const MAX_FOOTNOTE_TOTAL_BYTES: usize = 16 * 1_024 * 1_024;
pub(crate) const MAX_FOOTNOTE_ENTRY_JSON_BYTES: usize = MAX_FOOTNOTE_BODY_BYTES * 6 + 256;
pub(crate) const MAX_FOOTNOTE_JSON_BYTES: usize =
    MAX_FOOTNOTE_TOTAL_BYTES * 6 + MAX_FOOTNOTES_PER_DOCUMENT * 128;

impl WriterDocument {
    pub(crate) fn footnotes_to_content_json(&self) -> String {
        let mut out = String::from("[");
        for (index, note) in self.footnotes.iter().enumerate() {
            if index > 0 {
                out.push(',');
            }
            out.push_str(&format!(
                "{{\"id\":{},\"block_id\":{},\"offset\":{},\"body\":{}}}",
                note.id,
                note.block_id,
                note.offset,
                pkg_json::escape(&note.body),
            ));
        }
        out.push(']');
        out
    }

    pub(crate) fn footnotes_from_content_json(
        value: &str,
    ) -> Result<Vec<FootnoteEntry>, WriterError> {
        if value.len() > MAX_FOOTNOTE_JSON_BYTES {
            return Err(WriterError::Invalid(format!(
                "footnotes exceed the {}-byte JSON limit",
                MAX_FOOTNOTE_JSON_BYTES
            )));
        }
        ContentParser::new(value)
            .parse_array_bounded(MAX_FOOTNOTES_PER_DOCUMENT, MAX_FOOTNOTE_ENTRY_JSON_BYTES)?
            .iter()
            .map(|raw| footnote_from_raw(raw))
            .collect()
    }

    pub(crate) fn validate_footnotes(&self) -> Result<(), String> {
        if self.footnotes.len() > MAX_FOOTNOTES_PER_DOCUMENT {
            return Err(format!(
                "document has more than {MAX_FOOTNOTES_PER_DOCUMENT} footnotes"
            ));
        }

        let mut ids = HashSet::with_capacity(self.footnotes.len());
        let mut anchors = HashSet::with_capacity(self.footnotes.len());
        let mut total_body_bytes = 0usize;
        for note in &self.footnotes {
            if note.id == 0 || !ids.insert(note.id) {
                return Err("footnote IDs must be unique and nonzero".into());
            }
            if note.body.len() > MAX_FOOTNOTE_BODY_BYTES {
                return Err(format!(
                    "footnote {} exceeds the {MAX_FOOTNOTE_BODY_BYTES}-byte body limit",
                    note.id
                ));
            }
            total_body_bytes = total_body_bytes.saturating_add(note.body.len());
            if !anchors.insert((note.block_id, note.offset)) {
                return Err("footnote references must have unique anchor positions".into());
            }
            let matching_blocks = self
                .blocks
                .iter()
                .filter(|block| block.id == note.block_id)
                .count();
            if matching_blocks != 1 {
                return Err(format!(
                    "footnote {} refers to a missing or ambiguous block",
                    note.id
                ));
            }
            let block = self
                .blocks
                .iter()
                .find(|block| block.id == note.block_id)
                .expect("the unique matching block was counted");
            if note.offset > block.text.len_bytes()
                || !block.text.as_str().is_char_boundary(note.offset)
            {
                return Err(format!(
                    "footnote {} has an invalid UTF-8 anchor offset",
                    note.id
                ));
            }
        }
        if total_body_bytes > MAX_FOOTNOTE_TOTAL_BYTES {
            return Err(format!(
                "footnote bodies exceed the {MAX_FOOTNOTE_TOTAL_BYTES}-byte document limit"
            ));
        }
        Ok(())
    }
}

fn footnote_from_raw(raw: &str) -> Result<FootnoteEntry, WriterError> {
    let fields = ContentParser::new(raw).parse()?;
    let mut seen = HashSet::with_capacity(fields.len());
    let mut id = None;
    let mut block_id = None;
    let mut offset = None;
    let mut body = None;
    for (key, value) in fields {
        if !seen.insert(key.clone()) {
            return Err(WriterError::Json("duplicate field in footnote".into()));
        }
        match key.as_str() {
            "id" => id = Some(parse_raw::<u64>(&value, "footnote id")?),
            "block_id" => block_id = Some(parse_raw::<u64>(&value, "footnote block id")?),
            "offset" => offset = Some(parse_raw::<usize>(&value, "footnote offset")?),
            "body" => match value {
                JsonValue::String(text) => body = Some(text),
                JsonValue::Raw(_) => {
                    return Err(WriterError::Json("expected a footnote body string".into()))
                }
            },
            _ => {}
        }
    }
    Ok(FootnoteEntry {
        id: id.ok_or_else(|| WriterError::Json("footnote is missing id".into()))?,
        block_id: block_id
            .ok_or_else(|| WriterError::Json("footnote is missing block_id".into()))?,
        offset: offset.ok_or_else(|| WriterError::Json("footnote is missing offset".into()))?,
        body: body.ok_or_else(|| WriterError::Json("footnote is missing body".into()))?,
    })
}

fn parse_raw<T: std::str::FromStr>(value: &JsonValue, label: &str) -> Result<T, WriterError> {
    match value {
        JsonValue::Raw(raw) => raw
            .parse()
            .map_err(|_| WriterError::Json(format!("invalid {label}"))),
        JsonValue::String(_) => Err(WriterError::Json(format!("expected numeric {label}"))),
    }
}

pub(crate) fn rebase_through(
    footnotes: &mut Vec<FootnoteEntry>,
    old_blocks: &[RichBlock],
    new_blocks: &[RichBlock],
    edit: TextEdit,
) {
    footnotes.retain_mut(|note| rebase_one(note, old_blocks, new_blocks, edit));
}

fn rebase_one(
    note: &mut FootnoteEntry,
    old_blocks: &[RichBlock],
    new_blocks: &[RichBlock],
    edit: TextEdit,
) -> bool {
    let TextEdit {
        old_start,
        old_end,
        new_start,
        new_end,
    } = edit;
    let Some((old_block_start, old_block)) =
        crate::comments::block_start(old_blocks, note.block_id)
    else {
        return false;
    };
    if note.offset > old_block.text.len_bytes()
        || !old_block.text.as_str().is_char_boundary(note.offset)
    {
        return false;
    }

    let old_anchor = old_block_start.saturating_add(note.offset);
    if old_start < old_end && old_start <= old_anchor && old_anchor < old_end {
        return false;
    }
    let new_anchor = crate::comments::map_comment_endpoint(
        old_anchor, old_start, old_end, new_start, new_end, true,
    );
    let Some((new_block, local_offset)) = crate::comments::block_at_offset(new_blocks, new_anchor)
    else {
        return false;
    };
    if local_offset > new_block.text.len_bytes()
        || !new_block.text.as_str().is_char_boundary(local_offset)
    {
        return false;
    }
    note.block_id = new_block.id;
    note.offset = local_offset;
    true
}

pub(crate) fn rebase_after_split(
    footnotes: &mut [FootnoteEntry],
    block_id: u64,
    new_block_id: u64,
    split_offset: usize,
) {
    for note in footnotes
        .iter_mut()
        .filter(|note| note.block_id == block_id)
    {
        if note.offset > split_offset {
            note.block_id = new_block_id;
            note.offset -= split_offset;
        }
    }
}

pub(crate) fn rebase_after_merge(
    footnotes: &mut [FootnoteEntry],
    first_block_id: u64,
    second_block_id: u64,
    first_block_len: usize,
) -> Result<(), String> {
    let collides = footnotes
        .iter()
        .filter(|note| note.block_id == second_block_id)
        .any(|second| {
            let merged_offset = first_block_len.saturating_add(second.offset);
            footnotes
                .iter()
                .any(|first| first.block_id == first_block_id && first.offset == merged_offset)
        });
    if collides {
        return Err("merging blocks would collapse distinct footnote anchors".into());
    }
    for note in footnotes
        .iter_mut()
        .filter(|note| note.block_id == second_block_id)
    {
        note.block_id = first_block_id;
        note.offset = first_block_len.saturating_add(note.offset);
    }
    Ok(())
}

pub(crate) fn rebase_block_changes(
    footnotes: &mut Vec<FootnoteEntry>,
    block_id: u64,
    old_text: &str,
    new_text: &str,
    changes: &[(usize, usize, usize)],
) {
    footnotes.retain_mut(|note| {
        if note.block_id != block_id {
            return true;
        }
        if note.offset > old_text.len() || !old_text.is_char_boundary(note.offset) {
            return false;
        }
        let mut offset = note.offset;
        for &(start, end, inserted_len) in changes.iter().rev() {
            if start > end
                || end > old_text.len()
                || !old_text.is_char_boundary(start)
                || !old_text.is_char_boundary(end)
            {
                return false;
            }
            if start < end && start <= offset && offset < end {
                return false;
            }
            if offset >= end {
                offset = offset
                    .saturating_sub(end - start)
                    .saturating_add(inserted_len);
            }
        }
        if offset > new_text.len() || !new_text.is_char_boundary(offset) {
            return false;
        }
        note.offset = offset;
        true
    });
}

fn rebase_within_block(note: &mut FootnoteEntry, old: &RichBlock, new: &RichBlock) -> bool {
    let edit = TextEdit::diff(old.text.as_str(), new.text.as_str());
    if edit.old_start < edit.old_end && edit.old_start <= note.offset && note.offset < edit.old_end
    {
        return false;
    }
    if note.offset > old.text.len_bytes() || !old.text.as_str().is_char_boundary(note.offset) {
        return false;
    }
    let offset = crate::comments::map_comment_endpoint(
        note.offset,
        edit.old_start,
        edit.old_end,
        edit.new_start,
        edit.new_end,
        true,
    );
    if offset > new.text.len_bytes() || !new.text.as_str().is_char_boundary(offset) {
        return false;
    }
    note.offset = offset;
    true
}

pub(crate) fn rebase_after_block_rebuild(
    footnotes: &mut Vec<FootnoteEntry>,
    old_blocks: &[RichBlock],
    new_blocks: &[RichBlock],
) {
    let old_text = crate::comments::blocks_to_text(old_blocks);
    let new_text = crate::comments::blocks_to_text(new_blocks);
    let edit = TextEdit::diff(&old_text, &new_text);
    footnotes.retain_mut(|note| {
        let local = match (
            old_blocks.iter().find(|block| block.id == note.block_id),
            new_blocks.iter().find(|block| block.id == note.block_id),
        ) {
            (Some(old), Some(new)) => rebase_within_block(note, old, new),
            _ => false,
        };
        local || rebase_one(note, old_blocks, new_blocks, edit)
    });
}

impl WriterDocument {
    /// Adds a footnote at a unique, valid position in a document block.
    pub fn insert_footnote(
        &mut self,
        block_id: u64,
        offset: usize,
        body: &str,
    ) -> Result<u64, String> {
        if self.footnotes.len() >= MAX_FOOTNOTES_PER_DOCUMENT {
            return Err(format!(
                "document has reached the {MAX_FOOTNOTES_PER_DOCUMENT}-footnote limit"
            ));
        }
        self.validate_footnotes()?;
        if body.len() > MAX_FOOTNOTE_BODY_BYTES {
            return Err(format!(
                "footnote body exceeds the {MAX_FOOTNOTE_BODY_BYTES}-byte limit"
            ));
        }

        let mut blocks = self.blocks.iter().filter(|block| block.id == block_id);
        let Some(block) = blocks.next() else {
            return Err("footnote refers to a missing block".into());
        };
        if blocks.next().is_some() {
            return Err("footnote refers to an ambiguous block".into());
        }
        if offset > block.text.len_bytes() || !block.text.as_str().is_char_boundary(offset) {
            return Err("footnote reference has an invalid UTF-8 anchor offset".into());
        }
        if self
            .footnotes
            .iter()
            .any(|note| note.block_id == block_id && note.offset == offset)
        {
            return Err("footnote reference position is already in use".into());
        }

        let body_bytes = self
            .footnotes
            .iter()
            .map(|note| note.body.len())
            .sum::<usize>();
        if body.len() > MAX_FOOTNOTE_TOTAL_BYTES.saturating_sub(body_bytes) {
            return Err(format!(
                "footnote bodies exceed the {MAX_FOOTNOTE_TOTAL_BYTES}-byte document limit"
            ));
        }

        let id = self
            .footnotes
            .iter()
            .map(|note| note.id)
            .max()
            .unwrap_or(0)
            .checked_add(1)
            .or_else(|| {
                (1_u64..).find(|candidate| self.footnotes.iter().all(|note| note.id != *candidate))
            })
            .ok_or_else(|| "no footnote IDs are available".to_string())?;
        self.footnotes.push(FootnoteEntry {
            id,
            block_id,
            offset,
            body: body.to_owned(),
        });
        Ok(id)
    }

    /// Updates a footnote body without changing the note ID or reference.
    pub fn update_footnote_body(&mut self, id: u64, body: &str) -> Result<(), String> {
        self.validate_footnotes()?;
        let Some(index) = self.footnotes.iter().position(|note| note.id == id) else {
            return Err(format!("footnote {id} does not exist"));
        };
        if body.len() > MAX_FOOTNOTE_BODY_BYTES {
            return Err(format!(
                "footnote body exceeds the {MAX_FOOTNOTE_BODY_BYTES}-byte limit"
            ));
        }

        let body_bytes = self
            .footnotes
            .iter()
            .map(|note| note.body.len())
            .sum::<usize>();
        let remaining = MAX_FOOTNOTE_TOTAL_BYTES
            .saturating_sub(body_bytes.saturating_sub(self.footnotes[index].body.len()));
        if body.len() > remaining {
            return Err(format!(
                "footnote bodies exceed the {MAX_FOOTNOTE_TOTAL_BYTES}-byte document limit"
            ));
        }
        self.footnotes[index].body = body.to_owned();
        Ok(())
    }

    /// Removes exactly one note with this ID. Invalid duplicate IDs are left untouched.
    pub fn remove_footnote(&mut self, id: u64) -> bool {
        let mut matches = self
            .footnotes
            .iter()
            .enumerate()
            .filter(|(_, note)| note.id == id);
        let Some((index, _)) = matches.next() else {
            return false;
        };
        if matches.next().is_some() {
            return false;
        }
        self.footnotes.remove(index);
        true
    }

    pub fn numbered_footnotes(&self) -> Vec<(usize, &FootnoteEntry)> {
        if self.footnotes.is_empty() {
            return Vec::new();
        }
        let mut block_positions = std::collections::HashMap::with_capacity(self.footnotes.len());
        for note in &self.footnotes {
            block_positions.entry(note.block_id).or_insert(None);
        }
        for (index, block) in self.blocks.iter().enumerate() {
            if let Some(position) = block_positions.get_mut(&block.id) {
                if position.is_none() {
                    *position = Some(index);
                }
            }
        }

        let mut notes: Vec<_> = self.footnotes.iter().collect();
        notes.sort_by_key(|note| {
            (
                block_positions
                    .get(&note.block_id)
                    .copied()
                    .flatten()
                    .unwrap_or(usize::MAX),
                note.offset,
                note.id,
            )
        });
        notes
            .into_iter()
            .enumerate()
            .map(|(index, note)| (index + 1, note))
            .collect()
    }
}

impl ContentParser {
    pub(crate) fn capture_raw_value_bounded(
        &mut self,
        max_bytes: usize,
    ) -> Result<String, WriterError> {
        self.skip_ws();
        let start = self.pos;
        let mut depth = 0usize;
        let mut in_string = false;
        let mut escaped = false;
        while let Some(byte) = self.peek() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
                self.pos += 1;
            } else {
                match byte {
                    b'"' => {
                        in_string = true;
                        self.pos += 1;
                    }
                    b'{' | b'[' => {
                        depth = depth.saturating_add(1);
                        self.pos += 1;
                    }
                    b'}' | b']' if depth > 0 => {
                        depth -= 1;
                        self.pos += 1;
                        if depth == 0 {
                            break;
                        }
                    }
                    b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r' if depth == 0 => break,
                    _ => self.pos += 1,
                }
            }
            if self.pos.saturating_sub(start) > max_bytes {
                return Err(WriterError::Invalid(format!(
                    "JSON value exceeds the {max_bytes}-byte limit"
                )));
            }
        }
        if self.pos == start || in_string || depth != 0 {
            return Err(WriterError::Json("invalid JSON value".into()));
        }
        std::str::from_utf8(&self.bytes[start..self.pos])
            .map(str::to_owned)
            .map_err(|_| WriterError::Json("invalid utf8".into()))
    }

    pub(crate) fn parse_array_bounded(
        &self,
        max_items: usize,
        max_item_bytes: usize,
    ) -> Result<Vec<String>, WriterError> {
        let mut parser = self.clone();
        parser.skip_ws();
        if parser.next() != Some(b'[') {
            return Err(WriterError::Json("expected array".into()));
        }
        let mut items = Vec::new();
        loop {
            parser.skip_ws();
            if parser.peek() == Some(b']') {
                parser.pos += 1;
                return Ok(items);
            }
            if items.len() >= max_items {
                return Err(WriterError::Invalid(format!(
                    "footnote count exceeds the {max_items}-item limit"
                )));
            }
            items.push(parser.capture_raw_value_bounded(max_item_bytes)?);
            parser.skip_ws();
            match parser.peek() {
                Some(b',') => parser.pos += 1,
                Some(b']') => {
                    parser.pos += 1;
                    return Ok(items);
                }
                _ => return Err(WriterError::Json("expected ',' or ']'".into())),
            }
        }
    }
}

#[cfg(test)]
mod tests {

    use crate::{load_document, save_document, RichBlock, WriterDocument};

    use super::{FootnoteEntry, MAX_FOOTNOTE_BODY_BYTES};

    fn document(text: &str) -> WriterDocument {
        let mut document = WriterDocument::new("footnotes", "Footnotes");
        document.blocks.push(RichBlock::new(7, "paragraph", text));
        document
    }

    fn note(block_id: u64, offset: usize) -> FootnoteEntry {
        FootnoteEntry {
            id: 41,
            block_id,
            offset,
            body: "A note body.".into(),
        }
    }

    #[test]
    fn footnotes_round_trip_in_the_package_and_old_content_defaults_empty() {
        let mut document = document("A source sentence.");
        document.footnotes.push(note(7, 2));
        let bytes = save_document(&document).expect("save document with note");
        assert_eq!(
            load_document(&bytes).expect("reopen note").footnotes,
            document.footnotes
        );

        let current = document.to_content_json();
        let missing_footnotes = current.replace(
            ",\"footnotes\":[{\"id\":41,\"block_id\":7,\"offset\":2,\"body\":\"A note body.\"}]",
            "",
        );
        assert!(WriterDocument::from_content_json(&missing_footnotes).is_err());

        let tracked_state = format!(
            ",\"tracked_changes\":{}",
            crate::TrackedChanges::default().write_content_json()
        );
        let old = current
            .replace(
                &format!("\"format\":{}", crate::WRITER_CONTENT_FORMAT),
                "\"format\":5",
            )
            .replace(",\"appearance\":\"white\"", "")
            .replace(
                ",\"footnotes\":[{\"id\":41,\"block_id\":7,\"offset\":2,\"body\":\"A note body.\"}]",
                "",
            )
            .replace(&tracked_state, "");
        assert!(WriterDocument::from_content_json(&old)
            .expect("format 5 remains readable")
            .footnotes
            .is_empty());
    }

    #[test]
    fn invalid_ids_dangling_blocks_and_non_boundary_offsets_are_rejected() {
        let mut document = document("éx");
        document.footnotes.push(note(99, 0));
        assert!(document
            .validate_footnotes()
            .unwrap_err()
            .contains("missing or ambiguous"));

        document.footnotes[0] = note(7, 1);
        assert!(document.validate_footnotes().unwrap_err().contains("UTF-8"));

        document.footnotes[0].id = 0;
        document.footnotes[0].offset = 2;
        assert!(document
            .validate_footnotes()
            .unwrap_err()
            .contains("unique and nonzero"));
    }

    #[test]
    fn footnote_bodies_are_bounded_before_a_document_can_be_saved() {
        let mut document = document("Text");
        let mut entry = note(7, 0);
        entry.body = "x".repeat(MAX_FOOTNOTE_BODY_BYTES + 1);
        document.footnotes.push(entry);
        assert!(save_document(&document).is_err());
    }

    #[test]
    fn document_footnote_actions_keep_unique_ids_and_validate_anchors() {
        let mut doc = document("AéBC");
        let first = doc
            .insert_footnote(7, 1, "first note")
            .expect("insert first note");
        let second = doc
            .insert_footnote(7, 3, "second note")
            .expect("insert second note");
        assert_eq!([first, second], [1, 2]);
        assert_eq!(
            doc.footnotes
                .iter()
                .map(|note| (note.id, note.block_id, note.offset, note.body.clone()))
                .collect::<Vec<_>>(),
            vec![
                (1, 7, 1, "first note".to_string()),
                (2, 7, 3, "second note".to_string()),
            ]
        );

        let before = doc.footnotes.clone();
        assert!(doc.insert_footnote(7, 2, "inside a character").is_err());
        assert!(doc.insert_footnote(7, 1, "duplicate anchor").is_err());
        assert_eq!(doc.footnotes, before);

        let mut duplicate_id = doc.clone();
        let mut duplicate = note(7, 4);
        duplicate.id = first;
        duplicate_id.footnotes.push(duplicate);
        let before = duplicate_id.footnotes.clone();
        assert!(duplicate_id.insert_footnote(7, 5, "rejected").is_err());
        assert_eq!(duplicate_id.footnotes, before);

        let mut wrapped_id = document("text");
        wrapped_id.footnotes.push(FootnoteEntry {
            id: u64::MAX,
            block_id: 7,
            offset: 0,
            body: String::new(),
        });
        assert_eq!(
            wrapped_id
                .insert_footnote(7, 1, "next")
                .expect("ID wraps to a free value"),
            1
        );
    }

    #[test]
    fn document_footnote_body_updates_and_removals_are_safe() {
        let mut document = document("text");
        let id = document
            .insert_footnote(7, 1, "initial")
            .expect("insert note");
        document
            .update_footnote_body(id, "edited")
            .expect("update existing note");
        assert_eq!(document.footnotes[0].body, "edited");
        assert_eq!(document.footnotes[0].id, id);
        assert_eq!(document.footnotes[0].offset, 1);

        let before = document.footnotes.clone();
        let too_long = "x".repeat(MAX_FOOTNOTE_BODY_BYTES + 1);
        assert!(document.update_footnote_body(id, &too_long).is_err());
        assert!(document.update_footnote_body(u64::MAX, "missing").is_err());
        assert_eq!(document.footnotes, before);

        assert!(document.remove_footnote(id));
        assert!(!document.remove_footnote(id));
        assert!(!document.remove_footnote(u64::MAX));
        assert!(document.footnotes.is_empty());
    }

    #[test]
    fn document_footnote_caps_reject_mutations_without_changing_notes() {
        let mut document = document("text");
        let before = document.footnotes.clone();
        let too_long = "x".repeat(MAX_FOOTNOTE_BODY_BYTES + 1);
        assert!(document.insert_footnote(7, 1, &too_long).is_err());
        assert_eq!(document.footnotes, before);

        let mut near_total_limit = WriterDocument::new("footnotes", "Footnotes");
        for id in 1..=258_u64 {
            near_total_limit
                .blocks
                .push(RichBlock::new(id, "paragraph", ""));
        }
        for id in 1..=255_u64 {
            near_total_limit.footnotes.push(FootnoteEntry {
                id,
                block_id: id,
                offset: 0,
                body: "x".repeat(MAX_FOOTNOTE_BODY_BYTES),
            });
        }
        near_total_limit.footnotes.push(FootnoteEntry {
            id: 256,
            block_id: 256,
            offset: 0,
            body: "x".repeat(MAX_FOOTNOTE_BODY_BYTES - 1),
        });
        near_total_limit.footnotes.push(FootnoteEntry {
            id: 257,
            block_id: 257,
            offset: 0,
            body: String::new(),
        });
        assert_eq!(
            near_total_limit
                .footnotes
                .iter()
                .map(|note| note.body.len())
                .sum::<usize>(),
            super::MAX_FOOTNOTE_TOTAL_BYTES - 1
        );
        assert!(near_total_limit.validate_footnotes().is_ok());
        assert!(near_total_limit.insert_footnote(258, 0, "xx").is_err());
        assert!(near_total_limit.update_footnote_body(257, "xx").is_err());
        assert_eq!(near_total_limit.footnotes.len(), 257);
        assert!(near_total_limit
            .footnotes
            .iter()
            .find(|note| note.id == 257)
            .expect("empty note remains present")
            .body
            .is_empty());

        let mut at_count_limit = WriterDocument::new("footnotes", "Footnotes");
        for id in 1..=super::MAX_FOOTNOTES_PER_DOCUMENT as u64 + 1 {
            at_count_limit
                .blocks
                .push(RichBlock::new(id, "paragraph", ""));
            if id <= super::MAX_FOOTNOTES_PER_DOCUMENT as u64 {
                at_count_limit.footnotes.push(FootnoteEntry {
                    id,
                    block_id: id,
                    offset: 0,
                    body: String::new(),
                });
            }
        }
        assert!(at_count_limit
            .insert_footnote(super::MAX_FOOTNOTES_PER_DOCUMENT as u64 + 1, 0, "",)
            .is_err());
        assert_eq!(
            at_count_limit.footnotes.len(),
            super::MAX_FOOTNOTES_PER_DOCUMENT
        );
    }
}
