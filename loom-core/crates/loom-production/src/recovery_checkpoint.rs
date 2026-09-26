//! Lock-scoped checkpoint and bounded append transactions.

use super::checkpoint_generations::{
    checkpoint_generation_metadata_path, checkpoint_generation_payload_path,
    checkpoint_pointer_target_path, checkpoint_reconciliation_plan, next_checkpoint_generation,
    next_checkpoint_generation_with_limit, publish_checkpoint_pointer, read_checkpoint_generations,
    read_checkpoint_state, reconcile_checkpoint_generations, CheckpointPointer, CheckpointState,
};
use super::recovery_inspection::{RecoveryInspectionLimits, DEFAULT_MAX_DIRECTORY_ENTRIES};
use super::{
    atomic_write, checked_next_sequence, compact_journal_with, lock_recovery_writes,
    read_records_at, repair_journal, sha256_hex, unix_time_ms, CheckpointMetadata, JournalRecord,
    ProductionError, RecoveryJournal, JOURNAL_FILE, MAX_JOURNAL_BYTES, MAX_JOURNAL_RECORDS,
    MAX_JOURNAL_RECORD_LINE_BYTES,
};
use serde_json::to_vec_pretty;
use std::fs;
use std::path::Path;

/// Reserve applied to every newly introduced entry in a recovery directory.
/// The scanner continues to count `max(metadata.len(), 4096)` for each
/// directory; this reserve covers expected logical directory growth before a
/// publication can add its files or atomic-write staging directory.
pub const RECOVERY_DIRECTORY_ENTRY_GROWTH_RESERVE_BYTES: u64 = 64 * 1024;

const MIN_TEMPORARY_DIRECTORY_CHARGE_BYTES: u64 = 4096;

/// Exact encoded-file projection for a bounded journal append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JournalAppendProjection {
    /// Bytes in the journal before any repair or append.
    pub current_journal_bytes: u64,
    /// Bytes in the journal after optional torn-tail repair and append.
    pub resulting_journal_bytes: u64,
    /// Encoded append line bytes, including the newline delimiter.
    pub appended_line_bytes: u64,
    /// Maximum temporary encoded bytes above the current directory inventory.
    pub temporary_peak_additional_bytes: u64,
    /// Conservative retained reserve for a newly created journal directory entry.
    pub retained_directory_growth_reserve_bytes: u64,
}

impl JournalAppendProjection {
    pub(super) fn new(
        current_journal_bytes: u64,
        repaired_prefix_bytes: Option<u64>,
        appended_line_bytes: u64,
        journal_exists: bool,
    ) -> Result<Self, ProductionError> {
        let resulting_journal_bytes = repaired_prefix_bytes
            .unwrap_or(current_journal_bytes)
            .checked_add(appended_line_bytes)
            .ok_or_else(|| ProductionError::InvalidData("journal byte length overflowed".into()))?;
        let temporary_peak_additional_bytes = match repaired_prefix_bytes {
            Some(repaired_bytes) => {
                let staging_peak = repaired_bytes
                    .checked_add(MIN_TEMPORARY_DIRECTORY_CHARGE_BYTES)
                    .and_then(|bytes| {
                        bytes.checked_add(RECOVERY_DIRECTORY_ENTRY_GROWTH_RESERVE_BYTES)
                    })
                    .ok_or_else(|| {
                        ProductionError::InvalidData(
                            "temporary recovery byte count overflowed".into(),
                        )
                    })?;
                staging_peak.max(resulting_journal_bytes.saturating_sub(current_journal_bytes))
            }
            None if !journal_exists => appended_line_bytes
                .checked_add(MIN_TEMPORARY_DIRECTORY_CHARGE_BYTES)
                .and_then(|bytes| {
                    bytes.checked_add(RECOVERY_DIRECTORY_ENTRY_GROWTH_RESERVE_BYTES * 2)
                })
                .ok_or_else(|| {
                    ProductionError::InvalidData("temporary recovery byte count overflowed".into())
                })?,
            None => appended_line_bytes,
        };
        let retained_directory_growth_reserve_bytes = if journal_exists {
            0
        } else {
            RECOVERY_DIRECTORY_ENTRY_GROWTH_RESERVE_BYTES
        };
        Ok(Self {
            current_journal_bytes,
            resulting_journal_bytes,
            appended_line_bytes,
            temporary_peak_additional_bytes,
            retained_directory_growth_reserve_bytes,
        })
    }
}

