//! Versioned Sheets cell-edit recovery layered on complete workbook packages.

use fs2::FileExt;
use loom_production::snapshot::application_state_directory;
use loom_production::{CheckpointMetadata, JournalRecord, RecoveryJournal};
use loom_sheets_core::{CellRef, Sheet};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const VERSIONED_DIRECTORY_SUFFIX: &str = ".sheets-recovery-v1";
const VERSIONED_SCHEMA_PREFIX: &str = "loom.sheets.recovery/1";
pub(super) const RECORD_VERSION: u32 = 1;
const JOURNAL_FILE: &str = "operations.jsonl";
const CHECKPOINT_FILE: &str = "checkpoint.bin";
const CHECKPOINT_METADATA_FILE: &str = "checkpoint.json";
const GENERATION_PREFIX: &str = "checkpoint-generation-";
const GENERATION_PAYLOAD_SUFFIX: &str = ".bin";
const GENERATION_METADATA_SUFFIX: &str = ".json";
#[cfg(windows)]
const COMMIT_PREFIX: &str = "checkpoint-commit-";
#[cfg(windows)]
const COMMIT_SUFFIX: &str = ".json";
const WRITER_LOCK_FILE: &str = ".sheets-writer.lock";

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RecoveryIdentity {
    pub(super) session_id: String,
    pub(super) workbook_id: String,
    pub(super) baseline_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CellAssignment {
    pub(super) sheet: usize,
    pub(super) row: u32,
    pub(super) col: u32,
    pub(super) raw: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct CellEditBatch {
    pub(super) format_version: u32,
    pub(super) session_id: String,
    pub(super) workbook_id: String,
    pub(super) baseline_id: String,
    pub(super) predecessor_sequence: u64,
    pub(super) active_sheet: usize,
    pub(super) edits: Vec<CellAssignment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CheckpointIdentity {
    generation: u64,
    last_sequence: u64,
    sha256: String,
    timestamp_ms: u128,
    schema: String,
}

impl CheckpointIdentity {
    fn metadata(&self) -> CheckpointMetadata {
        CheckpointMetadata {
            last_sequence: self.last_sequence,
            sha256: self.sha256.clone(),
            timestamp_ms: self.timestamp_ms,
            schema: self.schema.clone(),
        }
    }
}

/// The durable writer and the replayed startup payload for one Sheets session.
pub(crate) struct CellEditRecovery {
    journal: RecoveryJournal,
    identity: Option<RecoveryIdentity>,
    last_sequence: u64,
    failed: Option<String>,
    _writer_lock: File,
}

impl CellEditRecovery {
    pub(crate) fn open(application_id: &str) -> Result<(Self, Option<Vec<u8>>), String> {
        let directory =
            application_state_directory(application_id).map_err(|error| error.to_string())?;
        Self::open_at(&directory)
    }

    pub(crate) fn open_at(
        legacy_directory: impl AsRef<Path>,
    ) -> Result<(Self, Option<Vec<u8>>), String> {
        let legacy_directory = legacy_directory.as_ref();
        let versioned_directory = versioned_directory_for(legacy_directory)?;
        fs::create_dir_all(&versioned_directory)
            .map_err(|error| format!("create Sheets recovery directory: {error}"))?;
        let writer_lock = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(versioned_directory.join(WRITER_LOCK_FILE))
            .map_err(|error| format!("open Sheets recovery writer lock: {error}"))?;
        FileExt::try_lock_exclusive(&writer_lock).map_err(|error| {
            if error.kind() == io::ErrorKind::WouldBlock {
                "Sheets recovery already has an active writer".to_string()
            } else {
                format!("lock Sheets recovery writer: {error}")
            }
        })?;

        let journal =
            RecoveryJournal::open(&versioned_directory).map_err(|error| error.to_string())?;
        let recovered = journal.recover().map_err(|error| error.to_string())?;
        let (identity, last_sequence, restored_payload) = match (
            recovered.checkpoint.as_deref(),
            recovered.checkpoint_metadata.as_ref(),
        ) {
            (Some(package), Some(metadata)) => {
                let identity = parse_checkpoint_identity(&metadata.schema)?;
                let mut workbook = crate::workbook_io::restore_workbook_from_snapshot(package)
                    .ok_or_else(|| "Sheets recovery checkpoint package is invalid".to_string())?;
                if workbook.sheets.is_empty() {
                    return Err("Sheets recovery checkpoint has no worksheets".to_string());
                }
                let mut last_sequence = metadata.last_sequence;
                for record in &recovered.operations {
                    let expected_sequence = last_sequence.checked_add(1).ok_or_else(|| {
                        "Sheets recovery sequence is exhausted at u64::MAX".to_string()
                    })?;
                    if record.sequence != expected_sequence {
                        return Err(format!(
                            "Sheets recovery sequence {} does not follow {last_sequence}",
                            record.sequence
                        ));
                    }
                    let batch = decode_batch(record)?;
                    validate_batch(&batch, &identity, last_sequence, &workbook.sheets)?;
                    apply_batch(&mut workbook.sheets, &mut workbook.active, &batch);
                    last_sequence = record.sequence;
                }
                let payload =
                    crate::workbook_io::workbook_package_bytes(&workbook.sheets, workbook.active)?;
                (Some(identity), last_sequence, Some(payload))
            }
            (None, None) => {
                if !recovered.operations.is_empty() {
                    return Err(
                        "Sheets recovery edit journal has no complete baseline checkpoint".into(),
                    );
                }
                let legacy_payload = read_legacy_payload(legacy_directory)?;
                (None, 0, legacy_payload)
            }
            _ => {
                return Err("Sheets recovery checkpoint payload and metadata are incomplete".into())
            }
        };

        Ok((
            Self {
                journal,
                identity,
                last_sequence,
                failed: None,
                _writer_lock: writer_lock,
            },
            restored_payload,
        ))
    }

    pub(crate) fn needs_initial_checkpoint(&self) -> bool {
        self.identity.is_none() && self.failed.is_none()
    }

    pub(crate) fn record_cells(
        &mut self,
        active_sheet: usize,
        edits: impl IntoIterator<Item = (usize, CellRef, Option<String>)>,
    ) -> Result<(), String> {
        if let Some(error) = &self.failed {
            return Err(error.clone());
        }
        let identity = self
            .identity
            .as_ref()
            .ok_or_else(|| "Sheets recovery baseline has not been checkpointed".to_string())?;
        let mut edits = edits
            .into_iter()
            .map(|(sheet, cell, raw)| CellAssignment {
                sheet,
                row: cell.row,
                col: cell.col,
                raw,
            })
            .collect::<Vec<_>>();
        edits.sort_by_key(|edit| (edit.sheet, edit.row, edit.col));
        let mut seen = HashSet::with_capacity(edits.len());
        if edits
            .iter()
            .any(|edit| !seen.insert((edit.sheet, edit.row, edit.col)))
        {
            return self.fail("Sheets recovery batch contains duplicate cell assignments".into());
        }
        let batch = CellEditBatch {
            format_version: RECORD_VERSION,
            session_id: identity.session_id.clone(),
            workbook_id: identity.workbook_id.clone(),
            baseline_id: identity.baseline_id.clone(),
            predecessor_sequence: self.last_sequence,
            active_sheet,
            edits,
        };
        let payload = serde_json::to_vec(&batch)
            .map_err(|error| format!("encode Sheets recovery edit batch: {error}"))?;
        let expected_sequence = self
            .last_sequence
            .checked_add(1)
            .ok_or_else(|| "Sheets recovery sequence is exhausted at u64::MAX".to_string())?;
        match self.journal.append(
            format!("sheets-cell-batch-{expected_sequence}"),
            "Sheets cell edits",
            payload,
        ) {
            Ok(record) if record.sequence == expected_sequence => {
                self.last_sequence = record.sequence;
                Ok(())
            }
            Ok(record) => self.fail(format!(
                "Sheets recovery wrote unexpected sequence {}; expected {expected_sequence}",
                record.sequence
            )),
            Err(error) => self.fail(error.to_string()),
        }
    }

    pub(crate) fn checkpoint_package(
        &mut self,
        package: Vec<u8>,
        new_workbook: bool,
    ) -> Result<(), String> {
        let durable_sequence = match self.journal.recover() {
            Ok(recovered) => recovered.operations.last().map_or_else(
                || {
                    recovered
                        .checkpoint_metadata
                        .as_ref()
                        .map_or(0, |metadata| metadata.last_sequence)
                },
                |record| record.sequence,
            ),
            Err(error) => return self.fail(error.to_string()),
        };
        let identity = match &self.identity {
            Some(current) => RecoveryIdentity {
                session_id: current.session_id.clone(),
                workbook_id: if new_workbook {
                    new_identity_id()
                } else {
                    current.workbook_id.clone()
                },
                baseline_id: new_identity_id(),
            },
            None => RecoveryIdentity {
                session_id: new_identity_id(),
                workbook_id: new_identity_id(),
                baseline_id: new_identity_id(),
            },
        };
        let schema = encode_checkpoint_identity(&identity);
        if let Err(error) = self.journal.checkpoint(durable_sequence, schema, &package) {
            return self.fail(error.to_string());
        }

        // The checkpoint pointer is now durable, so its baseline identity is
        // authoritative even if subsequent journal compaction reports an error.
        self.identity = Some(identity);
        self.last_sequence = durable_sequence;
        if let Err(error) = self.journal.compact(durable_sequence) {
            return self.fail(error.to_string());
        }
        self.failed = None;
        Ok(())
    }

    pub(crate) fn mark_failed(&mut self, error: String) {
        self.failed = Some(error);
    }

    fn fail<T>(&mut self, error: String) -> Result<T, String> {
        self.failed = Some(error.clone());
        Err(error)
    }
}

pub(crate) fn versioned_directory_for(legacy_directory: &Path) -> Result<PathBuf, String> {
    let name = legacy_directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Sheets recovery directory has no valid name".to_string())?;
    Ok(legacy_directory.with_file_name(format!("{name}{VERSIONED_DIRECTORY_SUFFIX}")))
}

#[cfg(test)]
pub(crate) fn remove_test_recovery_data(legacy_directory: &Path) {
    let _ = fs::remove_dir_all(legacy_directory);
    if let Ok(versioned_directory) = versioned_directory_for(legacy_directory) {
        let _ = fs::remove_dir_all(versioned_directory);
    }
}

fn new_identity_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    format!("{:x}-{:x}-{:x}", std::process::id(), timestamp, sequence)
}

fn encode_checkpoint_identity(identity: &RecoveryIdentity) -> String {
    format!(
        "{VERSIONED_SCHEMA_PREFIX};session={};workbook={};baseline={}",
        identity.session_id, identity.workbook_id, identity.baseline_id
    )
}

pub(super) fn parse_checkpoint_identity(schema: &str) -> Result<RecoveryIdentity, String> {
    let mut parts = schema.split(';');
    let version = parts.next().unwrap_or_default();
    if version != VERSIONED_SCHEMA_PREFIX {
        return Err(format!(
            "unsupported Sheets recovery checkpoint version: {version}"
        ));
    }
    let session_id = parts
        .next()
        .and_then(|part| part.strip_prefix("session="))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Sheets recovery checkpoint has no session identity".to_string())?;
    let workbook_id = parts
        .next()
        .and_then(|part| part.strip_prefix("workbook="))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Sheets recovery checkpoint has no workbook identity".to_string())?;
    let baseline_id = parts
        .next()
        .and_then(|part| part.strip_prefix("baseline="))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Sheets recovery checkpoint has no baseline identity".to_string())?;
    if parts.next().is_some() {
        return Err("Sheets recovery checkpoint identity has extra fields".into());
    }
    Ok(RecoveryIdentity {
        session_id: session_id.to_string(),
        workbook_id: workbook_id.to_string(),
        baseline_id: baseline_id.to_string(),
    })
}

fn decode_batch(record: &JournalRecord) -> Result<CellEditBatch, String> {
    serde_json::from_slice(&record.payload).map_err(|error| {
        format!(
            "Sheets recovery record {} is invalid: {error}",
            record.sequence
        )
    })
}

fn validate_batch(
    batch: &CellEditBatch,
    identity: &RecoveryIdentity,
    predecessor_sequence: u64,
    sheets: &[Sheet],
) -> Result<(), String> {
    if batch.format_version != RECORD_VERSION {
        return Err(format!(
            "unsupported Sheets recovery edit version {}",
            batch.format_version
        ));
    }
    if batch.session_id != identity.session_id {
        return Err("Sheets recovery edit session identity does not match its checkpoint".into());
    }
    if batch.workbook_id != identity.workbook_id {
        return Err("Sheets recovery edit workbook identity does not match its checkpoint".into());
    }
    if batch.baseline_id != identity.baseline_id {
        return Err("Sheets recovery edit baseline identity does not match its checkpoint".into());
    }
    if batch.predecessor_sequence != predecessor_sequence {
        return Err(format!(
            "Sheets recovery predecessor {} does not match sequence {predecessor_sequence}",
            batch.predecessor_sequence
        ));
    }
    if batch.active_sheet >= sheets.len() {
        return Err(format!(
            "Sheets recovery active tab {} is outside the workbook",
            batch.active_sheet
        ));
    }
    let mut seen = HashSet::with_capacity(batch.edits.len());
    for edit in &batch.edits {
        if edit.sheet >= sheets.len() {
            return Err(format!(
                "Sheets recovery edit targets missing sheet {}",
                edit.sheet
            ));
        }
        if !seen.insert((edit.sheet, edit.row, edit.col)) {
            return Err("Sheets recovery batch contains duplicate cell assignments".into());
        }
    }
    Ok(())
}

fn apply_batch(sheets: &mut [Sheet], active_sheet: &mut usize, batch: &CellEditBatch) {
    // The entire batch was validated before this function can mutate the
    // candidate workbook.
    for edit in &batch.edits {
        let sheet = &mut sheets[edit.sheet];
        let cell = CellRef {
            row: edit.row,
            col: edit.col,
        };
        match &edit.raw {
            Some(raw) => sheet.set_raw(cell, raw.clone()),
            None => {
                sheet.clear_cell(cell);
            }
        }
    }
    *active_sheet = batch.active_sheet;
}

// SnapshotRecovery::open repairs a torn legacy tail in place. Read the legacy
// formats directly with the same committed-prefix, digest, and monotonicity
// checks so startup does not rewrite or copy any old recovery data.
fn read_legacy_payload(directory: &Path) -> Result<Option<Vec<u8>>, String> {
    let (checkpoint, metadata) = read_legacy_checkpoint(directory)?;
    let checkpoint_sequence = metadata
        .as_ref()
        .map_or(0, |metadata| metadata.last_sequence);
    Ok(read_legacy_records_after(directory, checkpoint_sequence)?.or(checkpoint))
}

fn read_legacy_records_after(
    directory: &Path,
    checkpoint_sequence: u64,
) -> Result<Option<Vec<u8>>, String> {
    let path = directory.join(JOURNAL_FILE);
    if !path.exists() {
        return Ok(None);
    }
    let file = File::open(&path).map_err(|error| format!("read legacy Sheets journal: {error}"))?;
    let mut reader = BufReader::new(file);
    let mut line = Vec::new();
    let mut pending_line: Option<Vec<u8>> = None;
    let mut previous_sequence = 0;
    let mut newest_payload = None;
    let mut line_number = 0;
    loop {
        line.clear();
        if reader
            .read_until(b'\n', &mut line)
            .map_err(|error| format!("read legacy Sheets journal: {error}"))?
            == 0
        {
            break;
        }
        let committed = line.last() == Some(&b'\n');
        if committed {
            line.pop();
        }
        let current_line = std::mem::take(&mut line);
        if let Some(previous_line) = pending_line.take() {
            line_number += 1;
            process_legacy_record_line(
                &previous_line,
                false,
                line_number,
                checkpoint_sequence,
                &mut previous_sequence,
                &mut newest_payload,
            )?;
        }
        if !committed {
            // Match RecoveryJournal's rule: a record needs its final newline
            // delimiter to be committed. Still reject invalid UTF-8 anywhere
            // in the legacy journal, as its shared reader does.
            std::str::from_utf8(&current_line)
                .map_err(|error| format!("legacy Sheets journal is not UTF-8: {error}"))?;
            break;
        }
        pending_line = Some(current_line);
    }
    if let Some(final_line) = pending_line {
        line_number += 1;
        process_legacy_record_line(
            &final_line,
            true,
            line_number,
            checkpoint_sequence,
            &mut previous_sequence,
            &mut newest_payload,
        )?;
    }
    Ok(newest_payload)
}

fn process_legacy_record_line(
    line: &[u8],
    tolerate_invalid_final_record: bool,
    line_number: usize,
    checkpoint_sequence: u64,
    previous_sequence: &mut u64,
    newest_payload: &mut Option<Vec<u8>>,
) -> Result<(), String> {
    let line = std::str::from_utf8(line)
        .map_err(|error| format!("legacy Sheets journal is not UTF-8: {error}"))?;
    if line.trim().is_empty() {
        return Ok(());
    }
    let record: JournalRecord = match serde_json::from_str(line) {
        Ok(record) => record,
        Err(_) if tolerate_invalid_final_record => return Ok(()),
        Err(error) => return Err(format!("legacy Sheets journal line {line_number}: {error}")),
    };
    let checksum =
        loom_package::manifest::Checksum::from_bytes(loom_package::zip::sha256(&record.payload))
            .to_hex();
    if checksum != record.payload_sha256 {
        return Err(format!(
            "legacy Sheets journal operation {} payload digest mismatch",
            record.operation_id
        ));
    }
    if record.sequence <= *previous_sequence {
        return Err(format!(
            "legacy Sheets journal sequence {} is not monotonic",
            record.sequence
        ));
    }
    *previous_sequence = record.sequence;
    if record.sequence > checkpoint_sequence {
        *newest_payload = Some(record.payload);
    }
    Ok(())
}

fn read_legacy_checkpoint(
    directory: &Path,
) -> Result<(Option<Vec<u8>>, Option<CheckpointMetadata>), String> {
    #[cfg(windows)]
    if let Some(pointer) = latest_commit_pointer(directory)? {
        let (bytes, metadata) = read_legacy_generation(directory, pointer.generation)?;
        if metadata != pointer.metadata() {
            return Err(format!(
                "legacy Sheets checkpoint generation {} does not match its commit record",
                pointer.generation
            ));
        }
        return Ok((Some(bytes), Some(metadata)));
    }

    let metadata_path = directory.join(CHECKPOINT_METADATA_FILE);
    let payload_path = directory.join(CHECKPOINT_FILE);
    if !metadata_path.exists() {
        if payload_path.exists() {
            return Err("legacy Sheets checkpoint metadata and payload must both exist".into());
        }
        return Ok((None, None));
    }
    let value: serde_json::Value = serde_json::from_slice(
        &fs::read(&metadata_path)
            .map_err(|error| format!("read legacy Sheets checkpoint metadata: {error}"))?,
    )
    .map_err(|error| format!("legacy Sheets checkpoint metadata is invalid: {error}"))?;
    if value.get("generation").is_some() {
        let pointer: CheckpointIdentity = serde_json::from_value(value)
            .map_err(|error| format!("legacy Sheets checkpoint pointer is invalid: {error}"))?;
        let (bytes, metadata) = read_legacy_generation(directory, pointer.generation)?;
        if metadata != pointer.metadata() {
            return Err(format!(
                "legacy Sheets checkpoint generation {} does not match its pointer",
                pointer.generation
            ));
        }
        return Ok((Some(bytes), Some(metadata)));
    }
    if !payload_path.exists() {
        return Err("legacy Sheets checkpoint metadata and payload must both exist".into());
    }
    let metadata: CheckpointMetadata = serde_json::from_value(value)
        .map_err(|error| format!("legacy Sheets checkpoint metadata is invalid: {error}"))?;
    let bytes = fs::read(payload_path)
        .map_err(|error| format!("read legacy Sheets checkpoint payload: {error}"))?;
    verify_legacy_checkpoint_digest(&bytes, &metadata.sha256)?;
    Ok((Some(bytes), Some(metadata)))
}

#[cfg(windows)]
fn latest_commit_pointer(directory: &Path) -> Result<Option<CheckpointIdentity>, String> {
    let mut latest: Option<(u64, PathBuf)> = None;
    if !directory.exists() {
        return Ok(None);
    }
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read legacy Sheets recovery directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read legacy Sheets recovery entry: {error}"))?;
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let generation = name
            .strip_prefix(COMMIT_PREFIX)
            .and_then(|name| name.strip_suffix(COMMIT_SUFFIX))
            .and_then(|generation| generation.parse::<u64>().ok());
        if let Some(generation) = generation {
            if latest
                .as_ref()
                .map_or(true, |(current, _)| generation > *current)
            {
                latest = Some((generation, entry.path()));
            }
        }
    }
    let Some((generation, path)) = latest else {
        return Ok(None);
    };
    let pointer: CheckpointIdentity = serde_json::from_slice(
        &fs::read(path).map_err(|error| format!("read legacy Sheets commit record: {error}"))?,
    )
    .map_err(|error| format!("legacy Sheets commit record is invalid: {error}"))?;
    if pointer.generation != generation {
        return Err(format!(
            "legacy Sheets commit record {generation} names generation {}",
            pointer.generation
        ));
    }
    Ok(Some(pointer))
}

fn read_legacy_generation(
    directory: &Path,
    generation: u64,
) -> Result<(Vec<u8>, CheckpointMetadata), String> {
    let payload_path = directory.join(format!(
        "{GENERATION_PREFIX}{generation}{GENERATION_PAYLOAD_SUFFIX}"
    ));
    let metadata_path = directory.join(format!(
        "{GENERATION_PREFIX}{generation}{GENERATION_METADATA_SUFFIX}"
    ));
    if !payload_path.exists() || !metadata_path.exists() {
        return Err(format!(
            "legacy Sheets checkpoint generation {generation} is incomplete"
        ));
    }
    let metadata: CheckpointMetadata = serde_json::from_slice(
        &fs::read(metadata_path)
            .map_err(|error| format!("read legacy Sheets generation metadata: {error}"))?,
    )
    .map_err(|error| format!("legacy Sheets generation metadata is invalid: {error}"))?;
    let bytes = fs::read(payload_path)
        .map_err(|error| format!("read legacy Sheets generation payload: {error}"))?;
    verify_legacy_checkpoint_digest(&bytes, &metadata.sha256)?;
    Ok((bytes, metadata))
}

fn verify_legacy_checkpoint_digest(bytes: &[u8], expected: &str) -> Result<(), String> {
    let actual =
        loom_package::manifest::Checksum::from_bytes(loom_package::zip::sha256(bytes)).to_hex();
    if actual != expected {
        return Err("legacy Sheets checkpoint digest mismatch".into());
    }
    Ok(())
}
