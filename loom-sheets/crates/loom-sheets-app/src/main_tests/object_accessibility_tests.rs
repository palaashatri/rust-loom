use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};
use loom_sheets_core::SheetObject;
use std::ops::ControlFlow;

fn press_text(app: &SheetsApp, text: &str) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: text.into() });
}

fn press_key(app: &SheetsApp, key: slint::platform::Key) {
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.into() });
}

fn press_control_text(app: &SheetsApp, text: &str) {
    press_key(app, slint::platform::Key::Control);
    press_text(app, text);
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Control.into(),
        });
}

#[test]
fn worksheet_objects_expose_distinct_names_and_persisted_geometry_to_accessibility() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    let mut sheet = Sheet::new("Object labels");
    let mut first = SheetObject::shape(CellRef { row: 1, col: 2 }, "Callout");
    first.width = 301;
    first.height = 121;
    sheet.objects.push(first);
    let mut second = SheetObject::shape(CellRef { row: 4, col: 5 }, "Callout");
    second.width = 202;
    second.height = 102;
    sheet.objects.push(second);
    let mut image =
        SheetObject::image(CellRef { row: 7, col: 8 }, "photo.png").expect("valid image path");
    image.embedded = Some(vec![
        137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 4,
        0, 0, 0, 181, 28, 12, 2, 0, 0, 0, 11, 73, 68, 65, 84, 120, 218, 99, 100, 248, 15, 0, 1, 5,
        1, 1, 39, 24, 227, 102, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
    ]);
    image.label = "Callout".into();
    sheet.objects.push(image);

    app.set_selected_object(0);
    project_sheet(&app, &sheet);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render object accessibility tree");

    let mut objects = Vec::new();
    for label in ["Shape 1: Callout", "Shape 2: Callout", "Image 3: Callout"] {
        let matches: Vec<_> = ElementHandle::find_by_accessible_label(&app, label).collect();
        assert_eq!(matches.len(), 1, "expected one object named {label:?}");
        assert_eq!(matches[0].accessible_role(), Some(AccessibleRole::ListItem));
        objects.push(matches[0].clone());
    }

    for (item, expected) in objects.iter().zip(["C2", "F5", "I8"]) {
        let detail = item
            .accessible_description()
            .expect("object exposes anchor, size, and keyboard actions")
            .to_string();
        assert!(
            detail.contains(expected),
            "missing anchor {expected}: {detail:?}"
        );
        assert!(
            detail.contains("Tab"),
            "missing navigation help: {detail:?}"
        );
        assert!(detail.contains("Enter"), "missing commit help: {detail:?}");
        assert!(detail.contains("Escape"), "missing cancel help: {detail:?}");
    }

    let mut list_count = 0;
    let lists: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Worksheet objects").collect();
    assert_eq!(lists.len(), 1, "objects should be grouped as a named list");
    assert_eq!(lists[0].accessible_role(), Some(AccessibleRole::List));
    lists[0].visit_descendants(|element| {
        if element.accessible_role() == Some(AccessibleRole::ListItem) {
            list_count += 1;
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(list_count, 3);
    let handles: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Resize Shape 1: Callout").collect();
    assert_eq!(
        handles.len(),
        1,
        "selected object exposes its resize action"
    );
    let target_size = handles[0].size();
    assert_eq!((target_size.width, target_size.height), (20.0, 20.0));
}

#[test]
fn keyboard_object_move_and_resize_support_cancel_undo_and_save_reopen() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let mut sheet = Sheet::new("Keyboard geometry");
    let mut object = SheetObject::shape(CellRef { row: 1, col: 2 }, "Callout");
    object.width = 301;
    object.height = 121;
    sheet.objects.push(object);
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_history_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    project_current(&app, &state);

    app.invoke_object_move_started(0);
    app.invoke_object_keyboard_action(0, 1, 1, 1);
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 2 },
        "a live preview must stay outside the canonical workbook model"
    );
    app.invoke_object_keyboard_action(0, -3, 0, 0);
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 2 },
        "Escape restores the original anchor"
    );
    assert!(state.undo_stack.borrow().is_empty());

    app.invoke_object_resize_started(0);
    app.invoke_object_keyboard_action(0, 2, 1, -1);
    assert_eq!(
        state
            .object_gesture
            .borrow()
            .as_ref()
            .map(|gesture| (gesture.preview_width, gesture.preview_height)),
        Some((311, 111)),
        "resize arrows change only the preview dimensions"
    );
    assert_eq!(
        (
            state.current.borrow().objects[0].width,
            state.current.borrow().objects[0].height
        ),
        (301, 121),
        "resize preview stays outside the canonical workbook"
    );
    app.invoke_object_keyboard_action(0, -4, 0, 0);
    assert_eq!(
        (
            state.current.borrow().objects[0].width,
            state.current.borrow().objects[0].height
        ),
        (301, 121),
        "Escape restores the original dimensions"
    );
    assert!(state.undo_stack.borrow().is_empty());

    app.invoke_object_move_started(0);
    app.invoke_object_keyboard_action(0, 1, 1, 1);
    app.invoke_object_keyboard_action(0, -1, 0, 0);
    assert_eq!(state.undo_stack.borrow().len(), 1);
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 2, col: 3 }
    );

    let path = std::env::temp_dir().join(format!(
        "loom-sheets-keyboard-object-{}-{}.loomtable",
        std::process::id(),
        state.next_worker_revision()
    ));
    let current = state.current.borrow().clone();
    save_workbook(&path, &[current], 0).expect("save keyboard-edited geometry");
    let reopened = load_workbook(&path).expect("reopen keyboard-edited geometry");
    let persisted = reopened.sheets[0].objects[0].clone();
    assert_eq!(persisted.anchor, CellRef { row: 2, col: 3 });
    assert_eq!((persisted.width, persisted.height), (301, 121));
    std::fs::remove_file(&path).expect("remove test workbook");

    app.invoke_undo();
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 2 },
        "the keyboard move is one undoable change"
    );
    assert_eq!(
        (
            state.current.borrow().objects[0].width,
            state.current.borrow().objects[0].height
        ),
        (301, 121)
    );
}

