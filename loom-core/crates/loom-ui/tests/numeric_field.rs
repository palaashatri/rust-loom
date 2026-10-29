include!(concat!(env!("OUT_DIR"), "/numeric_field_probe.rs"));

use slint::platform::{Key, WindowEvent};
use slint::{ComponentHandle, SharedString};

fn press(window: &NumericFieldProbeWindow, key: Key) {
    let text = SharedString::from(key);
    window
        .window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    window
        .window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

#[test]
fn numeric_field_keeps_units_separate_and_dispatches_accept_cancel_and_invalid_state() {
    loom_test_support::capture::set_platform();
    let window = NumericFieldProbeWindow::new().expect("create numeric field probe");
    assert_eq!(window.get_field_value(), "12.5");
    assert_eq!(window.get_field_unit(), "pt");

    let theme = Theme::get(&window);
    let mut focus_visual_checked = false;
    for theme_name in ["light", "dark", "high-contrast"] {
        theme.set_active_theme(theme_name.into());
        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            theme.set_text_scale(scale);
            window.set_field_invalid(false);
            if !focus_visual_checked {
                let unfocused =
                    loom_test_support::capture::snapshot_component(&window, 420.0, 120.0, 1.0)
                        .expect("render unfocused numeric field");
                window.invoke_focus_field();
                let focused =
                    loom_test_support::capture::snapshot_component(&window, 420.0, 120.0, 1.0)
                        .expect("render focused numeric field");
                assert_ne!(
                    unfocused, focused,
                    "keyboard focus must have a visible ring"
                );
                focus_visual_checked = true;
            } else {
                window.invoke_focus_field();
            }
            let valid = loom_test_support::capture::snapshot_component(&window, 420.0, 120.0, 1.0)
                .expect("render valid numeric field");
            window.set_field_invalid(true);
            let invalid =
                loom_test_support::capture::snapshot_component(&window, 420.0, 120.0, 1.0)
                    .expect("render invalid numeric field");
            assert_ne!(
                valid, invalid,
                "invalid border must show in {theme_name} at {scale}x text"
            );
        }
    }

    window.invoke_focus_field();
    press(&window, Key::Return);
    assert_eq!(window.get_accepted_count(), 1);
    assert_eq!(window.get_accepted_value(), "12.5");

    window.set_field_value("invalid".into());
    window.invoke_focus_field();
    press(&window, Key::Escape);
    assert_eq!(window.get_cancelled_count(), 1);
}
