use super::workbook_interop_tests::{
    write_xlsx_with_absolute_anchor_shape, write_xlsx_with_unsupported_area_chart,
};
use super::*;
use loom_desktop::{NativeMenuBar, ScriptedFileDialogs};
use std::time::{Duration, Instant};

/// One staged-loss scenario: a tag, the warning label it must name, and the
/// fixture writer for that XLSX feature.
type WarningScenario = (&'static str, &'static str, fn(&std::path::Path));

fn wait_for_warning(
    app: &SheetsApp,
    state: &GuiState,
    menu: &std::sync::Arc<NativeMenuBar>,
    path: &str,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !app.get_xlsx_import_warning_open() {
        crate::open_operations::process_completions(app, state, menu);
        assert!(
            Instant::now() < deadline,
            "{path} should stage the import warning"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn xlsx_loss_warnings_are_not_bypassed_by_picker_or_startup_open() {
    set_platform();
    let incoming_dir = std::env::temp_dir().join(format!(
        "loom-sheets-xlsx-warning-dispatch-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&incoming_dir).expect("create incoming workbook directory");
    let scenarios: [WarningScenario; 2] = [
        (
            "area-chart",
            "unsupported area charts",
            write_xlsx_with_unsupported_area_chart,
        ),
        (
            "absolute-anchor",
            "objects positioned with absolute anchors",
            write_xlsx_with_absolute_anchor_shape,
        ),
    ];

    for (tag, warning_label, write_fixture) in scenarios {
        let incoming_path = incoming_dir.join(format!("{tag}.xlsx"));
        write_fixture(&incoming_path);

        set_platform();
        let picker_app = SheetsApp::new().expect("create picker SheetsApp");
        let picker_state = cross_sheet_state_with_dialogs(std::rc::Rc::new(
            ScriptedFileDialogs::new([Some(incoming_path.clone())], []),
        ));
        picker_state.mark_saved();
        let picker_menu = std::sync::Arc::new(NativeMenuBar::new());
        crate::open_operations::open_workbook_from_picker(
            &picker_app,
            &picker_state,
            &picker_menu,
            None,
        );
        wait_for_warning(&picker_app, &picker_state, &picker_menu, "file-picker Open");
        assert_eq!(picker_state.current.borrow().name, "Data");
        assert!(picker_app
            .get_xlsx_import_warning_message()
            .contains(warning_label));
        crate::xlsx_import::cancel_pending_xlsx_import(&picker_app, &picker_state);
        assert_eq!(picker_state.current.borrow().name, "Data");
        assert!(!picker_app.get_xlsx_import_warning_open());

        set_platform();
        let startup_app = SheetsApp::new().expect("create startup SheetsApp");
        let startup_state = cross_sheet_state();
        startup_state.mark_saved();
        crate::open_operations::start_startup_open(
            &startup_app,
            &startup_state,
            incoming_path,
            crate::open_operations::StartupOpenOptions::new(false, false, false),
        );
        let startup_menu = std::sync::Arc::new(NativeMenuBar::new());
        wait_for_warning(
            &startup_app,
            &startup_state,
            &startup_menu,
            "startup --open",
        );
        assert_eq!(startup_state.current.borrow().name, "Data");
        assert!(startup_app
            .get_xlsx_import_warning_message()
            .contains(warning_label));

        crate::xlsx_import::continue_pending_xlsx_import(
            &startup_app,
            &startup_state,
            &startup_menu,
        );
        assert!(!startup_app.get_xlsx_import_warning_open());
        assert_eq!(startup_state.current.borrow().name, "Imported");
        assert!(startup_state.current.borrow().chart.is_none());
        assert!(startup_state.current.borrow().objects.is_empty());
        assert!(startup_app
            .get_status_left()
            .contains(&format!("dropped: {warning_label}")));
    }

    std::fs::remove_dir_all(&incoming_dir).ok();
}
