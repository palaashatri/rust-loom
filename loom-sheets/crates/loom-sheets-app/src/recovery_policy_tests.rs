use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-sheets-recovery-policy-{}-{}",
            std::process::id(),
            NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("create recovery policy test directory");
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn approved_limits_match_the_owner_decision() {
    assert_eq!(MAX_RECOVERY_PACKAGE_BYTES, 256 * MIB);
    assert_eq!(MAX_RECOVERY_METADATA_BYTES, 64 * MIB);
    assert_eq!(MAX_RETAINED_RECOVERY_BYTES, 640 * MIB);
    assert_eq!(MAX_TEMPORARY_RECOVERY_BYTES, 1024 * MIB);
    assert_eq!(MAX_JOURNAL_BYTES, 64 * MIB);
    assert_eq!(MAX_JOURNAL_RECORDS, 10_000);
    assert_eq!(MAX_JOURNAL_RECORD_BYTES, MIB);
    assert_eq!(CHECKPOINT_AFTER_BYTES, 16 * MIB);
    assert_eq!(CHECKPOINT_AFTER_RECORDS, 2_000);
    assert_eq!(CHECKPOINT_AFTER_AGE, Duration::from_secs(300));
    assert_eq!(RETAINED_CHECKPOINT_GENERATIONS, 2);
    assert_eq!(EDIT_DURABILITY_TARGET, Duration::from_millis(250));
}

#[test]
fn bounded_file_read_refuses_content_over_its_limit() {
    let fixture = TestDirectory::new();
    let path = fixture.path().join("oversized");
    fs::write(&path, b"12345").expect("write oversized fixture");

    let error = read_bounded_file(&path, 4, "test recovery file")
        .expect_err("bounded read must refuse bytes beyond the ceiling");

    assert!(error.contains("4 byte limit"), "unexpected error: {error}");
}

#[test]
fn cadence_triggers_on_exact_size_and_record_boundaries_and_resets() {
    let start = Instant::now();
    let mut by_size = CheckpointCadence::default();
    by_size.record_durable_batch(CHECKPOINT_AFTER_BYTES - 1, start);
    assert!(!by_size.is_due_at(start));
    by_size.record_durable_batch(1, start);
    assert!(by_size.is_due_at(start));

    let mut by_count = CheckpointCadence::default();
    for _ in 0..CHECKPOINT_AFTER_RECORDS - 1 {
        by_count.record_durable_batch(0, start);
    }
    assert!(!by_count.is_due_at(start));
    by_count.record_durable_batch(0, start);
    assert!(by_count.is_due_at(start));

    by_count.reset_after_checkpoint();
    assert!(!by_count.is_due_at(start));
}

#[test]
fn cadence_uses_oldest_durable_edit_without_sleeping() {
    let start = Instant::now();
    let mut cadence = CheckpointCadence::default();
    cadence.record_durable_batch(1, start);
    cadence.record_durable_batch(1, start + Duration::from_secs(120));

    assert_eq!(cadence.next_deadline(), Some(start + CHECKPOINT_AFTER_AGE));
    assert!(!cadence.is_due_at(start + Duration::from_secs(299)));
    assert!(cadence.is_due_at(start + Duration::from_secs(300)));
}

#[test]
fn checkpoint_preflight_accepts_exact_caps_and_rejects_one_over() {
    let empty = RecoveryStorageBytes::default();
    let package_at_limit = preflight_checkpoint(
        empty,
        MAX_RECOVERY_PACKAGE_BYTES,
        MAX_RECOVERY_PACKAGE_BYTES,
        0,
        APPROVED_RECOVERY_LIMITS,
    );
    assert!(package_at_limit.is_ok());
    assert!(preflight_checkpoint(
        empty,
        MAX_RECOVERY_PACKAGE_BYTES + 1,
        MAX_RECOVERY_PACKAGE_BYTES + 1,
        0,
        APPROVED_RECOVERY_LIMITS,
    )
    .is_err());

    assert!(preflight_checkpoint(
        empty,
        0,
        MAX_RETAINED_RECOVERY_BYTES,
        0,
        APPROVED_RECOVERY_LIMITS,
    )
    .is_ok());
    assert!(preflight_checkpoint(
        empty,
        0,
        MAX_RETAINED_RECOVERY_BYTES + 1,
        0,
        APPROVED_RECOVERY_LIMITS,
    )
    .is_err());

    let current_at_peak_minus_one = RecoveryStorageBytes {
        versioned_bytes: MAX_TEMPORARY_RECOVERY_BYTES - 2,
        legacy_bytes: 1,
    };
    assert!(
        preflight_checkpoint(current_at_peak_minus_one, 1, 1, 0, APPROVED_RECOVERY_LIMITS,).is_ok()
    );
    assert!(
        preflight_checkpoint(current_at_peak_minus_one, 2, 2, 0, APPROVED_RECOVERY_LIMITS,)
            .is_err()
    );

    assert!(preflight_checkpoint(
        empty,
        1,
        1,
        MAX_TEMPORARY_RECOVERY_BYTES - 1,
        APPROVED_RECOVERY_LIMITS,
    )
    .is_ok());
    assert!(preflight_checkpoint(
        empty,
        1,
        1,
        MAX_TEMPORARY_RECOVERY_BYTES,
        APPROVED_RECOVERY_LIMITS,
    )
    .is_err());
}

