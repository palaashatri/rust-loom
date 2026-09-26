use super::recovery_inspection::{read_bounded_file, RecoveryInspectionLimits};
use super::{
    atomic_write, sha256_hex, CheckpointMetadata, ProductionError, CHECKPOINT_FILE,
    CHECKPOINT_META_FILE,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(super) const CHECKPOINT_GENERATION_PREFIX: &str = "checkpoint-generation-";
pub(super) const CHECKPOINT_GENERATION_PAYLOAD_SUFFIX: &str = ".bin";
pub(super) const CHECKPOINT_GENERATION_METADATA_SUFFIX: &str = ".json";
pub(super) const CHECKPOINT_COMMIT_PREFIX: &str = "checkpoint-commit-";
pub(super) const CHECKPOINT_COMMIT_SUFFIX: &str = ".json";

/// One published checkpoint generation and its exact committed predecessor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct CheckpointPointer {
    pub(super) generation: u64,
    pub(super) last_sequence: u64,
    pub(super) sha256: String,
    pub(super) timestamp_ms: u128,
    pub(super) schema: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) previous_generation: Option<u64>,
}

impl CheckpointPointer {
    fn metadata(&self) -> CheckpointMetadata {
        CheckpointMetadata {
            last_sequence: self.last_sequence,
            sha256: self.sha256.clone(),
            timestamp_ms: self.timestamp_ms,
            schema: self.schema.clone(),
        }
    }
}

pub(super) struct CheckpointState {
    pub(super) bytes: Option<Vec<u8>>,
    pub(super) metadata: Option<CheckpointMetadata>,
    pub(super) pointer: Option<CheckpointPointer>,
}

impl CheckpointState {
    fn empty() -> Self {
        Self {
            bytes: None,
            metadata: None,
            pointer: None,
        }
    }
}

pub(super) fn read_checkpoint_sequence(directory: &Path) -> Result<Option<u64>, ProductionError> {
    let (candidates, legacy_metadata) = read_pointer_candidates(directory)?;
    let pointers = unique_pointers(candidates)?;
    if let Some((_, pointer)) = pointers.into_iter().next_back() {
        validate_pointer_lineage(&pointer)?;
        return Ok(Some(pointer.last_sequence));
    }
    Ok(legacy_metadata.map(|metadata| metadata.last_sequence))
}

pub(super) fn read_checkpoint(
    directory: &Path,
) -> Result<(Option<Vec<u8>>, Option<CheckpointMetadata>), ProductionError> {
    let state = read_checkpoint_state(directory)?;
    Ok((state.bytes, state.metadata))
}

#[cfg(windows)]
pub(super) fn read_latest_commit_pointer(
    directory: &Path,
) -> Result<Option<CheckpointPointer>, ProductionError> {
    Ok(read_checkpoint_state(directory)?.pointer)
}

pub(super) fn read_checkpoint_state(directory: &Path) -> Result<CheckpointState, ProductionError> {
    let (candidates, legacy_metadata) = read_pointer_candidates(directory)?;
    let by_generation = unique_pointers(candidates)?;
    if let Some((_, pointer)) = by_generation.into_iter().next_back() {
        let (bytes, metadata) = validate_pointer(directory, &pointer)?;
        return Ok(CheckpointState {
            bytes: Some(bytes),
            metadata: Some(metadata),
            pointer: Some(pointer),
        });
    }
    let Some(metadata) = legacy_metadata else {
        let checkpoint_path = directory.join(CHECKPOINT_FILE);
        if checkpoint_path.exists() {
            return Err(ProductionError::Integrity(
                "checkpoint metadata and payload must both exist".into(),
            ));
        }
        return Ok(CheckpointState::empty());
    };
    let checkpoint_path = directory.join(CHECKPOINT_FILE);
    if !checkpoint_path.exists() {
        return Err(ProductionError::Integrity(
            "checkpoint metadata and payload must both exist".into(),
        ));
    }
    let bytes = fs::read(checkpoint_path)?;
    if sha256_hex(&bytes) != metadata.sha256 {
        return Err(ProductionError::Integrity(
            "checkpoint digest mismatch".into(),
        ));
    }
    Ok(CheckpointState {
        bytes: Some(bytes),
        metadata: Some(metadata),
        pointer: None,
    })
}

