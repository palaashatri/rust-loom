//! Approved bounds and deterministic accounting for Sheets recovery.

use loom_production::RecoveryInspectionLimits;
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path};
#[cfg(test)]
use std::time::{Duration, Instant};
use std::{fs, io};

const MIB: u64 = 1024 * 1024;

pub(super) const MAX_RECOVERY_PACKAGE_BYTES: u64 = 256 * MIB;
pub(super) const MAX_RECOVERY_METADATA_BYTES: u64 = 64 * MIB;
pub(super) const MAX_RETAINED_RECOVERY_BYTES: u64 = 640 * MIB;
pub(super) const MAX_TEMPORARY_RECOVERY_BYTES: u64 = 1024 * MIB;
pub(super) const MAX_JOURNAL_BYTES: u64 = 64 * MIB;
pub(super) const MAX_JOURNAL_RECORDS: u64 = 10_000;
pub(super) const MAX_JOURNAL_RECORD_BYTES: u64 = MIB;
#[cfg(test)]
pub(super) const CHECKPOINT_AFTER_BYTES: u64 = 16 * MIB;
#[cfg(test)]
pub(super) const CHECKPOINT_AFTER_RECORDS: u64 = 2_000;
#[cfg(test)]
pub(super) const CHECKPOINT_AFTER_AGE: Duration = Duration::from_secs(5 * 60);
#[cfg(test)]
pub(super) const RETAINED_CHECKPOINT_GENERATIONS: usize = 2;
#[cfg(test)]
pub(super) const EDIT_DURABILITY_TARGET: Duration = Duration::from_millis(250);
pub(super) const MAX_RECOVERY_DIRECTORY_ENTRIES: usize = 10_000;
pub(super) const MAX_RECOVERY_DIRECTORY_DEPTH: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RecoveryLimits {
    pub(super) package_bytes: u64,
    pub(super) retained_bytes: u64,
    pub(super) temporary_peak_bytes: u64,
}

pub(super) const APPROVED_RECOVERY_LIMITS: RecoveryLimits = RecoveryLimits {
    package_bytes: MAX_RECOVERY_PACKAGE_BYTES,
    retained_bytes: MAX_RETAINED_RECOVERY_BYTES,
    temporary_peak_bytes: MAX_TEMPORARY_RECOVERY_BYTES,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RecoveryStorageBytes {
    pub(super) versioned_bytes: u64,
    pub(super) legacy_bytes: u64,
}

impl RecoveryStorageBytes {
    pub(super) fn total_bytes(self) -> Result<u64, String> {
        self.versioned_bytes
            .checked_add(self.legacy_bytes)
            .ok_or_else(|| "recovery byte count overflow".to_string())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RecoveryBudget {
    pub(super) retained_bytes: u64,
    pub(super) temporary_peak_bytes: u64,
}

pub(super) fn validate_recovery_roots(
    legacy_directory: &Path,
    versioned_directory: &Path,
) -> Result<(), String> {
    validate_directory_path(legacy_directory, "legacy")?;
    validate_directory_path(versioned_directory, "versioned")?;
    validate_lock_path(
        &legacy_directory.join(".checkpoint.lock"),
        "legacy .checkpoint.lock",
    )?;
    validate_lock_path(
        &versioned_directory.join(".checkpoint.lock"),
        "versioned .checkpoint.lock",
    )?;
    validate_lock_path(
        &versioned_directory.join(".sheets-writer.lock"),
        "versioned .sheets-writer.lock",
    )
}

fn validate_directory_path(directory: &Path, label: &str) -> Result<(), String> {
    if directory
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(format!("{label} recovery path contains a parent component"));
    }
    let absolute = if directory.is_absolute() {
        directory.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("resolve {label} recovery path: {error}"))?
            .join(directory)
    };
    let ancestors = absolute.ancestors().collect::<Vec<_>>();
    for path in ancestors.into_iter().rev() {
        match fs::symlink_metadata(path) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "{label} recovery path contains a symlink: {}",
                    path.display()
                ));
            }
            Ok(metadata) if !metadata.file_type().is_dir() => {
                return Err(format!(
                    "{label} recovery path is not a directory: {}",
                    path.display()
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(format!(
                    "inspect {label} recovery path {}: {error}",
                    path.display()
                ));
            }
        }
    }
    Ok(())
}

