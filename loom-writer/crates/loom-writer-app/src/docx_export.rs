//! "Export Word Document (.docx)": the save dialog, the write and the
//! status messages. The conversion itself lives in `loom_writer_core`.

use std::path::{Path, PathBuf};

use loom_command::{CommandInvocation, InvocationSource};
use loom_desktop::{FileFilter, SaveFileRequest};
use loom_writer_core::{export_docx, DocxExport, WriterDocument};
use slint::SharedString;

use super::{initial_directory, GuiState, WriterApp};

/// Command id shared by the menu, palette and toolbar surfaces.
pub(crate) const COMMAND_ID: &str = "file.export_docx";

const FALLBACK_NAME: &str = "loom-writer-export";

/// File name offered by the save dialog: the document title made safe for a
/// file system, with the `.docx` extension.
pub(crate) fn suggested_name(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    let stem = if cleaned.is_empty() {
        FALLBACK_NAME
    } else {
        cleaned
    };
    format!("{stem}.docx")
}

fn export_request(state: &GuiState) -> Result<SaveFileRequest, String> {
    let filter = FileFilter::new("Word document", ["docx"]).map_err(|error| error.to_string())?;
    Ok(SaveFileRequest {
        title: "Export Loom Writer Word Document".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(suggested_name(&state.current.borrow().title)),
        filters: vec![filter],
    })
}

/// Writes `doc` to `path` (adding `.docx` when the user typed no extension)
/// and returns the path written together with the export report.
pub(crate) fn export_docx_file(
    path: &Path,
    doc: &WriterDocument,
) -> Result<(PathBuf, DocxExport), String> {
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err("Word destination is empty".into());
    }
    let path = if path.extension().is_none() {
        path.with_extension("docx")
    } else {
        path.to_path_buf()
    };
    let export = export_docx(doc).map_err(|error| error.to_string())?;
    loom_storage::atomic_write(&path, &export.bytes)
        .map_err(|error| format!("atomic write {}: {error}", path.display()))?;
    Ok((path, export))
}

/// Status line after a successful export. Comments that Word cannot anchor
/// are reported, not silently dropped.
pub(crate) fn success_message(path: &Path, export: &DocxExport) -> String {
    let mut message = format!("Exported {}", path.display());
    if export.comments_skipped > 0 {
        let count = export.comments_skipped;
        message.push_str(&format!(
            " ({count} comment{} could not be placed and {} left out)",
            if count == 1 { "" } else { "s" },
            if count == 1 { "was" } else { "were" }
        ));
    }
    message
}

/// Handler of the `export-docx` callback.
pub(crate) fn run(app: &WriterApp, state: &GuiState) {
    let allowed = state
        .registry
        .lock()
        .unwrap()
        .invoke(&CommandInvocation::new(COMMAND_ID, InvocationSource::Menu));
    if allowed.is_err() {
        app.set_status_right("Add content before exporting a Word document".into());
        return;
    }
    let request = match export_request(state) {
        Ok(request) => request,
        Err(error) => {
            app.set_status_left(SharedString::from(format!("Export failed: {error}")));
            return;
        }
    };
    match state.dialogs.save_file(&request) {
        Ok(Some(path)) => match export_docx_file(&path, &state.current.borrow()) {
            Ok((path, export)) => {
                app.set_status_left(SharedString::from(success_message(&path, &export)));
            }
            Err(error) => {
                app.set_status_left(SharedString::from(format!("Export failed: {error}")));
            }
        },
        Ok(None) => app.set_status_left("Export cancelled".into()),
        Err(error) => {
            app.set_status_left(SharedString::from(format!("Export dialog failed: {error}")))
        }
    }
}
