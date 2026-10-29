//! DOCX import warning dialog and clipboard chart embedding actions.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use loom_writer_core::{import_docx, inspect_docx_unsupported, WriterDocument};
use slint::{ComponentHandle, SharedString};

use crate::{
    apply_with_history, replace_opened_document, sync_writer_menu_if_present, GuiState,
    HistoryKind, NativeMenuBar, WriterApp,
};

#[derive(Clone)]
pub struct PendingDocxImport {
    pub path: PathBuf,
    pub document: WriterDocument,
    pub guard_before_replace: bool,
}

/// A DOCX that has been read, preflighted, and imported once for a single open flow.
pub(crate) struct PreparedDocxImport {
    pub(crate) path: PathBuf,
    pub(crate) document: WriterDocument,
    pub(crate) warnings: Vec<String>,
}

/// Reads a DOCX once and fails closed if unsupported-content inspection cannot complete.
pub(crate) fn prepare_docx_import(path: &Path) -> Result<PreparedDocxImport, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    prepare_docx_bytes_with(path, &bytes, inspect_docx_unsupported, import_docx)
}

fn prepare_docx_bytes_with(
    path: &Path,
    bytes: &[u8],
    inspect: impl FnOnce(&[u8]) -> Result<Vec<String>, String>,
    import: impl FnOnce(&[u8]) -> Result<WriterDocument, String>,
) -> Result<PreparedDocxImport, String> {
    let warnings =
        inspect(bytes).map_err(|error| format!("inspect docx {}: {error}", path.display()))?;
    let document =
        import(bytes).map_err(|error| format!("import docx {}: {error}", path.display()))?;
    Ok(PreparedDocxImport {
        path: path.to_path_buf(),
        document,
        warnings,
    })
}

/// Builds the startup baseline and retains one prepared import for the GUI handoff.
pub(crate) fn startup_documents_with_prepared_docx(
    recovered: Option<crate::recovery_draft::RecoveredDraft>,
    open: Option<&str>,
    template: Option<crate::TemplateId>,
) -> Result<(WriterDocument, WriterDocument, Option<PreparedDocxImport>), String> {
    let baseline = open
        .map(std::path::PathBuf::from)
        .or_else(|| recovered.as_ref().and_then(|draft| draft.source.clone()));
    let mut prepared_docx = None;
    let fresh = match baseline {
        None => template
            .map(crate::template_document)
            .unwrap_or_else(crate::blank_startup_document),
        Some(path) => {
            if path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("docx"))
            {
                let prepared = prepare_docx_import(&path)?;
                let fresh = if prepared.warnings.is_empty() {
                    prepared.document.clone()
                } else {
                    template
                        .map(crate::template_document)
                        .unwrap_or_else(crate::blank_startup_document)
                };
                prepared_docx = Some(prepared);
                fresh
            } else {
                match crate::load_file(&path) {
                    Ok(document) => document,
                    // A draft whose file has since disappeared still opens and reads as
                    // unsaved; saving recreates the file.
                    Err(_)
                        if recovered
                            .as_ref()
                            .is_some_and(|draft| draft.source.is_some()) =>
                    {
                        template
                            .map(crate::template_document)
                            .unwrap_or_else(crate::blank_startup_document)
                    }
                    Err(error) => return Err(error),
                }
            }
        }
    };
    let (document, saved) = match recovered {
        Some(draft) => (draft.document, fresh),
        None => (fresh.clone(), fresh),
    };
    Ok((document, saved, prepared_docx))
}

/// Starts or stages an already prepared DOCX import without reading it again.
pub(crate) fn open_prepared_docx_import(
    app: &WriterApp,
    state: &GuiState,
    prepared: PreparedDocxImport,
    menu_service: &Option<Arc<NativeMenuBar>>,
    guard_before_replace: bool,
) -> bool {
    let warning_message = if prepared.warnings.is_empty() {
        None
    } else {
        Some(format!(
            "This Word document contains elements that Loom Writer cannot fully edit:\n\n• {}\n\nDo you want to continue importing?",
            prepared.warnings.join("\n• ")
        ))
    };
    let path = prepared.path.clone();
    PENDING_DOCX_IMPORT.with(|pending| {
        *pending.borrow_mut() = Some(PendingDocxImport {
            path,
            document: prepared.document,
            guard_before_replace,
        });
    });

    if let Some(message) = warning_message {
        app.set_docx_import_warning_message(SharedString::from(message));
        app.set_docx_import_warning_open(true);
        app.set_status_left("Review the Word import warning before replacing this document".into());
        false
    } else {
        if guard_before_replace && crate::request_docx_import_replacement(app, state) {
            return false;
        }
        apply_pending_docx_import(app, state, menu_service);
        true
    }
}

