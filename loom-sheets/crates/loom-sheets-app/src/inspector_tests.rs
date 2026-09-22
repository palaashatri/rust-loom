use super::*;

#[test]
fn sheets_inspector_remains_available_and_remembers_the_user_choice_at_compact_width() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    assert!(!app.get_show_inspector());
    assert!(!app.get_inspector_preference());
    apply_layout_breakpoints(&app, 1024);
    assert!(app.get_overflow_toolbar());
    assert!(app.get_inspector_available());
    assert!(!app.get_show_inspector());
    apply_layout_breakpoints(&app, 1180);
    assert!(app.get_overflow_toolbar());
    assert!(app.get_inspector_available());
    assert!(!app.get_show_inspector());
    apply_layout_breakpoints(&app, 1280);
    assert!(app.get_overflow_toolbar());
    assert!(app.get_inspector_available() && !app.get_show_inspector());
    app.set_inspector_preference(true);
    app.set_show_inspector(true);
    apply_layout_breakpoints(&app, 1024);
    assert!(app.get_inspector_available() && app.get_show_inspector());
    apply_layout_breakpoints(&app, 1280);
    assert!(app.get_show_inspector());
    app.set_inspector_preference(false);
    app.set_show_inspector(false);
    apply_layout_breakpoints(&app, 1280);
    assert!(!app.get_show_inspector());
    apply_layout_breakpoints(&app, 1320);
    assert!(!app.get_overflow_toolbar());
}

#[test]
fn compact_inspector_escape_restores_keyboard_activation_to_format() {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    apply_layout_breakpoints(&app, 1024);
    app.set_selected_cell("C4".into());
    app.set_selection_range("C4:D5".into());
    let weak = app.as_weak();
    app.on_toggle_inspector(move || {
        let app = weak.upgrade().expect("live window");
        app.set_show_inspector(!app.get_show_inspector());
    });
    app.set_show_inspector(true);
    let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render compact inspector");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Escape.into(),
        });
    assert!(!app.get_show_inspector(), "Escape closes the inspector");
    let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render closed inspector");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Return.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Return.into(),
        });
    assert!(
        app.get_show_inspector(),
        "Enter reopens through the focused Format trigger"
    );
    let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render reopened inspector");
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Tab.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Tab.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyPressed {
            text: slint::platform::Key::Return.into(),
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::KeyReleased {
            text: slint::platform::Key::Return.into(),
        });
    assert!(!app.get_show_inspector(), "Tab then Enter activates Close");
    assert_eq!(app.get_selected_cell().as_str(), "C4");
    assert_eq!(app.get_selection_range().as_str(), "C4:D5");
}