fn validate_lock_path(path: &Path, label: &str) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.file_type().is_file() => {
            Err(format!("{label} is not a regular file"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("inspect {label}: {error}")),
    }
}

pub(super) fn inspection_limits() -> RecoveryInspectionLimits {
    RecoveryInspectionLimits {
        max_checkpoint_bytes: MAX_RECOVERY_PACKAGE_BYTES,
        max_checkpoint_metadata_bytes: MAX_RECOVERY_METADATA_BYTES,
        max_journal_bytes: MAX_JOURNAL_BYTES,
        max_records: MAX_JOURNAL_RECORDS as usize,
        max_record_line_bytes: MAX_JOURNAL_RECORD_BYTES as usize,
        max_directory_entries: MAX_RECOVERY_DIRECTORY_ENTRIES,
    }
}

pub(super) fn open_bounded_file(
    path: &Path,
    maximum_bytes: u64,
    description: &str,
) -> Result<File, String> {
    let path_metadata =
        fs::symlink_metadata(path).map_err(|error| format!("inspect {description}: {error}"))?;
    if path_metadata.file_type().is_symlink() || !path_metadata.file_type().is_file() {
        return Err(format!("{description} is not a regular file"));
    }
    if path_metadata.len() > maximum_bytes {
        return Err(format!(
            "{description} exceeds its {maximum_bytes} byte limit"
        ));
    }

    let file = File::open(path).map_err(|error| format!("open {description}: {error}"))?;
    let opened_metadata = file
        .metadata()
        .map_err(|error| format!("inspect opened {description}: {error}"))?;
    if !opened_metadata.file_type().is_file() {
        return Err(format!("opened {description} is not a regular file"));
    }
    if opened_metadata.len() > maximum_bytes {
        return Err(format!(
            "opened {description} exceeds its {maximum_bytes} byte limit"
        ));
    }
    Ok(file)
}

pub(super) fn read_bounded_file(
    path: &Path,
    maximum_bytes: u64,
    description: &str,
) -> Result<Vec<u8>, String> {
    let file = open_bounded_file(path, maximum_bytes, description)?;
    let read_limit = maximum_bytes
        .checked_add(1)
        .ok_or_else(|| format!("{description} limit cannot be incremented"))?;
    let mut bytes = Vec::new();
    file.take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("read {description}: {error}"))?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(format!(
            "{description} grew beyond its {maximum_bytes} byte limit while reading"
        ));
    }
    Ok(bytes)
}

/// Measures logical file lengths without following links. Both the exclusive session-writer lock
/// and legacy-directory lock must remain held continuously from inventory through publication,
/// compaction, pruning, and cleanup. Every competing writer or migrator must honor those same
/// locks for that full interval. CellEditRecovery retains both locks for the recovery object's
/// lifetime, so callers must keep the same object alive through inventory and publication.
pub(super) fn scan_recovery_storage(
    versioned_directory: &Path,
    legacy_directory: &Path,
) -> Result<RecoveryStorageBytes, String> {
    let versioned_bytes = versioned_file_bytes(versioned_directory)?;
    let legacy_root_bytes = directory_root_bytes(legacy_directory, "legacy")?;
    let legacy_bytes = if legacy_root_bytes > 0 {
        let manifest = super::legacy_migration::scan_legacy(legacy_directory)?;
        if !manifest.unsupported_entries().is_empty() {
            return Err(format!(
                "legacy recovery contains unsupported entries: {}",
                manifest.unsupported_entries().join(", ")
            ));
        }
        let mut total = legacy_root_bytes;
        add_bytes(
            &mut total,
            root_lock_file_bytes(legacy_directory, "legacy")?,
            "legacy recovery byte count",
        )?;
        add_bytes(
            &mut total,
            sum_legacy_files(legacy_directory)?,
            "legacy recovery byte count",
        )?;
        total
    } else {
        0
    };

    Ok(RecoveryStorageBytes {
        versioned_bytes,
        legacy_bytes,
    })
}

