use super::checkpoint_generations::{
    CHECKPOINT_COMMIT_PREFIX, CHECKPOINT_COMMIT_SUFFIX, CHECKPOINT_GENERATION_METADATA_SUFFIX,
    CHECKPOINT_GENERATION_PAYLOAD_SUFFIX, CHECKPOINT_GENERATION_PREFIX,
};
use super::{
    read_checkpoint_state, CheckpointMetadata, JournalRecord, ProductionError, CHECKPOINT_FILE,
    CHECKPOINT_META_FILE, JOURNAL_FILE,
};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::Path;

const DEFAULT_MAX_CHECKPOINT_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_MAX_CHECKPOINT_METADATA_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_DIRECTORY_ENTRIES: usize = 10_000;
const DEFAULT_MAX_JOURNAL_BYTES: u64 = 64 * 1024 * 1024;
const DEFAULT_MAX_RECORDS: usize = 10_000;
const DEFAULT_MAX_RECORD_LINE_BYTES: usize = 1024 * 1024;

/// Caller-controlled ceilings for a read-only recovery inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryInspectionLimits {
    /// Maximum logical size of any checkpoint package or metadata file read.
    pub max_checkpoint_bytes: u64,
    /// Maximum aggregate logical size of checkpoint metadata candidate files.
    pub max_checkpoint_metadata_bytes: u64,
    /// Maximum number of direct entries in the recovery directory.
    pub max_directory_entries: usize,
    /// Maximum logical size of the complete journal file, including a torn tail.
    pub max_journal_bytes: u64,
    /// Maximum number of complete, verified records returned.
    pub max_records: usize,
    /// Maximum byte length of a verified JSONL record, including its newline.
    pub max_record_line_bytes: usize,
}

impl Default for RecoveryInspectionLimits {
    fn default() -> Self {
        Self {
            max_checkpoint_bytes: DEFAULT_MAX_CHECKPOINT_BYTES,
            max_checkpoint_metadata_bytes: DEFAULT_MAX_CHECKPOINT_METADATA_BYTES,
            max_directory_entries: DEFAULT_MAX_DIRECTORY_ENTRIES,
            max_journal_bytes: DEFAULT_MAX_JOURNAL_BYTES,
            max_records: DEFAULT_MAX_RECORDS,
            max_record_line_bytes: DEFAULT_MAX_RECORD_LINE_BYTES,
        }
    }
}

/// Read-only snapshot of a selected checkpoint and the verified journal prefix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveryInspection {
    /// Selected checkpoint package bytes, when a checkpoint exists.
    pub checkpoint: Option<Vec<u8>>,
    /// Metadata for the selected checkpoint, when one exists.
    pub checkpoint_metadata: Option<CheckpointMetadata>,
    /// Complete verified records, including records already represented by the checkpoint.
    pub records: Vec<JournalRecord>,
    /// Logical length of the on-disk journal, including an incomplete tail.
    pub journal_bytes: u64,
    /// Number of complete verified records returned in `records`.
    pub complete_record_count: usize,
    /// Longest verified JSONL record length, including its newline.
    pub max_complete_line_bytes: usize,
    /// Whether a final incomplete or malformed record tail was observed.
    pub incomplete_tail: bool,
}

/// Exact journal statistics available after a cursor has reached EOF.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryInspectionStats {
    /// Logical length of the on-disk journal, including an incomplete tail.
    pub journal_bytes: u64,
    /// Number of complete verified records yielded by the cursor.
    pub complete_record_count: usize,
    /// Longest verified JSONL record length, including its newline.
    pub max_complete_line_bytes: usize,
    /// Whether a final incomplete or malformed record tail was observed.
    pub incomplete_tail: bool,
}

/// Read-only, bounded streaming access to one recovery directory.
///
/// The selected checkpoint is available immediately. Journal records are
/// verified and yielded one at a time, and exact journal statistics become
/// available only after `next_record` returns `None`. The caller must hold
/// [`super::RecoveryDirectoryLock`] for the full inspection when cooperating
/// readers and writers must observe a stable recovery directory. This type
/// does not create, repair, or prune recovery files.
pub struct RecoveryInspectionCursor {
    checkpoint: Option<Vec<u8>>,
    checkpoint_metadata: Option<CheckpointMetadata>,
    journal: Option<BufReader<File>>,
    limits: RecoveryInspectionLimits,
    line_buffer: Vec<u8>,
    journal_bytes: u64,
    complete_record_count: usize,
    max_complete_line_bytes: usize,
    previous_sequence: u64,
    line_number: usize,
    incomplete_tail: bool,
    stats: Option<RecoveryInspectionStats>,
    failed: bool,
}

