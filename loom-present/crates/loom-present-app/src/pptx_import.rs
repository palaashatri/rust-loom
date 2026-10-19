//! Shared PowerPoint loading and loss inspection for picker and startup opens.

use std::path::{Path, PathBuf};

use loom_present_core::{
    import_pptx, inspect_pptx_unsupported, load_presentation_session, PresentationSession,
};

/// An imported PowerPoint deck and the losses the user needs to review.
#[derive(Debug, Clone)]
pub(crate) struct Candidate {
    pub(crate) path: PathBuf,
    pub(crate) session: PresentationSession,
    pub(crate) warnings: Vec<String>,
}

/// Imports a PowerPoint file once and keeps its warning data with the candidate.
pub(crate) fn prepare(path: &Path) -> Result<Candidate, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("failed to read presentation '{}': {error}", path.display()))?;
    let warnings = inspect_pptx_unsupported(&bytes)?;
    let mut session = import_pptx(&bytes).map(PresentationSession::new)?;
    session.document.active_index = 0;
    Ok(Candidate {
        path: path.to_path_buf(),
        session,
        warnings,
    })
}

/// Loads a Loom deck or PowerPoint file for paths that do not need a warning.
pub(crate) fn load_session(path: &Path) -> Result<PresentationSession, String> {
    let is_pptx = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pptx"));
    let mut session = if is_pptx {
        prepare(path)?.session
    } else {
        let bytes = std::fs::read(path).map_err(|error| {
            format!("failed to read presentation '{}': {error}", path.display())
        })?;
        load_presentation_session(&bytes)
            .map_err(|error| format!("failed to load presentation '{}': {error}", path.display()))?
    };
    session.document.active_index = 0;
    Ok(session)
}

pub(crate) fn is_pptx_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pptx"))
}

pub(crate) fn warning_message(warnings: &[String]) -> String {
    format!(
        "This PowerPoint presentation contains elements that Loom Present cannot fully edit:\n\n• {}\n\nDo you want to continue importing?",
        warnings.join("\n• ")
    )
}

pub(crate) fn warning_message_with_error(warnings: &[String], error: &str) -> String {
    format!("{error}\n\n{}", warning_message(warnings))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_inspection_error_is_the_same_fatal_package_error_as_import() {
        let directory = std::env::temp_dir().join(format!(
            "loom-present-unreadable-pptx-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).expect("create fixture directory");
        let path = directory.join("invalid.pptx");
        std::fs::write(&path, b"not a zip archive").expect("write invalid PPTX");
        let bytes = std::fs::read(&path).expect("read invalid PPTX");

        let inspection_error = inspect_pptx_unsupported(&bytes).expect_err("inspection fails");
        let import_error = import_pptx(&bytes).expect_err("import fails too");
        assert_eq!(inspection_error, import_error);
        assert_eq!(
            prepare(&path).expect_err("preparation must fail"),
            inspection_error
        );

        let _ = std::fs::remove_dir_all(directory);
    }
}
