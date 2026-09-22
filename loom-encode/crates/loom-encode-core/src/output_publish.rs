use super::EncodeError;
use std::path::Path;

/// Publishes a completed encoder result while preserving the selected
/// collision policy at the final filesystem operation.
pub(super) fn commit_encode_output(
    temporary: &Path,
    output: &Path,
    overwrite: bool,
) -> Result<(), EncodeError> {
    if !overwrite {
        // A hard link creates the destination name only when it is absent. The
        // temporary file is a sibling, so this is one same-filesystem publish
        // operation. If a filesystem cannot do that operation, fail closed;
        // copying or renaming after an `exists()` check could replace a new file.
        return match std::fs::hard_link(temporary, output) {
            Ok(()) => {
                let _ = std::fs::remove_file(temporary);
                Ok(())
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(EncodeError::InvalidJob(format!(
                    "output appeared during encoding; refusing to replace {}",
                    output.display()
                )))
            }
            Err(error) => Err(EncodeError::Io(error)),
        };
    }

    let backup = if output.exists() {
        let nonce = super::NEXT_EXEC_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Some(output.with_file_name(format!(
            ".{}.loom-encode-backup-{nonce}",
            output
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("output")
        )))
    } else {
        None
    };
    if let Some(backup) = backup.as_deref() {
        let _ = std::fs::remove_file(backup);
        std::fs::rename(output, backup).map_err(EncodeError::Io)?;
    }
    if let Err(error) = std::fs::rename(temporary, output) {
        if let Some(backup) = backup.as_deref() {
            let _ = std::fs::rename(backup, output);
        }
        return Err(EncodeError::Io(error));
    }
    if let Some(backup) = backup {
        let _ = std::fs::remove_file(backup);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::commit_encode_output;
    use crate::{EncodeError, NEXT_EXEC_TEMP};
    use std::fs;
    use std::path::PathBuf;

    fn output_paths() -> (PathBuf, PathBuf, PathBuf) {
        let id = NEXT_EXEC_TEMP.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let directory =
            std::env::temp_dir().join(format!("loom-encode-publish-{}-{id}", std::process::id()));
        fs::create_dir_all(&directory).expect("create isolated output directory");
        let temporary = directory.join(".result.mp4.loom-encode.tmp");
        let output = directory.join("result.mp4");
        (directory, temporary, output)
    }

    #[test]
    fn no_overwrite_publish_preserves_a_destination_created_during_encoding() {
        let (directory, temporary, output) = output_paths();
        fs::write(&temporary, b"NEWENCODE").unwrap();
        fs::write(&output, b"IMPORTANT_OTHER_FILE").unwrap();

        let error = commit_encode_output(&temporary, &output, false).unwrap_err();
        assert!(matches!(
            error,
            EncodeError::InvalidJob(message) if message.contains("appeared during encoding")
        ));
        assert_eq!(fs::read(&output).unwrap(), b"IMPORTANT_OTHER_FILE");
        assert!(temporary.exists(), "the caller owns temporary-file cleanup");
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn no_overwrite_publish_creates_an_absent_destination() {
        let (directory, temporary, output) = output_paths();
        fs::write(&temporary, b"NEWENCODE").unwrap();

        commit_encode_output(&temporary, &output, false).unwrap();

        assert_eq!(fs::read(&output).unwrap(), b"NEWENCODE");
        assert!(
            !temporary.exists(),
            "successful publication removes the temporary name"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn explicit_overwrite_replaces_an_existing_destination() {
        let (directory, temporary, output) = output_paths();
        fs::write(&temporary, b"NEWENCODE").unwrap();
        fs::write(&output, b"OLDENCODE").unwrap();

        commit_encode_output(&temporary, &output, true).unwrap();

        assert_eq!(fs::read(&output).unwrap(), b"NEWENCODE");
        assert!(
            !temporary.exists(),
            "successful publication removes the temporary name"
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
