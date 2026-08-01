//! `loom-document` provides a shared document model: dense text buffers,
//! block/paragraph structures, cursor mapping, and immutable edit operations
//! that applications (Writer, Present, etc.) build their engines on.
//!
//! Documents are plain Rust values with deterministic serialization; the
//! application-specific engines (rich text, spreadsheet grid, etc.) layer on
//! top of these primitives.

use std::sync::Arc;

/// A binary-safe character index into a `Text`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Offset(pub usize);

/// Length of a text span.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Length(pub usize);

/// A dense, immutable UTF-8 text buffer.
///
/// Text is stored as a single `Arc<str>` so clones are cheap and edits
/// produce new versions via structural sharing where useful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text {
    data: Arc<str>,
}

impl Text {
    /// Create an empty text.
    pub fn empty() -> Self {
        Self {
            data: Arc::from(""),
        }
    }

    /// Create from a string.
    ///
    /// Kept as an infallible inherent constructor (not `std::str::FromStr`,
    /// which requires a fallible signature); the name matches the public API.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Self {
        Self { data: Arc::from(s) }
    }

    /// Number of bytes.
    pub fn len_bytes(&self) -> usize {
        self.data.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Borrow the underlying string.
    pub fn as_str(&self) -> &str {
        &self.data
    }

    /// Insert `s` at byte offset `pos` (must be a char boundary).
    pub fn insert(&self, pos: Offset, s: &str) -> Self {
        let n = self.data.len();
        let p = pos.0;
        assert!(p <= n, "insert offset out of range");
        assert!(
            self.data.is_char_boundary(p),
            "insert offset is not a char boundary"
        );
        let mut out = String::with_capacity(n + s.len());
        out.push_str(&self.data[..p]);
        out.push_str(s);
        out.push_str(&self.data[p..]);
        Self {
            data: Arc::from(out),
        }
    }

    /// Remove `len` bytes starting at `pos` (must both be char boundaries).
    pub fn delete(&self, pos: Offset, len: Length) -> Self {
        let p = pos.0;
        let l = len.0;
        let n = self.data.len();
        assert!(p + l <= n, "delete range out of bounds");
        assert!(self.data.is_char_boundary(p), "start not char boundary");
        assert!(self.data.is_char_boundary(p + l), "end not char boundary");
        let mut out = String::with_capacity(n - l);
        out.push_str(&self.data[..p]);
        out.push_str(&self.data[p + l..]);
        Self {
            data: Arc::from(out),
        }
    }

    /// Replace the range `[pos, pos+len)` with `s`.
    pub fn replace(&self, pos: Offset, len: Length, s: &str) -> Self {
        self.delete(pos, len).insert(pos, s)
    }

    /// Validate that a byte offset is a char boundary; returns the offset if so.
    pub fn check_boundary(&self, pos: usize) -> Option<usize> {
        if pos <= self.data.len() && self.data.is_char_boundary(pos) {
            Some(pos)
        } else {
            None
        }
    }

    /// Convert a byte offset to a Unicode scalar count (code points).
    pub fn byte_to_char(&self, pos: usize) -> usize {
        self.data[..pos.min(self.data.len())].chars().count()
    }

    /// Convert a code-point count to a byte offset.
    pub fn char_to_byte(&self, chars: usize) -> usize {
        self.data
            .char_indices()
            .nth(chars)
            .map(|(i, _)| i)
            .unwrap_or(self.data.len())
    }
}

/// Granularity of a tracked edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditKind {
    /// Insertion.
    Insert,
    /// Deletion.
    Delete,
    /// Replacement (delete+insert).
    Replace,
}

/// A single immutable, redoable/undoable edit on a `Text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    /// Offset where the edit applies.
    pub pos: Offset,
    /// Removed bytes.
    pub removed: Text,
    /// Inserted bytes.
    pub inserted: Text,
    /// Kind.
    pub kind: EditKind,
}

impl TextEdit {
    /// Create an insert edit.
    pub fn insert(pos: Offset, s: &str) -> Self {
        Self {
            pos,
            removed: Text::empty(),
            inserted: Text::from_str(s),
            kind: EditKind::Insert,
        }
    }

    /// Create a delete edit.
    pub fn delete(pos: Offset, _len: Length, removed: &str) -> Self {
        Self {
            pos,
            removed: Text::from_str(removed),
            inserted: Text::empty(),
            kind: EditKind::Delete,
        }
    }

    /// Create a replace edit.
    pub fn replace(pos: Offset, _len: Length, removed: &str, inserted: &str) -> Self {
        Self {
            pos,
            removed: Text::from_str(removed),
            inserted: Text::from_str(inserted),
            kind: EditKind::Replace,
        }
    }

    /// Apply this edit to `input`, returning a new `Text`.
    pub fn apply(&self, input: &Text) -> Text {
        input.replace(
            self.pos,
            Length(self.removed.len_bytes()),
            self.inserted.as_str(),
        )
    }