pub(super) fn read_checkpoint_state_with_limits(
    directory: &Path,
    limits: RecoveryInspectionLimits,
) -> Result<CheckpointState, ProductionError> {
    let mut metadata_bytes = 0_u64;
    let (candidates, legacy_metadata) =
        read_pointer_candidates_with_limits(directory, limits, &mut metadata_bytes)?;
    let by_generation = unique_pointers(candidates)?;
    if let Some((_, pointer)) = by_generation.into_iter().next_back() {
        let (bytes, metadata) =
            validate_pointer_with_limits(directory, &pointer, limits, &mut metadata_bytes)?;
        return Ok(CheckpointState {
            bytes: Some(bytes),
            metadata: Some(metadata),
            pointer: Some(pointer),
        });
    }
    let Some(metadata) = legacy_metadata else {
        let checkpoint_path = directory.join(CHECKPOINT_FILE);
        if checkpoint_path.exists() {
            return Err(ProductionError::Integrity(
                "checkpoint metadata and payload must both exist".into(),
            ));
        }
        return Ok(CheckpointState::empty());
    };
    let checkpoint_path = directory.join(CHECKPOINT_FILE);
    if !checkpoint_path.exists() {
        return Err(ProductionError::Integrity(
            "checkpoint metadata and payload must both exist".into(),
        ));
    }
    let bytes = read_bounded_file(
        &checkpoint_path,
        limits.max_checkpoint_bytes,
        "checkpoint package",
    )?;
    if sha256_hex(&bytes) != metadata.sha256 {
        return Err(ProductionError::Integrity(
            "checkpoint digest mismatch".into(),
        ));
    }
    Ok(CheckpointState {
        bytes: Some(bytes),
        metadata: Some(metadata),
        pointer: None,
    })
}

fn unique_pointers(
    candidates: Vec<CheckpointPointer>,
) -> Result<BTreeMap<u64, CheckpointPointer>, ProductionError> {
    let mut by_generation = BTreeMap::new();
    for pointer in candidates {
        match by_generation.get(&pointer.generation) {
            Some(existing) if existing != &pointer => {
                return Err(ProductionError::Integrity(format!(
                    "checkpoint pointers conflict for generation {}",
                    pointer.generation
                )));
            }
            Some(_) => {}
            None => {
                by_generation.insert(pointer.generation, pointer);
            }
        }
    }
    Ok(by_generation)
}

fn read_pointer_candidates(
    directory: &Path,
) -> Result<(Vec<CheckpointPointer>, Option<CheckpointMetadata>), ProductionError> {
    let mut candidates = Vec::new();
    let mut legacy_metadata = None;
    let metadata_path = directory.join(CHECKPOINT_META_FILE);
    if metadata_path.exists() {
        let value = read_json_value(&metadata_path, "checkpoint metadata")?;
        if value.get("generation").is_some() {
            let pointer = serde_json::from_value(value).map_err(|error| {
                ProductionError::InvalidData(format!("checkpoint pointer is invalid: {error}"))
            })?;
            candidates.push(pointer);
        } else {
            legacy_metadata = Some(serde_json::from_value(value).map_err(|error| {
                ProductionError::InvalidData(format!("checkpoint metadata is invalid: {error}"))
            })?);
        }
    }
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            let Some(filename_generation) = parse_checkpoint_generation(
                name,
                CHECKPOINT_COMMIT_PREFIX,
                CHECKPOINT_COMMIT_SUFFIX,
            ) else {
                continue;
            };
            let pointer: CheckpointPointer =
                serde_json::from_slice(&fs::read(path)?).map_err(|error| {
                    ProductionError::InvalidData(format!(
                        "checkpoint commit record {filename_generation} is invalid: {error}"
                    ))
                })?;
            if pointer.generation != filename_generation {
                return Err(ProductionError::Integrity(format!(
                    "checkpoint commit record {filename_generation} names generation {}",
                    pointer.generation
                )));
            }
            candidates.push(pointer);
        }
    }
    Ok((candidates, legacy_metadata))
}