#[test]
fn cell_command_cancels_object_preview_without_queuing_preview_geometry() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut sheet = Sheet::new("Preview command boundary");
    let cell = CellRef::parse("A1").unwrap();
    sheet.set_raw(cell, "clear me");
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 1, col: 2 }, "Callout"));
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_sheet_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    app.set_selected_cell("A1".into());

    app.invoke_object_move_started(0);
    app.invoke_object_keyboard_action(0, 1, 1, 0);
    app.invoke_clear_selected_cells();

    assert!(state.object_gesture.borrow().is_none());
    assert_eq!(state.current.borrow().raw(cell), None);
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 2 },
        "the cell edit and its full-workbook snapshot must exclude preview geometry"
    );
}

#[test]
fn stale_object_gesture_cannot_commit_into_a_replacement_tab_at_the_same_index() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let mut first = Sheet::new("First tab");
    first.objects.push(SheetObject::shape(
        CellRef { row: 1, col: 2 },
        "First object",
    ));
    let mut replacement = Sheet::new("Replacement tab");
    let replacement_anchor = CellRef { row: 8, col: 6 };
    replacement
        .objects
        .push(SheetObject::shape(replacement_anchor, "Replacement object"));
    let state = Rc::new(GuiState::new(
        first,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let first = state.current.borrow().clone();
    state.install_workbook(vec![first, replacement.clone()], 0);
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_sheet_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    app.invoke_object_move_started(0);
    app.invoke_object_keyboard_action(0, 1, 1, 0);

    assert!(crate::actions::delete_active_sheet(
        &app,
        &state,
        &menu_service
    ));
    app.invoke_object_keyboard_action(0, 1, 1, 0);
    app.invoke_object_keyboard_action(0, -1, 0, 0);

    assert_eq!(state.current.borrow().name, "Replacement tab");
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        replacement_anchor,
        "a late preview update or commit must not target a different tab that reused index zero"
    );
}

