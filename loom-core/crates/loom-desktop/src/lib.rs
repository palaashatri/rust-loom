//! Native desktop service contracts and adapters for Loom applications.
//!
//! This crate keeps platform dialogs behind an injectable interface so the
//! production applications can use native system dialogs while deterministic
//! tests use a scripted backend without opening windows.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::VecDeque;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A display name and extension list presented by a native file dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileFilter {
    /// Human-readable filter name, such as `Loom Writer document`.
    pub name: String,
    /// Extensions without a leading dot, such as `loomdoc` or `pdf`.
    pub extensions: Vec<String>,
}

impl FileFilter {
    /// Creates a validated file filter.
    pub fn new(
        name: impl Into<String>,
        extensions: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self, DesktopError> {
        let name = name.into().trim().to_string();
        if name.is_empty() {
            return Err(DesktopError::InvalidRequest(
                "file-filter name must not be empty".into(),
            ));
        }
        let extensions = extensions
            .into_iter()
            .map(Into::into)
            .map(|extension: String| extension.trim_start_matches('.').to_ascii_lowercase())
            .collect::<Vec<_>>();
        if extensions.is_empty()
            || extensions.iter().any(|extension| {
                extension.is_empty()
                    || !extension.bytes().all(|byte| {
                        byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
                    })
            })
        {
            return Err(DesktopError::InvalidRequest(
                "file-filter extensions must be non-empty and contain only ASCII letters, digits, '-' or '_'"
                    .into(),
            ));
        }
        Ok(Self { name, extensions })
    }
}

/// Request for opening one existing file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenFileRequest {
    /// Native dialog title.
    pub title: String,
    /// Initial directory when known.
    pub initial_directory: Option<PathBuf>,
    /// Optional suggested file name.
    pub suggested_name: Option<String>,
    /// Allowed file types.
    pub filters: Vec<FileFilter>,
}

/// Request for choosing an output file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SaveFileRequest {
    /// Native dialog title.
    pub title: String,
    /// Initial directory when known.
    pub initial_directory: Option<PathBuf>,
    /// Suggested output file name.
    pub suggested_name: Option<String>,
    /// Allowed output file types.
    pub filters: Vec<FileFilter>,
}

/// Desktop-service failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DesktopError {
    /// A request was malformed.
    InvalidRequest(String),
    /// A deterministic test script did not contain another response.
    ScriptExhausted(&'static str),
}

impl fmt::Display for DesktopError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "invalid desktop request: {message}"),
            Self::ScriptExhausted(operation) => {
                write!(formatter, "scripted desktop service exhausted during {operation}")
            }
        }
    }
}

impl std::error::Error for DesktopError {}

/// Injectable native file-dialog interface.
pub trait FileDialogService: Send + Sync {
    /// Opens a platform file picker. `Ok(None)` means the user cancelled.
    fn open_file(&self, request: &OpenFileRequest) -> Result<Option<PathBuf>, DesktopError>;

    /// Opens a platform save picker. `Ok(None)` means the user cancelled.
    fn save_file(&self, request: &SaveFileRequest) -> Result<Option<PathBuf>, DesktopError>;
}

/// Production file dialogs implemented through the operating system.
#[derive(Debug, Default, Clone, Copy)]
pub struct NativeFileDialogs;

impl FileDialogService for NativeFileDialogs {
    fn open_file(&self, request: &OpenFileRequest) -> Result<Option<PathBuf>, DesktopError> {
        validate_open_request(request)?;
        Ok(configure_open_dialog(request).pick_file())
    }

    fn save_file(&self, request: &SaveFileRequest) -> Result<Option<PathBuf>, DesktopError> {
        validate_save_request(request)?;
        Ok(configure_save_dialog(request).save_file())
    }
}

fn configure_open_dialog(request: &OpenFileRequest) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if !request.title.trim().is_empty() {
        dialog = dialog.set_title(request.title.trim());
    }
    if let Some(directory) = request.initial_directory.as_deref() {
        dialog = dialog.set_directory(directory);
    }
    if let Some(name) = request.suggested_name.as_deref() {
        dialog = dialog.set_file_name(name);
    }
    for filter in &request.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        dialog = dialog.add_filter(&filter.name, &extensions);
    }
    dialog
}

fn configure_save_dialog(request: &SaveFileRequest) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new();
    if !request.title.trim().is_empty() {
        dialog = dialog.set_title(request.title.trim());
    }
    if let Some(directory) = request.initial_directory.as_deref() {
        dialog = dialog.set_directory(directory);
    }
    if let Some(name) = request.suggested_name.as_deref() {
        dialog = dialog.set_file_name(name);
    }
    for filter in &request.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        dialog = dialog.add_filter(&filter.name, &extensions);
    }
    dialog
}

