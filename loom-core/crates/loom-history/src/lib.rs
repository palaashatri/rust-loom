//! `loom-history` provides transactional undo/redo history for Loom applications.
//! Supports atomic edits, compound operations, nested groups, coalescing, named
//! undo/redo descriptions, memory budgets, and deterministic replay.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use loom_command::{CommandId, InvocationSource};

/// Opaque payload describing an undoable/redoable change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryPayload {
    /// Tag identifying the payload type (e.g. `"writer.text_delta"`, `"sheets.cell_edit"`).
    pub kind: &'static str,
    /// Serialized delta or mutation bytes.
    pub data: Vec<u8>,
}

impl HistoryPayload {
    /// Create a new payload.
    pub fn new(kind: &'static str, data: Vec<u8>) -> Self {
        Self { kind, data }
    }

    /// Number of bytes in the payload.
    pub fn byte_size(&self) -> usize {
        self.data.len()
    }
}

/// A single recorded history entry: either an atomic leaf edit or a compound group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryEntry {
    /// An atomic leaf edit.
    Leaf {
        /// Command identifier.
        id: CommandId,
        /// Short human-readable description for undo.
        name: String,
        /// Undo payload (reverts document from post-state to pre-state).
        undo: HistoryPayload,
        /// Redo payload (advances document from pre-state to post-state).
        redo: HistoryPayload,
    },
    /// A compound group that preserves child boundaries, payload types, and order.
    Group {
        /// Group command identifier.
        id: CommandId,
        /// Group name.
        name: String,
        /// Children in forward execution order.
        children: Vec<HistoryEntry>,
    },
}

impl HistoryEntry {
    /// Command identifier.
    pub fn id(&self) -> &CommandId {
        match self {
            Self::Leaf { id, .. } => id,
            Self::Group { id, .. } => id,
        }
    }

    /// User-visible operation name.
    pub fn name(&self) -> &str {
        match self {
            Self::Leaf { name, .. } => name.as_str(),
            Self::Group { name, .. } => name.as_str(),
        }
    }

    /// Whether this entry is a compound group.
    pub fn is_group(&self) -> bool {
        matches!(self, Self::Group { .. })
    }

    /// Total payload memory occupied by this entry in bytes.
    pub fn byte_size(&self) -> usize {
        match self {
            Self::Leaf { undo, redo, .. } => undo.byte_size() + redo.byte_size(),
            Self::Group { children, .. } => children.iter().map(HistoryEntry::byte_size).sum(),
        }
    }

    /// Unroll undo actions in reverse chronological order.
    pub fn unroll_undo(&self) -> Vec<(&CommandId, &str, &HistoryPayload)> {
        let mut ops = Vec::new();
        self.collect_undo_ops(&mut ops);
        ops
    }

    fn collect_undo_ops<'a>(&'a self, out: &mut Vec<(&'a CommandId, &'a str, &'a HistoryPayload)>) {
        match self {
            Self::Leaf { id, name, undo, .. } => {
                out.push((id, name.as_str(), undo));
            }
            Self::Group { children, .. } => {
                for child in children.iter().rev() {
                    child.collect_undo_ops(out);
                }
            }
        }
    }

    /// Unroll redo actions in forward chronological order.
    pub fn unroll_redo(&self) -> Vec<(&CommandId, &str, &HistoryPayload)> {
        let mut ops = Vec::new();
        self.collect_redo_ops(&mut ops);
        ops
    }

    fn collect_redo_ops<'a>(&'a self, out: &mut Vec<(&'a CommandId, &'a str, &'a HistoryPayload)>) {
        match self {
            Self::Leaf { id, name, redo, .. } => {
                out.push((id, name.as_str(), redo));
            }
            Self::Group { children, .. } => {
                for child in children.iter() {
                    child.collect_redo_ops(out);
                }
            }
        }
    }
}

/// Budget constraints for undo/redo history in memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryBudget {
    /// Maximum number of undo entries retained.
    pub max_entries: usize,
    /// Maximum total payload bytes retained.
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

/// Coalescing policy for high-frequency adjacent edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Coalesce {
    /// Never coalesce entries.
    Never,
    /// Coalesce adjacent leaf entries with identical command ID.
    #[default]
    SameCommandAdjacent,
}