/// Encoded-file accounting supplied to the application preflight callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CheckpointWriteProjection {
    /// Sequence represented by the complete candidate checkpoint.
    pub last_sequence: u64,
    /// Candidate package bytes.
    pub package_bytes: u64,
    /// New retained bytes after pointer publication, journal compaction, and pruning.
    pub retained_added_bytes: u64,
    /// Existing journal, replaced pointer, and generation artifacts removed by the transaction.
    pub retained_removed_bytes: u64,
    /// Maximum concurrent encoded bytes above the pre-transaction inventory.
    pub temporary_peak_additional_bytes: u64,
    /// Exact new metadata file size.
    pub metadata_bytes: u64,
    /// Exact platform-specific pointer file size.
    pub pointer_bytes: u64,
    /// Exact compacted journal size.
    pub compacted_journal_bytes: u64,
    /// Logical bytes removed by checkpoint-generation reconciliation.
    pub pruned_artifact_bytes: u64,
}

/// Result of checkpoint publication and journal compaction.
#[derive(Debug)]
pub struct CheckpointAndCompactOutcome {
    /// Metadata for the checkpoint pointer that is now durable.
    pub metadata: CheckpointMetadata,
    /// Last durable sequence represented by that pointer.
    pub last_sequence: u64,
    /// A post-publication compaction or reconciliation error, if one occurred.
    /// The checkpoint is authoritative even when this is populated.
    pub compaction_error: Option<String>,
}

impl RecoveryJournal {
    /// Append within local journal limits after an application-owned aggregate
    /// storage preflight. The callback runs under `.checkpoint.lock` and before
    /// torn-tail repair or journal mutation.
    pub fn append_bounded_with_preflight(
        &mut self,
        operation_id: impl Into<String>,
        label: impl Into<String>,
        payload: Vec<u8>,
        mut preflight: impl FnMut(&JournalAppendProjection) -> Result<(), ProductionError>,
    ) -> Result<JournalRecord, ProductionError> {
        self.append_record_with_preflight(
            operation_id,
            label,
            payload,
            Some(super::JournalAppendLimits {
                max_record_line_bytes: MAX_JOURNAL_RECORD_LINE_BYTES,
                max_journal_bytes: MAX_JOURNAL_BYTES,
                max_records: MAX_JOURNAL_RECORDS,
            }),
            Some(&mut preflight),
        )
    }

    /// Atomically checkpoint through a verified journal sequence, compact the
    /// covered records, and enforce application-provided bounds in one lock
    /// scope. Records newer than `covered_sequence` remain in the journal. The
    /// callback runs before journal repair, generation cleanup, receipt
    /// mutation, or publication.
    pub fn checkpoint_and_compact_with_preflight(
        &mut self,
        schema: impl Into<String>,
        covered_sequence: u64,
        bytes: &[u8],
        preflight: impl FnOnce(&CheckpointWriteProjection) -> Result<(), ProductionError>,
    ) -> Result<CheckpointAndCompactOutcome, ProductionError> {
        self.checkpoint_and_compact_with_entry_limit(
            schema,
            covered_sequence,
            bytes,
            None,
            preflight,
        )
    }

