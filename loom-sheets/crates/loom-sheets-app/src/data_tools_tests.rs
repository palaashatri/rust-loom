use std::rc::Rc;
use std::sync::Arc;

use loom_desktop::{FileFilter, NativeMenuBar};
use loom_sheets_core::{CellRef, Sheet};
use slint::{ComponentHandle, Model};

use crate::command_dispatch::{dispatch_command, dispatch_shortcut};
use crate::local_menu::sheets_menu_bar;
use crate::palette::{dispatch_palette_action, palette_action_for_id, PaletteAction};
use crate::{apply_sheet, register_cell_edit_action, DataToolsState, GuiState, SheetsApp};

fn test_app_and_state() -> (SheetsApp, Rc<GuiState>, Arc<NativeMenuBar>) {
    i_slint_backend_testing::init_no_event_loop();
    let app = SheetsApp::new().expect("create SheetsApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let filter = FileFilter::new("Workbook", ["loomtable"]).expect("filter");
    let mut sheet = Sheet::new("Sheet 1");
    sheet.set_str("A1", "Category");
    sheet.set_str("B1", "Amount");
    sheet.set_str("A2", "Alpha");
    sheet.set_str("B2", "10");
    sheet.set_str("A3", "Beta");
    sheet.set_str("B3", "20");
    sheet.set_str("A4", "Alpha");
    sheet.set_str("B4", "30");

    let state = Rc::new(GuiState::new(
        sheet,
        None,
        dialogs,
        filter.clone(),
        filter.clone(),
        filter.clone(),
        filter,
    ));
    let menu = Arc::new(NativeMenuBar::new());
    register_cell_edit_action(&app, &state, &menu);
    super::register_data_tools_actions(&app, &state, &menu);
    apply_sheet(&app, &state);
    (app, state, menu)
}

#[test]
fn find_replace_autofilter_named_ranges_and_validation_wire_through_dispatch_palette_and_menu() {
    let (app, _state, _menu) = test_app_and_state();

    assert!(dispatch_shortcut(&app, "f", false, false));
    assert!(app.global::<DataToolsState>().get_find_open());
    assert!(!app.global::<DataToolsState>().get_find_replace_mode());

    assert!(dispatch_shortcut(&app, "h", false, false));
    assert!(app.global::<DataToolsState>().get_find_open());
    assert!(app.global::<DataToolsState>().get_find_replace_mode());
    app.global::<DataToolsState>().invoke_close_find();
    assert!(!app.global::<DataToolsState>().get_find_open());

    assert_eq!(
        palette_action_for_id("edit.find"),
        Some(PaletteAction::Find)
    );
    assert_eq!(
        palette_action_for_id("edit.replace"),
        Some(PaletteAction::Replace)
    );
    assert_eq!(
        palette_action_for_id("data.toggle_filter"),
        Some(PaletteAction::ToggleAutoFilter)
    );
    assert_eq!(
        palette_action_for_id("data.named_ranges"),
        Some(PaletteAction::NamedRanges)
    );
    assert_eq!(
        palette_action_for_id("data.validation"),
        Some(PaletteAction::DataValidation)
    );
    assert!(dispatch_palette_action(&app, PaletteAction::Find));
    assert!(app.global::<DataToolsState>().get_find_open());

    let bar = sheets_menu_bar();
    let ids: Vec<&str> = bar
        .menus
        .iter()
        .flat_map(|m| m.items.iter())
        .filter_map(|item| match item {
            loom_desktop::MenuItem::Action { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    assert!(ids.contains(&"edit.find"));
    assert!(ids.contains(&"edit.replace"));
    assert!(ids.contains(&"data.toggle_filter"));
    assert!(ids.contains(&"data.named_ranges"));
    assert!(ids.contains(&"data.validation"));
}

#[test]
fn find_next_and_replace_all_update_selection_and_cells() {
    let (app, state, _menu) = test_app_and_state();
    let dt = app.global::<DataToolsState>();
    dt.set_find_query("Alpha".into());
    dt.set_replace_text("Gamma".into());
    dt.invoke_find_next();
    assert_eq!(app.get_selected_cell().as_str(), "A2");
    dt.invoke_find_next();
    assert_eq!(app.get_selected_cell().as_str(), "A4");

    dt.invoke_replace_all();
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A2").unwrap()),
        Some("Gamma")
    );
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A4").unwrap()),
        Some("Gamma")
    );
}

#[test]
fn autofilter_hides_and_restores_rows_non_destructively() {
    let (app, state, _menu) = test_app_and_state();
    assert!(dispatch_command(&app, "data.toggle_filter"));
    let dt = app.global::<DataToolsState>();
    assert!(dt.get_autofilter_enabled());

    dt.invoke_open_column_filter(0);
    assert!(dt.get_filter_popup_open());
    // Uncheck "Beta" (index 1 in sorted ["Alpha", "Beta"]).
    dt.invoke_toggle_filter_value(1);
    dt.invoke_apply_column_filter();
    assert!(!dt.get_filter_popup_open());
    assert!(state.current.borrow().hidden_rows.contains(&2));
    let heights = app.get_grid_row_heights();
    assert_eq!(heights.row_data(2), Some(0.0));
    // Data remains intact.
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A3").unwrap()),
        Some("Beta")
    );

    assert!(dispatch_command(&app, "data.clear_filter"));
    assert!(state.current.borrow().hidden_rows.is_empty());
    assert!(app.get_grid_row_heights().row_data(2).unwrap_or(0.0) > 0.0);
}

#[test]
fn named_ranges_and_data_validation_work_end_to_end() {
    let (app, state, _menu) = test_app_and_state();
    let dt = app.global::<DataToolsState>();

    assert!(dispatch_command(&app, "data.named_ranges"));
    assert!(dt.get_named_ranges_open());
    dt.invoke_save_named_range("Bad Ref A1".into(), "B2:B4".into());
    assert!(!dt.get_named_range_error().is_empty());

    dt.invoke_save_named_range("Amounts".into(), "B2:B4".into());
    assert!(dt.get_named_range_error().is_empty());
    dt.invoke_goto_named_range("Amounts".into());
    assert_eq!(app.get_selection_range().as_str(), "B2:B4");

    // Apply Whole Number 1..=50 validation to B2:B4 and test commit rejection + acceptance.
    dt.set_val_kind(3);
    dt.set_val_min("1".into());
    dt.set_val_max("50".into());
    dt.set_val_error_message("Must be 1..50".into());
    dt.invoke_apply_validation();

    app.set_selected_cell("B2".into());
    app.invoke_commit_selected_cell("999".into());
    assert!(dt.get_val_feedback_error());
    assert!(dt.get_val_feedback().as_str().contains("Must be 1..50"));
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("B2").unwrap()),
        Some("10")
    );

    app.invoke_commit_selected_cell("42".into());
    assert!(!dt.get_val_feedback_error());
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("B2").unwrap()),
        Some("42")
    );
}