fn read_pointer_candidates_with_limits(
    directory: &Path,
    limits: RecoveryInspectionLimits,
    metadata_bytes: &mut u64,
) -> Result<(Vec<CheckpointPointer>, Option<CheckpointMetadata>), ProductionError> {
    let mut candidates = Vec::new();
    let mut legacy_metadata = None;
    let metadata_path = directory.join(CHECKPOINT_META_FILE);
    if path_exists(&metadata_path)? {
        let bytes = read_checkpoint_metadata_with_limit(&metadata_path, limits, metadata_bytes)?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|error| {
            ProductionError::InvalidData(format!("checkpoint metadata is not valid JSON: {error}"))
        })?;
        if value.get("generation").is_some() {
            let pointer = serde_json::from_value(value).map_err(|error| {
                ProductionError::InvalidData(format!("checkpoint pointer is invalid: {error}"))
            })?;
            candidates.push(pointer);
        } else {
            legacy_metadata = Some(serde_json::from_value(value).map_err(|error| {
                ProductionError::InvalidData(format!("checkpoint metadata is invalid: {error}"))
            })?);
        }
    }

    let mut entry_count = 0_usize;
    for entry in fs::read_dir(directory)? {
        entry_count = entry_count.saturating_add(1);
        if entry_count > limits.max_directory_entries {
            return Err(ProductionError::InvalidData(
                "recovery directory entry count exceeds the inspection ceiling".into(),
            ));
        }
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(filename_generation) =
            parse_checkpoint_generation(name, CHECKPOINT_COMMIT_PREFIX, CHECKPOINT_COMMIT_SUFFIX)
        else {
            continue;
        };
        let raw = read_checkpoint_metadata_with_limit(&path, limits, metadata_bytes)?;
        let pointer: CheckpointPointer = serde_json::from_slice(&raw).map_err(|error| {
            ProductionError::InvalidData(format!(
                "checkpoint commit record {filename_generation} is invalid: {error}"
            ))
        })?;
        if pointer.generation != filename_generation {
            return Err(ProductionError::Integrity(format!(
                "checkpoint commit record {filename_generation} names generation {}",
                pointer.generation
            )));
        }
        candidates.push(pointer);
    }
    Ok((candidates, legacy_metadata))
}

fn read_checkpoint_metadata_with_limit(
    path: &Path,
    limits: RecoveryInspectionLimits,
    metadata_bytes: &mut u64,
) -> Result<Vec<u8>, ProductionError> {
    let remaining = limits
        .max_checkpoint_metadata_bytes
        .checked_sub(*metadata_bytes)
        .ok_or_else(|| {
            ProductionError::InvalidData(
                "checkpoint metadata total exceeds the inspection ceiling".into(),
            )
        })?;
    let bytes = read_bounded_file(path, remaining, "checkpoint metadata")?;
    *metadata_bytes = metadata_bytes
        .checked_add(bytes.len() as u64)
        .ok_or_else(|| {
            ProductionError::InvalidData("checkpoint metadata byte count overflow".into())
        })?;
    Ok(bytes)
}

fn path_exists(path: &Path) -> Result<bool, ProductionError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn read_json_value(path: &Path, description: &str) -> Result<serde_json::Value, ProductionError> {
    serde_json::from_slice(&fs::read(path)?).map_err(|error| {
        ProductionError::InvalidData(format!("{description} is not valid JSON: {error}"))
    })
}

fn validate_pointer(
    directory: &Path,
    pointer: &CheckpointPointer,
) -> Result<(Vec<u8>, CheckpointMetadata), ProductionError> {
    validate_pointer_lineage(pointer)?;
    let (bytes, metadata) = read_checkpoint_generations(directory, pointer.generation)?;
    if metadata != pointer.metadata() {
        return Err(ProductionError::Integrity(format!(
            "checkpoint generation {} does not match its commit record",
            pointer.generation
        )));
    }
    Ok((bytes, metadata))
}

fn validate_pointer_with_limits(
    directory: &Path,
    pointer: &CheckpointPointer,
    limits: RecoveryInspectionLimits,
    metadata_bytes: &mut u64,
) -> Result<(Vec<u8>, CheckpointMetadata), ProductionError> {
    validate_pointer_lineage(pointer)?;
    let (bytes, metadata) = read_checkpoint_generations_with_limits(
        directory,
        pointer.generation,
        limits,
        metadata_bytes,
    )?;
    if metadata != pointer.metadata() {
        return Err(ProductionError::Integrity(format!(
            "checkpoint generation {} does not match its commit record",
            pointer.generation
        )));
    }
    Ok((bytes, metadata))
}

fn validate_pointer_lineage(pointer: &CheckpointPointer) -> Result<(), ProductionError> {
    if let Some(previous_generation) = pointer.previous_generation {
        if previous_generation >= pointer.generation {
            return Err(ProductionError::Integrity(format!(
                "checkpoint generation {} has invalid predecessor {previous_generation}",
                pointer.generation
            )));
        }
    }
    Ok(())
}

pub(super) fn checkpoint_generation_payload_path(directory: &Path, generation: u64) -> PathBuf {
    directory.join(format!(
        "{CHECKPOINT_GENERATION_PREFIX}{generation}{CHECKPOINT_GENERATION_PAYLOAD_SUFFIX}"
    ))
}

