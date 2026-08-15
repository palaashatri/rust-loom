//! `loom-storage` provides crash-resilient, transactional local storage primitives:
//! atomic file writes via unique temporary files, directory syncing, timestamp-aware
//! autosave management, failure injection, and append-only recovery journaling with
//! torn-tail self-healing and interior corruption detection.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use loom_package::zip::sha256;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static TMP_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Result shorthand for storage operations.
pub type Result<T> = std::result::Result<T, StorageError>;

/// Storage error variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// Filesystem I/O failure.
    Io(String),
    /// Target path is unsafe or disallowed.
    UnsafePath(String),
    /// Required file path is missing.
    NoPath,
    /// Data integrity corruption detected in storage/journal.
    Corruption(String),
    /// Injected failure for testing.
    InjectedFailure(&'static str),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "io error: {e}"),
            Self::UnsafePath(p) => write!(f, "unsafe path: {p}"),
            Self::NoPath => write!(f, "no path specified"),
            Self::Corruption(c) => write!(f, "data corruption: {c}"),
            Self::InjectedFailure(fp) => write!(f, "injected failure at: {fp}"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

/// Check if a path is absolute.
pub fn is_absolute_path(p: &Path) -> bool {
    p.is_absolute()
}

/// Path safety check ensuring relative path does not escape sandbox via `..`.
pub fn is_safe_storage_path(p: &Path) -> bool {
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

/// Failure injection point for testing persistence boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailPoint {
    /// Fail before temporary file creation.
    BeforeTempCreate,
    /// Fail during data write to temporary file.
    DuringWrite,
    /// Fail before syncing temporary file to disk.
    BeforeSync,
    /// Fail before atomic rename/replace.
    BeforeRename,
}

thread_local! {
    static FAIL_POINT: std::cell::Cell<Option<FailPoint>> = const { std::cell::Cell::new(None) };
}

/// Set an active failure point for deterministic testing on the current thread.
pub fn set_fail_point(fp: Option<FailPoint>) {
    FAIL_POINT.with(|cell| cell.set(fp));
}

fn check_fail_point(fp: FailPoint) -> Result<()> {
    let active = FAIL_POINT.with(|cell| cell.get());
    if active == Some(fp) {
        return Err(StorageError::InjectedFailure(match fp {
            FailPoint::BeforeTempCreate => "BeforeTempCreate",
            FailPoint::DuringWrite => "DuringWrite",
            FailPoint::BeforeSync => "BeforeSync",
            FailPoint::BeforeRename => "BeforeRename",
        }));
    }
    Ok(())
}

/// Atomically write bytes to `path` with durable fsync and unique temporary files:
/// 1. Creates a unique temporary file in the destination's parent directory.
/// 2. Writes data and calls `sync_all()`.
/// 3. Closes temporary file handle.
/// 4. Replaces target file via atomic rename.
/// 5. Durably syncs parent directory on supported platforms.
pub fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| StorageError::UnsafePath(path.display().to_string()))?;
    fs::create_dir_all(parent)?;

    check_fail_point(FailPoint::BeforeTempCreate)?;

    let file_name = path
        .file_name()
        .ok_or_else(|| StorageError::UnsafePath(path.display().to_string()))?
        .to_string_lossy();

    let count = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tmp_name = format!(".{file_name}.{pid}.{count}.{now}.loom-tmp");
    let tmp_path = parent.join(tmp_name);

    {
        let mut file = File::create(&tmp_path)?;
        check_fail_point(FailPoint::DuringWrite)?;
        file.write_all(data)?;
        file.flush()?;
        check_fail_point(FailPoint::BeforeSync)?;
        file.sync_all()?;
    }

    check_fail_point(FailPoint::BeforeRename)?;

    // On Windows, rename over an existing file may fail unless replaced
    #[cfg(windows)]
    {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }

    fs::rename(&tmp_path, path)?;

    // Durably sync directory on Unix
    #[cfg(unix)]
    {
        let _ = File::open(parent).and_then(|d| d.sync_all());
    }

    Ok(())
}

/// Read a file completely into memory.
pub fn read_file(path: &Path) -> Result<Vec<u8>> {
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(buf)
}

