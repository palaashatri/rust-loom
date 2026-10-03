use super::*;

#[test]
fn tab_run_and_enter_returns_to_tab_start_column() {
    let (app, state) = super::grid_pointer_tests::projected(&[]);
    crate::tab_run::register_navigation(&app, &state);
    register_cell_edit_action(
        &app,
        &state,
        &std::sync::Arc::new(loom_desktop::NativeMenuBar::new()),
    );

    // Start at A1
    app.set_selected_cell("A1".into());

    // Type 1, Tab to B1
    app.invoke_commit_selected_cell("1".into());
    app.invoke_navigate_selection(0, 1); // Simulate Tab
    assert_eq!(
        app.get_selected_cell().as_str(),
        "B1",
        "Tab should move to B1"
    );

    // Type 2, Tab to C1
    app.invoke_commit_selected_cell("2".into());
    app.invoke_navigate_selection(0, 1); // Simulate Tab
    assert_eq!(
        app.get_selected_cell().as_str(),
        "C1",
        "Tab should move to C1"
    );

    // Type 3, Press Enter (should go to A2, the row below the start)
    app.invoke_commit_selected_cell("3".into());
    app.invoke_navigate_selection(1, 0); // Enter currently moves down from current column

    // WITHOUT the feature, this would be C2
    // WITH the feature, this should be A2
    assert_eq!(
        app.get_selected_cell().as_str(),
        "A2",
        "Enter after Tab run should return to column A (the start column)"
    );
}

#[test]
fn enter_without_tab_moves_straight_down() {
    let (app, state) = super::grid_pointer_tests::projected(&[]);
    crate::tab_run::register_navigation(&app, &state);
    register_cell_edit_action(
        &app,
        &state,
        &std::sync::Arc::new(loom_desktop::NativeMenuBar::new()),
    );

    // Start at B2
    app.set_selected_cell("B2".into());

    // Type value, Press Enter (no Tab, so should move straight down to B3)
    app.invoke_commit_selected_cell("test".into());
    app.invoke_navigate_selection(1, 0); // Enter
    assert_eq!(
        app.get_selected_cell().as_str(),
        "B3",
        "Enter without Tab should move straight down in the current column"
    );
}

#[test]
fn shift_tab_does_not_break_tab_run() {
    let (app, state) = super::grid_pointer_tests::projected(&[]);
    crate::tab_run::register_navigation(&app, &state);
    register_cell_edit_action(
        &app,
        &state,
        &std::sync::Arc::new(loom_desktop::NativeMenuBar::new()),
    );

    // Start at A1
    app.set_selected_cell("A1".into());

    // Type 1, Tab to B1
    app.invoke_commit_selected_cell("1".into());
    app.invoke_navigate_selection(0, 1); // Tab
    assert_eq!(app.get_selected_cell().as_str(), "B1");

    // Type 2, Shift+Tab back to A1
    app.invoke_commit_selected_cell("2".into());
    app.invoke_navigate_selection(0, -1); // Shift+Tab
    assert_eq!(app.get_selected_cell().as_str(), "A1");

    // Type 3, Tab forward to B1
    app.invoke_commit_selected_cell("3".into());
    app.invoke_navigate_selection(0, 1); // Tab
    assert_eq!(app.get_selected_cell().as_str(), "B1");

    // Type 4, Press Enter - should still go to A2 (we're still in the tab run)
    app.invoke_commit_selected_cell("4".into());
    app.invoke_navigate_selection(1, 0); // Enter
    assert_eq!(
        app.get_selected_cell().as_str(),
        "A2",
        "Enter after Shift+Tab that's part of Tab run should still go to start column"
    );
}

#[test]
fn clicking_a_cell_ends_the_tab_run() {
    let (app, state) = super::grid_pointer_tests::projected(&[]);
    crate::tab_run::register_navigation(&app, &state);
    register_cell_edit_action(
        &app,
        &state,
        &std::sync::Arc::new(loom_desktop::NativeMenuBar::new()),
    );

    app.set_selected_cell("A1".into());
    app.invoke_commit_selected_cell("1".into());
    app.invoke_navigate_selection(0, 1); // Tab to B1, run starts at column A
    assert_eq!(app.get_selected_cell().as_str(), "B1");

    // Click B2: the run is over and a new one starts from here.
    app.invoke_commit_selected_cell("2".into());
    app.invoke_cell_clicked(1, 1);
    assert_eq!(app.get_selected_cell().as_str(), "B2");
    app.invoke_commit_selected_cell("3".into());
    app.invoke_navigate_selection(0, 1); // Tab to C2, run starts at column B
    assert_eq!(app.get_selected_cell().as_str(), "C2");
    app.invoke_commit_selected_cell("4".into());
    app.invoke_navigate_selection(1, 0); // Enter
    assert_eq!(
        app.get_selected_cell().as_str(),
        "B3",
        "Enter returns to the column where the new run started, not column A"
    );
}

#[test]
fn an_arrow_move_without_a_commit_ends_the_tab_run() {
    let (app, state) = super::grid_pointer_tests::projected(&[]);
    crate::tab_run::register_navigation(&app, &state);
    register_cell_edit_action(
        &app,
        &state,
        &std::sync::Arc::new(loom_desktop::NativeMenuBar::new()),
    );

    app.set_selected_cell("A1".into());
    app.invoke_commit_selected_cell("1".into());
    app.invoke_navigate_selection(0, 1); // Tab to B1
    app.invoke_navigate_selection(0, 1); // Right arrow (no commit) to C1
    assert_eq!(app.get_selected_cell().as_str(), "C1");
    app.invoke_commit_selected_cell("2".into());
    app.invoke_navigate_selection(1, 0); // Enter: not in a run
    assert_eq!(app.get_selected_cell().as_str(), "C2");
}
