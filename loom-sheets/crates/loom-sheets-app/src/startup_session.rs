//! The first state of a Sheets window. A recovered draft restores as unsaved
//! work under the file it belongs to. The file named on the command line loads
//! after the window shows, and waits behind the Save, Discard, or Cancel prompt
//! whenever a draft would otherwise be replaced.

use std::path::{Path, PathBuf};

use loom_sheets_core::persistence::WorkbookFile;
use loom_sheets_core::Sheet;

use crate::analysis::{plan_chart, plan_chart_in_range};
use crate::workbook_io::{
    blank_sheet, is_untouched_starter, restore_workbook_from_snapshot, starter_workbook,
};
use crate::workbook_worker::{WorkbookWorker, WorkerModel, WorkerStartup};

/// What the command line asks the first window to show.
pub(crate) struct StartupChoices {
    pub(crate) example: bool,
    pub(crate) objects: bool,
    pub(crate) chart: bool,
    pub(crate) requested: Option<PathBuf>,
}

/// The first window state, ready to install.
pub(crate) struct StartupSession {
    pub(crate) model: WorkerModel,
    /// True when a recovered draft is the window's unsaved contents.
    pub(crate) recovered_unsaved: bool,
    /// The file the window saves to: the draft's file, or `None` when the
    /// draft is untitled or the window starts fresh.
    pub(crate) save_path: Option<PathBuf>,
    /// True when the requested file must load after the window shows.
    pub(crate) opens_file: bool,
}

/// Restores the recovery store into the worker and picks the first workbook.
/// `revision` is the worker revision of this first model.
pub(crate) fn begin(
    worker: &WorkbookWorker,
    startup: &WorkerStartup,
    revision: u64,
    choices: &StartupChoices,
) -> Result<StartupSession, String> {
    let stored_draft = startup
        .restored_payload
        .as_deref()
        .and_then(restore_workbook_from_snapshot);
    // The store holds only an untouched starter: the fresh blank replaces it,
    // so no later checkpoint or journal entry is built on the stale tab.
    let replace_stored_starter = startup.restored_payload.is_some()
        && stored_draft.as_ref().is_some_and(is_untouched_starter);
    let (mut initial, recovered_unsaved) = startup_workbook(stored_draft, choices.example);
    if initial.sheets.is_empty() {
        initial.sheets.push(blank_sheet());
    }
    initial.active = initial.active.min(initial.sheets.len() - 1);
    let save_path = recovered_unsaved
        .then(|| startup.restored_source.clone())
        .flatten();
    let opens_file = opens_requested_file(
        recovered_unsaved,
        save_path.as_deref(),
        choices.requested.as_deref(),
    );
    if choices.requested.is_none() {
        seed_startup_sheet(&mut initial.sheets[initial.active], choices);
    }
    let model = if choices.requested.is_some() && !recovered_unsaved {
        // The file is still loading: a temporary model must not replace the store.
        worker.initialize_workbook_without_recovery(revision, initial.active, initial.sheets)?
    } else if replace_stored_starter {
        worker.initialize_workbook_replacing_stored_draft(
            revision,
            initial.active,
            initial.sheets,
        )?
    } else {
        worker.initialize_workbook(revision, initial.active, initial.sheets)?
    };
    Ok(StartupSession {
        model,
        recovered_unsaved,
        save_path,
        opens_file,
    })
}

/// Picks the workbook a session starts with. A recovered workbook has no save
/// path of its own here: the caller gives it the path its store recorded, and
/// it stays unsaved. The bool is true exactly when the workbook was recovered.
pub(crate) fn startup_workbook(
    recovered: Option<WorkbookFile>,
    example: bool,
) -> (WorkbookFile, bool) {
    match recovered {
        // An untouched starter in the store is not a draft: the window closed
        // before any edit, so there is nothing to recover or to ask about.
        Some(file) if !is_untouched_starter(&file) => (file, true),
        _ => (
            WorkbookFile {
                sheets: vec![if example {
                    starter_workbook()
                } else {
                    blank_sheet()
                }],
                active: 0,
            },
            false,
        ),
    }
}

/// Whether the requested file loads after the window shows. A draft for that
/// same file is the newer copy, so it stays and the file is not reloaded over
/// it. Any other draft stays unsaved, so the file waits for the user's decision.
pub(crate) fn opens_requested_file(
    recovered: bool,
    draft_source: Option<&Path>,
    requested: Option<&Path>,
) -> bool {
    match (requested, draft_source) {
        (None, _) => false,
        (Some(requested), Some(source)) if recovered => !same_document(requested, source),
        _ => true,
    }
}

/// True when both paths name one file. Canonical forms are compared when the
/// files exist, so a relative and an absolute spelling of one file match.
pub(crate) fn same_document(requested: &Path, source: &Path) -> bool {
    match (
        std::fs::canonicalize(requested),
        std::fs::canonicalize(source),
    ) {
        (Ok(requested), Ok(source)) => requested == source,
        _ => requested == source,
    }
}

/// The status line that names a restored draft.
pub(crate) fn restored_status(name: &str) -> String {
    format!("Restored unsaved changes to {name}")
}

fn seed_startup_sheet(sheet: &mut Sheet, choices: &StartupChoices) {
    if choices.objects {
        crate::object_actions::seed_demo_objects(sheet);
    }
    if choices.chart {
        let chart = if sheet.name == "Example Budget" {
            plan_chart_in_range(sheet, 0, 1, 1, 3).ok()
        } else {
            plan_chart(sheet, 0, 1).ok()
        };
        if let Some(chart) = chart {
            sheet.chart = Some(chart);
        }
    }
}