    #[cfg(test)]
    pub(super) fn checkpoint_and_compact_with_entry_limit_for_test(
        &mut self,
        schema: impl Into<String>,
        covered_sequence: u64,
        bytes: &[u8],
        max_directory_entries: usize,
        preflight: impl FnOnce(&CheckpointWriteProjection) -> Result<(), ProductionError>,
    ) -> Result<CheckpointAndCompactOutcome, ProductionError> {
        self.checkpoint_and_compact_with_entry_limit(
            schema,
            covered_sequence,
            bytes,
            Some(max_directory_entries),
            preflight,
        )
    }

    fn checkpoint_and_compact_with_entry_limit(
        &mut self,
        schema: impl Into<String>,
        covered_sequence: u64,
        bytes: &[u8],
        max_directory_entries: Option<usize>,
        preflight: impl FnOnce(&CheckpointWriteProjection) -> Result<(), ProductionError>,
    ) -> Result<CheckpointAndCompactOutcome, ProductionError> {
        let _recovery_lock = lock_recovery_writes(&self.directory)?;
        let max_directory_entries = max_directory_entries
            .unwrap_or(DEFAULT_MAX_DIRECTORY_ENTRIES)
            .min(DEFAULT_MAX_DIRECTORY_ENTRIES);
        let inspection_limits = RecoveryInspectionLimits {
            max_directory_entries,
            ..RecoveryInspectionLimits::default()
        };
        let inspection_limits = Some(inspection_limits);
        let journal_path = self.directory.join(JOURNAL_FILE);
        let journal_read = read_records_at(&journal_path, true)?;
        let current_checkpoint = read_checkpoint_state(&self.directory)?;
        let checkpoint_sequence = current_checkpoint
            .metadata
            .as_ref()
            .map_or(0, |metadata| metadata.last_sequence);
        let frontier_sequence = journal_read
            .records
            .last()
            .map_or(checkpoint_sequence, |record| {
                record.sequence.max(checkpoint_sequence)
            });
        if covered_sequence > frontier_sequence {
            return Err(ProductionError::Integrity(format!(
                "checkpoint covered sequence {covered_sequence} exceeds recovery frontier {frontier_sequence}"
            )));
        }
        if covered_sequence < checkpoint_sequence {
            return Err(ProductionError::Integrity(format!(
                "checkpoint covered sequence {covered_sequence} precedes current checkpoint sequence {checkpoint_sequence}"
            )));
        }
        let next_sequence = checked_next_sequence(frontier_sequence)?;
        let last_sequence = covered_sequence;
        let schema = schema.into();
        let metadata = CheckpointMetadata {
            last_sequence,
            sha256: sha256_hex(bytes),
            timestamp_ms: unix_time_ms(),
            schema,
        };
        let metadata_bytes = to_vec_pretty(&metadata)
            .map_err(|error| ProductionError::InvalidData(error.to_string()))?;
        let generation =
            next_checkpoint_generation_with_limit(&self.directory, max_directory_entries)?;
        let pointer = CheckpointPointer {
            generation,
            last_sequence,
            sha256: metadata.sha256.clone(),
            timestamp_ms: metadata.timestamp_ms,
            schema: metadata.schema.clone(),
            previous_generation: current_checkpoint
                .pointer
                .as_ref()
                .map(|pointer| pointer.generation),
        };
        let pointer_bytes = to_vec_pretty(&pointer)
            .map_err(|error| ProductionError::InvalidData(error.to_string()))?;
        let pointer_path = checkpoint_pointer_target_path(&self.directory, generation)?;
        let new_state = CheckpointState {
            bytes: None,
            metadata: Some(metadata.clone()),
            pointer: Some(pointer),
        };
        let current_prune_plan = checkpoint_reconciliation_plan(
            &self.directory,
            &current_checkpoint,
            inspection_limits,
            None,
            MIN_TEMPORARY_DIRECTORY_CHARGE_BYTES,
        )?;
        let prune_plan = checkpoint_reconciliation_plan(
            &self.directory,
            &new_state,
            inspection_limits,
            Some(&pointer_path),
            MIN_TEMPORARY_DIRECTORY_CHARGE_BYTES,
        )?;

        let old_journal_bytes = file_length(&journal_path)?;
        let compacted_records = journal_read
            .records
            .iter()
            .filter(|record| record.sequence > last_sequence)
            .cloned()
            .collect::<Vec<_>>();
        let compacted_journal_bytes = encoded_records_bytes(&compacted_records)?;
        let replaced_pointer_bytes = file_length(&pointer_path)?;
        let new_pointer_entry = u64::from(!pointer_path.exists());
        let new_journal_entry = u64::from(!journal_path.exists());
        let added_entry_count = 2_usize
            .checked_add(new_pointer_entry as usize)
            .and_then(|entries| entries.checked_add(new_journal_entry as usize))
            .ok_or_else(|| {
                ProductionError::InvalidData("recovery directory entry count overflowed".into())
            })?;
        let entries_after_safe_cleanup = current_prune_plan
            .entry_count
            .checked_sub(current_prune_plan.removable_entry_count)
            .ok_or_else(|| {
                ProductionError::InvalidData(
                    "recovery directory cleanup projection underflowed".into(),
                )
            })?;
        let entries_before_post_publication_pruning = entries_after_safe_cleanup
            .checked_add(added_entry_count)
            .ok_or_else(|| {
                ProductionError::InvalidData("recovery directory entry count overflowed".into())
            })?;
        let entries_removed_after_publication =
            prune_plan.additional_removable_entry_count(&current_prune_plan);
        let final_retained_entry_count = entries_before_post_publication_pruning
            .checked_sub(entries_removed_after_publication)
            .ok_or_else(|| {
                ProductionError::InvalidData(
                    "recovery directory prune projection underflowed".into(),
                )
            })?;
        let temporary_peak_entry_count = entries_before_post_publication_pruning
            .checked_add(2)
            .ok_or_else(|| {
                ProductionError::InvalidData("recovery directory entry count overflowed".into())
            })?;
        if final_retained_entry_count > max_directory_entries
            || temporary_peak_entry_count > max_directory_entries
        {
            return Err(ProductionError::InvalidData(format!(
                "recovery directory entry count exceeds the write ceiling (final {final_retained_entry_count}, peak {temporary_peak_entry_count}, limit {max_directory_entries})"
            )));
        }
        let new_retained_file_entries = 2_u64
            .checked_add(new_pointer_entry)
            .and_then(|entries| entries.checked_add(new_journal_entry))
            .ok_or_else(|| {
                ProductionError::InvalidData("recovery directory entry count overflowed".into())
            })?;
        let retained_growth_reserve = new_retained_file_entries
            .checked_mul(RECOVERY_DIRECTORY_ENTRY_GROWTH_RESERVE_BYTES)
            .ok_or_else(|| {
                ProductionError::InvalidData("recovery directory reserve overflowed".into())
            })?;
        let retained_added_bytes = (bytes.len() as u64)
            .checked_add(metadata_bytes.len() as u64)
            .and_then(|value| value.checked_add(pointer_bytes.len() as u64))
            .and_then(|value| value.checked_add(compacted_journal_bytes))
            .and_then(|value| value.checked_add(retained_growth_reserve))
            .ok_or_else(|| ProductionError::InvalidData("recovery byte count overflowed".into()))?;
        let retained_removed_bytes = old_journal_bytes
            .checked_add(replaced_pointer_bytes)
            .and_then(|value| value.checked_add(prune_plan.removed_logical_bytes))
            .ok_or_else(|| ProductionError::InvalidData("recovery byte count overflowed".into()))?;
        let temporary_peak_additional_bytes = (bytes.len() as u64)
            .checked_add(metadata_bytes.len() as u64)
            .and_then(|value| value.checked_add(pointer_bytes.len() as u64))
            .and_then(|value| value.checked_add(compacted_journal_bytes))
            .and_then(|value| value.checked_add(MIN_TEMPORARY_DIRECTORY_CHARGE_BYTES))
            .and_then(|value| {
                value.checked_add(
                    (new_retained_file_entries + 1)
                        .checked_mul(RECOVERY_DIRECTORY_ENTRY_GROWTH_RESERVE_BYTES)?,
                )
            })
            .ok_or_else(|| {
                ProductionError::InvalidData("temporary recovery byte count overflowed".into())
            })?;
        let journal_repair_bytes = if journal_read.skipped_tail {
            encoded_records_bytes(&journal_read.records)?
                .checked_add(MIN_TEMPORARY_DIRECTORY_CHARGE_BYTES)
                .and_then(|value| value.checked_add(RECOVERY_DIRECTORY_ENTRY_GROWTH_RESERVE_BYTES))
                .ok_or_else(|| {
                    ProductionError::InvalidData("temporary recovery byte count overflowed".into())
                })?
        } else {
            0
        };
        let projection = CheckpointWriteProjection {
            last_sequence,
            package_bytes: bytes.len() as u64,
            retained_added_bytes,
            retained_removed_bytes,
            temporary_peak_additional_bytes: temporary_peak_additional_bytes
                .max(journal_repair_bytes),
            metadata_bytes: metadata_bytes.len() as u64,
            pointer_bytes: pointer_bytes.len() as u64,
            compacted_journal_bytes,
            pruned_artifact_bytes: prune_plan.removed_logical_bytes,
        };

        preflight(&projection)?;

        // Every mutation follows the callback. Keep the previous valid
        // checkpoint and journal until all projected bounds have passed.
        reconcile_checkpoint_generations(&self.directory, &current_checkpoint)?;
        if journal_read.skipped_tail {
            repair_journal(&self.directory, &journal_read.records)?;
        }
        let payload_path = checkpoint_generation_payload_path(&self.directory, generation);
        let metadata_path = checkpoint_generation_metadata_path(&self.directory, generation);
        atomic_write(&payload_path, bytes)?;
        atomic_write(&metadata_path, &metadata_bytes)?;
        let (verified_bytes, verified_metadata) =
            read_checkpoint_generations(&self.directory, generation)?;
        if verified_bytes != bytes || verified_metadata != metadata {
            return Err(ProductionError::Integrity(
                "checkpoint generation changed before publication".into(),
            ));
        }
        publish_checkpoint_pointer(&self.directory, generation, &pointer_bytes)?;
        self.next_sequence = self.next_sequence.max(next_sequence);

        let compaction_error = compact_journal_with(&self.directory, last_sequence, atomic_write)
            .and_then(|()| reconcile_checkpoint_generations(&self.directory, &new_state))
            .err()
            .map(|error| error.to_string());
        Ok(CheckpointAndCompactOutcome {
            metadata,
            last_sequence,
            compaction_error,
        })
    }