    /// The inverse edit (undo).
    pub fn inverse(&self) -> Self {
        Self {
            pos: self.pos,
            removed: self.inserted.clone(),
            inserted: self.removed.clone(),
            // Inverse kind is opposite.
            kind: match self.kind {
                EditKind::Insert => EditKind::Delete,
                EditKind::Delete => EditKind::Insert,
                EditKind::Replace => EditKind::Replace,
            },
        }
    }
}

/// A parsed hierarchy node (block) within a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// Unique identifier.
    pub id: u64,
    /// Block type name (e.g. "paragraph", "heading", "table").
    pub kind: String,
    /// Byte range into the concatenated content stream, if text-backed.
    pub range: Option<(usize, usize)>,
}

/// A simple ordered collection of blocks.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BlockTree {
    /// Root blocks in order.
    pub children: Vec<Block>,
}

impl BlockTree {
    /// New empty tree.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a block.
    pub fn push(&mut self, b: Block) {
        self.children.push(b);
    }

    /// Number of blocks.
    pub fn len(&self) -> usize {
        self.children.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Find block by id.
    pub fn find(&self, id: u64) -> Option<&Block> {
        self.children.iter().find(|b| b.id == id)
    }
}

/// A reversible document mutation described as a sequence of `TextEdit`s.
#[derive(Debug, Clone, Default)]
pub struct Mutation {
    /// Ordered edits.
    pub edits: Vec<TextEdit>,
}

impl Mutation {
    /// New empty mutation.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an edit.
    pub fn push(&mut self, e: TextEdit) {
        self.edits.push(e);
    }

    /// Apply to a text.
    pub fn apply(&self, t: &Text) -> Text {
        let mut cur = t.clone();
        for e in &self.edits {
            cur = e.apply(&cur);
        }
        cur
    }

    /// Inverse mutation.
    pub fn inverse(&self) -> Self {
        let mut out = Vec::with_capacity(self.edits.len());
        // Apply in reverse, invert each.
        let offset = 0;
        for e in self.edits.iter().rev() {
            // Inverse positions relative to the ORIGINAL text need shifting by
            // the net insert/delete before this edit. For a robust model we
            // keep a simpler invariant: inverse edits are applied to the
            // pre-edit text with re-computed offsets. Compute the delta as we
            // go from the end.
            let _ = offset;
            let delta_before: isize = self
                .edits
                .iter()
                .take_while(|x| x.pos > e.pos)
                .map(|x| x.inserted.len_bytes() as isize - x.removed.len_bytes() as isize)
                .sum();
            let shifted = (e.pos.0 as isize + delta_before) as usize;
            let mut e2 = e.inverse();
            e2.pos = Offset(shifted);
            out.push(e2);
        }
        Mutation { edits: out }
    }
}

/// Cursor mapping model for bidirectional text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor(pub usize);

/// Test helpers.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_delete_replace() {
        let t = Text::from_str("hello world");
        let u = t.insert(Offset(5), ", brave");
        assert_eq!(u.as_str(), "hello, brave world");
        // ", brave" is 7 bytes: comma + space + "brave".
        let v = u.delete(Offset(5), Length(7));
        assert_eq!(v.as_str(), "hello world");
        let w = t.replace(Offset(6), Length(5), "there");
        assert_eq!(w.as_str(), "hello there");
    }

    #[test]
    fn char_boundary_validation() {
        let t = Text::from_str("héllo");
        assert!(t.check_boundary(0).is_some());
        assert!(t.check_boundary(1).is_some());
        assert!(t.check_boundary(2).is_none()); // middle of é
        assert!(t.check_boundary(6).is_some());
    }

    #[test]
    fn byte_char_conversion() {
        let t = Text::from_str("aéβc");
        assert_eq!(t.byte_to_char(0), 0);
        assert_eq!(t.char_to_byte(2), 3); // a(1) + é(2) = offset 3
        assert_eq!(t.byte_to_char(t.len_bytes()), 4);
    }

    #[test]
    fn mutation_inverse_roundtrip() {
        let t = Text::from_str("abc");
        let mut m = Mutation::new();
        m.push(TextEdit::insert(Offset(1), "X"));
        let after = m.apply(&t);
        assert_eq!(after.as_str(), "aXbc");
        let inv = m.inverse();
        let back = inv.apply(&after);
        assert_eq!(back.as_str(), "abc");
    }

    #[test]
    fn multiple_edits_apply() {
        let t = Text::from_str("");
        let mut m = Mutation::new();
        m.push(TextEdit::insert(Offset(0), "a"));
        m.push(TextEdit::insert(Offset(1), "b"));
        m.push(TextEdit::insert(Offset(2), "c"));
        assert_eq!(m.apply(&t).as_str(), "abc");
    }

    #[test]
    fn block_tree_basics() {
        let mut tree = BlockTree::new();
        tree.push(Block {
            id: 1,
            kind: "paragraph".into(),
            range: Some((0, 5)),
        });
        tree.push(Block {
            id: 2,
            kind: "heading".into(),
            range: None,
        });
        assert_eq!(tree.len(), 2);
        assert_eq!(tree.find(2).unwrap().kind, "heading");
    }
}