pub(super) fn checkpoint_generation_metadata_path(directory: &Path, generation: u64) -> PathBuf {
    directory.join(format!(
        "{CHECKPOINT_GENERATION_PREFIX}{generation}{CHECKPOINT_GENERATION_METADATA_SUFFIX}"
    ))
}

fn checkpoint_commit_path(directory: &Path, generation: u64) -> PathBuf {
    directory.join(format!(
        "{CHECKPOINT_COMMIT_PREFIX}{generation}{CHECKPOINT_COMMIT_SUFFIX}"
    ))
}

fn parse_checkpoint_generation(name: &str, prefix: &str, suffix: &str) -> Option<u64> {
    name.strip_prefix(prefix)
        .and_then(|name| name.strip_suffix(suffix))
        .and_then(|id| id.parse().ok())
}

pub(super) fn next_checkpoint_generation(directory: &Path) -> Result<u64, ProductionError> {
    let mut highest = 0;
    if directory.exists() {
        for entry in fs::read_dir(directory)? {
            let name = entry?.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let generation = parse_checkpoint_generation(
                name,
                CHECKPOINT_GENERATION_PREFIX,
                CHECKPOINT_GENERATION_PAYLOAD_SUFFIX,
            )
            .or_else(|| {
                parse_checkpoint_generation(
                    name,
                    CHECKPOINT_GENERATION_PREFIX,
                    CHECKPOINT_GENERATION_METADATA_SUFFIX,
                )
            })
            .or_else(|| {
                parse_checkpoint_generation(
                    name,
                    CHECKPOINT_COMMIT_PREFIX,
                    CHECKPOINT_COMMIT_SUFFIX,
                )
            });
            if let Some(generation) = generation {
                highest = highest.max(generation);
            }
        }
    }
    highest.checked_add(1).ok_or_else(|| {
        ProductionError::Integrity("checkpoint generation is exhausted at u64::MAX".into())
    })
}

pub(super) fn read_checkpoint_generations(
    directory: &Path,
    generation: u64,
) -> Result<(Vec<u8>, CheckpointMetadata), ProductionError> {
    let payload_path = checkpoint_generation_payload_path(directory, generation);
    let metadata_path = checkpoint_generation_metadata_path(directory, generation);
    if !payload_path.exists() || !metadata_path.exists() {
        return Err(ProductionError::Integrity(format!(
            "checkpoint generation {generation} is incomplete"
        )));
    }
    let raw = fs::read(&metadata_path)?;
    let metadata: CheckpointMetadata = serde_json::from_slice(&raw).map_err(|error| {
        ProductionError::InvalidData(format!(
            "checkpoint generation {generation} metadata is invalid: {error}"
        ))
    })?;
    let bytes = fs::read(&payload_path)?;
    if sha256_hex(&bytes) != metadata.sha256 {
        return Err(ProductionError::Integrity(format!(
            "checkpoint generation {generation} digest mismatch"
        )));
    }
    Ok((bytes, metadata))
}

fn read_checkpoint_generations_with_limits(
    directory: &Path,
    generation: u64,
    limits: RecoveryInspectionLimits,
    metadata_bytes: &mut u64,
) -> Result<(Vec<u8>, CheckpointMetadata), ProductionError> {
    let payload_path = checkpoint_generation_payload_path(directory, generation);
    let metadata_path = checkpoint_generation_metadata_path(directory, generation);
    if !path_exists(&payload_path)? || !path_exists(&metadata_path)? {
        return Err(ProductionError::Integrity(format!(
            "checkpoint generation {generation} is incomplete"
        )));
    }
    let raw = read_checkpoint_metadata_with_limit(&metadata_path, limits, metadata_bytes)?;
    let metadata: CheckpointMetadata = serde_json::from_slice(&raw).map_err(|error| {
        ProductionError::InvalidData(format!(
            "checkpoint generation {generation} metadata is invalid: {error}"
        ))
    })?;
    let bytes = read_bounded_file(
        &payload_path,
        limits.max_checkpoint_bytes,
        "checkpoint package",
    )?;
    if sha256_hex(&bytes) != metadata.sha256 {
        return Err(ProductionError::Integrity(format!(
            "checkpoint generation {generation} digest mismatch"
        )));
    }
    Ok((bytes, metadata))
}

