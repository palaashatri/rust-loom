//! Regressions for the 2026-10-07 hostile Linux review: actual geometry/pixels.
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};

fn control(app: &SheetsApp, label: &str) -> ElementHandle {
    // Buttons also expose their painted text run. Select the actual control,
    // while keeping the prompt's text node available for the ink measurement.
    let role = if label == app.get_save_changes_prompt().as_str() {
        AccessibleRole::Text
    } else if ["Table name", "Worksheet rows", "Worksheet columns"].contains(&label) {
        AccessibleRole::TextInput
    } else {
        AccessibleRole::Button
    };
    let items: Vec<_> = ElementHandle::find_by_accessible_label(app, label)
        .filter(|item| item.accessible_role() == Some(role))
        .collect();
    assert_eq!(items.len(), 1, "one control named {label}");
    items.into_iter().next().unwrap()
}

#[test]
fn hostile_inspector_fields_and_both_step_buttons_fit_inside_padding() {
    for scale in [1.0, 1.5, 2.0] {
        for rtl in [false, true] {
            let app = super::scale_surfaces_tests::editor(1024.0, 720.0, scale);
            configure_direction(&app, rtl);
            app.set_show_inspector(true);
            app.set_inspector_tab(0);
            let picture = snapshot_component(&app, 1024.0, 720.0, 1.0).unwrap();
            if let Ok(dir) = std::env::var("LOOM_SCALE_DUMP") {
                picture
                    .save(format!("{dir}/sheets-inspector-fit-{scale}-{rtl}.png"))
                    .unwrap();
            }
            eprintln!("inspector geometry at {scale}x, rtl={rtl}");
            let panel = ElementHandle::find_by_element_id(&app, "SheetsApp::inspector-panel")
                .next()
                .unwrap();
            let left = panel.absolute_position().x + 16.0;
            let right = panel.absolute_position().x + panel.size().width - 16.0;
            for label in [
                "Table name",
                "Worksheet rows",
                "Worksheet columns",
                "Add worksheet row",
                "Delete selected row",
                "Add worksheet column",
                "Delete selected column",
            ] {
                let item = control(&app, label);
                let pos = item.absolute_position();
                let size = item.size();
                assert!(pos.x >= left - 0.5 && pos.x + size.width <= right + 0.5,
                    "{label} must fit in inspector padding at {scale}x rtl={rtl}: x={} width={} allowed={left}..{right}", pos.x, size.width);
            }
        }
    }
}

#[test]
fn hostile_save_dialog_has_no_stretched_message_to_button_gap() {
    for scale in [1.0, 1.5, 2.0] {
        let app = super::scale_surfaces_tests::editor(1024.0, 720.0, scale);
        app.set_save_changes_document("Untitled".into());
        app.set_save_changes_close_mode(true);
        app.set_save_changes_open(true);
        let picture = snapshot_component(&app, 1024.0, 720.0, 1.0).unwrap();
        let prompt = control(&app, app.get_save_changes_prompt().as_str());
        let save = control(&app, "Save and close");
        let pos = prompt.absolute_position();
        let size = prompt.size();
        let ink = Theme::get(&app).invoke_palette().ink_secondary;
        let bottom_ink = (pos.y.ceil() as u32..(pos.y + size.height).floor() as u32)
            .rfind(|&y| {
                (pos.x.ceil() as u32..(pos.x + size.width).floor() as u32).any(|x| {
                    let pixel = picture.get_pixel(x, y).0;
                    pixel[0].abs_diff(ink.red()) < 20
                        && pixel[1].abs_diff(ink.green()) < 20
                        && pixel[2].abs_diff(ink.blue()) < 20
                })
            })
            .expect("prompt has painted text");
        let gap = save.absolute_position().y - bottom_ink as f32;
        assert!(
            (0.0..=24.0).contains(&gap),
            "content-sized dialog at {scale}x: message/action gap = {gap}"
        );
    }
}

#[test]
fn hostile_save_dialog_focus_ring_is_visible_outside_each_button() {
    for theme in ["light", "dark", "high-contrast"] {
        let app = super::scale_surfaces_tests::editor(1024.0, 720.0, 1.0);
        apply_theme(&app, theme);
        app.set_save_changes_document("Untitled".into());
        app.set_save_changes_close_mode(true);
        app.set_save_changes_open(true);
        for label in ["Save and close", "Cancel", "Discard"] {
            let picture = snapshot_component(&app, 1024.0, 720.0, 1.0).unwrap();
            let button = control(&app, label);
            let pos = button.absolute_position();
            let size = button.size();
            let expected = Theme::get(&app).invoke_palette().focus;
            let x = (pos.x + size.width / 2.0).round() as u32;
            let y = (pos.y - 3.0).round() as u32;
            let pixel = picture.get_pixel(x, y).0;
            assert_eq!(
                &pixel[..3],
                &[expected.red(), expected.green(), expected.blue()],
                "{theme}: {label} must have the shared focus ring outside its fill at ({x},{y})"
            );
            let key: SharedString = slint::platform::Key::Tab.into();
            app.window()
                .dispatch_event(slint::platform::WindowEvent::KeyPressed { text: key.clone() });
            app.window()
                .dispatch_event(slint::platform::WindowEvent::KeyReleased { text: key });
        }
    }
}
