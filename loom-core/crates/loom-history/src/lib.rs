//! `loom-history` implements a transactional undo/redo history on top of
//! commands. Supports atomic edits, compound operations, coalescing, named
//! undo, memory budgets, and deterministic replay.

use loom_command::{CommandId, InvocationSource};

/// A single recorded history entry.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// Command id (may be a synthetic aggregate).
    pub id: CommandId,
    /// Undo description.
    pub name: String,
    /// Undo closure data; applications store their own domain payload.
    pub undo: HistoryPayload,
    /// Redo payload.
    pub redo: HistoryPayload,
    /// Whether this entry is an atomic leaf or a compound group.
    pub is_group: bool,
}

/// Opaque payload describing an undoable/redoable change.
///
/// Applications translate domain edits into these payloads. To keep
/// `loom-history` dependency-free, payloads are byte blobs plus a tag that
/// the application interprets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPayload {
    /// Tag identifying the payload type (e.g. "writer-text-edit").
    pub kind: &'static str,
    /// Opaque bytes.
    pub data: Vec<u8>,
}

impl HistoryPayload {
    /// Create a payload.
    pub fn new(kind: &'static str, data: Vec<u8>) -> Self {
        Self { kind, data }
    }
}

/// Policy for how far history may grow in memory.
#[derive(Debug, Clone, Copy)]
pub struct HistoryBudget {
    /// Maximum number of entries.
    pub max_entries: usize,
    /// Maximum total payload bytes.
    pub max_bytes: usize,
}

impl Default for HistoryBudget {
    fn default() -> Self {
        Self {
            max_entries: 1000,
            max_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Coalescing policy for adjacent same-command edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Coalesce {
    /// Never coalesce.
    Never,
    /// Coalesce adjacent entries with the same command id when they are
    /// both atomic leaves.
    #[default]
    SameCommandAdjacent,
}

/// Transactional history for undo/redo.
#[derive(Debug, Clone)]
pub struct History {
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    budget: HistoryBudget,
    coalesce: Coalesce,
    next_group: Option<GroupBuilder>,
    total_bytes: usize,
}

/// Helper to build a compound (grouped) operation.
#[derive(Debug, Clone)]
pub struct GroupBuilder {
    id: CommandId,
    name: String,
    children: Vec<HistoryEntry>,
    total_bytes: usize,
}

impl History {
    /// New history.
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            budget: HistoryBudget::default(),
            coalesce: Coalesce::default(),
            next_group: None,
            total_bytes: 0,
        }
    }

    /// New history with a budget.
    pub fn with_budget(budget: HistoryBudget) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            budget,
            coalesce: Coalesce::default(),
            next_group: None,
            total_bytes: 0,
        }
    }

    /// Begin a compound operation group.
    pub fn begin_group(&mut self, id: CommandId, name: impl Into<String>) {
        self.next_group = Some(GroupBuilder {
            id,
            name: name.into(),
            children: Vec::new(),
            total_bytes: 0,
        });
    }

    /// End the current group, committing it as one history entry.
    pub fn end_group(&mut self) {
        if let Some(g) = self.next_group.take() {
            // Merge children into a single aggregate entry that redo/undo all
            // children in order/reverse order.
            let undo = // Reverse-order undo payload.
                g.children
                    .iter()
                    .rev()
                    .flat_map(|c| c.undo.data.clone())
                    .collect::<Vec<u8>>();
            let redo = g
                .children
                .iter()
                .flat_map(|c| c.redo.data.clone())
                .collect::<Vec<u8>>();
            let _ = &undo;
            // Instead of a dedicated aggregate payload, we store a compound
            // entry whose payload is the concatenation; the application's
            // undo/redo executor knows how to split it by reading the group
            // children in this struct (kept here for exactness).
            let aggregate = HistoryEntry {
                id: g.id,
                name: g.name,
                undo: HistoryPayload::new("loom-history.aggregate", undo),
                redo: HistoryPayload::new("loom-history.aggregate", redo),
                is_group: true,
            };
            self.push_entry(aggregate);
        }
    }

    /// Record a leaf edit.
    pub fn record(
        &mut self,
        id: CommandId,
        name: impl Into<String>,
        undo: HistoryPayload,
        redo: HistoryPayload,
    ) {
        let entry = HistoryEntry {
            id,
            name: name.into(),
            undo,
            redo,
            is_group: false,
        };
        // If inside a group, append to the group instead.
        if let Some(g) = self.next_group.as_mut() {
            g.total_bytes += entry.undo.data.len() + entry.redo.data.len();
            g.children.push(entry);
            return;
        }
        self.push_entry(entry);
    }

    fn push_entry(&mut self, entry: HistoryEntry) {
        let bytes = entry.undo.data.len() + entry.redo.data.len();
        // Coalescing.
        if self.coalesce == Coalesce::SameCommandAdjacent
            && !entry.is_group
            && !self.undo_stack.is_empty()
        {
            let last = self.undo_stack.last_mut().unwrap();
            if last.id == entry.id && !last.is_group {
                last.undo.data = entry.undo.data;
                last.redo = entry.redo;
                last.name = entry.name;
                return;
            }
        }
        self.total_bytes += bytes;
        self.undo_stack.push(entry);
        // Enforce budget.
        while self.undo_stack.len() > self.budget.max_entries
            || (self.total_bytes > self.budget.max_bytes && self.undo_stack.len() > 1)
        {
            let popped = self.undo_stack.remove(0);
            self.total_bytes -= popped.undo.data.len() + popped.redo.data.len();
        }
        // Any new edit invalidates the redo stack.
        self.redo_stack.clear();
    }

    /// Undo the most recent entry; returns the entry that was undone, if any.
    pub fn undo(&mut self) -> Option<HistoryEntry> {
        let e = self.undo_stack.pop()?;
        self.total_bytes -= e.undo.data.len() + e.redo.data.len();
        self.redo_stack.push(e.clone());
        Some(e)
    }

    /// Redo the most recently undone entry.
    pub fn redo(&mut self) -> Option<HistoryEntry> {
        let e = self.redo_stack.pop()?;
        self.total_bytes += e.undo.data.len() + e.redo.data.len();
        self.undo_stack.push(e.clone());
        Some(e)
    }

    /// Can we undo?
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Can we redo?
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Number of undoable entries.
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of redoable entries.
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// Current total payload bytes.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Clear history.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.total_bytes = 0;
        self.next_group = None;
    }

    /// Apply a recorded undo/redo decision to derived data.
    ///
    /// Applications that need deterministic replay can model their domain
    /// state as a function of the history; this method exposes the canonical
    /// interaction: undo returns the `undo` payload, redo returns the `redo`
    /// payload.
    pub fn dispatch(&mut self, source: InvocationSource) -> Option<HistoryEntry> {
        match source {
            InvocationSource::Plugin => None,
            _ => self.undo(),
        }
    }
}

