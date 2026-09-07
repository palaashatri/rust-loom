//! Document lifecycle state machine shared across all Loom applications.
//! Enforces invariants around clean/dirty states, path transitions, crash recovery,
//! Save As semantics, and failure/cancellation rollbacks.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::fmt;
use std::path::{Path, PathBuf};

/// Errors returned by invalid lifecycle state transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleError {
    /// Cannot modify a document that is in read-only mode.
    ReadOnlyModification(PathBuf),
    /// Cannot save without a path when document is untitled.
    NoPathForSave,
    /// Invalid state transition for current lifecycle state.
    InvalidTransition(String),
}

impl fmt::Display for LifecycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReadOnlyModification(p) => {
                write!(f, "cannot modify read-only document: {}", p.display())
            }
            Self::NoPathForSave => write!(f, "save requires a path for untitled documents"),
            Self::InvalidTransition(msg) => write!(f, "invalid lifecycle transition: {msg}"),
        }
    }
}

impl std::error::Error for LifecycleError {}

/// Explicit document lifecycle state machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentLifecycle {
    /// Blank document with no path and no unsaved changes.
    UntitledClean,
    /// Blank document with unsaved changes.
    UntitledModified,
    /// Document backed by a filesystem path with no unsaved changes.
    PathBackedClean(PathBuf),
    /// Document backed by a filesystem path with unsaved changes.
    PathBackedModified(PathBuf),
    /// Actively loading a document from a path.
    Loading {
        /// Target file path being loaded.
        target: PathBuf,
        /// Prior lifecycle state restored if loading fails or is cancelled.
        previous: Box<DocumentLifecycle>,
    },
    /// Actively saving a document to disk.
    Saving {
        /// Target file path being written.
        target: PathBuf,
        /// Prior lifecycle state restored if save fails or is cancelled.
        previous: Box<DocumentLifecycle>,
    },
    /// Document recovered from crash journal/snapshot.
    Recovering {
        /// Path the recovered document originally came from, if known.
        recovered_from: Option<PathBuf>,
    },
    /// Document has conflict with external disk modification.
    Conflicted {
        /// Conflicted document path.
        path: PathBuf,
    },
    /// Document is opened in read-only mode.
    ReadOnly(PathBuf),
    /// An operation failed, preserving the exact prior lifecycle state.
    FailedOperation {
        /// Prior lifecycle state.
        previous: Box<DocumentLifecycle>,
        /// Reason for failure.
        error: String,
    },
}

impl DocumentLifecycle {
    /// Create a new untitled clean document state.
    pub fn new_untitled() -> Self {
        Self::UntitledClean
    }

    /// Create a clean path-backed document state.
    pub fn new_path_backed(path: impl Into<PathBuf>) -> Self {
        Self::PathBackedClean(path.into())
    }

    /// Whether the document currently has unsaved modifications (dirty).
    pub fn is_dirty(&self) -> bool {
        match self {
            Self::UntitledClean | Self::PathBackedClean(_) | Self::ReadOnly(_) => false,
            Self::UntitledModified
            | Self::PathBackedModified(_)
            | Self::Recovering { .. }
            | Self::Conflicted { .. } => true,
            Self::Loading { previous, .. }
            | Self::Saving { previous, .. }
            | Self::FailedOperation { previous, .. } => previous.is_dirty(),
        }
    }

    /// Return the active backing file path if available.
    pub fn path(&self) -> Option<&Path> {
        match self {
            Self::PathBackedClean(p)
            | Self::PathBackedModified(p)
            | Self::Conflicted { path: p }
            | Self::ReadOnly(p) => Some(p.as_path()),
            Self::Saving { target, .. } => Some(target.as_path()),
            Self::Loading { previous, .. } | Self::FailedOperation { previous, .. } => {
                previous.path()
            }
            Self::Recovering { recovered_from } => recovered_from.as_deref(),
            Self::UntitledClean | Self::UntitledModified => None,
        }
    }

