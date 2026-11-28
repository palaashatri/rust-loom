include!(concat!(env!("OUT_DIR"), "/font_picker_probe.rs"));

use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, SharedString};

fn click(window: &FontPickerProbeWindow, x: f32, y: f32) {
    let position = slint::LogicalPosition::new(x, y);
    window
        .window()
        .dispatch_event(WindowEvent::PointerMoved { position });
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

fn press(window: &FontPickerProbeWindow, key: Key) {
    let text = SharedString::from(key);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn font_picker_opens_navigates_selects_and_dismisses_via_keyboard_and_pointer() {
    loom_test_support::capture::set_platform();
    let window = FontPickerProbeWindow::new().expect("create font picker probe");
    Theme::get(&window).set_active_theme("light".into());

    let closed = loom_test_support::capture::snapshot_component(&window, 360.0, 360.0, 1.0)
        .expect("render closed font picker");
    assert!(!window.get_open());

    // Click trigger to focus and open popup.
    let trigger =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&window, "Font family")
            .next()
            .expect("Font family combobox exists");
    let pos = trigger.absolute_position();
    let size = trigger.size();
    click(&window, pos.x + size.width * 0.5, pos.y + size.height * 0.5);
    assert!(
        window.get_open(),
        "clicking trigger opens font picker (pos={pos:?}, size={size:?})"
    );
    let opened = loom_test_support::capture::snapshot_component(&window, 360.0, 360.0, 1.0)
        .expect("render open font picker");
    assert_ne!(closed, opened, "opening font picker changes rendered frame");

    // Navigate down twice (0 -> 2: Georgia), then up once (2 -> 1: SF Pro Text), then Enter.
    press(&window, Key::DownArrow);
    press(&window, Key::DownArrow);
    assert_eq!(window.get_highlighted_index(), 2);
    press(&window, Key::UpArrow);
    assert_eq!(window.get_highlighted_index(), 1);
    press(&window, Key::Return);

    assert!(!window.get_open(), "Enter closes popup after selecting");
    assert_eq!(window.get_selected_font(), "SF Pro Text");
    assert_eq!(window.get_select_count(), 1);

    // Open with DownArrow and dismiss with Escape.
    press(&window, Key::DownArrow);
    assert!(window.get_open(), "DownArrow opens closed font picker");
    press(&window, Key::Escape);
    assert!(!window.get_open(), "Escape closes font picker");
    assert_eq!(window.get_cancel_count(), 1);

    // Verify combobox accessible label exists and RTL renders cleanly.
    let combobox =
        i_slint_backend_testing::ElementHandle::find_by_accessible_label(&window, "Font family")
            .next()
            .expect("Font family combobox is accessible");
    assert_eq!(combobox.accessible_value().as_deref(), Some("SF Pro Text"));

    Theme::get(&window).set_rtl(true);
    window.set_open(true);
    let rtl_opened = loom_test_support::capture::snapshot_component(&window, 360.0, 360.0, 1.0)
        .expect("render RTL open font picker");
    assert_ne!(opened, rtl_opened, "RTL mode mirrors font picker rendering");
}