#[test]
fn f6_object_navigation_previews_keyboard_actions_and_keeps_saved_geometry() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let mut sheet = Sheet::new("Keyboard navigation");
    sheet.objects.push(SheetObject::shape(
        CellRef { row: 1, col: 2 },
        "First callout",
    ));
    sheet.objects.push(SheetObject::shape(
        CellRef { row: 4, col: 5 },
        "Second callout",
    ));
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_history_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    project_current(&app, &state);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render the keyboard focus target");

    app.invoke_focus_grid();
    press_key(&app, slint::platform::Key::F6);
    assert_eq!(app.get_selected_object(), 0);
    press_key(&app, slint::platform::Key::Tab);
    assert_eq!(app.get_selected_object(), 1, "Tab selects the next object");

    press_text(&app, "m");
    press_key(&app, slint::platform::Key::RightArrow);
    assert_eq!(
        state
            .object_gesture
            .borrow()
            .as_ref()
            .unwrap()
            .preview_anchor,
        CellRef { row: 4, col: 6 },
        "arrow movement previews one worksheet cell"
    );
    assert_eq!(
        state.current.borrow().objects[1].anchor,
        CellRef { row: 4, col: 5 },
        "preview geometry remains outside the canonical workbook"
    );
    assert!(
        state.undo_stack.borrow().is_empty(),
        "preview is not committed"
    );
    press_key(&app, slint::platform::Key::F6);
    assert_eq!(
        state.current.borrow().objects[1].anchor,
        CellRef { row: 4, col: 5 },
        "re-entering object navigation cancels an outstanding move preview"
    );
    assert!(state.object_gesture.borrow().is_none());
    press_key(&app, slint::platform::Key::F6);
    press_text(&app, "m");
    press_key(&app, slint::platform::Key::RightArrow);
    press_key(&app, slint::platform::Key::Return);
    assert_eq!(
        state.undo_stack.borrow().len(),
        1,
        "Enter commits one undo step"
    );

    let original_size = {
        let object = &state.current.borrow().objects[1];
        (object.width, object.height)
    };
    press_text(&app, "r");
    press_key(&app, slint::platform::Key::DownArrow);
    assert_eq!(
        state
            .object_gesture
            .borrow()
            .as_ref()
            .map(|gesture| (gesture.preview_width, gesture.preview_height)),
        Some((original_size.0, original_size.1 + 10)),
        "resize arrows preview ten logical pixels"
    );
    assert_eq!(
        (
            state.current.borrow().objects[1].width,
            state.current.borrow().objects[1].height
        ),
        original_size,
        "resize preview does not change saved geometry before Enter"
    );
    press_key(&app, slint::platform::Key::Escape);
    assert_eq!(
        (
            state.current.borrow().objects[1].width,
            state.current.borrow().objects[1].height
        ),
        original_size,
        "Escape cancels the resize preview"
    );

    let path = std::env::temp_dir().join(format!(
        "loom-sheets-object-keyboard-{}-{}.loomtable",
        std::process::id(),
        state.next_worker_revision()
    ));
    save_workbook(&path, &[state.current.borrow().clone()], 0)
        .expect("save keyboard-selected object geometry");
    let reopened = load_workbook(&path).expect("reopen keyboard-edited workbook");
    assert_eq!(
        reopened.sheets[0].objects[1].anchor,
        CellRef { row: 4, col: 6 }
    );
    assert_eq!(
        (
            reopened.sheets[0].objects[1].width,
            reopened.sheets[0].objects[1].height
        ),
        original_size
    );
    std::fs::remove_file(&path).expect("remove test workbook");

    press_key(&app, slint::platform::Key::F6);
    app.invoke_focus_formula_bar();
    press_key(&app, slint::platform::Key::Tab);
    assert_eq!(
        app.get_selected_object(),
        1,
        "Tab after object focus leaves must follow the formula bar focus order"
    );
}

#[test]
fn keyboard_move_preview_beyond_committed_extent_remains_visible() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    let mut sheet = Sheet::new("Object preview viewport");
    let mut object = SheetObject::shape(CellRef { row: 99, col: 25 }, "Far callout");
    object.width = 300;
    object.height = 120;
    sheet.objects.push(object);
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    object_actions::register_object_actions(&app, &state, &menu_service);
    project_current(&app, &state);
    app.invoke_object_keyboard_action(0, 0, 0, 0);
    app.invoke_object_move_started(0);

    for _ in 0..20 {
        app.invoke_object_keyboard_action(0, 1, 0, 1);
    }

    assert_eq!(
        state
            .object_gesture
            .borrow()
            .as_ref()
            .unwrap()
            .preview_anchor
            .row,
        119
    );
    assert!(
        app.get_object_views()
            .row_data(0)
            .is_some_and(|object| object.visible),
        "the moved preview should remain visible after its anchor passes the committed sheet extent"
    );
}