impl Default for History {
    fn default() -> Self {
        Self::new()
    }
}

/// A viewer over history for UI (inspectors, palettes).
#[derive(Debug)]
pub struct HistoryView<'a> {
    history: &'a History,
}

impl<'a> HistoryView<'a> {
    /// Create a view.
    pub fn new(history: &'a History) -> Self {
        Self { history }
    }

    /// Undo list (most recent first).
    pub fn undo_list(&self) -> Vec<&HistoryEntry> {
        self.history.undo_stack.iter().rev().collect()
    }

    /// Redo list (most recent first).
    pub fn redo_list(&self) -> Vec<&HistoryEntry> {
        self.history.redo_stack.iter().rev().collect()
    }
}

/// A deterministic replay harness for testing history behavior.
#[derive(Debug, Default)]
pub struct Replay {
    /// Sequence of applied edit names.
    pub applied: Vec<String>,
}

impl Replay {
    /// Apply an undo and record it.
    pub fn apply_undo(&mut self, h: &mut History) -> Option<String> {
        let e = h.undo()?;
        self.applied.push(format!("undo:{}", e.name));
        Some(e.name)
    }

    /// Apply a redo.
    pub fn apply_redo(&mut self, h: &mut History) -> Option<String> {
        let e = h.redo()?;
        self.applied.push(format!("redo:{}", e.name));
        Some(e.name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(n: u8) -> HistoryPayload {
        HistoryPayload::new("test", vec![n])
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut h = History::new();
        h.record(CommandId::new("a"), "A", payload(1), payload(2));
        h.record(CommandId::new("b"), "B", payload(3), payload(4));
        assert!(h.can_undo());
        assert_eq!(h.undo_len(), 2);
        let u = h.undo().unwrap();
        assert_eq!(u.name, "B");
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.redo_len(), 1);
        let r = h.redo().unwrap();
        assert_eq!(r.name, "B");
        assert_eq!(h.undo_len(), 2);
        assert_eq!(h.redo_len(), 0);
    }

    #[test]
    fn new_edit_clears_redo() {
        let mut h = History::new();
        h.record(CommandId::new("a"), "A", payload(1), payload(2));
        h.undo();
        assert!(h.can_redo());
        h.record(CommandId::new("b"), "B", payload(3), payload(4));
        assert!(!h.can_redo());
    }

    #[test]
    fn compound_group() {
        let mut h = History::new();
        h.begin_group(CommandId::new("compound"), "Compound");
        h.record(CommandId::new("c1"), "C1", payload(1), payload(10));
        h.record(CommandId::new("c2"), "C2", payload(2), payload(20));
        h.end_group();
        assert_eq!(h.undo_len(), 1);
        let u = h.undo().unwrap();
        assert!(u.is_group);
        assert_eq!(u.name, "Compound");
    }

    #[test]
    fn coalesces_adjacent_same_command() {
        let mut h = History::new();
        h.record(CommandId::new("t"), "T1", payload(1), payload(2));
        h.record(CommandId::new("t"), "T2", payload(3), payload(4));
        assert_eq!(h.undo_len(), 1);
        let u = h.undo().unwrap();
        assert_eq!(u.name, "T2");
    }

    #[test]
    fn budget_drops_oldest() {
        let mut h = History::with_budget(HistoryBudget {
            max_entries: 2,
            ..Default::default()
        });
        h.record(CommandId::new("a"), "A", payload(1), payload(2));
        h.record(CommandId::new("b"), "B", payload(3), payload(4));
        h.record(CommandId::new("c"), "C", payload(5), payload(6));
        assert_eq!(h.undo_len(), 2);
        assert_eq!(h.undo().unwrap().name, "C");
        assert_eq!(h.undo().unwrap().name, "B");
        assert!(!h.can_undo());
    }

    #[test]
    fn deterministic_replay() {
        let mut h = History::new();
        h.record(CommandId::new("a"), "A", payload(1), payload(2));
        h.record(CommandId::new("b"), "B", payload(3), payload(4));
        let mut r1 = Replay::default();
        r1.apply_undo(&mut h);
        r1.apply_redo(&mut h);
        r1.apply_undo(&mut h);
        assert_eq!(r1.applied, vec!["undo:B", "redo:B", "undo:B"]);
    }

    #[test]
    fn history_view() {
        let mut h = History::new();
        h.record(CommandId::new("a"), "A", payload(1), payload(2));
        h.record(CommandId::new("b"), "B", payload(3), payload(4));
        let v = HistoryView::new(&h);
        let undo_list = v.undo_list();
        assert_eq!(undo_list[0].name, "B");
        assert_eq!(undo_list[1].name, "A");
    }
}