    /// Atomically write an application checkpoint.
    pub fn checkpoint(
        &self,
        last_sequence: u64,
        schema: impl Into<String>,
        bytes: &[u8],
    ) -> Result<CheckpointMetadata, ProductionError> {
        let _recovery_lock = lock_recovery_writes(&self.directory)?;
        let journal_read = read_records_at(&self.directory.join(JOURNAL_FILE), true)?;
        if journal_read.skipped_tail {
            repair_journal(&self.directory, &journal_read.records)?;
        }
        let current_checkpoint = read_checkpoint_state(&self.directory)?;
        let current_checkpoint_sequence = current_checkpoint
            .metadata
            .as_ref()
            .map(|metadata| metadata.last_sequence);
        if let Some(current_sequence) = current_checkpoint_sequence {
            if last_sequence < current_sequence {
                return Err(ProductionError::Integrity(format!(
                    "checkpoint sequence {last_sequence} cannot replace newer sequence {current_sequence}"
                )));
            }
        }
        let highest_saved_sequence = journal_read
            .records
            .last()
            .map(|record| record.sequence)
            .into_iter()
            .chain(current_checkpoint_sequence)
            .max()
            .unwrap_or(0);
        if last_sequence > highest_saved_sequence {
            return Err(ProductionError::Integrity(format!(
                "checkpoint sequence {last_sequence} is newer than saved recovery data {highest_saved_sequence}"
            )));
        }
        reconcile_checkpoint_generations(&self.directory, &current_checkpoint)?;

        let metadata = CheckpointMetadata {
            last_sequence,
            sha256: sha256_hex(bytes),
            timestamp_ms: unix_time_ms(),
            schema: schema.into(),
        };
        let metadata_bytes = to_vec_pretty(&metadata)
            .map_err(|error| ProductionError::InvalidData(error.to_string()))?;
        let generation = next_checkpoint_generation(&self.directory)?;
        let payload_path = checkpoint_generation_payload_path(&self.directory, generation);
        let metadata_path = checkpoint_generation_metadata_path(&self.directory, generation);
        atomic_write(&payload_path, bytes)?;
        atomic_write(&metadata_path, &metadata_bytes)?;
        let (verified_bytes, verified_metadata) =
            read_checkpoint_generations(&self.directory, generation)?;
        if verified_bytes != bytes || verified_metadata != metadata {
            return Err(ProductionError::Integrity(
                "checkpoint generation changed before publication".into(),
            ));
        }

        let pointer = CheckpointPointer {
            generation,
            last_sequence,
            sha256: metadata.sha256.clone(),
            timestamp_ms: metadata.timestamp_ms,
            schema: metadata.schema.clone(),
            previous_generation: current_checkpoint
                .pointer
                .as_ref()
                .map(|pointer| pointer.generation),
        };
        let pointer_bytes = to_vec_pretty(&pointer)
            .map_err(|error| ProductionError::InvalidData(error.to_string()))?;
        publish_checkpoint_pointer(&self.directory, generation, &pointer_bytes)?;
        Ok(metadata)
    }

