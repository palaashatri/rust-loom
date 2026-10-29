include!(concat!(env!("OUT_DIR"), "/local_menu_probe.rs"));

use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, SharedString};

fn move_pointer(window: &LocalMenuProbeWindow, x: f32, y: f32) {
    window.window().dispatch_event(WindowEvent::PointerMoved {
        position: slint::LogicalPosition::new(x, y),
    });
}

fn click(window: &LocalMenuProbeWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    window.window().dispatch_event(WindowEvent::PointerPressed {
        position,
        button: slint::platform::PointerEventButton::Left,
    });
    window
        .window()
        .dispatch_event(WindowEvent::PointerReleased {
            position,
            button: slint::platform::PointerEventButton::Left,
        });
}

fn press(window: &LocalMenuProbeWindow, key: Key) {
    let text = SharedString::from(key);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn local_menu_pointer_hover_switches_and_enter_activates_hovered_item() {
    loom_test_support::capture::set_platform();
    let window = LocalMenuProbeWindow::new().expect("create local menu probe");
    Theme::get(&window).set_active_theme("light".into());

    let closed = loom_test_support::capture::snapshot_component(&window, 520.0, 300.0, 1.0)
        .expect("render closed menu");
    move_pointer(&window, 24.0, 18.0);
    let file_hovered = loom_test_support::capture::snapshot_component(&window, 520.0, 300.0, 1.0)
        .expect("render hovered File menu");
    assert_ne!(closed, file_hovered, "Light menu hover needs visible paint");

    click(&window, 24.0, 18.0);
    assert_eq!(window.get_open_index(), 0);
    let file_open = loom_test_support::capture::snapshot_component(&window, 520.0, 300.0, 1.0)
        .expect("render open File menu");
    assert_ne!(closed, file_open, "opening the menu must change its paint");

    move_pointer(&window, 92.0, 18.0);
    assert_eq!(
        window.get_open_index(),
        1,
        "hover switches to Edit while open"
    );
    let edit_open = loom_test_support::capture::snapshot_component(&window, 520.0, 300.0, 1.0)
        .expect("render open Edit menu");
    assert_ne!(
        file_open, edit_open,
        "top-level switching must repaint the popup"
    );

    let paste = i_slint_backend_testing::ElementHandle::find_by_accessible_label(&window, "Paste")
        .next()
        .expect("Paste menu item is accessible");
    let paste_position = paste.absolute_position();
    let paste_size = paste.size();
    move_pointer(
        &window,
        paste_position.x + paste_size.width / 2.0,
        paste_position.y + paste_size.height / 2.0,
    );
    assert_eq!(
        window.get_selected_index(),
        1,
        "pointer hover selects Paste at {paste_position:?} {paste_size:?}"
    );
    assert_eq!(window.get_pointer_selection_sync_count(), 1);
    let row_hovered = loom_test_support::capture::snapshot_component(&window, 520.0, 300.0, 1.0)
        .expect("render hovered Paste row");
    assert_ne!(
        edit_open, row_hovered,
        "Light popup hover needs visible paint"
    );

    press(&window, Key::Return);
    assert_eq!(window.get_activated_command(), "edit.paste");
    assert_eq!(window.get_open_index(), -1);
}
