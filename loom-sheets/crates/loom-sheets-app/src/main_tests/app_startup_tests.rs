use super::*;
use crate::open_operations::{is_native_workbook, open_request};
use i_slint_backend_testing::ElementHandle;

#[test]
fn rtl_argument_is_parsed_and_applied_to_the_root() {
    let args = parse_args_from(["--rtl"] as [&str; 1]).expect("parse --rtl");
    assert!(args.rtl);

    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    configure_direction(&app, args.rtl);
    assert!(app.get_rtl());
}

#[test]
fn objects_argument_enables_the_native_object_fixture() {
    let args = parse_args_from(["--objects"] as [&str; 1]).expect("parse --objects");
    assert!(args.objects);
}

#[test]
fn example_argument_is_explicit() {
    let blank = parse_args_from(["--screenshot", "/tmp/blank.png"] as [&str; 2])
        .expect("parse blank screenshot");
    assert!(!blank.example);
    let example = parse_args_from(["--example"] as [&str; 1]).expect("parse --example");
    assert!(example.example);
}

#[test]
fn inspector_capture_flag_is_supported() {
    let args = parse_args_from(["--inspector"] as [&str; 1]).expect("parse --inspector");
    assert!(args.inspector);
}

#[test]
fn new_workbook_is_blank_and_named_untitled() {
    let sheet = blank_sheet();
    assert!(sheet.cells.is_empty());
    assert_eq!(sheet.name, "Untitled");
}

#[test]
fn example_workbook_uses_one_unit_and_live_formulas() {
    let sheet = starter_workbook();
    assert_eq!(sheet.name, "Example Budget");
    assert_eq!(
        sheet.raw(CellRef::parse("B1").unwrap()),
        Some("Amount (USD/month)")
    );
    assert_eq!(sheet.col_width(1), 190.0);
    assert_eq!(sheet.raw(CellRef::parse("C2").unwrap()), Some("Monthly"));
    assert_eq!(sheet.raw(CellRef::parse("C3").unwrap()), Some("Monthly"));
    assert_eq!(sheet.raw(CellRef::parse("C4").unwrap()), Some("Monthly"));
    assert_eq!(
        sheet.raw(CellRef::parse("B5").unwrap()),
        Some("=SUM(B2:B4)")
    );
    assert_eq!(
        sheet.raw(CellRef::parse("B6").unwrap()),
        Some("=AVERAGE(B2:B4)")
    );
    let values = evaluate(&sheet);
    assert_eq!(
        values.get(&CellRef::parse("B5").unwrap()),
        Some(&Value::Number(1800.0))
    );
    assert_eq!(
        values.get(&CellRef::parse("B6").unwrap()),
        Some(&Value::Number(600.0))
    );
}

#[test]
fn scripted_dialog_request_uses_current_workbook_directory() {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [Some(PathBuf::from("/tmp/import.csv"))],
        [Some(PathBuf::from("/tmp/workbook.loomtable"))],
    ));
    let state = GuiState::new(
        starter_workbook(),
        Some(PathBuf::from("/tmp/current.loomtable")),
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    );
    let request = open_request(&state);
    assert_eq!(request.initial_directory, Some(PathBuf::from("/tmp")));
    assert_eq!(
        state.dialogs.open_file(&request).expect("open"),
        Some(PathBuf::from("/tmp/import.csv"))
    );
}

#[test]
fn csv_import_does_not_become_native_save_target() {
    assert!(!is_native_workbook(Path::new("budget.csv")));
    assert!(is_native_workbook(Path::new("budget.loomtable")));
}

#[test]
fn workbook_window_title_uses_the_saved_file_or_a_truthful_unsaved_name() {
    assert_eq!(
        workbook_window_title(
            Some(std::path::Path::new("/tmp/Household.loomtable")),
            "Example Budget",
            false
        ),
        "Household.loomtable"
    );
    assert_eq!(workbook_window_title(None, "Sheet1", false), "Untitled");
    assert_eq!(workbook_window_title(None, "Checklist", false), "Checklist");
    assert_eq!(
        workbook_window_title(None, "Checklist", true),
        "Checklist *"
    );
    assert_eq!(
        workbook_window_title(
            Some(std::path::Path::new("/tmp/Household.loomtable")),
            "Example Budget",
            true
        ),
        "Household.loomtable *"
    );
}

