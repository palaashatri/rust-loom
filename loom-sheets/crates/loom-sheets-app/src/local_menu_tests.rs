use super::*;

#[test]
fn non_macos_window_exposes_a_local_application_menu_bar() {
    #[cfg(not(target_os = "macos"))]
    {
        i_slint_backend_testing::init_no_event_loop();
        let app = SheetsApp::new().expect("create SheetsApp");
        for label in ["File", "Edit", "View", "Table", "Help"] {
            let entries: Vec<_> =
                i_slint_backend_testing::ElementHandle::find_by_accessible_label(&app, label)
                    .collect();
            assert!(!entries.is_empty(), "the {label} menu should be visible");
        }
    }
}

#[test]
fn local_application_menu_supports_keyboard_navigation_and_activation() {
    #[cfg(not(target_os = "macos"))]
    {
        set_platform();
        let app = SheetsApp::new().expect("create SheetsApp");
        app.set_local_menu_items(ModelRc::new(VecModel::from(vec![
            LocalMenuEntry {
                menu_index: 0,
                label: "New".into(),
                command_id: "file.new".into(),
                shortcut: "Ctrl+N".into(),
                enabled: true,
                checked: false,
                separator: false,
            },
            LocalMenuEntry {
                menu_index: 0,
                label: "Save".into(),
                command_id: "file.save".into(),
                shortcut: "Ctrl+S".into(),
                enabled: false,
                checked: false,
                separator: false,
            },
            LocalMenuEntry {
                menu_index: 0,
                label: "Save As".into(),
                command_id: "file.save_as".into(),
                shortcut: "Ctrl+Shift+S".into(),
                enabled: true,
                checked: false,
                separator: false,
            },
            LocalMenuEntry {
                menu_index: 1,
                label: "Undo".into(),
                command_id: "edit.undo".into(),
                shortcut: "Ctrl+Z".into(),
                enabled: true,
                checked: false,
                separator: false,
            },
        ])));
        let invoked = Rc::new(RefCell::new(Vec::new()));
        let invoked_ref = invoked.clone();
        app.on_local_menu_action(move |id| invoked_ref.borrow_mut().push(id.to_string()));
        crate::local_menu::wire_keyboard(&app);

        app.invoke_focus_grid();
        let focus_before_menu =
            slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
                .focus_item
                .borrow()
                .upgrade()
                .expect("grid should own focus before opening the menu");
        app.set_local_menu_open_index(0);
        let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render File menu");
        assert_eq!(app.get_local_menu_popup_items().row_count(), 3);
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::DownArrow.into(),
            });
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Return.into(),
            });

        assert_eq!(*invoked.borrow(), ["file.save_as"]);
        assert_eq!(app.get_local_menu_open_index(), -1);

        app.set_local_menu_open_index(1);
        let _ = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render Edit menu");
        assert_eq!(app.get_local_menu_popup_items().row_count(), 1);
        assert_eq!(
            app.get_local_menu_popup_items().row_data(0).unwrap().label,
            "Undo"
        );
        app.window()
            .dispatch_event(slint::platform::WindowEvent::KeyPressed {
                text: slint::platform::Key::Escape.into(),
            });

        let focus_after_menu =
            slint::private_unstable_api::re_exports::WindowInner::from_pub(app.window())
                .focus_item
                .borrow()
                .upgrade()
                .expect("grid should regain focus when the menu closes");
        assert_eq!(focus_after_menu, focus_before_menu);
    }
}