    /// Transition to modified (dirty) state due to a user edit.
    pub fn mark_modified(&mut self) -> Result<(), LifecycleError> {
        match self {
            Self::UntitledClean => {
                *self = Self::UntitledModified;
                Ok(())
            }
            Self::UntitledModified => Ok(()),
            Self::PathBackedClean(p) => {
                *self = Self::PathBackedModified(p.clone());
                Ok(())
            }
            Self::PathBackedModified(_) => Ok(()),
            Self::Recovering { .. } | Self::Conflicted { .. } => Ok(()),
            Self::ReadOnly(p) => Err(LifecycleError::ReadOnlyModification(p.clone())),
            Self::Loading { .. } | Self::Saving { .. } => Err(LifecycleError::InvalidTransition(
                "cannot edit while I/O operation is in flight".into(),
            )),
            Self::FailedOperation { previous, .. } => {
                *self = *previous.clone();
                self.mark_modified()
            }
        }
    }

    /// Initiate an open operation.
    pub fn start_open(&mut self, path: impl Into<PathBuf>) -> Result<(), LifecycleError> {
        if matches!(self, Self::Loading { .. } | Self::Saving { .. }) {
            return Err(LifecycleError::InvalidTransition(
                "an I/O operation is already in flight".into(),
            ));
        }
        let previous = Box::new(self.clone());
        *self = Self::Loading {
            target: path.into(),
            previous,
        };
        Ok(())
    }

    /// Finish an open operation. If cancelled or failed, restores previous document state untouched.
    pub fn finish_open(&mut self, success: bool) -> Result<Option<PathBuf>, LifecycleError> {
        match self.clone() {
            Self::Loading { target, previous } => {
                if success {
                    *self = Self::PathBackedClean(target.clone());
                    Ok(Some(target))
                } else {
                    // Cancelled or failed: restore previous document state completely untouched!
                    *self = *previous;
                    Ok(None)
                }
            }
            _ => Err(LifecycleError::InvalidTransition(
                "cannot finish open when not in Loading state".into(),
            )),
        }
    }

    /// Initiate a save or Save As operation.
    pub fn start_save(
        &mut self,
        explicit_path: Option<PathBuf>,
    ) -> Result<PathBuf, LifecycleError> {
        if matches!(self, Self::Loading { .. } | Self::Saving { .. }) {
            return Err(LifecycleError::InvalidTransition(
                "an I/O operation is already in flight".into(),
            ));
        }

        let target = match (explicit_path, self.path()) {
            (Some(path), _) => path,
            (None, Some(existing)) => existing.to_path_buf(),
            (None, None) => return Err(LifecycleError::NoPathForSave),
        };

        let previous = Box::new(self.clone());
        *self = Self::Saving {
            target: target.clone(),
            previous,
        };
        Ok(target)
    }

    /// Finish a save operation.
    /// On success, document becomes `PathBackedClean` with target path (Save As path updated).
    /// On failure or cancellation, previous dirty state is strictly preserved!
    pub fn finish_save(&mut self, success: bool) -> Result<PathBuf, LifecycleError> {
        match self.clone() {
            Self::Saving { target, previous } => {
                if success {
                    *self = Self::PathBackedClean(target.clone());
                    Ok(target)
                } else {
                    // Preserve previous dirty/modified state on failure
                    *self = *previous;
                    Ok(target)
                }
            }
            _ => Err(LifecycleError::InvalidTransition(
                "cannot finish save when not in Saving state".into(),
            )),
        }
    }

    /// Initialize recovery state from a crash journal.
    pub fn start_recovery(recovered_from: Option<PathBuf>) -> Self {
        Self::Recovering { recovered_from }
    }