fn legacy_previous_generation(
    directory: &Path,
    current_generation: u64,
    limits: Option<RecoveryInspectionLimits>,
) -> Result<Option<u64>, ProductionError> {
    let mut generations = BTreeSet::new();
    let mut entry_count = 0_usize;
    for entry in fs::read_dir(directory)? {
        entry_count = entry_count.saturating_add(1);
        if limits.is_some_and(|limits| entry_count > limits.max_directory_entries) {
            return Err(ProductionError::InvalidData(
                "recovery directory entry count exceeds the inspection ceiling".into(),
            ));
        }
        let name = entry?.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if let Some(generation) = parse_checkpoint_generation(
            name,
            CHECKPOINT_GENERATION_PREFIX,
            CHECKPOINT_GENERATION_PAYLOAD_SUFFIX,
        ) {
            if generation < current_generation {
                generations.insert(generation);
            }
        }
    }
    for generation in generations.into_iter().rev() {
        let checkpoint = match limits {
            Some(limits) => {
                let mut metadata_bytes = 0;
                read_checkpoint_generations_with_limits(
                    directory,
                    generation,
                    limits,
                    &mut metadata_bytes,
                )
            }
            None => read_checkpoint_generations(directory, generation),
        };
        if checkpoint.is_ok() {
            return Ok(Some(generation));
        }
    }
    Ok(None)
}

pub(super) fn reconcile_checkpoint_generations(
    directory: &Path,
    state: &CheckpointState,
) -> Result<(), ProductionError> {
    reconcile_checkpoint_generations_inner(directory, state, None)
}

pub(super) fn reconcile_checkpoint_generations_with_limits(
    directory: &Path,
    state: &CheckpointState,
    limits: RecoveryInspectionLimits,
) -> Result<(), ProductionError> {
    reconcile_checkpoint_generations_inner(directory, state, Some(limits))
}

fn reconcile_checkpoint_generations_inner(
    directory: &Path,
    state: &CheckpointState,
    limits: Option<RecoveryInspectionLimits>,
) -> Result<(), ProductionError> {
    let plan = checkpoint_reconciliation_plan(directory, state, limits, None, 4096)?;
    for path in plan.obsolete_files {
        remove_generated_file(&path)?;
    }
    for path in plan.abandoned_atomic_writes {
        remove_abandoned_atomic_write(&path)?;
    }
    Ok(())
}

pub(super) struct CheckpointReconciliationPlan {
    obsolete_files: Vec<PathBuf>,
    abandoned_atomic_writes: Vec<PathBuf>,
    pub(super) removed_logical_bytes: u64,
}

/// Plan the exact generated artifacts that reconciliation would remove. The
/// optional replacement path is excluded because publication replaces it
/// before reconciliation runs.
pub(super) fn checkpoint_reconciliation_plan(
    directory: &Path,
    state: &CheckpointState,
    limits: Option<RecoveryInspectionLimits>,
    replacement_path: Option<&Path>,
    minimum_directory_charge: u64,
) -> Result<CheckpointReconciliationPlan, ProductionError> {
    let mut retained = BTreeSet::new();
    if let Some(pointer) = &state.pointer {
        retained.insert(pointer.generation);
        if let Some(previous) = pointer.previous_generation {
            retained.insert(previous);
        } else if let Some(previous) =
            legacy_previous_generation(directory, pointer.generation, limits)?
        {
            retained.insert(previous);
        }
    }

    let mut obsolete_pointers = Vec::new();
    let mut obsolete_generations = Vec::new();
    let mut abandoned_atomic_writes = Vec::new();
    let mut entry_count = 0_usize;
    for entry in fs::read_dir(directory)? {
        entry_count = entry_count.saturating_add(1);
        if limits.is_some_and(|limits| entry_count > limits.max_directory_entries) {
            return Err(ProductionError::InvalidData(
                "recovery directory entry count exceeds the inspection ceiling".into(),
            ));
        }
        let path = entry?.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with(".atomicwrite") {
            abandoned_atomic_writes.push(path);
            continue;
        }
        if name == CHECKPOINT_META_FILE {
            let value = match limits {
                Some(limits) => read_bounded_file(
                    &path,
                    limits.max_checkpoint_metadata_bytes,
                    "checkpoint metadata",
                )
                .and_then(|bytes| {
                    serde_json::from_slice(&bytes).map_err(|error| {
                        ProductionError::InvalidData(format!(
                            "checkpoint metadata is not valid JSON: {error}"
                        ))
                    })
                }),
                None => read_json_value(&path, "checkpoint metadata"),
            };
            if let Ok(value) = value {
                if value
                    .get("generation")
                    .and_then(serde_json::Value::as_u64)
                    .is_some_and(|generation| !retained.contains(&generation))
                    && replacement_path != Some(path.as_path())
                {
                    obsolete_pointers.push(path);
                }
            }
            continue;
        }
        let commit_generation =
            parse_checkpoint_generation(name, CHECKPOINT_COMMIT_PREFIX, CHECKPOINT_COMMIT_SUFFIX);
        if commit_generation.is_some_and(|generation| !retained.contains(&generation))
            && replacement_path != Some(path.as_path())
        {
            obsolete_pointers.push(path);
            continue;
        }
        let generation = parse_checkpoint_generation(
            name,
            CHECKPOINT_GENERATION_PREFIX,
            CHECKPOINT_GENERATION_PAYLOAD_SUFFIX,
        )
        .or_else(|| {
            parse_checkpoint_generation(
                name,
                CHECKPOINT_GENERATION_PREFIX,
                CHECKPOINT_GENERATION_METADATA_SUFFIX,
            )
        });
        if generation.is_some_and(|generation| !retained.contains(&generation)) {
            obsolete_generations.push(path);
        }
    }

    let mut obsolete_files = obsolete_pointers;
    obsolete_files.extend(obsolete_generations);
    let mut removed_logical_bytes = 0_u64;
    for path in obsolete_files.iter().chain(abandoned_atomic_writes.iter()) {
        let bytes = logical_entry_bytes(path, minimum_directory_charge)?;
        removed_logical_bytes = removed_logical_bytes
            .checked_add(bytes)
            .ok_or_else(|| ProductionError::InvalidData("recovery byte count overflow".into()))?;
    }
    Ok(CheckpointReconciliationPlan {
        obsolete_files,
        abandoned_atomic_writes,
        removed_logical_bytes,
    })
}

