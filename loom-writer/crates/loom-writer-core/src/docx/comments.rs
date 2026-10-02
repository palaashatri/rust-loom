//! Writer comment threads as real Word comments.
//!
//! A thread anchors to a block and a UTF-8 byte range. Word anchors a
//! comment between `commentRangeStart` and `commentRangeEnd` markers inside a
//! paragraph, followed by a run holding the `commentReference`. Threads whose
//! words were deleted ("orphaned") have no honest place in the text, and
//! threads on a table block anchor to Markdown source rather than to cell
//! text, so both are counted as skipped instead of being attached to
//! unrelated words.

use std::collections::HashMap;

use super::xml::{self, MC_NS, W14_NS, W15_NS, W_NS, XML_DECLARATION};
use crate::{WriterDocument, TABLE_BLOCK_KIND};

/// One comment that will be written.
struct Entry {
    id: u32,
    author: String,
    body: String,
    resolved: bool,
}

/// Where one comment sits inside a block's text.
#[derive(Debug, Clone, Copy)]
pub(super) struct Anchor {
    pub id: u32,
    pub start: usize,
    pub end: usize,
}

/// The comments of a document, ready to place into paragraphs.
pub(super) struct CommentPlan {
    entries: Vec<Entry>,
    by_block: HashMap<u64, Vec<Anchor>>,
    skipped: usize,
}

impl CommentPlan {
    /// Decides which threads can be exported and numbers them in order.
    pub(super) fn new(doc: &WriterDocument) -> Self {
        let mut plan = Self {
            entries: Vec::new(),
            by_block: HashMap::new(),
            skipped: 0,
        };
        for thread in &doc.comments {
            let block = doc.blocks.iter().find(|block| block.id == thread.block_id);
            let anchored = block.filter(|block| {
                let text = block.text.as_str();
                !thread.orphaned
                    && block.kind != TABLE_BLOCK_KIND
                    && thread.start <= thread.end
                    && thread.end <= text.len()
                    && text.is_char_boundary(thread.start)
                    && text.is_char_boundary(thread.end)
            });
            let Some(block) = anchored else {
                plan.skipped += 1;
                continue;
            };
            let id = plan.entries.len() as u32;
            plan.entries.push(Entry {
                id,
                author: thread.author.clone(),
                body: thread.body.clone(),
                resolved: thread.resolved,
            });
            plan.by_block.entry(block.id).or_default().push(Anchor {
                id,
                start: thread.start,
                end: thread.end,
            });
        }
        plan
    }

    /// Comments that will appear in the file.
    pub(super) fn exported(&self) -> usize {
        self.entries.len()
    }

    /// Threads left out because they have no honest anchor.
    pub(super) fn skipped(&self) -> usize {
        self.skipped
    }

    /// Anchors that fall inside `block_id`, in comment order.
    pub(super) fn anchors_for(&self, block_id: u64) -> &[Anchor] {
        self.by_block.get(&block_id).map_or(&[], Vec::as_slice)
    }

    /// `word/comments.xml`.
    pub(super) fn comments_xml(&self) -> String {
        let mut out = String::from(XML_DECLARATION);
        out.push_str(&format!(
            "<w:comments xmlns:w=\"{W_NS}\" xmlns:mc=\"{MC_NS}\" xmlns:w14=\"{W14_NS}\" mc:Ignorable=\"w14\">"
        ));
        for entry in &self.entries {
            let initials = entry
                .author
                .chars()
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().collect::<String>())
                .unwrap_or_default();
            out.push_str(&format!(
                "<w:comment w:id=\"{}\" w:author=\"{}\" w:initials=\"{}\">",
                entry.id,
                xml::attr(&entry.author),
                xml::attr(&initials)
            ));
            let lines: Vec<&str> = entry.body.split('\n').collect();
            for (index, line) in lines.iter().enumerate() {
                out.push_str(&format!(
                    "<w:p w14:paraId=\"{}\"><w:pPr><w:pStyle w:val=\"CommentText\"/></w:pPr>",
                    para_id(entry.id, index)
                ));
                if index == 0 {
                    out.push_str(
                        "<w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr><w:annotationRef/></w:r>",
                    );
                }
                let mut children = String::new();
                if xml::push_run_children(&mut children, line) {
                    out.push_str("<w:r>");
                    out.push_str(&children);
                    out.push_str("</w:r>");
                }
                out.push_str("</w:p>");
            }
            out.push_str("</w:comment>");
        }
        out.push_str("</w:comments>");
        out
    }

    /// `word/commentsExtended.xml`: Word's record of which comments are
    /// resolved, keyed by the id of each comment's last paragraph.
    pub(super) fn comments_extended_xml(&self) -> String {
        let mut out = String::from(XML_DECLARATION);
        out.push_str(&format!(
            "<w15:commentsEx xmlns:mc=\"{MC_NS}\" xmlns:w14=\"{W14_NS}\" xmlns:w15=\"{W15_NS}\" mc:Ignorable=\"w14 w15\">"
        ));
        for entry in &self.entries {
            let last_line = entry.body.split('\n').count().saturating_sub(1);
            out.push_str(&format!(
                "<w15:commentEx w15:paraId=\"{}\" w15:done=\"{}\"/>",
                para_id(entry.id, last_line),
                u8::from(entry.resolved)
            ));
        }
        out.push_str("</w15:commentsEx>");
        out
    }
}

/// A paragraph id: eight hex digits below 0x80000000, unique per comment
/// paragraph and stable between runs.
fn para_id(comment: u32, line: usize) -> String {
    format!(
        "{:08X}",
        0x1000_0000u32 + comment * 0x100 + line.min(0xFF) as u32
    )
}

/// Appends the markers for comments that start or end at byte `at`. Ends of
/// non-empty ranges come first so adjacent comments do not nest, then starts,
/// then the end of ranges that are empty.
pub(super) fn push_events(out: &mut String, anchors: &[Anchor], at: usize) {
    for anchor in anchors.iter().filter(|a| a.end == at && a.start < a.end) {
        push_end(out, anchor.id);
    }
    for anchor in anchors.iter().filter(|a| a.start == at) {
        out.push_str(&format!("<w:commentRangeStart w:id=\"{}\"/>", anchor.id));
    }
    for anchor in anchors.iter().filter(|a| a.end == at && a.start == a.end) {
        push_end(out, anchor.id);
    }
}

fn push_end(out: &mut String, id: u32) {
    out.push_str(&format!(
        "<w:commentRangeEnd w:id=\"{id}\"/><w:r><w:rPr><w:rStyle w:val=\"CommentReference\"/></w:rPr><w:commentReference w:id=\"{id}\"/></w:r>"
    ));
}
