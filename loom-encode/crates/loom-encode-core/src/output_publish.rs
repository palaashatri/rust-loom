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
