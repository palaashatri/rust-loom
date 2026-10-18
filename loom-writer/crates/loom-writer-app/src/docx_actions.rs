//! DOCX import warning dialog and clipboard chart embedding actions.

use std::cell::RefCell;
use std::path::PathBuf;
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
}

thread_local! {
    pub static PENDING_DOCX_IMPORT: RefCell<Option<PendingDocxImport>> = const { RefCell::new(None) };
}

/// Stages or opens a DOCX document at `path`.
/// If unsupported elements are detected, opens the warning dialog without replacing the current document.
/// Otherwise, immediately replaces the document.
pub fn open_docx_path(
    app: &WriterApp,
    state: &GuiState,
    path: PathBuf,
    menu_service: &Option<Arc<NativeMenuBar>>,
) -> Result<bool, String> {
    let bytes =
        std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?;

    let warnings = inspect_docx_unsupported(&bytes).unwrap_or_default();
    let document =
        import_docx(&bytes).map_err(|error| format!("import docx {}: {error}", path.display()))?;

    if !warnings.is_empty() {
        let msg = format!(
            "This Word document contains elements that Loom Writer cannot fully edit:\n\n• {}\n\nDo you want to continue importing?",
            warnings.join("\n• ")
        );
        app.set_docx_import_warning_message(SharedString::from(msg));
        PENDING_DOCX_IMPORT.with(|p| {
            *p.borrow_mut() = Some(PendingDocxImport {
                path: path.clone(),
                document,
            });
        });
        app.set_docx_import_warning_open(true);
        app.set_status_left("Review the Word import warning before replacing this document".into());
        Ok(false)
    } else {
        replace_opened_document(app, state, path.clone(), document);
        sync_writer_menu_if_present(menu_service, app, state);
        app.set_status_left(SharedString::from(format!("Opened {}", path.display())));
        Ok(true)
    }
}

/// Cancels the pending DOCX import. Leaves current document and recovery untouched.
pub fn cancel_docx_import(app: &WriterApp) {
    app.set_docx_import_warning_open(false);
    PENDING_DOCX_IMPORT.with(|p| *p.borrow_mut() = None);
    app.set_status_left("Word import cancelled".into());
    app.invoke_focus_page();
}

/// Continues the pending DOCX import, replacing the document.
pub fn continue_docx_import(
    app: &WriterApp,
    state: &GuiState,
    menu_service: &Option<Arc<NativeMenuBar>>,
) {
    app.set_docx_import_warning_open(false);
    let pending = PENDING_DOCX_IMPORT.with(|p| p.borrow_mut().take());
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
    apply_with_history(app, state, next, HistoryKind::DocumentAction);
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