/// In-progress group builder on the group stack.
#[derive(Debug, Clone)]
struct GroupBuilder {
    id: CommandId,
    name: String,
    children: Vec<HistoryEntry>,
}

/// Transactional undo/redo history manager.
#[derive(Debug, Clone)]
pub struct History {
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    budget: HistoryBudget,
    coalesce: Coalesce,
    group_stack: Vec<GroupBuilder>,
    total_bytes: usize,
}

impl History {
    /// Create a new history manager with default budget.
    pub fn new() -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            budget: HistoryBudget::default(),
            coalesce: Coalesce::default(),
            group_stack: Vec::new(),
            total_bytes: 0,
        }
    }

    /// Create a new history manager with a custom memory budget.
    pub fn with_budget(budget: HistoryBudget) -> Self {
        Self {
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            budget,
            coalesce: Coalesce::default(),
            group_stack: Vec::new(),
            total_bytes: 0,
        }
    }

    /// Configure coalescing policy.
    pub fn set_coalesce(&mut self, coalesce: Coalesce) {
        self.coalesce = coalesce;
    }

    /// Begin a compound operation group. Supports nested groups.
    pub fn begin_group(&mut self, id: impl Into<CommandId>, name: impl Into<String>) {
        self.group_stack.push(GroupBuilder {
            id: id.into(),
            name: name.into(),
            children: Vec::new(),
        });
    }

    /// End the current compound group.
    /// Does not create empty history entries if the group had no children.
    pub fn end_group(&mut self) {
        if let Some(builder) = self.group_stack.pop() {
            if builder.children.is_empty() {
                // Empty groups do not generate fake history
                return;
            }
            let group_entry = HistoryEntry::Group {
                id: builder.id,
                name: builder.name,
                children: builder.children,
            };

            if let Some(parent) = self.group_stack.last_mut() {
                // Nested group: attach to parent builder
                parent.children.push(group_entry);
            } else {
                // Top-level group: push to undo stack
                self.push_entry(group_entry);
            }
        }
    }

    /// Record a leaf edit.
    pub fn record(
        &mut self,
        id: impl Into<CommandId>,
        name: impl Into<String>,
        undo: HistoryPayload,
        redo: HistoryPayload,
    ) {
        let entry = HistoryEntry::Leaf {
            id: id.into(),
            name: name.into(),
            undo,
            redo,
        };

        if let Some(current_group) = self.group_stack.last_mut() {
            current_group.children.push(entry);
            return;
        }

        self.push_entry(entry);
    }

    fn push_entry(&mut self, entry: HistoryEntry) {
        // Any new edit ALWAYS invalidates the redo stack
        self.redo_stack.clear();

        // Check coalescing: preserve earliest undo state, update to latest redo state
        if self.coalesce == Coalesce::SameCommandAdjacent {
            if let HistoryEntry::Leaf {
                ref id,
                ref name,
                ref redo,
                ..
            } = entry
            {
                if let Some(HistoryEntry::Leaf {
                    id: ref last_id,
                    name: ref mut last_name,
                    undo: ref _last_undo,
                    redo: ref mut last_redo,
                }) = self.undo_stack.last_mut()
                {
                    if last_id == id {
                        // Crucial correctness: Keep earliest undo, update to latest redo
                        let old_bytes = last_redo.byte_size();
                        let new_bytes = redo.byte_size();
                        *last_redo = redo.clone();
                        *last_name = name.clone();
                        if new_bytes >= old_bytes {
                            self.total_bytes += new_bytes - old_bytes;
                        } else {
                            self.total_bytes -= old_bytes - new_bytes;
                        }
                        return;
                    }
                }
            }
        }

        let bytes = entry.byte_size();
        self.total_bytes += bytes;
        self.undo_stack.push(entry);

        // Enforce memory & count budgets
        while self.undo_stack.len() > self.budget.max_entries
            || (self.total_bytes > self.budget.max_bytes && self.undo_stack.len() > 1)
        {
            let popped = self.undo_stack.remove(0);
            self.total_bytes -= popped.byte_size();
        }
    }

    /// Undo the most recent operation; moves it to redo stack and returns the entry.
    pub fn undo(&mut self) -> Option<HistoryEntry> {
        let e = self.undo_stack.pop()?;
        self.total_bytes -= e.byte_size();
        self.redo_stack.push(e.clone());
        Some(e)
    }

    /// Redo the most recently undone operation; moves it back to undo stack and returns it.
    pub fn redo(&mut self) -> Option<HistoryEntry> {
        let e = self.redo_stack.pop()?;
        self.total_bytes += e.byte_size();
        self.undo_stack.push(e.clone());
        Some(e)
    }

    /// Whether an undo operation is currently available.
    pub fn can_undo(&self) -> bool {
        !self.undo_stack.is_empty()
    }

    /// Whether a redo operation is currently available.
    pub fn can_redo(&self) -> bool {
        !self.redo_stack.is_empty()
    }

    /// Number of operations on the undo stack.
    pub fn undo_len(&self) -> usize {
        self.undo_stack.len()
    }

    /// Number of operations on the redo stack.
    pub fn redo_len(&self) -> usize {
        self.redo_stack.len()
    }

    /// Description of the next undoable operation, if available.
    pub fn undo_name(&self) -> Option<&str> {
        self.undo_stack.last().map(HistoryEntry::name)
    }

    /// Description of the next redoable operation, if available.
    pub fn redo_name(&self) -> Option<&str> {
        self.redo_stack.last().map(HistoryEntry::name)
    }

    /// Current total payload memory occupied in bytes.
    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    /// Clear all undo, redo, and in-progress group state.
    pub fn clear(&mut self) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.group_stack.clear();
        self.total_bytes = 0;
    }

    /// Dispatch helper for invocation source handling.
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

