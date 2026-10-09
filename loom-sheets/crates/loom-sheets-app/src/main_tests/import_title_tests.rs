//! An imported CSV is unsaved work named after its file. Its sheet tab uses the
//! spreadsheet default name, and the window title never shows the internal
//! import label.

use super::clipboard_tests::state_with;

#[test]
fn an_imported_csv_is_unsaved_and_titled_by_its_file_name() {
    let (app, state) = state_with(&[]);
    let dir = std::env::temp_dir().join(format!("loom-sheets-import-title-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp directory");
    let path = dir.join("sales.csv");
    std::fs::write(&path, "item,qty\napples,3\n").expect("write csv");
    let loaded = crate::workbook_io::load_workbook_with_report(&path).expect("import csv");
    crate::open_operations::replace_opened_workbook(
        &app,
        &state,
        path.clone(),
        loaded.workbook.sheets,
        loaded.workbook.active,
    );
    assert_eq!(state.sheets.borrow()[0].name, "Sheet 1");
    assert!(
        state.is_dirty(),
        "an import is not yet saved as a Loom workbook"
    );
    assert_eq!(app.get_window_title().as_str(), "sales *");
    let _ = std::fs::remove_dir_all(&dir);
}
