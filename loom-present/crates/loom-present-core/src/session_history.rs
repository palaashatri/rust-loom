//! Snapshot history for [`PresentationSession`]. A snapshot is the document
//! together with its per-slide transitions, so undo and redo reverse both.

use crate::{PresentationDocument, PresentationSession, TransitionKind};
use std::collections::BTreeMap;

/// One undo/redo step: the document and the slide transitions.
pub(crate) type Snapshot = (PresentationDocument, BTreeMap<String, TransitionKind>);

impl PresentationSession {
    fn snapshot(&self) -> Snapshot {
        (self.document.clone(), self.transitions.clone())
    }

    fn restore(&mut self, (document, transitions): Snapshot) -> Snapshot {
        (
            std::mem::replace(&mut self.document, document),
            std::mem::replace(&mut self.transitions, transitions),
        )
    }

    /// Records the current document and transitions before a mutation.
    pub fn checkpoint(&mut self) {
        self.checkpoint_redo = Some(std::mem::take(&mut self.redo));
        self.undo.push(self.snapshot());
        if self.undo.len() > self.history_limit {
            self.undo.remove(0);
        }
    }

    /// Cancels the most recent checkpoint and restores the state it captured.
    /// This is used when a pointer gesture is cancelled or returns to its
    /// starting geometry; cancelled gestures must not leave an undo entry or a
    /// partially transformed document.
    pub fn cancel_checkpoint(&mut self) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.restore(previous);
        self.redo = self.checkpoint_redo.take().unwrap_or_default();
        true
    }

    /// Restores the previous snapshot.
    pub fn undo(&mut self) -> bool {
        self.checkpoint_redo = None;
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = self.restore(previous);
        self.redo.push(current);
        true
    }

    /// Reapplies the next snapshot.
    pub fn redo(&mut self) -> bool {
        self.checkpoint_redo = None;
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = self.restore(next);
        self.undo.push(current);
        true
    }

    /// Returns whether the session has an undo snapshot.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Returns whether the session has a redo snapshot.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