pub(super) fn preflight_existing_storage(
    versioned_directory: &Path,
    legacy_directory: &Path,
    limits: RecoveryLimits,
) -> Result<RecoveryStorageBytes, String> {
    let storage = scan_recovery_storage(versioned_directory, legacy_directory)?;
    let retained_bytes = storage.total_bytes()?;
    preflight_checkpoint(storage, 0, retained_bytes, 0, limits)?;
    Ok(storage)
}

pub(super) fn preflight_checkpoint(
    current_storage: RecoveryStorageBytes,
    package_bytes: u64,
    // Includes all versioned and legacy files that remain after publication and pruning.
    retained_after_publication_bytes: u64,
    // Includes every concurrent file written besides the candidate package.
    additional_publication_bytes: u64,
    limits: RecoveryLimits,
) -> Result<RecoveryBudget, String> {
    if package_bytes > limits.package_bytes {
        return Err(format!(
            "recovery package limit exceeded ({} > {} bytes)",
            package_bytes, limits.package_bytes
        ));
    }
    if retained_after_publication_bytes < package_bytes {
        return Err(format!(
            "recovery retained-byte estimate ({retained_after_publication_bytes}) is smaller than the checkpoint package ({package_bytes})"
        ));
    }
    if retained_after_publication_bytes > limits.retained_bytes {
        return Err(format!(
            "recovery retained-storage limit exceeded ({} > {} bytes)",
            retained_after_publication_bytes, limits.retained_bytes
        ));
    }

    let temporary_peak_bytes = current_storage
        .total_bytes()?
        .checked_add(package_bytes)
        .and_then(|bytes| bytes.checked_add(additional_publication_bytes))
        .ok_or_else(|| "recovery temporary byte count overflow".to_string())?
        .max(retained_after_publication_bytes);
    if temporary_peak_bytes > limits.temporary_peak_bytes {
        return Err(format!(
            "recovery temporary-peak limit exceeded ({} > {} bytes)",
            temporary_peak_bytes, limits.temporary_peak_bytes
        ));
    }

    Ok(RecoveryBudget {
        retained_bytes: retained_after_publication_bytes,
        temporary_peak_bytes,
    })
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CheckpointCadence {
    encoded_batch_bytes: u64,
    records: u64,
    oldest_uncheckpointed_at: Option<Instant>,
}

#[cfg(test)]
impl CheckpointCadence {
    pub(super) fn record_durable_batch(&mut self, encoded_bytes: u64, durable_at: Instant) {
        if self.records == 0 {
            self.oldest_uncheckpointed_at = Some(durable_at);
        }
        self.encoded_batch_bytes = self.encoded_batch_bytes.saturating_add(encoded_bytes);
        self.records = self.records.saturating_add(1);
    }

    pub(super) fn is_due_at(self, now: Instant) -> bool {
        self.encoded_batch_bytes >= CHECKPOINT_AFTER_BYTES
            || self.records >= CHECKPOINT_AFTER_RECORDS
            || self
                .oldest_uncheckpointed_at
                .is_some_and(|oldest| now.saturating_duration_since(oldest) >= CHECKPOINT_AFTER_AGE)
    }

    pub(super) fn next_deadline(self) -> Option<Instant> {
        self.oldest_uncheckpointed_at
            .and_then(|oldest| oldest.checked_add(CHECKPOINT_AFTER_AGE))
    }

    pub(super) fn reset_after_checkpoint(&mut self) {
        *self = Self::default();
    }
}

const MIN_DIRECTORY_BYTES: u64 = 4096;
const LOCK_FILES: [&str; 2] = [".checkpoint.lock", ".sheets-writer.lock"];

fn versioned_file_bytes(directory: &Path) -> Result<u64, String> {
    let root_bytes = directory_root_bytes(directory, "versioned")?;
    if root_bytes == 0 {
        return Ok(0);
    }
    let mut total = root_bytes;
    add_bytes(
        &mut total,
        root_lock_file_bytes(directory, "versioned")?,
        "versioned recovery byte count",
    )?;
    let mut entry_count = 0_usize;
    sum_versioned_directory(directory, true, &mut total, &mut entry_count, 0)?;
    Ok(total)
}

fn directory_root_bytes(directory: &Path, label: &str) -> Result<u64, String> {
    let metadata = match fs::symlink_metadata(directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => {
            return Err(format!("inspect {label} recovery directory: {error}"));
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!("{label} recovery directory is a symlink"));
    }
    if !metadata.file_type().is_dir() {
        return Err(format!("{label} recovery path is not a directory"));
    }
    Ok(metadata.len().max(MIN_DIRECTORY_BYTES))
}

fn root_lock_file_bytes(directory: &Path, label: &str) -> Result<u64, String> {
    let mut total = 0;
    for lock_name in LOCK_FILES {
        let path = directory.join(lock_name);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "inspect {label} recovery lock {lock_name}: {error}"
                ));
            }
        };
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(format!(
                "{label} recovery lock {lock_name} is not a regular file"
            ));
        }
        add_bytes(&mut total, metadata.len(), "recovery lock byte count")?;
    }
    Ok(total)
}