/// UI-30: the dirty-close prompt must name the same document the window title
/// and the sheet tab show. A created template is identified by its sheet, not
/// by a generic "Untitled workbook" fallback.
#[test]
fn close_prompt_names_the_same_document_as_the_window_title() {
    let cases = [
        // (save path, sheet name, expected identity)
        (None, "Checklist", "Checklist"),
        (None, "Untitled", "Untitled"),
        (None, "Sheet1", "Untitled"),
        (
            Some(std::path::PathBuf::from("/tmp/Household.loomtable")),
            "Checklist",
            "Household.loomtable",
        ),
    ];
    for (save_path, sheet_name, expected) in cases {
        let state = state_with_identity(save_path.clone(), sheet_name);
        let display = workbook_display_name(&state);
        let title = workbook_window_title(save_path.as_deref(), sheet_name, false);
        assert_eq!(display, expected, "display name for {sheet_name:?}");
        assert_eq!(
            display,
            title.trim_end_matches(" *"),
            "the close prompt and the window title must name one document"
        );
    }
}

#[test]
fn formula_errors_are_visible_in_a_polite_live_region() {
    i_slint_backend_testing::init_no_event_loop();
    let app = SheetsApp::new().expect("create SheetsApp");
    let state = cross_sheet_state();
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_cell_edit_action(&app, &state, &menu_service);
    app.invoke_commit_selected_cell("=1/0".into());

    let expected_message = "Formula error in A1: #DIV/0!";
    assert_eq!(app.get_status_left().as_str(), expected_message);
    project_current(&app, &state);
    assert_eq!(app.get_status_left().as_str(), expected_message);
    assert_eq!(app.get_status_summary().as_str(), "2 cells · 2 formulas");

    let messages: Vec<_> =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, expected_message)
            .collect();
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].accessible_live_region(),
        Some(i_slint_backend_testing::AccessibleLiveness::Polite)
    );
}

#[test]
fn readonly_save_error_feedback_is_short_and_actionable() {
    assert_eq!(
        save_error_feedback(
            "Save failed",
            "atomic write /tmp/locked.loomtable: io error: destination '/tmp/locked.loomtable' is read-only",
        ),
        "Save failed: destination is read-only"
    );
}

#[test]
fn formula_bar_draft_is_not_applied_before_commit() {
    let mut sheet = Sheet::new("test");
    let selected = CellRef::parse("B1").unwrap();
    sheet.set_str("A1", "2");
    sheet.set_str("B1", "3");

    let mut edit = CellEditTransaction::begin(sheet.raw(selected));
    edit.update("=A1+1");

    assert_eq!(selected.to_a1(), "B1");
    assert_eq!(edit.commit().unwrap().after(), "=A1+1");
    assert_eq!(sheet.raw(selected), Some("3"));
}

#[test]
fn formula_bar_commit_preserves_formula_raw_and_selected_cell() {
    let mut sheet = Sheet::new("test");
    let selected = CellRef::parse("B1").unwrap();
    sheet.set_str("A1", "2");
    sheet.set_str("B1", "3");
    let mut undo = Vec::new();
    let mut redo = Vec::new();

    assert!(commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, selected, "=A1+1",
    ));

    assert_eq!(selected.to_a1(), "B1");
    assert_eq!(sheet.raw(selected), Some("=A1+1"));
    assert_eq!(evaluate(&sheet).get(&selected), Some(&Value::Number(3.0)));
}

#[test]
fn formula_bar_commit_preserves_literal_and_empty_raw_text() {
    let mut sheet = Sheet::new("test");
    let literal = CellRef::parse("A1").unwrap();
    let empty = CellRef::parse("B1").unwrap();
    sheet.set_raw(literal, "old");
    sheet.set_raw(empty, "old");
    let mut undo = Vec::new();
    let mut redo = Vec::new();

    assert!(commit_formula_edit(
        &mut sheet,
        &mut undo,
        &mut redo,
        literal,
        "  literal text  ",
    ));
    assert!(commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, empty, "",
    ));

    assert_eq!(sheet.raw(literal), Some("  literal text  "));
    assert_eq!(sheet.raw(empty), Some(""));
    assert_eq!(evaluate(&sheet).get(&empty), Some(&Value::Empty));
}

#[test]
fn formula_bar_commit_records_one_transaction_and_noop_records_none() {
    let mut sheet = Sheet::new("test");
    let selected = CellRef::parse("A1").unwrap();
    sheet.set_str("A1", "old");
    let mut undo = Vec::new();
    let mut redo = vec![SheetTransaction::Range(RangeEdit::replace(
        &sheet,
        selected,
        Some("redo".to_string()),
    ))];

    assert!(commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, selected, "new",
    ));
    assert_eq!(undo.len(), 1);
    assert!(redo.is_empty());

    assert!(!commit_formula_edit(
        &mut sheet, &mut undo, &mut redo, selected, "new",
    ));
    assert_eq!(undo.len(), 1);
}