fn validate_open_request(request: &OpenFileRequest) -> Result<(), DesktopError> {
    validate_initial_directory(request.initial_directory.as_deref())?;
    validate_suggested_name(request.suggested_name.as_deref())
}

fn validate_save_request(request: &SaveFileRequest) -> Result<(), DesktopError> {
    validate_initial_directory(request.initial_directory.as_deref())?;
    validate_suggested_name(request.suggested_name.as_deref())
}

fn validate_initial_directory(directory: Option<&Path>) -> Result<(), DesktopError> {
    if directory.is_some_and(|directory| directory.as_os_str().is_empty()) {
        return Err(DesktopError::InvalidRequest(
            "initial directory must not be an empty path".into(),
        ));
    }
    Ok(())
}

fn validate_suggested_name(name: Option<&str>) -> Result<(), DesktopError> {
    if let Some(name) = name {
        let name = name.trim();
        if name.is_empty() {
            return Err(DesktopError::InvalidRequest(
                "suggested file name must not be empty".into(),
            ));
        }
        if Path::new(name).components().count() != 1 {
            return Err(DesktopError::InvalidRequest(
                "suggested file name must not contain a directory".into(),
            ));
        }
    }
    Ok(())
}

/// Deterministic dialog backend for unit and application-controller tests.
#[derive(Debug, Default)]
pub struct ScriptedFileDialogs {
    open_results: Mutex<VecDeque<Option<PathBuf>>>,
    save_results: Mutex<VecDeque<Option<PathBuf>>>,
}

impl ScriptedFileDialogs {
    /// Creates a backend with ordered open and save responses.
    pub fn new(
        open_results: impl IntoIterator<Item = Option<PathBuf>>,
        save_results: impl IntoIterator<Item = Option<PathBuf>>,
    ) -> Self {
        Self {
            open_results: Mutex::new(open_results.into_iter().collect()),
            save_results: Mutex::new(save_results.into_iter().collect()),
        }
    }
}

impl FileDialogService for ScriptedFileDialogs {
    fn open_file(&self, request: &OpenFileRequest) -> Result<Option<PathBuf>, DesktopError> {
        validate_open_request(request)?;
        self.open_results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .ok_or(DesktopError::ScriptExhausted("open_file"))
    }

    fn save_file(&self, request: &SaveFileRequest) -> Result<Option<PathBuf>, DesktopError> {
        validate_save_request(request)?;
        self.save_results
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .pop_front()
            .ok_or(DesktopError::ScriptExhausted("save_file"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_normalizes_dotted_extensions() {
        let filter = FileFilter::new("Writer", [".LOOMDOC", "md"]).expect("valid filter");
        assert_eq!(filter.extensions, ["loomdoc", "md"]);
    }

    #[test]
    fn filter_rejects_path_like_extensions() {
        assert!(matches!(
            FileFilter::new("Invalid", ["../loomdoc"]),
            Err(DesktopError::InvalidRequest(_))
        ));
    }

    #[test]
    fn scripted_dialogs_preserve_cancel_and_paths() {
        let dialogs = ScriptedFileDialogs::new(
            [Some(PathBuf::from("draft.loomdoc")), None],
            [Some(PathBuf::from("export.pdf"))],
        );
        let open = OpenFileRequest {
            title: "Open".into(),
            filters: vec![FileFilter::new("Writer", ["loomdoc"]).expect("filter")],
            ..OpenFileRequest::default()
        };
        let save = SaveFileRequest {
            title: "Export".into(),
            suggested_name: Some("document.pdf".into()),
            filters: vec![FileFilter::new("PDF", ["pdf"]).expect("filter")],
            ..SaveFileRequest::default()
        };

        assert_eq!(
            dialogs.open_file(&open).expect("first open"),
            Some(PathBuf::from("draft.loomdoc"))
        );
        assert_eq!(dialogs.open_file(&open).expect("cancelled open"), None);
        assert_eq!(
            dialogs.save_file(&save).expect("save"),
            Some(PathBuf::from("export.pdf"))
        );
        assert_eq!(
            dialogs.save_file(&save),
            Err(DesktopError::ScriptExhausted("save_file"))
        );
    }

    #[test]
    fn suggested_name_cannot_escape_the_dialog_directory() {
        let dialogs = ScriptedFileDialogs::default();
        let request = SaveFileRequest {
            suggested_name: Some("../escape.loomdoc".into()),
            ..SaveFileRequest::default()
        };
        assert!(matches!(
            dialogs.save_file(&request),
            Err(DesktopError::InvalidRequest(_))
        ));
    }
}