/// A single recovery journal entry with sequence and checksum.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// Monotonically increasing sequence number.
    pub seq: u64,
    /// Operation name or identifier.
    pub op: String,
    /// Operation payload bytes.
    pub payload: Vec<u8>,
    /// SHA-256 digest of payload.
    pub checksum: [u8; 32],
}

/// Append-only recovery journal with torn-tail self healing and durable fsync.
#[derive(Debug)]
pub struct RecoveryJournal {
    path: PathBuf,
    entries: Vec<JournalEntry>,
    next_seq: u64,
}

impl RecoveryJournal {
    /// Open or create a recovery journal at `path`.
    /// Automatically detects and heals torn-tail appends caused by prior process termination.
    /// Interior corruption causes an explicit `StorageError::Corruption`.
    pub fn open(path: &Path) -> Result<Self> {
        let mut entries = Vec::new();
        if path.exists() {
            let data = read_file(path)?;
            let parse_result = parse_journal_with_healing(&data, &mut entries)?;
            if parse_result.repaired_bytes < data.len() {
                // Self-heal: rewrite the verified prefix without the torn tail
                let verified_data = &data[..parse_result.repaired_bytes];
                atomic_write(path, verified_data)?;
            }
        }
        let next_seq = entries.last().map(|e| e.seq + 1).unwrap_or(1);
        Ok(Self {
            path: path.to_path_buf(),
            entries,
            next_seq,
        })
    }

    /// Append an operation entry and durably sync it to disk.
    pub fn append(&mut self, op: impl Into<String>, payload: Vec<u8>) -> Result<JournalEntry> {
        let checksum = sha256(&payload);
        let e = JournalEntry {
            seq: self.next_seq,
            op: op.into(),
            payload,
            checksum,
        };
        self.next_seq += 1;

        let mut encoded = Vec::new();
        encode_entry(&e, &mut encoded);

        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        file.write_all(&encoded)?;
        file.flush()?;
        file.sync_all()?;

        self.entries.push(e.clone());
        Ok(e)
    }

