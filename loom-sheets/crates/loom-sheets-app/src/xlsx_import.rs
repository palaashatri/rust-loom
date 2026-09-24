//! Confirm known XLSX feature loss before replacing the current workbook.

use std::path::PathBuf;
use std::sync::Arc;

use loom_sheets_core::persistence::WorkbookFile;
use loom_sheets_core::XlsxImportWarning;
use slint::SharedString;

use super::*;

pub(super) struct PendingXlsxImport {
    pub(super) path: PathBuf,
    pub(super) workbook: WorkbookFile,
    pub(super) warnings: Vec<XlsxImportWarning>,
}

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

pub(super) fn stage_xlsx_import(
    app: &SheetsApp,
    state: &GuiState,
    path: PathBuf,
    workbook: WorkbookFile,
    warnings: Vec<XlsxImportWarning>,
) {
    if warnings.is_empty() {
        return;
    }
    app.set_xlsx_import_warning_message(SharedString::from(warning_message(&warnings)));
    *state.pending_xlsx_import.borrow_mut() = Some(PendingXlsxImport {
        path,
        workbook,
        warnings,
    });
    app.set_xlsx_import_warning_open(true);
    app.set_status_left("Review the Excel import warning before replacing this workbook".into());
}

fn handle_loaded_workbook(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
    path: PathBuf,
    loaded: LoadedWorkbook,
) {
    if !loaded.warnings.is_empty() {
        stage_xlsx_import(app, state, path, loaded.workbook, loaded.warnings);
        return;
    }
    let imported = !is_native_workbook(&path);
    let tabs = loaded.workbook.sheets.len();
    replace_opened_workbook(
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
    let Some(pending) = state.pending_xlsx_import.borrow_mut().take() else {
        app.set_xlsx_import_warning_open(false);
        return;
    };
    app.set_xlsx_import_warning_open(false);
    let tabs = pending.workbook.sheets.len();
    let omitted = pending
        .warnings
        .iter()
        .map(|warning| warning.label())
        .collect::<Vec<_>>()
        .join(", ");
    replace_opened_workbook(
        app,
        state,
        pending.path.clone(),
        pending.workbook.sheets,
        pending.workbook.active,
    );
    sync_menu_state(menu_service, app, state);
    app.set_status_left(SharedString::from(format!(
        "Imported {} ({} {}); dropped: {omitted}. Use Save As for a Loom workbook",
        pending.path.display(),
        tabs,
        if tabs == 1 { "sheet" } else { "sheets" }
    )));
}

pub(super) fn cancel_pending_xlsx_import(app: &SheetsApp, state: &GuiState) {
    state.pending_xlsx_import.borrow_mut().take();
    app.set_xlsx_import_warning_open(false);
    app.set_xlsx_import_warning_message(SharedString::new());
    app.set_status_left("Import cancelled; workbook and recovery were left unchanged".into());
}

pub(super) fn open_workbook_from_picker(
    app: &SheetsApp,
    state: &GuiState,
    menu_service: &Arc<loom_desktop::NativeMenuBar>,
) {
    match state.dialogs.open_file(&open_request(state)) {
        Ok(Some(path)) => match load_workbook_with_report(&path) {
            Ok(loaded) => handle_loaded_workbook(app, state, menu_service, path, loaded),
            Err(error) => app.set_status_left(SharedString::from(format!("Open failed: {error}"))),
        },
        Ok(None) => app.set_status_left("Open cancelled".into()),
        Err(error) => {
            app.set_status_left(SharedString::from(format!("Open dialog failed: {error}")))
        }
    }
}