/// Applies the staged DOCX after any required unsaved-change decision has completed.
pub(crate) fn apply_pending_docx_import(
    app: &WriterApp,
    state: &GuiState,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    app.set_docx_import_warning_open(false);
    let pending = PENDING_DOCX_IMPORT.with(|pending| pending.borrow_mut().take());
    if let Some(pending) = pending {
        replace_opened_document(app, state, pending.path.clone(), pending.document);
        sync_writer_menu_if_present(menu_service, app, state);
        app.set_status_left(SharedString::from(format!(
            "Opened {}",
            pending.path.display()
        )));
        app.invoke_focus_page();
    }
}

pub(crate) fn clear_pending_docx_import() {
    PENDING_DOCX_IMPORT.with(|pending| *pending.borrow_mut() = None);
}

thread_local! {
    pub static PENDING_DOCX_IMPORT: RefCell<Option<PendingDocxImport>> = const { RefCell::new(None) };
}

/// Stages or opens a DOCX document at `path`.
/// If unsupported elements are detected, opens the warning dialog without replacing the current document.
pub fn open_docx_path(
    app: &WriterApp,
    state: &GuiState,
    path: PathBuf,
    menu_service: &Option<Arc<NativeMenuBar>>,
) -> Result<bool, String> {
    let prepared = prepare_docx_import(&path)?;
    Ok(open_prepared_docx_import(
        app,
        state,
        prepared,
        menu_service,
        false,
    ))
}

/// Cancels the pending DOCX import. Leaves current document and recovery untouched.
pub fn cancel_docx_import(app: &WriterApp) {
    app.set_docx_import_warning_open(false);
    clear_pending_docx_import();
    app.set_status_left("Word import cancelled".into());
    app.invoke_focus_page();
}

/// Continues the pending DOCX import, replacing the document after any startup recovery guard.
pub fn continue_docx_import(
    app: &WriterApp,
    state: &GuiState,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    app.set_docx_import_warning_open(false);
    let guard_before_replace = PENDING_DOCX_IMPORT.with(|pending| {
        pending
            .borrow()
            .as_ref()
            .is_some_and(|pending| pending.guard_before_replace)
    });
    if guard_before_replace && crate::request_docx_import_replacement(app, state) {
        return;
    }
    apply_pending_docx_import(app, state, menu_service);
}

/// Inserts a chart block from the system clipboard into the current document.
pub fn insert_chart_from_clipboard(app: &WriterApp, state: &GuiState) -> bool {
    let Ok(clipboard) = loom_clipboard::read_clipboard() else {
        app.set_status_left("Clipboard unavailable".into());
        return false;
    };
    let Some(chart) = clipboard.to_chart() else {
        app.set_status_left("Clipboard does not contain a chart".into());
        return false;
    };
    let mut next = state.current.borrow().clone();
    let caret = next.selection().anchor;
    let Some(id) = next.insert_chart_block(caret, chart.clone()) else {
        app.set_status_left("Failed to insert chart".into());
        return false;
    };
    // The caret goes to the start of the paragraph after the chart.
    let mut after = 0usize;
    for block in &next.blocks {
        after += block.text.len_bytes() + 1;
        if block.id == id {
            break;
        }
    }
    next.set_selection(loom_writer_core::TextSelection::caret(
        after.min(next.editor_text().len()),
    ));
    if !apply_with_history(app, state, next, HistoryKind::DocumentAction) {
        return false;
    }
    let title_display = chart.title.as_deref().unwrap_or("Untitled Chart");
    app.set_status_left(SharedString::from(format!(
        "Inserted chart: {title_display}"
    )));
    true
}

/// Wires docx import and chart insertion callbacks.
pub fn wire(app: &WriterApp, state: &Rc<GuiState>, menu_service: &Option<Arc<NativeMenuBar>>) {
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        let menu_service = menu_service.clone();
        app.on_docx_import_continue(move || {
            let Some(app) = app_ref.upgrade() else { return };
            continue_docx_import(&app, &state, &menu_service);
        });
    }
    {
        let app_ref = app.as_weak();
        app.on_docx_import_cancel(move || {
            let Some(app) = app_ref.upgrade() else { return };
            cancel_docx_import(&app);
        });
    }
    {
        let state = state.clone();
        let app_ref = app.as_weak();
        app.on_insert_chart(move || {
            let Some(app) = app_ref.upgrade() else { return };
            insert_chart_from_clipboard(&app, &state);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn inspection_failure_stops_before_import() {
        let import_called = Cell::new(false);
        let error = prepare_docx_bytes_with(
            Path::new("inspection-error.docx"),
            b"inspection fails before import",
            |_| Err("injected inspection failure".to_string()),
            |_| {
                import_called.set(true);
                Ok(WriterDocument::new("imported", "Imported"))
            },
        )
        .err()
        .expect("inspection error should be returned");
        assert_eq!(
            error,
            "inspect docx inspection-error.docx: injected inspection failure"
        );
        assert!(
            !import_called.get(),
            "import must not run after inspection fails"
        );
    }
}