    /// Return all verified journal entries.
    pub fn entries(&self) -> &[JournalEntry] {
        &self.entries
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether journal is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Durably clear the journal file and reset entries.
    pub fn clear(&mut self) -> Result<()> {
        atomic_write(&self.path, b"")?;
        self.entries.clear();
        self.next_seq = 1;
        Ok(())
    }

    /// Return the path of the journal file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn encode_entry(e: &JournalEntry, out: &mut Vec<u8>) {
    // Magic header (4 bytes: 'L', 'O', 'O', 'M')
    out.extend_from_slice(b"LOOM");
    // Sequence number (u64 LE)
    out.extend_from_slice(&e.seq.to_le_bytes());
    // Op length (u32 LE) + op bytes
    out.extend_from_slice(&(e.op.len() as u32).to_le_bytes());
    out.extend_from_slice(e.op.as_bytes());
    // Payload length (u32 LE) + payload bytes
    out.extend_from_slice(&(e.payload.len() as u32).to_le_bytes());
    out.extend_from_slice(&e.payload);
    // SHA-256 Checksum (32 bytes)
    out.extend_from_slice(&e.checksum);
}

struct JournalParseResult {
    repaired_bytes: usize,
}

fn parse_journal_with_healing(
    data: &[u8],
    out: &mut Vec<JournalEntry>,
) -> Result<JournalParseResult> {
    let mut pos = 0usize;
    let mut last_valid_pos = 0usize;
    let mut expected_seq = 1u64;

    while pos < data.len() {
        let entry_start = pos;

        // Check magic header
        if data.len() - pos < 4 {
            // Torn tail at magic header
            return Ok(JournalParseResult {
                repaired_bytes: last_valid_pos,
            });
        }
        if &data[pos..pos + 4] != b"LOOM" {
            // Interior corruption
            return Err(StorageError::Corruption(format!(
                "invalid journal entry magic at byte offset {pos}"
            )));
        }
        pos += 4;

        // Sequence number
        if data.len() - pos < 8 {
            return Ok(JournalParseResult {
                repaired_bytes: last_valid_pos,
            });
        }
        let seq = u64::from_le_bytes(data[pos..pos + 8].try_into().unwrap());
        pos += 8;

        if seq != expected_seq {
            return Err(StorageError::Corruption(format!(
                "non-sequential sequence number {seq} at offset {entry_start}, expected {expected_seq}"
            )));
        }

        // Op length and string
        if data.len() - pos < 4 {
            return Ok(JournalParseResult {
                repaired_bytes: last_valid_pos,
            });
        }
        let op_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if data.len() - pos < op_len {
            return Ok(JournalParseResult {
                repaired_bytes: last_valid_pos,
            });
        }
        let op = match std::str::from_utf8(&data[pos..pos + op_len]) {
            Ok(s) => s.to_string(),
            Err(_) => {
                return Err(StorageError::Corruption(format!(
                    "non-UTF8 op name at offset {entry_start}"
                )));
            }
        };
        pos += op_len;

        // Payload length and data
        if data.len() - pos < 4 {
            return Ok(JournalParseResult {
                repaired_bytes: last_valid_pos,
            });
        }
        let payload_len = u32::from_le_bytes(data[pos..pos + 4].try_into().unwrap()) as usize;
        pos += 4;

        if data.len() - pos < payload_len {
            return Ok(JournalParseResult {
                repaired_bytes: last_valid_pos,
            });
        }
        let payload = data[pos..pos + payload_len].to_vec();
        pos += payload_len;

        // Checksum
        if data.len() - pos < 32 {
            return Ok(JournalParseResult {
                repaired_bytes: last_valid_pos,
            });
        }
        let checksum: [u8; 32] = data[pos..pos + 32].try_into().unwrap();
        pos += 32;

        let expected_checksum = sha256(&payload);
        if checksum != expected_checksum {
            return Err(StorageError::Corruption(format!(
                "checksum mismatch for entry sequence {seq} at offset {entry_start}"
            )));
        }

        out.push(JournalEntry {
            seq,
            op,
            payload,
            checksum,
        });

        expected_seq += 1;
        last_valid_pos = pos;
    }

    Ok(JournalParseResult {
        repaired_bytes: last_valid_pos,
    })
}

/// A managed autosave slot.
#[derive(Debug, Clone)]
pub struct AutosaveSlot {
    /// Full path to the autosave file.
    pub path: PathBuf,
    /// SHA-256 of the last written content.
    pub sha: [u8; 32],
}

/// Manages periodic autosaves for documents.
#[derive(Debug)]
pub struct Autosave {
    dir: PathBuf,
    last: Option<AutosaveSlot>,
}

impl Autosave {
    /// Create an autosave manager rooted in `dir`.
    pub fn in_dir(dir: &Path) -> Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
            last: None,
        })
    }

    /// Save a snapshot atomically.
    pub fn save_snapshot(&mut self, name: &str, data: &[u8]) -> Result<AutosaveSlot> {
        let count = TMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let path = self
            .dir
            .join(format!("{name}-{timestamp}-{count}.autosave"));
        atomic_write(&path, data)?;
        let slot = AutosaveSlot {
            path,
            sha: sha256(data),
        };
        self.last = Some(slot.clone());
        Ok(slot)
    }

    /// Return the most recent autosave slot.
    pub fn last(&self) -> Option<&AutosaveSlot> {
        self.last.as_ref()
    }

    /// Verify that the last autosave file exists and its checksum matches.
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

    /// Prune older autosaves based on actual file modification time, keeping `keep` newest files.
    pub fn prune_keep(&mut self, keep: usize) -> Result<()> {
        let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
        if self.dir.exists() {
            for entry in fs::read_dir(&self.dir)? {
                let entry = entry?;
                let path = entry.path();
                if path
                    .extension()
                    .map(|ext| ext == "autosave")
                    .unwrap_or(false)
                {
                    let mtime = entry
                        .metadata()
                        .and_then(|m| m.modified())
                        .unwrap_or(UNIX_EPOCH);
                    files.push((mtime, path));
                }
            }
        }

        // Sort by actual modification time (newest first)
        files.sort_by_key(|a| std::cmp::Reverse(a.0));

        if files.len() > keep {
            for (_, path) in &files[keep..] {
                let _ = fs::remove_file(path);
            }
        }

        Ok(())
    }
}