fn logical_entry_bytes(path: &Path, minimum_directory_charge: u64) -> Result<u64, ProductionError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_symlink() {
        return Err(ProductionError::InvalidData(format!(
            "recovery entry is a symlink: {}",
            path.display()
        )));
    }
    if metadata.file_type().is_file() {
        return Ok(metadata.len());
    }
    if !metadata.file_type().is_dir() {
        return Err(ProductionError::InvalidData(format!(
            "recovery entry has unsupported type: {}",
            path.display()
        )));
    }
    let mut total = metadata.len().max(minimum_directory_charge);
    for entry in fs::read_dir(path)? {
        total = total
            .checked_add(logical_entry_bytes(
                &entry?.path(),
                minimum_directory_charge,
            )?)
            .ok_or_else(|| ProductionError::InvalidData("recovery byte count overflow".into()))?;
    }
    Ok(total)
}

fn remove_generated_file(path: &Path) -> Result<(), ProductionError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_dir() {
        return Err(ProductionError::Io(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "refusing to recurse into checkpoint artifact {}",
                path.display()
            ),
        )));
    }
    fs::remove_file(path)?;
    Ok(())
}

fn remove_abandoned_atomic_write(path: &Path) -> Result<(), ProductionError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(super) fn publish_checkpoint_pointer(
    directory: &Path,
    generation: u64,
    pointer_bytes: &[u8],
) -> Result<(), ProductionError> {
    let legacy_temporary = directory.join(format!(
        ".{CHECKPOINT_META_FILE}.{}.tmp",
        std::process::id()
    ));
    if legacy_temporary.exists() {
        if legacy_temporary.is_dir() {
            return Err(ProductionError::Io(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "checkpoint pointer temp is blocked: {}",
                    legacy_temporary.display()
                ),
            )));
        }
        fs::remove_file(&legacy_temporary)?;
    }

    let path = checkpoint_pointer_target_path(directory, generation)?;
    atomic_write(&path, pointer_bytes)
}

pub(super) fn checkpoint_pointer_target_path(
    directory: &Path,
    generation: u64,
) -> Result<PathBuf, ProductionError> {
    #[cfg(windows)]
    {
        Ok(checkpoint_commit_path(directory, generation))
    }

    #[cfg(not(windows))]
    {
        let metadata_path = directory.join(CHECKPOINT_META_FILE);
        let preserves_legacy_metadata = metadata_path.exists()
            && read_json_value(&metadata_path, "checkpoint metadata")?
                .get("generation")
                .is_none();
        if preserves_legacy_metadata {
            Ok(checkpoint_commit_path(directory, generation))
        } else {
            Ok(metadata_path)
        }
    }
}