/// UI-38: `--text-scale` reached only the template chooser and the XLSX warning.
/// The shared theme now owns the scale, so chrome, controls, and dialogs all
/// honour it. A standard control keeps its contract height at every scale, so
/// the observable that proves the scale reached a visible label is the control
/// width: a larger label needs more room in the same control height.
#[test]
fn app_chrome_honours_the_text_scale() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1018, 728));

    // Measure a labeled button in a dialog that is present at every scale.
    let cancel_width_at = |scale: f32| -> f32 {
        app.set_template_text_scale(scale);
        app.set_save_changes_close_mode(true);
        app.set_save_changes_document("Household.loomtable".into());
        app.set_save_changes_open(true);
        let _ = snapshot_component(&app, 1018.0, 728.0, 1.0)
            .expect("render the dialog at the requested text scale");
        let control = dialog_control(&app, "Cancel");
        let size = control.size();
        assert_eq!(
            size.height, 28.0,
            "a standard control keeps its contract height at {scale}x"
        );
        size.width
    };

    let normal = cancel_width_at(1.0);
    let large = cancel_width_at(1.5);
    let stress = cancel_width_at(2.0);

    assert!(
        large > normal + 1.0,
        "a control label must grow at 1.5x text scale, got width {normal} then {large}"
    );
    assert!(
        stress > large + 1.0,
        "a control label must keep growing at 2x text scale, got width {large} then {stress}"
    );
    // Scaling back must restore the reference geometry, not compound.
    assert!(
        (cancel_width_at(1.0) - normal).abs() < 0.5,
        "returning to 1.0x must restore the original control width"
    );
}

/// The actionable control carrying `label`, ignoring the label's own text run,
/// which appears in the tree with the same name at a smaller size.
fn dialog_control(app: &SheetsApp, label: &str) -> ElementHandle {
    let matches: Vec<_> = ElementHandle::find_by_accessible_label(app, label)
        .filter(|element| element.size().height >= 20.0)
        .collect();
    assert!(!matches.is_empty(), "no actionable control named {label:?}");
    matches[0].clone()
}

/// UI-38: the Save Changes dialog had a fixed 208 px height, so a large text
/// scale clipped its content instead of reflowing. It must grow with its
/// content, stay inside the window, and keep both actions visible.
#[test]
fn the_save_changes_dialog_reflows_at_large_text_scale() {
    let mut panel_heights = Vec::new();
    for text_scale in [1.0f32, 1.5, 2.0] {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        app.window().set_size(PhysicalSize::new(1018, 560));
        app.set_template_text_scale(text_scale);
        app.set_save_changes_close_mode(true);
        app.set_save_changes_document("Household.loomtable".into());
        app.set_save_changes_open(true);
        let _ =
            snapshot_component(&app, 1018.0, 560.0, 1.0).expect("render the Save Changes dialog");

        // The accessible landmark is the full-window scrim, so measure the panel.
        let scrims: Vec<_> =
            ElementHandle::find_by_accessible_label(&app, "Save changes dialog").collect();
        assert_eq!(scrims.len(), 1, "one Save Changes dialog at {text_scale}x");
        let panels: Vec<_> =
            ElementHandle::find_by_element_id(&app, "SaveChangesDialog::panel").collect();
        assert_eq!(panels.len(), 1, "one Save Changes panel at {text_scale}x");
        let panel = panels[0].clone();
        let bounds = panel.absolute_position();
        let size = panel.size();
        panel_heights.push(size.height);

        assert!(
            size.height > 0.0 && size.height <= 560.0 - 32.0 + 1.0,
            "the panel must have real content height and stay inside the window at {text_scale}x, got {size:?}"
        );

        // Both actions must remain inside the panel at every scale.
        for label in ["Cancel", "Save and close"] {
            let inside: Vec<_> = ElementHandle::find_by_accessible_label(&app, label)
                .filter(|control| {
                    if control.size().height < 20.0 {
                        return false; // the label's own text run, not the control
                    }
                    let position = control.absolute_position();
                    position.y >= bounds.y - 1.0
                        && position.y + control.size().height <= bounds.y + size.height + 1.0
                        && position.x >= bounds.x - 1.0
                        && position.x + control.size().width <= bounds.x + size.width + 1.0
                })
                .collect();
            assert!(
                !inside.is_empty(),
                "{label} must sit inside the Save Changes panel at {text_scale}x; panel={bounds:?}{size:?}"
            );
            // The action must still be a full-size, operable control rather than
            // a shrunken remnant.
            assert_eq!(
                inside[0].size().height,
                28.0,
                "{label} must keep the standard control target at {text_scale}x"
            );
        }
    }
    assert!(
        panel_heights[2] > panel_heights[0] + 8.0,
        "the panel must grow with the text scale, measured {panel_heights:?}"
    );
}
