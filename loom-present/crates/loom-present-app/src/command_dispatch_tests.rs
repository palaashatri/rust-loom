use super::*;
use crate::command_dispatch::{
    dispatch_command, dispatch_shortcut, is_present_menu_command, shortcut_for_id, PRIMARY_MOD,
};

#[test]
fn command_dispatch_shortcut_hints_match_platform() {
    let expected_prefix = if cfg!(target_os = "macos") {
        "Cmd"
    } else {
        "Ctrl"
    };
    assert_eq!(PRIMARY_MOD, expected_prefix);
    assert!(shortcut_for_id("app.palette").starts_with(expected_prefix));
    assert!(shortcut_for_id("file.print_preview").starts_with(expected_prefix));
    assert!(shortcut_for_id("edit.find").starts_with(expected_prefix));
    assert_eq!(shortcut_for_id("view.preview"), "F5");
}

#[test]
fn command_dispatch_recognizes_all_present_menu_commands() {
    for id in [
        "file.new",
        "file.start_hub",
        "file.print_preview",
        "edit.find",
        "edit.replace",
        "insert.text_box",
        "slide.new",
        "format.bold",
        "arrange.bring_front",
        "help.welcome_guide",
        "app.palette",
    ] {
        assert!(is_present_menu_command(id), "missing command: {id}");
    }
}

#[test]
fn command_dispatch_shortcuts_open_modals() {
    let app = PresentApp::new().expect("create PresentApp");
    assert!(dispatch_shortcut(&app, "k", false, true, false));
    assert!(app.get_palette_open());
    assert!(dispatch_command(&app, "help.welcome_guide"));
    assert!(app.global::<crate::WelcomeGuideState>().get_open());
}