#[test]
fn test_limits_allow_bounded_overrides() {
    let small_limits = RecoveryLimits {
        package_bytes: 8,
        retained_bytes: 16,
        temporary_peak_bytes: 20,
    };
    assert!(preflight_checkpoint(RecoveryStorageBytes::default(), 8, 16, 0, small_limits).is_ok());
    assert!(preflight_checkpoint(RecoveryStorageBytes::default(), 9, 16, 0, small_limits).is_err());
    assert_eq!(
        APPROVED_RECOVERY_LIMITS.package_bytes,
        MAX_RECOVERY_PACKAGE_BYTES
    );
}

#[test]
fn filesystem_inventory_counts_both_recovery_stores_and_temporary_files() {
    let fixture = TestDirectory::new();
    let versioned = fixture.path().join("versioned");
    let legacy = fixture.path().join("legacy");
    fs::create_dir_all(&versioned).expect("create versioned directory");
    fs::create_dir_all(&legacy).expect("create legacy directory");
    fs::write(versioned.join("operations.jsonl"), b"journal").expect("write journal");
    fs::write(versioned.join("checkpoint.tmp"), b"staging").expect("write temp file");
    fs::create_dir(versioned.join("publication")).expect("create staging directory");
    fs::write(versioned.join("publication").join("pointer.tmp"), b"next")
        .expect("write nested temporary file");
    fs::write(versioned.join(".checkpoint.lock"), b"versioned lock").expect("write versioned lock");
    fs::write(legacy.join("checkpoint.bin"), b"legacy bytes").expect("write legacy file");
    fs::write(legacy.join(".sheets-writer.lock"), b"legacy lock").expect("write legacy lock");

    let usage = scan_recovery_storage(&versioned, &legacy).expect("inventory recovery files");
    let versioned_root_bytes = fs::metadata(&versioned)
        .expect("stat versioned root")
        .len()
        .max(MIN_DIRECTORY_BYTES);
    let legacy_root_bytes = fs::metadata(&legacy)
        .expect("stat legacy root")
        .len()
        .max(MIN_DIRECTORY_BYTES);
    let versioned_lock_bytes = fs::metadata(versioned.join(".checkpoint.lock"))
        .expect("stat versioned lock")
        .len();
    let legacy_lock_bytes = fs::metadata(legacy.join(".sheets-writer.lock"))
        .expect("stat legacy lock")
        .len();
    assert_eq!(
        usage.versioned_bytes,
        versioned_root_bytes
            + versioned_lock_bytes
            + b"journal".len() as u64
            + b"staging".len() as u64
            + MIN_DIRECTORY_BYTES
            + b"next".len() as u64
    );
    assert_eq!(
        usage.legacy_bytes,
        legacy_root_bytes + legacy_lock_bytes + b"legacy bytes".len() as u64
    );
    assert_eq!(
        usage.total_bytes().expect("total recovery bytes"),
        versioned_root_bytes
            + versioned_lock_bytes
            + legacy_root_bytes
            + legacy_lock_bytes
            + b"journal".len() as u64
            + b"staging".len() as u64
            + MIN_DIRECTORY_BYTES
            + b"next".len() as u64
            + b"legacy bytes".len() as u64
    );
}

#[test]
fn filesystem_inventory_fails_closed_on_unknown_legacy_entries() {
    let fixture = TestDirectory::new();
    let versioned = fixture.path().join("versioned");
    let legacy = fixture.path().join("legacy");
    fs::create_dir_all(&versioned).expect("create versioned directory");
    fs::create_dir_all(&legacy).expect("create legacy directory");
    fs::write(legacy.join("unrecognized.bin"), b"do not omit").expect("write unknown file");

    let error = scan_recovery_storage(&versioned, &legacy)
        .expect_err("unknown legacy entries must prevent incomplete accounting");
    assert!(error.contains("unsupported"), "unexpected error: {error}");
}

#[test]
fn versioned_inventory_stops_at_the_entry_limit() {
    let fixture = TestDirectory::new();
    let versioned = fixture.path().join("versioned");
    let legacy = fixture.path().join("legacy");
    fs::create_dir_all(&versioned).expect("create versioned directory");
    fs::create_dir_all(&legacy).expect("create legacy directory");
    for index in 0..10_001 {
        fs::File::create(versioned.join(format!("entry-{index:05}")))
            .expect("create versioned entry");
    }

    let error = preflight_existing_storage(&versioned, &legacy, APPROVED_RECOVERY_LIMITS)
        .expect_err("inventory must stop when the entry limit is exceeded");

    assert!(error.contains("entry count"), "unexpected error: {error}");
}

