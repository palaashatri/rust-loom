//! Safe migration of the old full-workbook recovery directory.

use loom_production::CheckpointMetadata;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::io::{self, Read};
use std::path::Path;

const RECEIPT_FILE: &str = "legacy-migration.json";
const RECEIPT_VERSION: u32 = 1;
const PACKAGE_LIMIT: u64 = 256 * 1024 * 1024;
const RETAINED_LIMIT: u64 = 640 * 1024 * 1024;
const TEMPORARY_LIMIT: u64 = 1024 * 1024 * 1024;
const METADATA_RESERVE: u64 = 64 * 1024;
const LOCK_FILES: [&str; 2] = [".checkpoint.lock", ".sheets-writer.lock"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LegacyFile {
    path: String,
    byte_length: u64,
    sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct LegacyManifest {
    files: Vec<LegacyFile>,
    total_bytes: u64,
    unknown_entries: Vec<String>,
}

impl LegacyManifest {
    pub(super) fn has_recovery_files(&self) -> bool {
        !self.files.is_empty()
    }

    pub(super) fn unsupported_entries(&self) -> &[String] {
        &self.unknown_entries
    }

    pub(super) fn is_empty(&self) -> bool {
        self.files.is_empty() && self.unknown_entries.is_empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ReceiptPhase {
    Prepared,
    Verified,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct MigrationReceipt {
    version: u32,
    phase: ReceiptPhase,
    manifest: LegacyManifest,
    checkpoint: CheckpointBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CheckpointBinding {
    last_sequence: u64,
    sha256: String,
    schema: String,
}

pub(super) enum OpenReceipt {
    None,
    Pending(MigrationReceipt),
    Complete(MigrationReceipt),
}

pub(super) fn scan_legacy(directory: &Path) -> Result<LegacyManifest, String> {
    let mut files = Vec::new();
    let mut unknown_entries = Vec::new();
    let mut total_bytes = 0u64;
    if !directory.exists() {
        return Ok(LegacyManifest {
            files,
            total_bytes,
            unknown_entries,
        });
    }
    let mut entry_count = 0_usize;
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read legacy Sheets recovery directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read legacy Sheets recovery entry: {error}"))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        entry_count = entry_count.saturating_add(1);
        if entry_count > super::recovery_policy::MAX_RECOVERY_DIRECTORY_ENTRIES {
            return Err(format!(
                "legacy Sheets recovery directory entry count exceeds {}",
                super::recovery_policy::MAX_RECOVERY_DIRECTORY_ENTRIES
            ));
        }
        if LOCK_FILES.contains(&name.as_str()) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("inspect legacy recovery entry {name}: {error}"))?;
        if !is_known_recovery_file(&name) || !metadata.file_type().is_file() {
            unknown_entries.push(name);
            continue;
        }
        let remaining_bytes = super::recovery_policy::MAX_RETAINED_RECOVERY_BYTES
            .checked_sub(total_bytes)
            .ok_or_else(|| "legacy recovery byte count exceeds the retained limit".to_string())?;
        let (byte_length, sha256) = digest_file(&entry.path(), remaining_bytes)?;
        total_bytes = total_bytes
            .checked_add(byte_length)
            .ok_or_else(|| "legacy recovery byte count overflow".to_string())?;
        files.push(LegacyFile {
            path: name,
            byte_length,
            sha256,
        });
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    unknown_entries.sort();
    Ok(LegacyManifest {
        files,
        total_bytes,
        unknown_entries,
    })
}

pub(super) fn read_receipt(versioned_directory: &Path) -> Result<OpenReceipt, String> {
    let path = versioned_directory.join(RECEIPT_FILE);
    if !path.exists() {
        return Ok(OpenReceipt::None);
    }
    let receipt: MigrationReceipt =
        serde_json::from_slice(&super::recovery_policy::read_bounded_file(
            &path,
            super::recovery_policy::MAX_RECOVERY_METADATA_BYTES,
            "legacy migration receipt",
        )?)
        .map_err(|error| format!("legacy migration receipt is invalid: {error}"))?;
    if receipt.version != RECEIPT_VERSION {
        return Err(format!(
            "unsupported legacy migration receipt version {}",
            receipt.version
        ));
    }
    validate_manifest(&receipt.manifest)?;
    Ok(match receipt.phase {
        ReceiptPhase::Complete => OpenReceipt::Complete(receipt),
        ReceiptPhase::Prepared | ReceiptPhase::Verified => OpenReceipt::Pending(receipt),
    })
}

pub(super) fn has_versioned_recovery_entries(versioned_directory: &Path) -> Result<bool, String> {
    for entry in fs::read_dir(versioned_directory)
        .map_err(|error| format!("read versioned Sheets recovery directory: {error}"))?
    {
        let entry =
            entry.map_err(|error| format!("read versioned Sheets recovery entry: {error}"))?;
        let name = entry.file_name();
        if name == ".checkpoint.lock" || name == ".sheets-writer.lock" {
            continue;
        }
        return Ok(true);
    }
    Ok(false)
}

pub(super) fn prepared(
    manifest: LegacyManifest,
    last_sequence: u64,
    package: &[u8],
    schema: &str,
) -> MigrationReceipt {
    MigrationReceipt {
        version: RECEIPT_VERSION,
        phase: ReceiptPhase::Prepared,
        manifest,
        checkpoint: CheckpointBinding {
            last_sequence,
            sha256: digest_bytes(package),
            schema: schema.to_string(),
        },
    }
}

pub(super) fn checkpoint_matches(
    receipt: &MigrationReceipt,
    package: Option<&[u8]>,
    metadata: Option<&CheckpointMetadata>,
) -> bool {
    package.is_some()
        && metadata.is_some_and(|metadata| {
            metadata.last_sequence == receipt.checkpoint.last_sequence
                && metadata.sha256 == receipt.checkpoint.sha256
                && metadata.schema == receipt.checkpoint.schema
        })
        && package.is_some_and(|bytes| digest_bytes(bytes) == receipt.checkpoint.sha256)
}

pub(super) fn receipt_manifest(receipt: &MigrationReceipt) -> &LegacyManifest {
    &receipt.manifest
}

pub(super) fn is_verified(receipt: &MigrationReceipt) -> bool {
    receipt.phase == ReceiptPhase::Verified
}

pub(super) fn persist_receipt(
    versioned_directory: &Path,
    receipt: &MigrationReceipt,
) -> Result<(), String> {
    let bytes = encode_receipt(receipt)?;
    persist_receipt_bytes(versioned_directory, &bytes)
}

pub(super) fn encode_receipt(receipt: &MigrationReceipt) -> Result<Vec<u8>, String> {
    serde_json::to_vec_pretty(receipt)
        .map_err(|error| format!("encode legacy migration receipt: {error}"))
}

pub(super) fn persist_receipt_bytes(
    versioned_directory: &Path,
    bytes: &[u8],
) -> Result<(), String> {
    loom_storage::atomic_write(&versioned_directory.join(RECEIPT_FILE), bytes)
        .map_err(|error| format!("write legacy migration receipt: {error}"))
}

pub(super) fn mark_verified(receipt: &mut MigrationReceipt) {
    receipt.phase = ReceiptPhase::Verified;
}

pub(super) fn mark_complete(receipt: &mut MigrationReceipt) {
    receipt.phase = ReceiptPhase::Complete;
}

pub(super) fn preflight(
    package_bytes: u64,
    receipt_bytes: u64,
    manifest: &LegacyManifest,
    versioned_directory: &Path,
    limits_override: Option<(u64, u64, u64)>,
) -> Result<(), String> {
    let (package_limit, retained_limit, temporary_limit) =
        limits_override.unwrap_or((PACKAGE_LIMIT, RETAINED_LIMIT, TEMPORARY_LIMIT));
    if package_bytes > package_limit {
        return Err(format!(
            "legacy migration package limit exceeded ({package_bytes} > {package_limit} bytes)"
        ));
    }
    if !manifest.unknown_entries.is_empty() {
        return Err(format!(
            "legacy migration found unknown recovery entries: {}",
            manifest.unknown_entries.join(", ")
        ));
    }
    let existing = versioned_file_bytes(versioned_directory)?;
    let retained = existing
        .saturating_add(manifest.total_bytes)
        .saturating_add(package_bytes)
        .saturating_add(receipt_bytes)
        .saturating_add(METADATA_RESERVE);
    if retained > retained_limit {
        return Err(format!(
            "legacy migration retained-storage limit exceeded ({retained} > {retained_limit} bytes)"
        ));
    }
    let temporary = retained
        .saturating_add(package_bytes)
        .saturating_add(receipt_bytes);
    if temporary > temporary_limit {
        return Err(format!(
            "legacy migration temporary-peak limit exceeded ({temporary} > {temporary_limit} bytes)"
        ));
    }
    Ok(())
}

pub(super) fn verify_expected_manifest(
    expected: &LegacyManifest,
    actual: &LegacyManifest,
    allow_missing: bool,
) -> Result<(), String> {
    if !expected.unknown_entries.is_empty() {
        return Err(format!(
            "legacy migration found unsupported source entries: {}",
            expected.unknown_entries.join(", ")
        ));
    }
    if !actual.unknown_entries.is_empty() {
        return Err(format!(
            "legacy migration found unknown recovery entries: {}",
            actual.unknown_entries.join(", ")
        ));
    }
    for current in &actual.files {
        if !expected.files.iter().any(|saved| saved == current) {
            return Err(format!(
                "legacy recovery source changed during migration: {}",
                current.path
            ));
        }
    }
    if !allow_missing {
        for saved in &expected.files {
            if !actual.files.contains(saved) {
                return Err(format!(
                    "legacy recovery source changed during migration: {} disappeared before verification",
                    saved.path
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn remove_unchanged(directory: &Path, expected: &LegacyManifest) -> Result<(), String> {
    let actual = scan_legacy(directory)?;
    verify_expected_manifest(expected, &actual, true)?;
    // Validate the full manifest before removing any file, then recheck each
    // digest immediately before its non-recursive removal.
    for saved in &expected.files {
        let path = directory.join(&saved.path);
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(format!(
                    "inspect legacy recovery file {}: {error}",
                    saved.path
                ))
            }
        };
        if !metadata.file_type().is_file() {
            return Err(format!("legacy recovery source changed: {}", saved.path));
        }
        let current = digest_file(&path, saved.byte_length)?;
        if current != (saved.byte_length, saved.sha256.clone()) {
            return Err(format!("legacy recovery source changed: {}", saved.path));
        }
        fs::remove_file(&path).map_err(|error| {
            format!(
                "remove migrated legacy recovery file {}: {error}",
                saved.path
            )
        })?;
    }
    let after = scan_legacy(directory)?;
    verify_expected_manifest(expected, &after, true)
}

pub(super) fn verify_complete_marker(
    receipt: &MigrationReceipt,
    actual: &LegacyManifest,
) -> Result<(), String> {
    if !actual.is_empty() {
        return Err("legacy recovery files reappeared after migration".into());
    }
    if !receipt.manifest.unknown_entries.is_empty() {
        return Err("legacy migration marker contains unsupported source entries".into());
    }
    Ok(())
}

fn versioned_file_bytes(directory: &Path) -> Result<u64, String> {
    let mut total = 0u64;
    let mut entry_count = 0;
    sum_versioned_directory(directory, true, &mut total, &mut entry_count, 0)?;
    Ok(total)
}

fn sum_versioned_directory(
    directory: &Path,
    root: bool,
    total: &mut u64,
    entry_count: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > super::recovery_policy::MAX_RECOVERY_DIRECTORY_DEPTH {
        return Err(format!(
            "versioned recovery directory depth exceeds {} levels",
            super::recovery_policy::MAX_RECOVERY_DIRECTORY_DEPTH
        ));
    }
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("read versioned recovery directory: {error}"))?
    {
        let entry = entry.map_err(|error| format!("read versioned recovery entry: {error}"))?;
        let name = entry.file_name();
        *entry_count = (*entry_count).saturating_add(1);
        if *entry_count > super::recovery_policy::MAX_RECOVERY_DIRECTORY_ENTRIES {
            return Err(format!(
                "versioned recovery directory entry count exceeds {}",
                super::recovery_policy::MAX_RECOVERY_DIRECTORY_ENTRIES
            ));
        }
        if root && (name == ".checkpoint.lock" || name == ".sheets-writer.lock") {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .map_err(|error| format!("inspect versioned recovery entry: {error}"))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "versioned recovery contains unsupported symlink {}",
                name.to_string_lossy()
            ));
        }
        if metadata.file_type().is_file() {
            *total = total.saturating_add(metadata.len());
        } else if metadata.file_type().is_dir() {
            // Include filesystem directory metadata in addition to every
            // descendant file so an obstruction cannot be silently omitted.
            *total = total.saturating_add(metadata.len().max(4096));
            sum_versioned_directory(
                &entry.path(),
                false,
                total,
                entry_count,
                depth.saturating_add(1),
            )?;
        } else {
            return Err(format!(
                "versioned recovery contains unsupported entry {}",
                name.to_string_lossy()
            ));
        }
    }
    Ok(())
}

fn validate_manifest(manifest: &LegacyManifest) -> Result<(), String> {
    let mut names = HashSet::with_capacity(manifest.files.len());
    let mut total_bytes = 0u64;
    let mut previous_name: Option<&str> = None;
    for file in &manifest.files {
        if file.path.contains('/')
            || file.path.contains('\\')
            || !is_known_recovery_file(&file.path)
            || !names.insert(file.path.as_str())
            || previous_name.is_some_and(|previous| previous >= file.path.as_str())
        {
            return Err("legacy migration receipt contains an unsafe file manifest".into());
        }
        if file.sha256.len() != 64
            || !file
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err("legacy migration receipt contains an invalid file digest".into());
        }
        total_bytes = total_bytes
            .checked_add(file.byte_length)
            .ok_or_else(|| "legacy migration receipt byte count overflow".to_string())?;
        previous_name = Some(&file.path);
    }
    if total_bytes != manifest.total_bytes {
        return Err("legacy migration receipt byte count does not match its manifest".into());
    }
    let mut unknown = HashSet::with_capacity(manifest.unknown_entries.len());
    if manifest.unknown_entries.iter().any(|name| {
        name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || !unknown.insert(name.as_str())
    }) {
        return Err("legacy migration receipt contains invalid unknown entry names".into());
    }
    Ok(())
}

fn is_known_recovery_file(name: &str) -> bool {
    matches!(
        name,
        "operations.jsonl" | "checkpoint.bin" | "checkpoint.json"
    ) || numbered_file(name, "checkpoint-generation-", &[".bin", ".json"])
        || numbered_file(name, "checkpoint-commit-", &[".json"])
}

fn numbered_file(name: &str, prefix: &str, suffixes: &[&str]) -> bool {
    let Some(rest) = name.strip_prefix(prefix) else {
        return false;
    };
    suffixes.iter().any(|suffix| {
        rest.strip_suffix(suffix).is_some_and(|number| {
            !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit())
        })
    })
}

fn digest_file(path: &Path, maximum_bytes: u64) -> Result<(u64, String), String> {
    let mut file =
        super::recovery_policy::open_bounded_file(path, maximum_bytes, "legacy recovery file")?;
    let mut digest = Sha256::new();
    let mut total = 0u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("hash legacy recovery file: {error}"))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
        total = total
            .checked_add(read as u64)
            .ok_or_else(|| "legacy recovery byte count overflow".to_string())?;
        if total > maximum_bytes {
            return Err(format!(
                "legacy recovery file grew beyond its {maximum_bytes} byte limit while hashing"
            ));
        }
    }
    Ok((total, hex_digest(digest.finalize().as_slice())))
}

fn digest_bytes(bytes: &[u8]) -> String {
    hex_digest(&Sha256::digest(bytes))
}

fn hex_digest(digest: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "loom-legacy-migration-preflight-{}-{}",
                std::process::id(),
                NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).expect("create preflight test directory");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn legacy_migration_preflight_counts_the_serialized_receipt_size() {
        let directory = TestDirectory::new();
        let manifest = LegacyManifest {
            files: (0..1500)
                .map(|generation| LegacyFile {
                    path: format!("checkpoint-generation-{generation:010}.bin"),
                    byte_length: 0,
                    sha256: digest_bytes(b""),
                })
                .collect(),
            total_bytes: 0,
            unknown_entries: Vec::new(),
        };
        let receipt = prepared(
            manifest.clone(),
            0,
            b"",
            "loom.sheets.recovery/1;session=a;workbook=b;baseline=c",
        );
        let serialized = serde_json::to_vec_pretty(&receipt).expect("serialize large receipt");
        assert!(serialized.len() > 128 * 1024);

        let retained_limit = METADATA_RESERVE + serialized.len() as u64 - 1;
        let retained_result = preflight(
            0,
            serialized.len() as u64,
            &manifest,
            &directory.0,
            Some((u64::MAX, retained_limit, u64::MAX)),
        );
        assert!(
            retained_result.is_err(),
            "the serialized receipt exceeds retained capacity even though the old 64 KiB reserve fits"
        );

        let atomic_temporary_limit = METADATA_RESERVE + serialized.len() as u64;
        let temporary_result = preflight(
            0,
            serialized.len() as u64,
            &manifest,
            &directory.0,
            Some((
                u64::MAX,
                atomic_temporary_limit,
                atomic_temporary_limit + serialized.len() as u64 - 1,
            )),
        );
        assert!(
            temporary_result.is_err(),
            "the receipt's atomic-write temporary copy must count toward peak storage"
        );
    }
}
