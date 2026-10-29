//! Windows file replacement through the documented `ReplaceFileW` API.
//!
//! Before replacing an existing destination, this module copies its exact bytes
//! to a unique same-directory recovery file. `ReplaceFileW` is used for the
//! actual same-volume replacement. If it reports failure, the destination is
//! compared with the recovery copy and restored when necessary. A failed
//! restoration leaves the recovery copy in place and reports its path.

use std::ffi::c_void;
use std::fs::{self, File, OpenOptions};
use std::io::{self, ErrorKind, Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;
static BACKUP_COUNTER: AtomicU64 = AtomicU64::new(1);

#[link(name = "kernel32")]
extern "system" {
    #[link_name = "ReplaceFileW"]
    fn replace_file_w(
        replaced_file_name: *const u16,
        replacement_file_name: *const u16,
        backup_file_name: *const u16,
        replace_flags: u32,
        exclude: *mut c_void,
        reserved: *mut c_void,
    ) -> i32;

    #[link_name = "MoveFileExW"]
    fn move_file_ex_w(existing_file_name: *const u16, new_file_name: *const u16, flags: u32)
        -> i32;
}

/// Replace an existing destination while retaining its previous bytes on any
/// reported failure. This uses the documented file-replacement API, but does
/// not claim a broader crash-atomic guarantee than Windows documents.
pub(super) fn replace_existing(target: &Path, replacement: &Path) -> io::Result<()> {
    let backup = copy_to_unique_backup(target)?;
    match call_replace_file(target, replacement, None) {
        Ok(()) => {
            // A stale snapshot is safer than returning failure after commit.
            if let Err(error) = fs::remove_file(&backup) {
                eprintln!(
                    "loom-storage: committed replacement but could not remove recovery copy '{}': {error}",
                    backup.display()
                );
            }
            Ok(())
        }
        Err(replace_error) => {
            let original_is_at_target = files_equal(target, &backup).unwrap_or(false);
            if original_is_at_target {
                return Err(replacement_failure(
                    replace_error,
                    "the original bytes remain at the destination",
                    &backup,
                    replacement,
                ));
            }

            match restore_from_backup(target, &backup) {
                Ok(()) => Err(replacement_failure(
                    replace_error,
                    "the original bytes were restored to the destination",
                    &backup,
                    replacement,
                )),
                Err(restore_error) => Err(io::Error::new(
                    ErrorKind::Other,
                    format!(
                        "ReplaceFileW failed for '{}': {replace_error}; restoring the original bytes also failed ({restore_error}). Recovery copy retained at '{}'; staged replacement path was '{}'.",
                        target.display(), backup.display(), replacement.display()
                    ),
                )),
            }
        }
    }
}

fn replacement_failure(
    cause: io::Error,
    recovery: &str,
    backup: &Path,
    replacement: &Path,
) -> io::Error {
    io::Error::new(
        cause.kind(),
        format!(
            "ReplaceFileW failed ({cause}); {recovery}. Recovery copy retained at '{}'; staged replacement path was '{}'.",
            backup.display(), replacement.display()
        ),
    )
}

fn restore_from_backup(target: &Path, backup: &Path) -> io::Result<()> {
    restore_from_backup_with(target, backup, |target, restore_copy| {
        if target.exists() {
            call_replace_file(target, restore_copy, None)
        } else {
            move_file_without_replacing(restore_copy, target)
        }
    })
}

fn restore_from_backup_with<F>(target: &Path, backup: &Path, replace: F) -> io::Result<()>
where
    F: FnOnce(&Path, &Path) -> io::Result<()>,
{
    if !backup.is_file() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            format!("recovery copy '{}' is missing", backup.display()),
        ));
    }

    // Keep the recovery copy untouched while restoring. ReplaceFileW can have
    // documented failure states that move names, so the copy remains available
    // if this second operation also fails.
    let restore_copy = unique_backup_path(target)?;
    if let Err(error) = fs::copy(backup, &restore_copy) {
        let _ = fs::remove_file(&restore_copy);
        return Err(io::Error::new(
            error.kind(),
            format!(
                "could not prepare restore copy from '{}': {error}",
                backup.display()
            ),
        ));
    }
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&restore_copy)
        .and_then(|file| file.sync_all())
        .map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "could not sync restore copy '{}'; recovery copy retained at '{}': {error}",
                    restore_copy.display(),
                    backup.display()
                ),
            )
        })?;

    let result = replace(target, &restore_copy);
    match result {
        Ok(()) => {
            if restore_copy.exists() {
                if let Err(error) = fs::remove_file(&restore_copy) {
                    eprintln!(
                        "loom-storage: restored original bytes but could not remove restore copy '{}': {error}",
                        restore_copy.display()
                    );
                }
            }
            Ok(())
        }
        Err(error) => Err(io::Error::new(
            error.kind(),
            format!(
                "restore from '{}' failed; recovery copies are retained at '{}' and '{}': {error}",
                backup.display(),
                backup.display(),
                restore_copy.display()
            ),
        )),
    }
}