#[test]
fn legacy_inventory_stops_at_the_entry_limit_before_collecting_unknown_names() {
    let fixture = TestDirectory::new();
    let versioned = fixture.path().join("versioned");
    let legacy = fixture.path().join("legacy");
    fs::create_dir_all(&versioned).expect("create versioned directory");
    fs::create_dir_all(&legacy).expect("create legacy directory");
    for index in 0..10_001 {
        fs::File::create(legacy.join(format!("unknown-{index:05}")))
            .expect("create unknown legacy entry");
    }

    let error = preflight_existing_storage(&versioned, &legacy, APPROVED_RECOVERY_LIMITS)
        .expect_err("legacy scan must stop when the entry limit is exceeded");

    assert!(error.contains("entry count"), "unexpected error: {error}");
}

#[test]
fn versioned_inventory_stops_at_the_directory_depth_limit() {
    let fixture = TestDirectory::new();
    let versioned = fixture.path().join("versioned");
    let legacy = fixture.path().join("legacy");
    fs::create_dir_all(&versioned).expect("create versioned directory");
    fs::create_dir_all(&legacy).expect("create legacy directory");
    let mut nested = versioned.join("nested");
    fs::create_dir(&nested).expect("create first nested directory");
    for _ in 0..33 {
        nested = nested.join("d");
        fs::create_dir(&nested).expect("create nested directory");
    }
    fs::write(nested.join("leaf"), b"").expect("create nested leaf");

    let error = preflight_existing_storage(&versioned, &legacy, APPROVED_RECOVERY_LIMITS)
        .expect_err("inventory must stop when directory depth is exceeded");

    assert!(error.contains("depth"), "unexpected error: {error}");
}

#[cfg(unix)]
#[test]
fn filesystem_inventory_fails_closed_on_symlinks_and_special_files() {
    use std::os::unix::fs::symlink;
    use std::os::unix::net::UnixListener;

    let fixture = TestDirectory::new();
    let versioned = fixture.path().join("versioned");
    let legacy = fixture.path().join("legacy");
    fs::create_dir_all(&versioned).expect("create versioned directory");
    fs::create_dir_all(&legacy).expect("create legacy directory");
    let target = fixture.path().join("outside.bin");
    fs::write(&target, b"outside").expect("write symlink target");
    symlink(&target, versioned.join("linked.bin")).expect("create recovery symlink");
    let error = scan_recovery_storage(&versioned, &legacy)
        .expect_err("versioned symlink must not be followed");
    assert!(error.contains("symlink"), "unexpected error: {error}");

    fs::remove_file(versioned.join("linked.bin")).expect("remove symlink");
    let _socket = UnixListener::bind(versioned.join("special.sock"))
        .expect("create non-regular recovery entry");
    let error = scan_recovery_storage(&versioned, &legacy)
        .expect_err("non-regular versioned entries must fail closed");
    assert!(error.contains("unsupported"), "unexpected error: {error}");
}

#[cfg(unix)]
#[test]
fn root_lock_symlinks_and_non_regular_entries_fail_closed() {
    use std::os::unix::fs::symlink;

    for store in ["versioned", "legacy"] {
        for lock_name in LOCK_FILES {
            for obstruction in ["symlink", "directory"] {
                let fixture = TestDirectory::new();
                let versioned = fixture.path().join("versioned");
                let legacy = fixture.path().join("legacy");
                fs::create_dir_all(&versioned).expect("create versioned directory");
                fs::create_dir_all(&legacy).expect("create legacy directory");
                let recovery_directory = match store {
                    "versioned" => &versioned,
                    "legacy" => &legacy,
                    _ => unreachable!("store case is fixed above"),
                };
                let lock_path = recovery_directory.join(lock_name);

                match obstruction {
                    "symlink" => {
                        let target = fixture.path().join("lock-target");
                        fs::write(&target, b"target").expect("write lock symlink target");
                        symlink(&target, &lock_path).expect("create lock symlink");
                    }
                    "directory" => {
                        fs::create_dir(&lock_path).expect("create non-regular lock entry");
                    }
                    _ => unreachable!("obstruction case is fixed above"),
                }

                let error = scan_recovery_storage(&versioned, &legacy)
                    .expect_err("root lock obstruction must fail closed");
                assert!(
                    error.contains(lock_name) && error.contains("not a regular file"),
                    "{store}/{lock_name} {obstruction} returned unexpected error: {error}"
                );
            }
        }
    }
}