#[test]
fn accessible_object_selection_cancels_another_objects_preview() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    let mut sheet = Sheet::new("Accessible object selection");
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 1, col: 2 }, "First"));
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 4, col: 5 }, "Second"));
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    object_actions::register_object_actions(&app, &state, &menu_service);
    project_current(&app, &state);
    app.set_object_state(2);
    app.invoke_object_move_started(0);
    app.invoke_object_keyboard_action(0, 1, 1, 0);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("build accessible object tree");
    let target = ElementHandle::find_by_accessible_label(&app, "Shape 2: Second")
        .next()
        .expect("second shape is an accessible object");

    target.invoke_accessible_default_action();

    assert_eq!(app.get_selected_object(), 1);
    assert!(
        state.object_gesture.borrow().is_none(),
        "selecting the second object must cancel the first object's preview"
    );
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 2 },
        "the uncommitted first-object geometry must not be committed"
    );
}

#[test]
fn worksheet_objects_command_cancels_preview_before_changing_selection() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    let mut sheet = Sheet::new("Command object selection");
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 1, col: 2 }, "First"));
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 4, col: 5 }, "Second"));
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        Rc::new(loom_desktop::ScriptedFileDialogs::new([], [])),
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    object_actions::register_object_actions(&app, &state, &menu_service);
    project_current(&app, &state);
    app.set_selected_object(1);
    app.set_object_state(2);
    app.invoke_object_move_started(1);
    app.invoke_object_keyboard_action(1, 1, 1, 0);

    assert!(dispatch_command(&app, "sheets.worksheet-objects"));

    assert_eq!(app.get_selected_object(), 1);
    assert!(
        state.object_gesture.borrow().is_none(),
        "the command must cancel the active preview before choosing its navigation target"
    );
    assert_eq!(
        state.current.borrow().objects[1].anchor,
        CellRef { row: 4, col: 5 },
        "the command must not commit an unconfirmed move"
    );
}

#[test]
fn control_z_cancels_object_preview_before_undoing_the_prior_edit() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let cell = CellRef::parse("A1").unwrap();
    let mut before = Sheet::new("Object undo interruption");
    before
        .objects
        .push(SheetObject::shape(CellRef { row: 1, col: 2 }, "Callout"));
    let mut after = before.clone();
    after.set_raw(cell, "cell edit");
    let state = Rc::new(GuiState::new(
        after.clone(),
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    push_history(
        &mut state.undo_stack.borrow_mut(),
        SheetTransaction::Snapshot {
            before: Box::new(before),
            after: Box::new(after),
        },
    );
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_history_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    project_current(&app, &state);

    app.invoke_focus_grid();
    press_key(&app, slint::platform::Key::F6);
    press_text(&app, "m");
    press_key(&app, slint::platform::Key::RightArrow);
    press_control_text(&app, "z");

    assert_eq!(
        state.current.borrow().raw(cell),
        None,
        "Ctrl+Z undoes the prior cell edit"
    );
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 2 },
        "the uncommitted object preview was cancelled first"
    );
    assert!(state.object_gesture.borrow().is_none());
    assert_eq!(state.redo_stack.borrow().len(), 1);
}

#[test]
fn cancelling_and_undoing_object_previews_preserve_interleaved_cell_edits() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let mut sheet = Sheet::new("Object preview isolation");
    sheet.set_raw(CellRef::parse("A1").unwrap(), "before");
    sheet
        .objects
        .push(SheetObject::shape(CellRef { row: 1, col: 2 }, "Callout"));
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_history_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    project_current(&app, &state);

    app.invoke_object_move_started(0);
    app.invoke_object_keyboard_action(0, 1, 1, 0);
    state
        .current
        .borrow_mut()
        .set_raw(CellRef::parse("A1").unwrap(), "edited during preview");
    app.invoke_object_keyboard_action(0, -3, 0, 0);
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("edited during preview"),
        "cancel restores only the object field being previewed"
    );
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 2 }
    );

    app.invoke_object_move_started(0);
    app.invoke_object_keyboard_action(0, 1, 1, 0);
    state
        .current
        .borrow_mut()
        .set_raw(CellRef::parse("A1").unwrap(), "committed during preview");
    app.invoke_object_keyboard_action(0, -1, 0, 0);
    app.invoke_undo();
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("committed during preview"),
        "undoing the object gesture preserves edits made before it committed"
    );
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 1, col: 2 }
    );
}

