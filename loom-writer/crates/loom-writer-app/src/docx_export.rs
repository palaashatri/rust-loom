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
    format!("{}.docx", file_stem(title))
}

/// The document title made safe for a file system, without an extension.
pub(crate) fn file_stem(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '-',
            c if c.is_control() => ' ',
            c => c,
        })
        .collect();
    let cleaned = cleaned.trim().trim_end_matches('.').trim();
    if cleaned.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        cleaned.to_string()
    }
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

/// Status line after a successful export. The warning stays ahead of the
/// destination, so an eliding status bar keeps the consequential part visible.
pub(crate) fn success_message(path: &Path, export: &DocxExport) -> String {
    if export.comments_skipped > 0 {
        let count = export.comments_skipped;
        let omitted = if count == 1 {
            "comment was left out"
        } else {
            "comments were left out"
        };
        return format!("Warning: {count} {omitted}. Exported {}", path.display());
    }
    format!("Exported {}", path.display())
}

/// Full content-loss report shown in a bounded, scrollable dialog. Explicit
/// line breaks keep even a single long path component readable at narrow widths.
pub(crate) fn report_message(path: &Path, export: &DocxExport) -> String {
    let count = export.comments_skipped;
    let omitted = if count == 1 {
        "comment could not be placed in Word and was left out"
    } else {
        "comments could not be placed in Word and were left out"
    };
    let mut message = format!("{count} {omitted}.");
    if export.comments_exported > 0 {
        let included = export.comments_exported;
        let other_comments = if included == 1 {
            "other comment was included"
        } else {
            "other comments were included"
        };
        message.push_str(&format!("\n{included} {other_comments}."));
    }
    message.push_str("\n\nSaved to:\n");
    message.push_str(&readable_path(path));
    message
}

fn readable_path(path: &Path) -> String {
    const MAX_LINE_CHARS: usize = 48;

    let mut readable = String::new();
    let mut line_chars = 0;
    for character in path.display().to_string().chars() {
        readable.push(character);
        line_chars += 1;
        if character == std::path::MAIN_SEPARATOR || line_chars >= MAX_LINE_CHARS {
            readable.push('\n');
            line_chars = 0;
        }
    }
    if readable.ends_with('\n') {
        readable.pop();
    }
    readable
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
                if export.comments_skipped > 0 {
                    app.set_docx_export_report_message(SharedString::from(report_message(
                        &path, &export,
                    )));
                    app.set_docx_export_report_open(true);
                } else {
                    app.set_docx_export_report_open(false);
                    app.set_docx_export_report_message("".into());
                }
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