/// A read-only view over history for UI inspectors and history panels.
#[derive(Debug)]
pub struct HistoryView<'a> {
    history: &'a History,
}

impl<'a> HistoryView<'a> {
    /// Create a new history view.
    pub fn new(history: &'a History) -> Self {
        Self { history }
    }

    /// List of undoable operations in reverse order (most recent first).
    pub fn undo_list(&self) -> Vec<&'a HistoryEntry> {
        self.history.undo_stack.iter().rev().collect()
    }

    /// List of redoable operations in reverse order (most recent first).
    pub fn redo_list(&self) -> Vec<&'a HistoryEntry> {
        self.history.redo_stack.iter().rev().collect()
    }
}

/// Deterministic replay harness for testing history and state recovery.
#[derive(Debug, Default)]
pub struct Replay {
    /// Sequence of applied events.
    pub applied: Vec<String>,
}

impl Replay {
    /// Apply an undo step and record it.
    pub fn apply_undo(&mut self, h: &mut History) -> Option<String> {
        let e = h.undo()?;
        self.applied.push(format!("undo:{}", e.name()));
        Some(e.name().to_string())
    }

    /// Apply a redo step and record it.
    pub fn apply_redo(&mut self, h: &mut History) -> Option<String> {
        let e = h.redo()?;
        self.applied.push(format!("redo:{}", e.name()));
        Some(e.name().to_string())
    }
}

/// One durable recovery journal record: a kind tag, payload bytes, and a CRC-32 over both
/// so torn or corrupted tails are detectable on replay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalRecord {
    /// Tag identifying the operation family (e.g. `"checkpoint"`, `"op.text_delta"`).
    pub kind: String,
    /// Serialized record body.
    pub payload: Vec<u8>,
    /// CRC-32 (IEEE, reflected, polynomial 0xEDB88320) over kind bytes + 0x00 + payload.
    pub crc32: u32,
}

impl JournalRecord {
    /// Creates a record and computes its checksum.
    pub fn new(kind: impl Into<String>, payload: Vec<u8>) -> Self {
        let kind = kind.into();
        let crc32 = crc32_ieee_record(&kind, &payload);
        Self {
            kind,
            payload,
            crc32,
        }
    }

    /// Recomputes and verifies the stored checksum.
    pub fn verify(&self) -> bool {
        crc32_ieee_record(&self.kind, &self.payload) == self.crc32
    }

    fn encode(&self) -> Vec<u8> {
        // Frame: [u32 kind_len][kind][u32 payload_len][payload][u32 crc]
        let mut out = Vec::with_capacity(12 + self.kind.len() + self.payload.len());
        out.extend_from_slice(&(self.kind.len() as u32).to_be_bytes());
        out.extend_from_slice(self.kind.as_bytes());
        out.extend_from_slice(&(self.payload.len() as u32).to_be_bytes());
        out.extend_from_slice(&self.payload);
        out.extend_from_slice(&self.crc32.to_be_bytes());
        out
    }