/// Compute SHA-256 hash of a file.
pub fn file_sha256(path: &Path) -> Result<[u8; 32]> {
    let data = read_file(path)?;
    Ok(sha256(&data))
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
    fn atomic_write_roundtrip_and_no_tmp_leak() {
        let dir = temp_dir("atomic");
        let p = dir.join("doc.loomdoc");
        atomic_write(&p, b"hello").unwrap();
        assert_eq!(read_file(&p).unwrap(), b"hello");
        atomic_write(&p, b"world").unwrap();
        assert_eq!(read_file(&p).unwrap(), b"world");

        let left: Vec<_> = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(left.is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn prior_valid_destination_survives_interrupted_write() {
        let dir = temp_dir("failpoint");
        let path = dir.join("important.loomdoc");
        atomic_write(&path, b"original-good-data").unwrap();

        for fp in [
            FailPoint::BeforeTempCreate,
            FailPoint::DuringWrite,
            FailPoint::BeforeSync,
            FailPoint::BeforeRename,
        ] {
            set_fail_point(Some(fp));
            let res = atomic_write(&path, b"corrupted-overwrite");
            assert!(res.is_err(), "failpoint {:?} should cause error", fp);
            // Prior valid file must be completely intact
            assert_eq!(read_file(&path).unwrap(), b"original-good-data");
        }

        set_fail_point(None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn journal_append_reopen_and_checksum() {
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
    fn journal_torn_tail_self_heals() {
        let dir = temp_dir("torn-tail");
        let p = dir.join("recovery.journal");
        {
            let mut j = RecoveryJournal::open(&p).unwrap();
            j.append("op.1", vec![10, 20]).unwrap();
            j.append("op.2", vec![30, 40]).unwrap();
        }

        // Simulate crash during append: append half a record to the end of the file
        let mut file = OpenOptions::new().append(true).open(&p).unwrap();
        file.write_all(b"LOOM\x03\x00\x00\x00\x00\x00\x00\x00partial")
            .unwrap();
        drop(file);

        // Open should heal torn tail, keeping the two verified records
        let mut j2 = RecoveryJournal::open(&p).unwrap();
        assert_eq!(j2.len(), 2);
        assert_eq!(j2.entries()[0].op, "op.1");
        assert_eq!(j2.entries()[1].op, "op.2");

        // Appending a new record works cleanly with next sequence number 3
        j2.append("op.3", vec![50]).unwrap();
        assert_eq!(j2.entries()[2].seq, 3);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn journal_interior_corruption_fails_loudly() {
        let dir = temp_dir("corrupt-interior");
        let p = dir.join("recovery.journal");
        {
            let mut j = RecoveryJournal::open(&p).unwrap();
            j.append("op.1", vec![10, 20]).unwrap();
            j.append("op.2", vec![30, 40]).unwrap();
            j.append("op.3", vec![50, 60]).unwrap();
        }

        // Corrupt a byte in the interior record payload
        let mut data = fs::read(&p).unwrap();
        data[25] ^= 0xff; // Flip bits in interior record
        fs::write(&p, data).unwrap();

        let result = RecoveryJournal::open(&p);
        assert!(matches!(result, Err(StorageError::Corruption(_))));

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
    fn autosave_recency_pruning() {
        let dir = temp_dir("autosave-prune");
        let mut a = Autosave::in_dir(&dir).unwrap();
        for i in 0..5 {
            a.save_snapshot(&format!("doc-{i}"), &[i]).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }

        a.prune_keep(2).unwrap();
        let remaining = fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|s| s == "autosave")
                    .unwrap_or(false)
            })
            .count();
        assert_eq!(remaining, 2);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn safe_path_validation() {
        assert!(is_safe_storage_path(Path::new("doc/contents.x")));
        assert!(!is_safe_storage_path(Path::new("../secret.x")));
        assert!(is_absolute_path(Path::new("/etc/passwd")));
        assert!(!is_absolute_path(Path::new("relative")));
    }
}
