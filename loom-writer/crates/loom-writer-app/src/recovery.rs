use loom_production::define_snapshot_recovery;
use loom_writer_core::WriterDocument;

define_snapshot_recovery!(
    application_id: "org.loom.writer",
    schema: "loom.writer.package/1"
);

/// Start recovery for every interactive Writer session. An explicitly opened
/// file wins over an older draft, but the recovery store remains active so new
/// edits to that file are still protected.
pub(super) fn initialize_editing_session(
    command_line_open: bool,
) -> Result<Option<WriterDocument>, String> {
    let recovered_payload = initialize_snapshot_recovery()?;
    if command_line_open {
        return Ok(None);
    }
    Ok(recovered_payload.and_then(|payload| loom_writer_core::load_document(&payload).ok()))
}

/// Record the current complete document after a user edit.
pub(super) fn record_document(document: &WriterDocument) -> Result<(), String> {
    let payload = loom_writer_core::save_document(document)
        .map_err(|error| format!("could not prepare recovery data: {error}"))?;
    record_snapshot_recovery("writer state", payload)
}

/// Checkpoint the recovery store after the document has been saved to disk.
pub(super) fn checkpoint_document(document: &WriterDocument) -> Result<(), String> {
    let payload = loom_writer_core::save_document(document)
        .map_err(|error| format!("could not prepare recovery checkpoint: {error}"))?;
    checkpoint_snapshot_recovery(payload)
}