#[test]
fn stale_object_gesture_cannot_replace_a_new_workbook() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let mut original = Sheet::new("Old document");
    original
        .objects
        .push(SheetObject::shape(CellRef { row: 1, col: 2 }, "Old object"));
    let state = Rc::new(GuiState::new(
        original,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    object_actions::register_object_actions(&app, &state, &menu_service);
    app.invoke_object_move_started(0);
    app.invoke_object_keyboard_action(0, 1, 1, 0);

    let mut replacement = Sheet::new("New document");
    replacement.set_raw(CellRef::parse("A1").unwrap(), "replacement");
    replacement
        .objects
        .push(SheetObject::shape(CellRef { row: 9, col: 9 }, "New object"));
    state.install_workbook(vec![replacement], 0);
    app.invoke_object_keyboard_action(0, -3, 0, 0);

    assert_eq!(state.current.borrow().name, "New document");
    assert_eq!(
        state.current.borrow().raw(CellRef::parse("A1").unwrap()),
        Some("replacement")
    );
    assert_eq!(
        state.current.borrow().objects[0].anchor,
        CellRef { row: 9, col: 9 }
    );
}

/// Build a workbook with two anchored shapes and a visible chart, which is the
/// configuration where the F6 collision appeared: a visible chart consumed F6
/// and left anchored objects unreachable from the keyboard.
fn sheet_with_shapes_and_chart() -> (
    SheetsApp,
    std::rc::Rc<GuiState>,
    std::sync::Arc<NativeMenuBar>,
) {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let mut sheet = Sheet::new("Mixed content");
    sheet.objects.push(SheetObject::shape(
        CellRef { row: 1, col: 2 },
        "First callout",
    ));
    sheet.objects.push(SheetObject::shape(
        CellRef { row: 4, col: 5 },
        "Second callout",
    ));
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_history_actions(&app, &state, &menu_service);
    object_actions::register_object_actions(&app, &state, &menu_service);
    app.set_chart_title("Revenue".into());
    app.set_chart_source_range("A1:B3".into());
    app.set_chart_categories(
        Rc::new(VecModel::from(vec![
            SharedString::from("One"),
            SharedString::from("Two"),
        ]))
        .into(),
    );
    app.set_chart_values_display(
        Rc::new(VecModel::from(vec![
            SharedString::from("10"),
            SharedString::from("20"),
        ]))
        .into(),
    );
    app.set_chart_normalized(Rc::new(VecModel::from(vec![0.5f32, 1.0f32])).into());
    project_current(&app, &state);
    // Show the chart last: the projection above resets the chart panel.
    app.set_chart_visible(true);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render objects with a chart");
    assert!(
        app.get_chart_visible(),
        "the chart must be visible for this case"
    );
    (app, state, menu_service)
}

/// UI-37: a visible chart used to take F6 exclusively, so the command palette's
/// advertised `sheets.worksheet-objects` shortcut could not reach the anchored
/// objects at all. F6 must cycle rather than collide.
#[test]
fn f6_reaches_anchored_objects_even_when_a_chart_is_visible() {
    let (app, _state, _menu) = sheet_with_shapes_and_chart();
    assert!(
        app.get_chart_visible(),
        "the chart is visible for this case"
    );

    app.invoke_focus_grid();
    press_key(&app, slint::platform::Key::F6);
    assert_eq!(
        app.get_selected_object(),
        0,
        "F6 from the grid must reach anchored objects even with a chart visible"
    );

    // F6 leaves object navigation and returns control to the grid.
    press_key(&app, slint::platform::Key::F6);
    assert_eq!(
        app.get_object_state(),
        0,
        "F6 must leave object navigation rather than trapping focus"
    );

    // And the palette's advertised command still reaches the same objects.
    app.invoke_focus_grid();
    press_key(&app, slint::platform::Key::F6);
    assert_eq!(app.get_selected_object(), 0);
}

/// UI-37: the move and resize instructions are only true of the object actually
/// in that mode. They used to be announced on every object in the list.
#[test]
fn only_the_selected_object_carries_the_move_or_resize_instructions() {
    let (app, _state, _menu) = sheet_with_shapes_and_chart();
    app.invoke_focus_grid();
    press_key(&app, slint::platform::Key::F6);
    assert_eq!(app.get_selected_object(), 0);
    press_text(&app, "m");
    assert_eq!(app.get_object_state(), 2, "Move preview is active");

    let lists: Vec<_> =
        ElementHandle::find_by_accessible_label(&app, "Worksheet objects").collect();
    assert_eq!(lists.len(), 1, "objects are grouped as one named list");
    let mut descriptions: Vec<String> = Vec::new();
    lists[0].visit_descendants(|element| {
        if element.accessible_role() == Some(AccessibleRole::ListItem) {
            if let Some(text) = element.accessible_description() {
                descriptions.push(text.to_string());
            }
        }
        ControlFlow::<()>::Continue(())
    });
    assert_eq!(
        descriptions.len(),
        2,
        "both shapes must expose a description, got {descriptions:?}"
    );
    let previewing = descriptions
        .iter()
        .filter(|text| text.contains("Move preview"))
        .count();
    assert_eq!(
        previewing, 1,
        "exactly one object may report a move preview, got {descriptions:?}"
    );
    assert!(
        descriptions[0].contains("Selected.") && descriptions[0].contains("Move preview"),
        "the selected object carries the live mode hint, got {:?}",
        descriptions[0]
    );
    assert!(
        descriptions[1].contains("Anchor") && !descriptions[1].contains("Move preview"),
        "an unselected object keeps its geometry but not the live mode hint, got {:?}",
        descriptions[1]
    );
}

/// UI-37: returning from object mode scrolled the grid to the object and left
/// the selected cell offscreen, so the address pill read a cell the user could
/// not see. Escape must reveal the selection before focus returns.
#[test]
fn escape_from_object_mode_reveals_the_selected_cell() {
    let (app, _state, _menu) = sheet_with_shapes_and_chart();
    // Put the selection far away from the objects, then focus an object so the
    // viewport scrolls away from it.
    app.set_selected_cell(SharedString::from("A1"));
    app.set_selection_range(SharedString::from("A1"));
    app.invoke_reveal_selection();
    app.invoke_focus_grid();
    press_key(&app, slint::platform::Key::F6);
    assert_eq!(app.get_selected_object(), 0);
    // Scroll far away from A1, as following a distant object would.
    app.set_grid_scroll_x(-4_000.0);
    app.set_grid_scroll_y(-6_000.0);

    press_key(&app, slint::platform::Key::Escape);
    app.invoke_reveal_selection();
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("re-render after Escape");

    assert_eq!(app.get_object_state(), 0, "Escape leaves object navigation");
    let revealed: Vec<_> = ElementHandle::find_by_accessible_label(&app, "A1").collect();
    assert!(
        !revealed.is_empty(),
        "the selected cell must be back in the accessibility tree after Escape"
    );
}

/// UI-37: object actions had no visible chrome, so a keyboard user could not see
/// what a move or resize did. Every transition now reports in the status bar.
#[test]
fn object_keyboard_actions_report_their_outcome_in_the_status_bar() {
    let (app, _state, _menu) = sheet_with_shapes_and_chart();
    app.invoke_focus_grid();
    press_key(&app, slint::platform::Key::F6);
    let selected = app.get_status_left().to_string();
    assert!(
        selected.contains("Shape 1") && selected.contains("M moves"),
        "selection must name the object and its keyboard actions, got {selected:?}"
    );

    press_text(&app, "m");
    press_key(&app, slint::platform::Key::RightArrow);
    let previewing = app.get_status_left().to_string();
    assert!(
        previewing.contains("Moving") && previewing.contains("Enter commits"),
        "a move preview must say what it is doing and how to finish, got {previewing:?}"
    );

    press_key(&app, slint::platform::Key::Return);
    let committed = app.get_status_left().to_string();
    assert!(
        committed.contains("moved") && committed.contains("pixels"),
        "a committed move must report the resulting geometry, got {committed:?}"
    );

    press_text(&app, "r");
    press_key(&app, slint::platform::Key::Escape);
    let cancelled = app.get_status_left().to_string();
    assert!(
        cancelled.contains("cancelled"),
        "cancelling a preview must say so, got {cancelled:?}"
    );
}
