//! "Export Markdown": the save dialog, the write and the status messages. The
//! conversion itself lives in `loom_writer_core`.

use std::path::{Path, PathBuf};

use loom_command::{CommandInvocation, InvocationSource};
use loom_desktop::{FileFilter, SaveFileRequest};
use loom_writer_core::WriterDocument;
use slint::SharedString;

use super::{docx_export, initial_directory, GuiState, WriterApp};

/// Command id shared by the menu, palette and toolbar surfaces.
pub(crate) const COMMAND_ID: &str = "file.export_md";

/// File name offered by the save dialog: the document title made safe for a
/// file system, with the `.md` extension.
pub(crate) fn suggested_name(title: &str) -> String {
    format!("{}.md", docx_export::file_stem(title))
}

fn export_request(state: &GuiState) -> Result<SaveFileRequest, String> {
    let filter = FileFilter::new("Markdown", ["md"]).map_err(|error| error.to_string())?;
    Ok(SaveFileRequest {
        title: "Export Loom Writer Markdown".into(),
        initial_directory: initial_directory(state.save_path.borrow().as_deref()),
        suggested_name: Some(suggested_name(&state.current.borrow().title)),
        filters: vec![filter],
    })
}

/// Writes `doc` to `path` (adding `.md` when the user typed no extension) and
/// returns the path written.
pub(crate) fn export_markdown_file(path: &Path, doc: &WriterDocument) -> Result<PathBuf, String> {
    if path.as_os_str().is_empty() || path.file_name().is_none() {
        return Err("Markdown destination is empty".into());
    }
    let path = if path.extension().is_none() {
        path.with_extension("md")
    } else {
        path.to_path_buf()
    };
    loom_storage::atomic_write(&path, doc.to_markdown().as_bytes())
        .map_err(|error| format!("atomic write {}: {error}", path.display()))?;
    Ok(path)
}

/// Handler of the `export-markdown` callback.
pub(crate) fn run(app: &WriterApp, state: &GuiState) {
    let allowed = state
        .registry
        .lock()
        .unwrap()
        .invoke(&CommandInvocation::new(
            COMMAND_ID,
            InvocationSource::Toolbar,
        ));
    if allowed.is_err() {
        app.set_status_right("Add content before exporting Markdown".into());
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
        Ok(Some(path)) => match export_markdown_file(&path, &state.current.borrow()) {
            Ok(path) => {
                app.set_status_left(SharedString::from(format!("Exported {}", path.display())));
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