fn sum_versioned_directory(
    directory: &Path,
    root: bool,
    total: &mut u64,
    entry_count: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > MAX_RECOVERY_DIRECTORY_DEPTH {
        return Err(format!(
            "versioned recovery directory depth exceeds {MAX_RECOVERY_DIRECTORY_DEPTH} levels"
        ));
    }
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read versioned recovery directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read versioned recovery entry: {error}"))?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        *entry_count = (*entry_count).saturating_add(1);
        if *entry_count > MAX_RECOVERY_DIRECTORY_ENTRIES {
            return Err(format!(
                "versioned recovery directory entry count exceeds {MAX_RECOVERY_DIRECTORY_ENTRIES}"
            ));
        }
        if root && LOCK_FILES.contains(&name_text.as_ref()) {
            continue;
        }
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| {
            format!(
                "inspect versioned recovery entry {}: {error}",
                name.to_string_lossy()
            )
        })?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(format!(
                "versioned recovery contains unsupported symlink {}",
                name.to_string_lossy()
            ));
        }
        if file_type.is_file() {
            add_bytes(total, metadata.len(), "versioned recovery byte count")?;
        } else if file_type.is_dir() {
            add_bytes(
                total,
                metadata.len().max(MIN_DIRECTORY_BYTES),
                "versioned recovery directory byte count",
            )?;
            sum_versioned_directory(&path, false, total, entry_count, depth.saturating_add(1))?;
        } else {
            return Err(format!(
                "versioned recovery contains unsupported entry {}",
                name.to_string_lossy()
            ));
        }
    }
    Ok(())
}

fn sum_legacy_files(directory: &Path) -> Result<u64, String> {
    let mut total = 0;
    let mut entry_count = 0_usize;
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read legacy recovery directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read legacy recovery entry: {error}"))?;
        let name = entry.file_name();
        entry_count = entry_count.saturating_add(1);
        if entry_count > MAX_RECOVERY_DIRECTORY_ENTRIES {
            return Err(format!(
                "legacy recovery directory entry count exceeds {MAX_RECOVERY_DIRECTORY_ENTRIES}"
            ));
        }
        if LOCK_FILES.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path()).map_err(|error| {
            format!(
                "inspect legacy recovery entry {}: {error}",
                name.to_string_lossy()
            )
        })?;
        if metadata.file_type().is_symlink() || !metadata.file_type().is_file() {
            return Err(format!(
                "legacy recovery contains unsupported entry {}",
                name.to_string_lossy()
            ));
        }
        add_bytes(&mut total, metadata.len(), "legacy recovery byte count")?;
    }
    Ok(total)
}

fn add_bytes(total: &mut u64, bytes: u64, label: &str) -> Result<(), String> {
    *total = total
        .checked_add(bytes)
        .ok_or_else(|| format!("{label} overflow"))?;
    Ok(())
}

#[cfg(test)]
#[path = "recovery_policy_tests.rs"]
mod tests;