    /// Compact the journal by retaining only records newer than `sequence`.
    pub fn compact(&self, sequence: u64) -> Result<(), ProductionError> {
        let _recovery_lock = lock_recovery_writes(&self.directory)?;
        let checkpoint_state = read_checkpoint_state(&self.directory)?;
        let checkpoint_sequence = checkpoint_state
            .metadata
            .as_ref()
            .map_or(0, |metadata| metadata.last_sequence);
        if sequence > checkpoint_sequence {
            return Err(ProductionError::Integrity(format!(
                "cannot compact through sequence {sequence}; durable checkpoint is {checkpoint_sequence}"
            )));
        }
        let journal_read = read_records_at(&self.directory.join(JOURNAL_FILE), true)?;
        if journal_read.skipped_tail {
            repair_journal(&self.directory, &journal_read.records)?;
        }
        compact_journal_with(&self.directory, sequence, atomic_write)?;
        reconcile_checkpoint_generations(&self.directory, &checkpoint_state)
    }
}

pub(super) fn encoded_records_bytes(records: &[JournalRecord]) -> Result<u64, ProductionError> {
    let mut total = 0_u64;
    for record in records {
        let encoded = serde_json::to_vec(record)
            .map_err(|error| ProductionError::InvalidData(error.to_string()))?;
        total = total
            .checked_add(encoded.len() as u64)
            .and_then(|bytes| bytes.checked_add(1))
            .ok_or_else(|| ProductionError::InvalidData("journal byte length overflowed".into()))?;
    }
    Ok(total)
}

fn file_length(path: &Path) -> Result<u64, ProductionError> {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(metadata.len()),
        Ok(_) => Err(ProductionError::InvalidData(format!(
            "recovery file path is not a regular file: {}",
            path.display()
        ))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}
