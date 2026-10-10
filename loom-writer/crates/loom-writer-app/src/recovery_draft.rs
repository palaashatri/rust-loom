//! A recovery draft: a Writer document plus the saved file it was edited from.
//!
//! The live recovery slot holds one draft at a time. Drafts that a later launch
//! displaces (another file was opened) are kept in a quarantine beside the slot,
//! one recovery store per draft, so they are never overwritten or deleted without
//! an explicit discard.

use loom_production::snapshot::SnapshotRecovery;
use loom_writer_core::WriterDocument;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Application id of the Writer recovery store. The macro in `recovery.rs`
/// takes the same literal.
const APPLICATION_ID: &str = "org.loom.writer";

/// Schema of the native package stored in a draft. Matches `recovery.rs`.
const PACKAGE_SCHEMA: &str = "loom.writer.package/1";

/// First line of a draft that records its file. A payload without it is a bare
/// package written before drafts recorded their paths.
const DRAFT_HEADER: &[u8] = b"LOOM-WRITER-DRAFT/1\n";

/// A recovered draft and the saved file it was being edited from, if any.
#[derive(Clone)]
pub(crate) struct RecoveredDraft {
    pub(crate) document: WriterDocument,
    pub(crate) source: Option<PathBuf>,
}

impl RecoveredDraft {
    /// Decode a stored payload. A payload that cannot be read yields `None`.
    pub(crate) fn from_payload(payload: &[u8]) -> Option<Self> {
        let (source, package) = decode(payload)?;
        let document = loom_writer_core::load_document(package).ok()?;
        Some(Self { document, source })
    }

    /// Whether this draft belongs to the launch: any draft on a plain launch, and
    /// only a draft of the same file when a file is opened.
    pub(crate) fn belongs_to(&self, open: Option<&Path>) -> bool {
        match open {
            None => true,
            Some(path) => self
                .source
                .as_deref()
                .is_some_and(|source| same_file(source, path)),
        }
    }

    /// The name the window uses for this draft.
    pub(crate) fn name(&self) -> String {
        crate::file_title::display_title(self.source.as_deref(), &self.document.title)
    }
}

/// Wrap a package with the file it belongs to. A path that is not valid UTF-8
/// is recorded without a file: the draft still restores, it just has no name.
pub(crate) fn encode(source: Option<&Path>, package: &[u8]) -> Vec<u8> {
    let path = source.and_then(Path::to_str).unwrap_or("");
    let mut payload = Vec::with_capacity(DRAFT_HEADER.len() + 24 + path.len() + package.len());
    payload.extend_from_slice(DRAFT_HEADER);
    payload.extend_from_slice(format!("{}\n", path.len()).as_bytes());
    payload.extend_from_slice(path.as_bytes());
    payload.extend_from_slice(package);
    payload
}

/// Split a stored payload into its source path and its package bytes.
pub(crate) fn decode(payload: &[u8]) -> Option<(Option<PathBuf>, &[u8])> {
    let Some(rest) = payload.strip_prefix(DRAFT_HEADER) else {
        return Some((None, payload));
    };
    let newline = rest.iter().position(|byte| *byte == b'\n')?;
    let length: usize = std::str::from_utf8(&rest[..newline]).ok()?.parse().ok()?;
    let body = &rest[newline + 1..];
    if body.len() < length {
        return None;
    }
    let (path, package) = body.split_at(length);
    let source = if length == 0 {
        None
    } else {
        Some(PathBuf::from(std::str::from_utf8(path).ok()?))
    };
    Some((source, package))
}

/// Whether two paths name the same file, allowing for links and spelling.
pub(crate) fn same_file(left: &Path, right: &Path) -> bool {
    let canonical =
        |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    left == right || canonical(left) == canonical(right)
}

/// Directory of kept drafts. It is a sibling of the live slot's directory, so
/// clearing the slot cannot touch a kept draft.
pub(crate) fn quarantine_root() -> Result<PathBuf, String> {
    let state = loom_production::snapshot::application_state_directory(APPLICATION_ID)
        .map_err(|error| error.to_string())?;
    Ok(state.with_file_name(format!("{APPLICATION_ID}-quarantine")))
}

/// Entries of the kept-draft directory, newest first. Unreadable entries are
/// still listed, so they are never removed by accident.
pub(crate) fn kept_entries(root: &Path) -> Vec<PathBuf> {
    let Ok(listing) = std::fs::read_dir(root) else {
        return Vec::new();
    };
    let mut entries: Vec<PathBuf> = listing
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    entries.sort_by(|left, right| right.file_name().cmp(&left.file_name()));
    entries
}

/// The payload one kept entry holds, if it holds one.
pub(crate) fn read_kept(entry: &Path) -> Result<Option<Vec<u8>>, String> {
    let mut store = SnapshotRecovery::open_at(entry).map_err(|error| error.to_string())?;
    Ok(store.take_restored_payload())
}

/// Keep a payload that a launch displaced. The copy is durable when this returns;
/// an identical copy that is already kept is not written twice.
pub(crate) fn keep(root: &Path, payload: &[u8]) -> Result<(), String> {
    for entry in kept_entries(root) {
        if read_kept(&entry).ok().flatten().as_deref() == Some(payload) {
            return Ok(());
        }
    }
    std::fs::create_dir_all(root).map_err(|error| format!("create {}: {error}", root.display()))?;
    let entry = new_entry(root)?;
    let mut store = SnapshotRecovery::open_at(&entry).map_err(|error| error.to_string())?;
    store
        .checkpoint(PACKAGE_SCHEMA, payload.to_vec())
        .map_err(|error| error.to_string())
}

/// Remove one kept entry: its draft is live again, or the user discarded it.
pub(crate) fn remove_kept(entry: &Path) -> Result<(), String> {
    let store = SnapshotRecovery::open_at(entry).map_err(|error| error.to_string())?;
    store.clear().map_err(|error| error.to_string())?;
    let _ = std::fs::remove_dir_all(entry);
    Ok(())
}

/// Create a new, empty entry. The name sorts by creation time.
fn new_entry(root: &Path) -> Result<PathBuf, String> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis());
    for _ in 0..1000 {
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let entry = root.join(format!("{millis:020}-{sequence:08}"));
        match std::fs::create_dir(&entry) {
            Ok(()) => return Ok(entry),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(format!("create {}: {error}", entry.display())),
        }
    }
    Err("could not name a new kept draft".into())
}
