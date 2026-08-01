//! `loom-storage` provides transactional, safe local storage primitives:
//! atomic file writes via temporary files, autosave management, and crash
//! recovery journaling. Everything operates purely on the filesystem with no
//! network dependency.

use loom_package::zip::sha256;
use std::fmt;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Result shorthand.
pub type Result<T> = std::result::Result<T, StorageError>;

/// Storage errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// IO failure.
    Io(String),
    /// Path is not allowed for storage.
    UnsafePath(String),
    /// A write was attempted without a path (e.g., Autosave for unsaved doc).
    NoPath,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io: {e}"),
            Self::UnsafePath(p) => write!(f, "unsafe path: {p}"),
            Self::NoPath => write!(f, "no path"),
        }
    }
}

impl core::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Determine whether a path is an absolute personal path that shouldn't leak.
pub fn is_absolute_path(p: &Path) -> bool {
    p.is_absolute()
}

/// Atomically write bytes to `path`:
/// writes to a temp file in the same directory, fsyncs, then renames it.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| StorageError::UnsafePath(path.display().to_string()))?;
    fs::create_dir_all(dir)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| StorageError::UnsafePath(path.display().to_string()))?;
    let mut tmp_name = PathBuf::from(&file_name);
    tmp_name.as_mut_os_string().push(".loom-tmp");
    let tmp_path = dir.join(tmp_name);
    {
        let mut f = fs::File::create(&tmp_path)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp_path, path)?;
    // Best-effort sync of the directory.
    let _ = fs::File::open(dir).and_then(|d| d.sync_all());
    Ok(())
}

/// Read a file fully.
pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let mut f = fs::File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

/// A simple recovery journal entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// Sequence number.
    pub seq: u64,
    /// Command id / operation name.
    pub op: String,
    /// Payload (document edit bytes).
    pub payload: Vec<u8>,
}

/// An append-only recovery journal stored on disk.
#[derive(Debug)]
pub struct RecoveryJournal {
    path: PathBuf,
    entries: Vec<JournalEntry>,
    next_seq: u64,
}

impl RecoveryJournal {
    /// Open (or create) a journal at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        let mut entries = Vec::new();
        if path.exists() {
            let data = read_file(path)?;
            // Format: length-prefixed entries for simplicity and safety.
            parse_journal(&data, &mut entries)?;
        }
        let next_seq = entries.last().map(|e| e.seq + 1).unwrap_or(1);
        Ok(Self {
            path: path.to_path_buf(),
            entries,
            next_seq,
        })
    }

    /// Append and persist an entry.
    pub fn append(&mut self, op: impl Into<String>, payload: Vec<u8>) -> Result<JournalEntry> {
        let e = JournalEntry {
            seq: self.next_seq,
            op: op.into(),
            payload,
        };
        self.next_seq += 1;
        // Append to the file (not atomic but journaling is append-only).
        let mut encoded = Vec::new();
        encode_entry(&e, &mut encoded);
        let mut f = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        f.write_all(&encoded)?;
        self.entries.push(e.clone());
        Ok(e)
    }

    /// Entries in order.
    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clear the journal file and its in-memory entries.
    pub fn clear(&mut self) -> Result<()> {
        fs::write(&self.path, b"")?;
        self.entries.clear();
        self.next_seq = 1;
        Ok(())
    }

    /// Path used.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn encode_entry(e: &JournalEntry, out: &mut Vec<u8>) {
    // Write op length (u32 LE), op bytes, payload length (u32 LE), payload.
    out.extend_from_slice(&(e.op.len() as u32).to_le_bytes());
    out.extend_from_slice(e.op.as_bytes());
    out.extend_from_slice(&(e.payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&e.payload);
}

fn parse_journal(data: &[u8], out: &mut Vec<JournalEntry>) -> Result<()> {
    let mut pos = 0usize;
    let mut seq = 1u64;
    while pos < data.len() {
        if data.len() - pos < 4 {
            // Trailing partial record: treat as corruption and stop.
            break;
        }
        let op_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + op_len > data.len() {
            break;
        }
        let op = std::str::from_utf8(&data[pos..pos + op_len])
            .map_err(|_| StorageError::Io("invalid journal op".into()))?
            .to_string();
        pos += op_len;
        if data.len() - pos < 4 {
            break;
        }
        let payload_len =
            u32::from_le_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]]) as usize;
        pos += 4;
        if pos + payload_len > data.len() {
            break;
        }
        let payload = data[pos..pos + payload_len].to_vec();
        pos += payload_len;
        out.push(JournalEntry { seq, op, payload });
        seq += 1;
    }
    Ok(())
}

/// A managed autosave slot.
#[derive(Debug, Clone)]
pub struct AutosaveSlot {
    /// Full path to the autosave file.
    pub path: PathBuf,
    /// SHA-256 of the last written content.
    pub sha: [u8; 32],
}

/// Manages periodic autosave for a single document.
#[derive(Debug)]
pub struct Autosave {
    /// Directory holding autosaves.
    dir: PathBuf,
    /// Last written slot.
    last: Option<AutosaveSlot>,
}