impl RecoveryInspectionCursor {
    /// Open an existing recovery directory with the default hard read ceilings.
    pub fn open(directory: impl AsRef<Path>) -> Result<Self, ProductionError> {
        Self::open_with_limits(directory, RecoveryInspectionLimits::default())
    }

    /// Open an existing recovery directory with caller-supplied hard read
    /// ceilings. These ceilings are separate from any application's write
    /// policy, so callers may choose larger finite limits for safe replay of
    /// valid over-policy recovery data.
    pub fn open_with_limits(
        directory: impl AsRef<Path>,
        limits: RecoveryInspectionLimits,
    ) -> Result<Self, ProductionError> {
        let directory = directory.as_ref();
        match fs::symlink_metadata(directory) {
            Ok(metadata) if metadata.file_type().is_dir() => {}
            Ok(_) => {
                return Err(obstructed_path(
                    directory,
                    "recovery root is not a directory",
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(Self::empty(limits));
            }
            Err(error) => return Err(error.into()),
        }

        validate_existing_entries(directory, limits)?;
        let checkpoint = read_checkpoint_state(directory)?;
        let journal_path = directory.join(JOURNAL_FILE);
        let journal = open_optional_bounded_journal(&journal_path, limits.max_journal_bytes)?;

        Ok(Self {
            checkpoint: checkpoint.bytes,
            checkpoint_metadata: checkpoint.metadata,
            journal,
            limits,
            line_buffer: Vec::new(),
            journal_bytes: 0,
            complete_record_count: 0,
            max_complete_line_bytes: 0,
            previous_sequence: 0,
            line_number: 0,
            incomplete_tail: false,
            stats: None,
            failed: false,
        })
    }

    /// Selected checkpoint package bytes, when one exists.
    pub fn checkpoint(&self) -> Option<&[u8]> {
        self.checkpoint.as_deref()
    }

    /// Metadata for the selected checkpoint, when one exists.
    pub fn checkpoint_metadata(&self) -> Option<&CheckpointMetadata> {
        self.checkpoint_metadata.as_ref()
    }

    /// Exact journal statistics, available only after the cursor reaches EOF.
    pub fn stats(&self) -> Option<&RecoveryInspectionStats> {
        self.stats.as_ref()
    }

    /// Yield the next complete, integrity-checked journal record.
    pub fn next_record(&mut self) -> Result<Option<JournalRecord>, ProductionError> {
        if self.failed {
            return Err(ProductionError::InvalidData(
                "recovery inspection cursor stopped after an earlier error".into(),
            ));
        }
        let result = self.next_record_inner();
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    fn next_record_inner(&mut self) -> Result<Option<JournalRecord>, ProductionError> {
        if self.stats.is_some() {
            return Ok(None);
        }

        loop {
            let Some((chunk, complete_line)) = self.read_line_chunk()? else {
                if !self.line_buffer.is_empty() {
                    std::str::from_utf8(&self.line_buffer).map_err(|error| {
                        ProductionError::InvalidData(format!("journal is not UTF-8: {error}"))
                    })?;
                    self.incomplete_tail = true;
                    self.line_buffer.clear();
                }
                self.finish();
                return Ok(None);
            };
            self.line_buffer.extend_from_slice(&chunk);
            if !complete_line {
                continue;
            }
            self.line_number = self.line_number.saturating_add(1);
            let line_bytes = self.line_buffer.len();
            let content = self
                .line_buffer
                .strip_suffix(b"\n")
                .unwrap_or(&self.line_buffer);
            let content = content.strip_suffix(b"\r").unwrap_or(content);
            let line = std::str::from_utf8(content).map_err(|error| {
                ProductionError::InvalidData(format!("journal is not UTF-8: {error}"))
            })?;
            if line.trim().is_empty() {
                self.line_buffer.clear();
                continue;
            }
            if self.complete_record_count >= self.limits.max_records {
                return Err(limit_error(
                    "journal record count",
                    u64::try_from(self.complete_record_count.saturating_add(1)).unwrap_or(u64::MAX),
                    u64::try_from(self.limits.max_records).unwrap_or(u64::MAX),
                    "records",
                ));
            }
            let record: JournalRecord = match serde_json::from_str(line) {
                Ok(record) => record,
                Err(_) if self.journal_is_at_eof()? => {
                    self.incomplete_tail = true;
                    self.line_buffer.clear();
                    self.finish();
                    return Ok(None);
                }
                Err(error) => {
                    return Err(ProductionError::InvalidData(format!(
                        "journal line {}: {error}",
                        self.line_number
                    )))
                }
            };
            record.verify()?;
            if record.sequence <= self.previous_sequence {
                return Err(ProductionError::Integrity(format!(
                    "journal sequence {} is not monotonic",
                    record.sequence
                )));
            }
            self.previous_sequence = record.sequence;
            self.complete_record_count += 1;
            self.max_complete_line_bytes = self.max_complete_line_bytes.max(line_bytes);
            self.line_buffer.clear();
            return Ok(Some(record));
        }
    }

    fn empty(limits: RecoveryInspectionLimits) -> Self {
        Self {
            checkpoint: None,
            checkpoint_metadata: None,
            journal: None,
            limits,
            line_buffer: Vec::new(),
            journal_bytes: 0,
            complete_record_count: 0,
            max_complete_line_bytes: 0,
            previous_sequence: 0,
            line_number: 0,
            incomplete_tail: false,
            stats: None,
            failed: false,
        }
    }

    fn read_line_chunk(&mut self) -> Result<Option<(Vec<u8>, bool)>, ProductionError> {
        let current_line_bytes = self.line_buffer.len();
        let current_journal_bytes = self.journal_bytes;
        let limits = self.limits;
        let Some(reader) = self.journal.as_mut() else {
            return Ok(None);
        };
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(None);
        }
        let complete_line = available.iter().position(|byte| *byte == b'\n');
        let chunk_length = complete_line.map_or(available.len(), |index| index + 1);
        let chunk_bytes = u64::try_from(chunk_length).unwrap_or(u64::MAX);
        let next_journal_bytes = current_journal_bytes.saturating_add(chunk_bytes);
        if next_journal_bytes > limits.max_journal_bytes {
            return Err(ceiling_error(
                "journal",
                next_journal_bytes,
                limits.max_journal_bytes,
            ));
        }
        let next_line_bytes = current_line_bytes.saturating_add(chunk_length);
        if next_line_bytes > limits.max_record_line_bytes {
            return Err(ceiling_error(
                "journal record line",
                u64::try_from(next_line_bytes).unwrap_or(u64::MAX),
                u64::try_from(limits.max_record_line_bytes).unwrap_or(u64::MAX),
            ));
        }
        let chunk = available[..chunk_length].to_vec();
        reader.consume(chunk_length);
        self.journal_bytes = next_journal_bytes;
        Ok(Some((chunk, complete_line.is_some())))
    }

    fn journal_is_at_eof(&mut self) -> Result<bool, ProductionError> {
        match self.journal.as_mut() {
            Some(reader) => Ok(reader.fill_buf()?.is_empty()),
            None => Ok(true),
        }
    }

    fn finish(&mut self) {
        self.stats = Some(RecoveryInspectionStats {
            journal_bytes: self.journal_bytes,
            complete_record_count: self.complete_record_count,
            max_complete_line_bytes: self.max_complete_line_bytes,
            incomplete_tail: self.incomplete_tail,
        });
    }
}

/// Inspect an existing recovery directory without creating, repairing, or pruning entries.
pub fn inspect_recovery(
    directory: impl AsRef<Path>,
) -> Result<RecoveryInspection, ProductionError> {
    inspect_recovery_with_limits(directory, RecoveryInspectionLimits::default())
}

/// Inspect recovery state while enforcing caller-supplied read ceilings.
pub fn inspect_recovery_with_limits(
    directory: impl AsRef<Path>,
    limits: RecoveryInspectionLimits,
) -> Result<RecoveryInspection, ProductionError> {
    let mut cursor = RecoveryInspectionCursor::open_with_limits(directory, limits)?;
    let checkpoint = cursor.checkpoint.clone();
    let checkpoint_metadata = cursor.checkpoint_metadata.clone();
    let mut records = Vec::new();
    while let Some(record) = cursor.next_record()? {
        records.push(record);
    }
    let stats = cursor
        .stats()
        .expect("cursor exposes stats after next_record reaches EOF");
    Ok(RecoveryInspection {
        checkpoint,
        checkpoint_metadata,
        records,
        journal_bytes: stats.journal_bytes,
        complete_record_count: stats.complete_record_count,
        max_complete_line_bytes: stats.max_complete_line_bytes,
        incomplete_tail: stats.incomplete_tail,
    })
}

fn open_optional_bounded_journal(
    path: &Path,
    maximum_bytes: u64,
) -> Result<Option<BufReader<File>>, ProductionError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.file_type().is_file() {
        return Err(obstructed_path(
            path,
            "read target is a symlink or special file",
        ));
    }
    if metadata.len() > maximum_bytes {
        return Err(ceiling_error("journal", metadata.len(), maximum_bytes));
    }
    let file = File::open(path)?;
    let opened_metadata = file.metadata()?;
    if !opened_metadata.file_type().is_file() {
        return Err(obstructed_path(
            path,
            "opened journal is not a regular file",
        ));
    }
    if opened_metadata.len() > maximum_bytes {
        return Err(ceiling_error(
            "journal",
            opened_metadata.len(),
            maximum_bytes,
        ));
    }
    Ok(Some(BufReader::new(file)))
}

fn validate_existing_entries(
    directory: &Path,
    limits: RecoveryInspectionLimits,
) -> Result<(), ProductionError> {
    let mut checkpoint_metadata_bytes = 0_u64;
    let mut directory_entries = 0_usize;
    for entry in fs::read_dir(directory)? {
        directory_entries = directory_entries.saturating_add(1);
        if directory_entries > limits.max_directory_entries {
            return Err(limit_error(
                "recovery directory entry count",
                u64::try_from(directory_entries).unwrap_or(u64::MAX),
                u64::try_from(limits.max_directory_entries).unwrap_or(u64::MAX),
                "entries",
            ));
        }
        let path = entry?.path();
        let metadata = fs::symlink_metadata(&path)?;
        if !metadata.file_type().is_file() {
            return Err(obstructed_path(
                &path,
                "recovery entry is a symlink, directory, or special file",
            ));
        }
        if is_checkpoint_metadata_path(&path) {
            checkpoint_metadata_bytes = checkpoint_metadata_bytes
                .checked_add(metadata.len())
                .ok_or_else(|| {
                    ProductionError::InvalidData(
                        "checkpoint metadata total exceeds u64 inspection ceiling".into(),
                    )
                })?;
            if checkpoint_metadata_bytes > limits.max_checkpoint_metadata_bytes {
                return Err(ceiling_error(
                    "checkpoint metadata total",
                    checkpoint_metadata_bytes,
                    limits.max_checkpoint_metadata_bytes,
                ));
            }
        }
        if path.file_name().is_some_and(|name| name == JOURNAL_FILE)
            && metadata.len() > limits.max_journal_bytes
        {
            return Err(ceiling_error(
                "journal",
                metadata.len(),
                limits.max_journal_bytes,
            ));
        }
        if is_checkpoint_read_path(&path) && metadata.len() > limits.max_checkpoint_bytes {
            return Err(ceiling_error(
                "checkpoint",
                metadata.len(),
                limits.max_checkpoint_bytes,
            ));
        }
    }
    Ok(())
}

fn is_checkpoint_read_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name == CHECKPOINT_FILE
        || name == CHECKPOINT_META_FILE
        || (name.starts_with(CHECKPOINT_GENERATION_PREFIX)
            && (name.ends_with(CHECKPOINT_GENERATION_PAYLOAD_SUFFIX)
                || name.ends_with(CHECKPOINT_GENERATION_METADATA_SUFFIX)))
        || (name.starts_with("checkpoint-commit-") && name.ends_with(".json"))
}

fn is_checkpoint_metadata_path(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
        return false;
    };
    name == CHECKPOINT_META_FILE
        || (name.starts_with(CHECKPOINT_GENERATION_PREFIX)
            && name.ends_with(CHECKPOINT_GENERATION_METADATA_SUFFIX))
        || (name.starts_with(CHECKPOINT_COMMIT_PREFIX) && name.ends_with(CHECKPOINT_COMMIT_SUFFIX))
}

fn ceiling_error(description: &str, actual: u64, maximum: u64) -> ProductionError {
    limit_error(description, actual, maximum, "bytes")
}

fn limit_error(description: &str, actual: u64, maximum: u64, unit: &str) -> ProductionError {
    ProductionError::InvalidData(format!(
        "{description} is {actual} {unit}, exceeding the {maximum} {unit} inspection ceiling"
    ))
}

fn obstructed_path(path: &Path, reason: &str) -> ProductionError {
    ProductionError::Io(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{reason}: {}", path.display()),
    ))
}