    /// Finalize recovery: remains modified/dirty until an explicit save occurs.
    pub fn finish_recovery(&mut self) -> Result<(), LifecycleError> {
        match self.clone() {
            Self::Recovering { recovered_from } => {
                if let Some(path) = recovered_from {
                    *self = Self::PathBackedModified(path);
                } else {
                    *self = Self::UntitledModified;
                }
                Ok(())
            }
            _ => Err(LifecycleError::InvalidTransition(
                "not in recovering state".into(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untitled_document_clean_and_modified() {
        let mut doc = DocumentLifecycle::new_untitled();
        assert!(!doc.is_dirty());
        assert_eq!(doc.path(), None);

        doc.mark_modified().expect("modify");
        assert!(doc.is_dirty());
        assert_eq!(doc.path(), None);
    }

    #[test]
    fn path_backed_document_transitions() {
        let mut doc = DocumentLifecycle::new_path_backed("test.loomdoc");
        assert!(!doc.is_dirty());
        assert_eq!(doc.path(), Some(Path::new("test.loomdoc")));

        doc.mark_modified().expect("modify");
        assert!(doc.is_dirty());
        assert_eq!(doc.path(), Some(Path::new("test.loomdoc")));
    }

    #[test]
    fn cancelled_open_leaves_old_document_untouched() {
        let mut doc = DocumentLifecycle::new_path_backed("original.loomdoc");
        doc.mark_modified().expect("modify");
        assert!(doc.is_dirty());

        // User starts opening a new file
        doc.start_open("new.loomdoc").expect("start open");
        assert!(doc.is_dirty());

        // User cancels file dialog / load
        let result = doc.finish_open(false).expect("finish open");
        assert_eq!(result, None);

        // Original document must be completely untouched and still modified!
        assert_eq!(doc.path(), Some(Path::new("original.loomdoc")));
        assert!(doc.is_dirty());
        assert_eq!(
            doc,
            DocumentLifecycle::PathBackedModified(PathBuf::from("original.loomdoc"))
        );
    }

    #[test]
    fn failed_save_preserves_dirty_state() {
        let mut doc = DocumentLifecycle::new_path_backed("document.loomdoc");
        doc.mark_modified().expect("modify");
        assert!(doc.is_dirty());

        // Start save
        let target = doc.start_save(None).expect("start save");
        assert_eq!(target, PathBuf::from("document.loomdoc"));

        // Save fails (e.g. disk full)
        doc.finish_save(false).expect("finish save");

        // Document MUST preserve dirty state
        assert!(doc.is_dirty());
        assert_eq!(doc.path(), Some(Path::new("document.loomdoc")));
        assert_eq!(
            doc,
            DocumentLifecycle::PathBackedModified(PathBuf::from("document.loomdoc"))
        );
    }

    #[test]
    fn save_as_changes_path_only_after_success() {
        let mut doc = DocumentLifecycle::new_path_backed("v1.loomdoc");
        doc.mark_modified().expect("modify");

        // Save As to v2 fails
        doc.start_save(Some(PathBuf::from("v2.loomdoc")))
            .expect("start save");
        doc.finish_save(false).expect("finish save fail");

        // Path MUST still be v1
        assert_eq!(doc.path(), Some(Path::new("v1.loomdoc")));
        assert!(doc.is_dirty());

        // Save As to v2 succeeds
        doc.start_save(Some(PathBuf::from("v2.loomdoc")))
            .expect("start save");
        doc.finish_save(true).expect("finish save success");

        // Path is now v2 and clean
        assert_eq!(doc.path(), Some(Path::new("v2.loomdoc")));
        assert!(!doc.is_dirty());
        assert_eq!(
            doc,
            DocumentLifecycle::PathBackedClean(PathBuf::from("v2.loomdoc"))
        );
    }

    #[test]
    fn recovery_remains_dirty_until_explicitly_saved() {
        let mut doc = DocumentLifecycle::start_recovery(Some(PathBuf::from("crashed.loomdoc")));
        assert!(doc.is_dirty());

        doc.finish_recovery().expect("finish recovery");
        assert!(doc.is_dirty());
        assert_eq!(doc.path(), Some(Path::new("crashed.loomdoc")));

        // Only after an explicit successful save does it become clean
        doc.start_save(None).expect("start save");
        doc.finish_save(true).expect("finish save");
        assert!(!doc.is_dirty());
    }

    #[test]
    fn read_only_cannot_be_modified() {
        let mut doc = DocumentLifecycle::ReadOnly(PathBuf::from("readonly.loomdoc"));
        assert!(!doc.is_dirty());
        let res = doc.mark_modified();
        assert!(matches!(res, Err(LifecycleError::ReadOnlyModification(_))));
    }
}