impl Autosave {
    /// Create an autosave manager rooted at `dir`.
    pub fn in_dir(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            last: None,
        })
    }

    /// Save a snapshot (used for periodic autosave or explicit save).
    pub fn save_snapshot(&mut self, name: &str, data: &[u8]) -> Result<AutosaveSlot> {
        let path = self.dir.join(format!("{name}.autosave"));
        atomic_write(&path, data)?;
        let slot = AutosaveSlot {
            path,
            sha: sha256(data),
        };
        self.last = Some(slot.clone());
        Ok(slot)
    }

    /// Most recent slot.
    pub fn last(&self) -> Option<&AutosaveSlot> {
        self.last.as_ref()
    }

    /// Verify the last autosave is intact.
    pub fn verify_last(&self) -> bool {
        if let Some(slot) = &self.last {
            match read_file(&slot.path) {
                Ok(data) => sha256(&data) == slot.sha,
                Err(_) => false,
            }
        } else {
            false
        }
    }

    /// Remove old autosaves beyond a count to bound disk use.
    pub fn prune_keep(&mut self, keep: usize) -> Result<()> {
        let mut paths: Vec<PathBuf> = Vec::new();
        for e in fs::read_dir(&self.dir)? {
            let e = e?;
            if e.path()
                .extension()
                .map(|s| s == "autosave")
                .unwrap_or(false)
            {
                paths.push(e.path());
            }
        }
        paths.sort();
        let remove = paths.len().saturating_sub(keep);
        for p in paths.into_iter().take(remove) {
            let _ = fs::remove_file(p);
        }
        Ok(())
    }
}

/// Compute a SHA-256 based fingerprint of a file (for dedup / integrity).
pub fn file_sha256(path: &Path) -> Result<[u8; 32]> {
    let data = read_file(path)?;
    Ok(sha256(&data))
}

/// Storage path validator: only relative or known-rooted paths allowed.
pub fn is_safe_storage_path(p: &Path) -> bool {
    // Reject absolute and any path starting with `..`.
    if p.is_absolute() {
        return false;
    }
    for comp in p.components() {
        if let std::path::Component::ParentDir = comp {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("loom-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn atomic_write_roundtrip() {
        let dir = temp_dir("atomic");
        let p = dir.join("doc.loomdoc");
        atomic_write(&p, b"hello").unwrap();
        assert_eq!(read_file(&p).unwrap(), b"hello");
        atomic_write(&p, b"world").unwrap();
        assert_eq!(read_file(&p).unwrap(), b"world");
        // No temp files left behind. Compare the file name, not the full
        // path: on Linux the temp dir itself is /tmp and would match.
        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(left.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn journal_append_reopen() {
        let dir = temp_dir("journal");
        let p = dir.join("recovery.journal");
        {
            let mut j = RecoveryJournal::open(&p).unwrap();
            j.append("edit.bold", vec![1, 2, 3]).unwrap();
            j.append("edit.italic", vec![4]).unwrap();
        }
        let j2 = RecoveryJournal::open(&p).unwrap();
        assert_eq!(j2.len(), 2);
        assert_eq!(j2.entries()[0].op, "edit.bold");
        assert_eq!(j2.entries()[0].payload, vec![1, 2, 3]);
        assert_eq!(j2.entries()[1].seq, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn journal_clear() {
        let dir = temp_dir("journal-clear");
        let p = dir.join("recovery.journal");
        let mut j = RecoveryJournal::open(&p).unwrap();
        j.append("a", vec![]).unwrap();
        assert_eq!(j.len(), 1);
        j.clear().unwrap();
        assert!(j.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_snapshot_and_verify() {
        let dir = temp_dir("autosave");
        let mut a = Autosave::in_dir(&dir).unwrap();
        let slot = a.save_snapshot("my-doc", b"data-v1").unwrap();
        assert!(slot.path.exists());
        assert!(a.verify_last());
        // Corrupt the file.
        fs::write(&slot.path, b"corrupted!").unwrap();
        assert!(!a.verify_last());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_prune() {
        let dir = temp_dir("prune");
        let mut a = Autosave::in_dir(&dir).unwrap();
        for i in 0..5 {
            a.save_snapshot(&format!("doc-{i}"), &[i]).unwrap();
        }
        a.prune_keep(2).unwrap();
        let count = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|s| s == "autosave")
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(count, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_path_validation() {
        assert!(is_safe_storage_path(Path::new("doc/contents.x")));
        assert!(!is_safe_storage_path(Path::new("../secret.x")));
        assert!(is_absolute_path(Path::new("/etc/passwd")));
        assert!(!is_absolute_path(Path::new("relative")));
    }

    #[test]
    fn file_sha_consistent() {
        let dir = temp_dir("sha");
        let p = dir.join("f.bin");
        fs::write(&p, b"abc").unwrap();
        let h = file_sha256(&p).unwrap();
        assert_eq!(
            h.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