fn call_replace_file(target: &Path, replacement: &Path, backup: Option<&Path>) -> io::Result<()> {
    let target_wide = nul_terminated_wide(target)?;
    let replacement_wide = nul_terminated_wide(replacement)?;
    let backup_wide = backup.map(nul_terminated_wide).transpose()?;
    let backup_ptr = backup_wide
        .as_ref()
        .map_or(std::ptr::null(), |path| path.as_ptr());

    // SAFETY: every non-null path pointer refers to a NUL-terminated buffer
    // retained for the duration of the call. The replacement and optional
    // backup are siblings of the destination, satisfying the same-volume
    // requirement. The documented reserved parameters are null.
    let result = unsafe {
        replace_file_w(
            target_wide.as_ptr(),
            replacement_wide.as_ptr(),
            backup_ptr,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn move_file_without_replacing(source: &Path, destination: &Path) -> io::Result<()> {
    let source_wide = nul_terminated_wide(source)?;
    let destination_wide = nul_terminated_wide(destination)?;

    // SAFETY: both buffers are NUL-terminated and retained for the call. The
    // destination is absent, and omitting MOVEFILE_REPLACE_EXISTING prevents
    // overwriting a file created concurrently.
    let result = unsafe {
        move_file_ex_w(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn nul_terminated_wide(path: &Path) -> io::Result<Vec<u16>> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "Windows paths cannot contain NUL",
        ));
    }
    wide.push(0);
    Ok(wide)
}

fn copy_to_unique_backup(target: &Path) -> io::Result<PathBuf> {
    let candidate = unique_backup_path(target)?;
    let copy = (|| {
        let mut source = File::open(target)?;
        let mut backup = OpenOptions::new().write(true).open(&candidate)?;
        io::copy(&mut source, &mut backup)?;
        backup.flush()?;
        backup.sync_all()
    })();
    if let Err(error) = copy {
        let _ = fs::remove_file(&candidate);
        return Err(error);
    }
    Ok(candidate)
}

fn unique_backup_path(target: &Path) -> io::Result<PathBuf> {
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    let name = target
        .file_name()
        .ok_or_else(|| io::Error::new(ErrorKind::InvalidInput, "destination has no file name"))?
        .to_string_lossy();

    for _ in 0..32 {
        let count = BACKUP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let candidate = parent.join(format!(
            ".{name}.{}.{}.{}.loom-backup",
            std::process::id(),
            count,
            now
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                drop(file);
                return Ok(candidate);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        ErrorKind::AlreadyExists,
        "could not choose a unique Windows replacement backup path",
    ))
}

fn files_equal(left: &Path, right: &Path) -> io::Result<bool> {
    let mut left = File::open(left)?;
    let mut right = File::open(right)?;
    let mut left_buffer = [0_u8; 8192];
    let mut right_buffer = [0_u8; 8192];
    loop {
        let left_len = left.read(&mut left_buffer)?;
        let right_len = right.read(&mut right_buffer)?;
        if left_len != right_len || left_buffer[..left_len] != right_buffer[..right_len] {
            return Ok(false);
        }
        if left_len == 0 {
            return Ok(true);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{copy_to_unique_backup, files_equal, replace_existing, restore_from_backup_with};
    use std::fs;
    use std::io::{self, ErrorKind};
    use std::path::PathBuf;

    fn temp_dir(tag: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "loom-storage-win-replace-{tag}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        path
    }

    #[test]
    fn replace_file_w_replaces_existing_target_and_cleans_recovery_copy() {
        let dir = temp_dir("success");
        let target = dir.join("document.docx");
        let replacement = dir.join("document.tmp");
        fs::write(&target, b"old bytes").expect("write old target");
        fs::write(&replacement, b"new bytes").expect("write replacement");

        replace_existing(&target, &replacement).expect("replace target");

        assert_eq!(fs::read(&target).expect("read target"), b"new bytes");
        assert!(!replacement.exists());
        assert!(fs::read_dir(&dir)
            .expect("list test directory")
            .all(|entry| !entry
                .expect("read test entry")
                .file_name()
                .to_string_lossy()
                .contains("loom-backup")));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_replace_keeps_original_target_bytes_and_recovery_copy() {
        let dir = temp_dir("failure");
        let target = dir.join("document.docx");
        let missing_replacement = dir.join("missing.tmp");
        fs::write(&target, b"original bytes").expect("write old target");

        let error = replace_existing(&target, &missing_replacement)
            .expect_err("replacement source is missing");

        assert_eq!(
            fs::read(&target).expect("read preserved target"),
            b"original bytes"
        );
        assert!(error.to_string().contains("loom-backup"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_restore_retains_the_original_recovery_copy() {
        let dir = temp_dir("restore-failure");
        let target = dir.join("document.docx");
        fs::write(&target, b"old bytes").expect("write old target");
        let backup = copy_to_unique_backup(&target).expect("make recovery copy");
        fs::write(&target, b"new bytes").expect("simulate failed first replacement");

        let error = restore_from_backup_with(&target, &backup, |_, _| {
            Err(io::Error::new(
                ErrorKind::PermissionDenied,
                "injected restore failure",
            ))
        })
        .expect_err("injected restoration failure");

        assert!(backup.exists(), "recovery copy must be retained");
        assert!(error.to_string().contains(&backup.display().to_string()));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn byte_comparison_checks_the_complete_recovery_copy() {
        let dir = temp_dir("compare");
        let left = dir.join("left");
        let right = dir.join("right");
        fs::write(&left, b"same bytes").expect("write left");
        fs::write(&right, b"same bytes").expect("write right");
        assert!(files_equal(&left, &right).expect("compare files"));
        fs::write(&right, b"different").expect("write different right");
        assert!(!files_equal(&left, &right).expect("compare files"));
        let _ = fs::remove_dir_all(dir);
    }
}