    fn decode(bytes: &[u8]) -> Option<(Self, usize)> {
        if bytes.len() < 4 {
            return None;
        }
        let kind_len = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
        let mut cursor = 4;
        if bytes.len() < cursor + kind_len + 4 {
            return None;
        }
        let kind = String::from_utf8(bytes[cursor..cursor + kind_len].to_vec()).ok()?;
        cursor += kind_len;
        let payload_len = u32::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]) as usize;
        cursor += 4;
        if bytes.len() < cursor + payload_len + 4 {
            return None;
        }
        let payload = bytes[cursor..cursor + payload_len].to_vec();
        cursor += payload_len;
        let crc32 = u32::from_be_bytes([
            bytes[cursor],
            bytes[cursor + 1],
            bytes[cursor + 2],
            bytes[cursor + 3],
        ]);
        cursor += 4;
        let record = Self {
            kind,
            payload,
            crc32,
        };
        if !record.verify() {
            return None;
        }
        Some((record, cursor))
    }
}

fn crc32_ieee_record(kind: &str, payload: &[u8]) -> u32 {
    let mut crc: u32 = 0xFFFF_FFFF;
    fn feed_byte(crc: &mut u32, byte: u8) {
        *crc ^= byte as u32;
        for _ in 0..8 {
            // Reflected CRC-32 (IEEE 802.3): branchless LSB polynomial step.
            let mask = (*crc & 1).wrapping_neg();
            *crc = (*crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    for byte in kind.as_bytes() {
        feed_byte(&mut crc, *byte);
    }
    feed_byte(&mut crc, 0x00);
    for byte in payload {
        feed_byte(&mut crc, *byte);
    }
    !crc
}

/// An append-only crash-recovery journal. Entries are individually checksummed; replay
/// accepts the longest valid prefix so a process killed mid-append loses only the torn
/// final record, never earlier durable state.
#[derive(Debug, Clone, Default)]
pub struct RecoveryJournal {
    encoded: Vec<u8>,
    record_count: usize,
}

impl RecoveryJournal {
    /// Creates an empty journal.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one record durably (in memory; hosts persist `encoded_bytes` after each call).
    pub fn append(&mut self, record: &JournalRecord) {
        self.encoded.extend_from_slice(&record.encode());
        self.record_count += 1;
    }

    /// Replays the longest valid prefix of records. A corrupted or torn entry stops replay
    /// and everything from that point is discarded; earlier records survive.
    pub fn replay(&self) -> Vec<JournalRecord> {
        let mut records = Vec::new();
        let mut cursor = 0usize;
        while let Some((record, consumed)) = JournalRecord::decode(&self.encoded[cursor..]) {
            records.push(record);
            cursor += consumed;
        }
        records
    }

    /// Number of records that survive [`RecoveryJournal::replay`] — i.e. the valid prefix
    /// length, which may be shorter than what was appended when the tail is damaged.
    pub fn recoverable_count(&self) -> usize {
        self.replay().len()
    }

    /// Records appended since creation, including any unrecoverable tail entries.
    pub fn appended_count(&self) -> usize {
        self.record_count
    }

    /// The serialized journal bytes for host persistence.
    pub fn encoded_bytes(&self) -> &[u8] {
        &self.encoded
    }

    /// Rebuilds a journal from previously persisted bytes, discarding any invalid tail.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut journal = Self::new();
        journal.encoded = bytes.to_vec();
        // Count only the recoverable prefix as authoritative history.
        journal.record_count = journal.recoverable_count();
        // Truncate the buffer to the valid prefix so future appends stay decodable.
        let mut cursor = 0usize;
        while let Some((_record, consumed)) = JournalRecord::decode(&journal.encoded[cursor..]) {
            cursor += consumed;
        }
        journal.encoded.truncate(cursor);
        journal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(kind: &'static str, data: &[u8]) -> HistoryPayload {
        HistoryPayload::new(kind, data.to_vec())
    }

    #[test]
    fn undo_redo_roundtrip() {
        let mut h = History::new();
        h.record("edit.a", "A", payload("t", &[1]), payload("t", &[2]));
        h.record("edit.b", "B", payload("t", &[3]), payload("t", &[4]));

        assert!(h.can_undo());
        assert_eq!(h.undo_len(), 2);
        assert_eq!(h.undo_name(), Some("B"));

        let u = h.undo().unwrap();
        assert_eq!(u.name(), "B");
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.redo_len(), 1);
        assert_eq!(h.redo_name(), Some("B"));

        let r = h.redo().unwrap();
        assert_eq!(r.name(), "B");
        assert_eq!(h.undo_len(), 2);
        assert_eq!(h.redo_len(), 0);
    }

    #[test]
    fn new_edit_clears_redo_even_when_coalesced() {
        let mut h = History::new();
        h.record(
            "edit.type",
            "Type A",
            payload("t", &[0]),
            payload("t", &[1]),
        );
        h.undo();
        assert!(h.can_redo());

        // Coalescing new edit must clear redo stack
        h.record(
            "edit.type",
            "Type B",
            payload("t", &[1]),
            payload("t", &[2]),
        );
        assert!(!h.can_redo());
    }

    #[test]
    fn coalescing_preserves_earliest_undo_state() {
        let mut h = History::new();
        // State 0 -> 1
        h.record(
            "edit.type",
            "Type 'h'",
            payload("t", &[0]),
            payload("t", &[1]),
        );
        // State 1 -> 2
        h.record(
            "edit.type",
            "Type 'he'",
            payload("t", &[1]),
            payload("t", &[2]),
        );
        // State 2 -> 3
        h.record(
            "edit.type",
            "Type 'hel'",
            payload("t", &[2]),
            payload("t", &[3]),
        );

        assert_eq!(h.undo_len(), 1);
        let entry = h.undo().unwrap();
        assert_eq!(entry.name(), "Type 'hel'");

        if let HistoryEntry::Leaf { undo, redo, .. } = entry {
            // Must preserve EARLIEST undo state (0), not intermediate (2)
            assert_eq!(undo.data, vec![0]);
            // Must preserve LATEST redo state (3)
            assert_eq!(redo.data, vec![3]);
        } else {
            panic!("expected leaf entry");
        }
    }

    #[test]
    fn compound_group_preserves_children_boundaries_and_types() {
        let mut h = History::new();
        h.begin_group("format.bold_and_color", "Format Text");
        h.record(
            "format.bold",
            "Bold",
            payload("writer.bold", &[0]),
            payload("writer.bold", &[1]),
        );
        h.record(
            "format.color",
            "Color",
            payload("writer.color", &[255, 0, 0]),
            payload("writer.color", &[0, 255, 0]),
        );
        h.end_group();

        assert_eq!(h.undo_len(), 1);
        let entry = h.undo().unwrap();
        assert!(entry.is_group());
        assert_eq!(entry.name(), "Format Text");

        // Undo unrolls in reverse order (color then bold)
        let undo_ops = entry.unroll_undo();
        assert_eq!(undo_ops.len(), 2);
        assert_eq!(undo_ops[0].1, "Color");
        assert_eq!(undo_ops[0].2.kind, "writer.color");
        assert_eq!(undo_ops[1].1, "Bold");
        assert_eq!(undo_ops[1].2.kind, "writer.bold");

        // Redo unrolls in forward order (bold then color)
        let redo_ops = entry.unroll_redo();
        assert_eq!(redo_ops.len(), 2);
        assert_eq!(redo_ops[0].1, "Bold");
        assert_eq!(redo_ops[1].1, "Color");
    }

    #[test]
    fn nested_groups_supported_without_overwriting() {
        let mut h = History::new();
        h.begin_group("outer", "Outer Group");
        h.record("item.1", "Item 1", payload("t", &[1]), payload("t", &[2]));

        h.begin_group("inner", "Inner Group");
        h.record("item.2", "Item 2", payload("t", &[3]), payload("t", &[4]));
        h.end_group(); // Ends inner group

        h.record("item.3", "Item 3", payload("t", &[5]), payload("t", &[6]));
        h.end_group(); // Ends outer group

        assert_eq!(h.undo_len(), 1);
        let outer = h.undo().unwrap();
        assert_eq!(outer.name(), "Outer Group");

        let undo_ops = outer.unroll_undo();
        assert_eq!(undo_ops.len(), 3);
        assert_eq!(undo_ops[0].1, "Item 3");
        assert_eq!(undo_ops[1].1, "Item 2");
        assert_eq!(undo_ops[2].1, "Item 1");
    }

    #[test]
    fn empty_groups_do_not_generate_fake_history() {
        let mut h = History::new();
        h.begin_group("empty", "Empty Group");
        h.end_group();
        assert_eq!(h.undo_len(), 0);
        assert!(!h.can_undo());
    }

    #[test]
    fn byte_budget_accounting_with_groups() {
        let mut h = History::with_budget(HistoryBudget {
            max_entries: 10,
            max_bytes: 20,
        });

        h.begin_group("g1", "Group 1");
        h.record("a", "A", payload("t", &[1; 5]), payload("t", &[2; 5])); // 10 bytes
        h.end_group();
        assert_eq!(h.total_bytes(), 10);

        h.begin_group("g2", "Group 2");
        h.record("b", "B", payload("t", &[3; 8]), payload("t", &[4; 8])); // 16 bytes (total 26 > 20)
        h.end_group();

        // Old group dropped due to byte budget
        assert_eq!(h.undo_len(), 1);
        assert_eq!(h.undo_name(), Some("Group 2"));
        assert_eq!(h.total_bytes(), 16);
    }

    #[test]
    fn deterministic_replay() {
        let mut h = History::new();
        h.record("a", "Edit A", payload("t", &[1]), payload("t", &[2]));
        h.record("b", "Edit B", payload("t", &[3]), payload("t", &[4]));

        let mut replay = Replay::default();
        replay.apply_undo(&mut h);
        replay.apply_redo(&mut h);
        replay.apply_undo(&mut h);

        assert_eq!(
            replay.applied,
            vec!["undo:Edit B", "redo:Edit B", "undo:Edit B"]
        );
    }

    #[test]
    fn history_view_ordering() {
        let mut h = History::new();
        h.record("a", "First", payload("t", &[1]), payload("t", &[2]));
        h.record("b", "Second", payload("t", &[3]), payload("t", &[4]));

        let view = HistoryView::new(&h);
        let list = view.undo_list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name(), "Second");
        assert_eq!(list[1].name(), "First");
    }

    #[test]
    fn recovery_journal_survives_torn_tail() {
        let mut journal = RecoveryJournal::new();
        journal.append(&JournalRecord::new("checkpoint", b"state-a".to_vec()));
        journal.append(&JournalRecord::new("op.text_delta", b"delta-1".to_vec()));
        journal.append(&JournalRecord::new("checkpoint", b"state-b".to_vec()));

        assert_eq!(journal.appended_count(), 3);
        assert_eq!(journal.recoverable_count(), 3);
        let records = journal.replay();
        assert_eq!(records[1].kind, "op.text_delta");
        assert_eq!(records[2].payload, b"state-b");

        // Simulate a crash mid-append: keep only the first bytes of the third record.
        let full = journal.encoded_bytes().to_vec();
        let mut torn = RecoveryJournal::from_bytes(&full[..full.len() - 6]);
        // First two records survive; the torn tail is discarded.
        assert_eq!(torn.recoverable_count(), 2);
        assert_eq!(torn.appended_count(), 2);
        assert_eq!(torn.replay()[1].payload, b"delta-1");

        // The truncated journal remains appendable and decodable.
        torn.append(&JournalRecord::new("checkpoint", b"state-c".to_vec()));
        assert_eq!(torn.recoverable_count(), 3);
        assert_eq!(torn.replay()[2].payload, b"state-c");

        // A corrupted checksum inside the stream stops replay at that record.
        let mut corrupted_bytes = full.clone();
        let mid = full.len() - 10;
        corrupted_bytes[mid] ^= 0xFF;
        let damaged = RecoveryJournal::from_bytes(&corrupted_bytes);
        assert!(damaged.recoverable_count() < 3);

        // Checksums distinguish records: verify() passes for intact, fails when tampered.
        let mut record = JournalRecord::new("k", vec![1, 2, 3]);
        assert!(record.verify());
        record.crc32 ^= 1;
        assert!(!record.verify());

        // Round-trip through bytes preserves everything valid.
        let reloaded = RecoveryJournal::from_bytes(journal.encoded_bytes());
        assert_eq!(reloaded.replay(), journal.replay());
    }
}
