//! Confirm known XLSX feature loss before replacing the current workbook.

use std::path::PathBuf;
use std::sync::Arc;

use loom_sheets_core::persistence::WorkbookFile;
use loom_sheets_core::XlsxImportWarning;
use slint::SharedString;

use super::open_operations::{is_native_workbook, OpenOperation, StartupOpenOptions};
use super::*;

pub(super) struct PendingXlsxImport {
    pub(super) path: PathBuf,
    pub(super) workbook: WorkbookFile,
    pub(super) warnings: Vec<XlsxImportWarning>,
    pub(super) operation: Option<OpenOperation>,
    pub(super) startup_options: Option<StartupOpenOptions>,
}

#[cfg(test)]
pub(super) fn prepare_startup_import(
    path: PathBuf,
    loaded: LoadedWorkbook,
    fallback: WorkbookFile,
) -> (WorkbookFile, Option<PendingXlsxImport>) {
    if loaded.warnings.is_empty() {
        return (loaded.workbook, None);
    }

    let pending = PendingXlsxImport {
        path,
        workbook: loaded.workbook,
        warnings: loaded.warnings,
        operation: None,
        startup_options: None,
    };
    (fallback, Some(pending))
}

fn warning_message(warnings: &[XlsxImportWarning]) -> String {
    let items = warnings
        .iter()
        .map(|warning| format!("• {}", warning.label()))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "Loom will import the supported content and drop these Excel features:\n{items}\n\nContinue replaces the workbook that is open now. Cancel leaves it as it is.\nThe original .xlsx file will stay unchanged."
    )
}

#[cfg(test)]
pub(super) fn stage_xlsx_import(
    app: &SheetsApp,
    state: &GuiState,
    path: PathBuf,
    workbook: WorkbookFile,
    warnings: Vec<XlsxImportWarning>,
) {
    stage_xlsx_import_candidate(app, state, path, workbook, warnings, None, None);
}

pub(super) fn stage_xlsx_import_candidate(
    app: &SheetsApp,
    state: &GuiState,
    path: PathBuf,
    workbook: WorkbookFile,
    warnings: Vec<XlsxImportWarning>,
    operation: Option<OpenOperation>,
    startup_options: Option<StartupOpenOptions>,
) {
    if warnings.is_empty() {
        return;
    }
    app.set_xlsx_import_warning_message(SharedString::from(warning_message(&warnings)));
    *state.pending_xlsx_import.borrow_mut() = Some(PendingXlsxImport {
        path,
        workbook,
        warnings,
        operation,
        startup_options,
    });
    app.set_xlsx_import_warning_open(true);
    app.invoke_focus_xlsx_import_warning();
    app.set_status_left("Review the Excel import warning before replacing this workbook".into());
}

pub(super) fn handle_loaded_workbook(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
    path: PathBuf,
    loaded: LoadedWorkbook,
    operation: OpenOperation,
    startup_options: Option<StartupOpenOptions>,
) {
    if !loaded.warnings.is_empty() {
        stage_xlsx_import_candidate(
            app,
            state,
            path,
            loaded.workbook,
            loaded.warnings,
            Some(operation),
            startup_options,
        );
        return;
    }
    let imported = !is_native_workbook(&path);
    let tabs = loaded.workbook.sheets.len();
    super::open_operations::replace_opened_workbook(
        app,
        state,
        path.clone(),
        loaded.workbook.sheets,
        loaded.workbook.active,
    );
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(if imported {
        format!(
            "Imported {}; use Save As for a Loom workbook",
            path.display()
        )
    } else {
        format!(
            "Opened {} ({} {})",
            path.display(),
            tabs,
            if tabs == 1 { "sheet" } else { "sheets" }
        )
    }));
}

pub(super) fn continue_pending_xlsx_import(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) {
    if state.save_operations.borrow().is_active() {
        app.set_status_left(
            "Save in progress — wait for it to finish before importing this workbook".into(),
        );
        return;
    }
    let Some(pending) = state.pending_xlsx_import.borrow_mut().take() else {
        app.set_xlsx_import_warning_open(false);
        return;
    };
    app.set_xlsx_import_warning_open(false);
    if let Some(operation) = pending.operation {
        if !state.open_operations.borrow().is_current(operation) {
            app.set_xlsx_import_warning_message(SharedString::new());
            return;
        }
        if super::open_operations::reload_candidate_after_save_if_needed(
            app,
            state,
            pending.path.clone(),
            operation,
            pending.startup_options,
        ) {
            app.set_xlsx_import_warning_message(SharedString::new());
            return;
        }
        let dirty_replacement_allowed = state
            .open_operations
            .borrow()
            .allows_dirty_replacement(operation, state.worker_revision.get());
        if (state.is_dirty() && !dirty_replacement_allowed)
            || super::open_operations::has_formula_draft(app)
        {
            *state.pending_xlsx_import.borrow_mut() = Some(pending);
            super::open_operations::request_replacement_after_dialog(
                app,
                state,
                PendingReplacement::OpenCandidate,
            );
            return;
        }
    }
    let tabs = pending.workbook.sheets.len();
    let omitted = pending
        .warnings
        .iter()
        .map(|warning| warning.label())
        .collect::<Vec<_>>()
        .join(", ");
    super::open_operations::replace_opened_workbook(
        app,
        state,
        pending.path.clone(),
        pending.workbook.sheets,
        pending.workbook.active,
    );
    sync_menu_state(menu_service, app, state);
    if let Some(options) = pending.startup_options {
        super::open_operations::apply_startup_projection(app, state, options);
    }
    app.set_status_left(SharedString::from(format!(
        "Imported {} ({} {}); dropped: {omitted}. Use Save As for a Loom workbook",
        pending.path.display(),
        tabs,
        if tabs == 1 { "sheet" } else { "sheets" }
    )));
}

pub(super) fn cancel_pending_xlsx_import(app: &SheetsApp, state: &GuiState) {
    if let Some(pending) = state.pending_xlsx_import.borrow_mut().take() {
        if let Some(operation) = pending.operation {
            state.open_operations.borrow_mut().cancel(operation);
        }
    }
    app.set_xlsx_import_warning_open(false);
    app.set_xlsx_import_warning_message(SharedString::new());
    app.set_status_left("Import cancelled; workbook and recovery were left unchanged".into());
}
