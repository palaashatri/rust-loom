//! Named Ranges modal and callback registration for Data Tools.

use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::NativeMenuBar;
use loom_sheets_core::{CellRange, GridSelection, Sheet};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::{
    evaluate_current, project_current, selection_from_app, update_selection_range, DataToolsState,
    GuiState, NamedRangeItem, SheetsApp,
};

use super::commit_sheet_metadata_edit;

pub(crate) fn open_named_ranges(app: &SheetsApp, state: &Rc<GuiState>) {
    let selection = selection_from_app(app);
    let dt = app.global::<DataToolsState>();
    dt.set_new_range_name(SharedString::default());
    dt.set_new_range_ref(SharedString::from(selection.range().to_a1()));
    dt.set_named_range_error(SharedString::default());
    sync_named_ranges_model(app, &state.current.borrow());
    dt.set_named_ranges_open(true);
}

pub(super) fn sync_named_ranges_model(app: &SheetsApp, sheet: &Sheet) {
    let items: Vec<NamedRangeItem> = sheet
        .named_ranges
        .iter()
        .map(|(name, range)| NamedRangeItem {
            name: SharedString::from(name.as_str()),
            range: SharedString::from(range.to_a1()),
        })
        .collect();
    app.global::<DataToolsState>()
        .set_named_ranges(ModelRc::new(VecModel::from(items)));
}

pub(super) fn register_callbacks(
    app: &SheetsApp,
    state: &Rc<GuiState>,
    menu_service: &Arc<NativeMenuBar>,
) {
    let dt = app.global::<DataToolsState>();
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_save_named_range(move |name, range_str| {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                let dt = app.global::<DataToolsState>();
                let Some(range) = CellRange::parse(range_str.as_str().trim()) else {
                    dt.set_named_range_error("Invalid A1 range (e.g. A1:B10)".into());
                    return;
                };
                let mut result = Ok(());
                let committed = commit_sheet_metadata_edit(&app, &state, &menu_service, |sheet| {
                    result = sheet.set_named_range(name.as_str(), range);
                    result.is_ok()
                });
                match result {
                    Ok(()) if committed => {
                        dt.set_named_range_error(SharedString::default());
                        dt.set_new_range_name(SharedString::default());
                    }
                    Ok(()) => {}
                    Err(err) => {
                        dt.set_named_range_error(SharedString::from(err));
                    }
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        let menu_service = menu_service.clone();
        dt.on_delete_named_range(move |name| {
            if let Some(app) = app_ref.upgrade() {
                if crate::mutation_guard::refused(&app, &state) {
                    return;
                }
                commit_sheet_metadata_edit(&app, &state, &menu_service, |sheet| {
                    sheet.remove_named_range(name.as_str())
                });
            }
        });
    }
    {
        let app_ref = app.as_weak();
        let state = state.clone();
        dt.on_goto_named_range(move |name| {
            if let Some(app) = app_ref.upgrade() {
                let target = state.current.borrow().resolve_named_range(name.as_str());
                if let Some(range) = target {
                    let vals = evaluate_current(&state);
                    let sheet = state.current.borrow();
                    update_selection_range(
                        &app,
                        &sheet,
                        &vals,
                        GridSelection {
                            anchor: range.start,
                            focus: range.end,
                        },
                    );
                    drop(sheet);
                    project_current(&app, &state);
                    app.global::<DataToolsState>().set_named_ranges_open(false);
                }
            }
        });
    }
    {
        let app_ref = app.as_weak();
        dt.on_close_named_ranges(move || {
            if let Some(app) = app_ref.upgrade() {
                app.global::<DataToolsState>().set_named_ranges_open(false);
            }
        });
    }
}
