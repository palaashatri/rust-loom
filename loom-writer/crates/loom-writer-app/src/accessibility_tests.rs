//! Every interactive element on the main window must be reachable by assistive
//! technology with a non-empty name.

use super::actions_tests::{test_state, text_document};
use super::*;
use i_slint_backend_testing::{AccessibleRole, ElementHandle};

fn unnamed_interactive(app: &WriterApp) -> Vec<String> {
    // The first Rectangle is the window's content root; its descendants are
    // the whole main-window tree (overlays included).
    let root = ElementHandle::find_by_element_type_name(app, "Rectangle")
        .next()
        .expect("root element");
    let mut bad = Vec::new();
    for role in [
        AccessibleRole::Button,
        AccessibleRole::TextInput,
        AccessibleRole::Checkbox,
        AccessibleRole::Combobox,
        AccessibleRole::Slider,
        AccessibleRole::Tab,
    ] {
        for el in root
            .query_descendants()
            .match_accessible_role(role)
            .find_all()
        {
            let name = el.accessible_label().unwrap_or_default();
            if name.trim().is_empty() {
                let pos = el.absolute_position();
                let size = el.size();
                bad.push(format!(
                    "{role:?} id={:?} type={:?} at {:?} size {:?}",
                    el.id(),
                    el.type_name(),
                    pos,
                    size
                ));
            }
        }
    }
    assert!(
        root.query_descendants()
            .match_accessible_role(AccessibleRole::Button)
            .find_all()
            .len()
            > 5,
        "the walk must actually reach the toolbar buttons"
    );
    bad
}

fn app_at(width: u32, height: u32) -> WriterApp {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document("Hello"), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    let _ = loom_test_support::capture::snapshot_component(&app, width as f32, height as f32, 1.0)
        .expect("render");
    app
}

#[test]
fn every_interactive_element_in_the_writer_window_has_an_accessible_name() {
    for (w, h) in [(1280, 800), (1024, 720)] {
        let app = app_at(w, h);
        let bad = unnamed_interactive(&app);
        assert!(bad.is_empty(), "unnamed at {w}x{h}: {bad:#?}");
    }
}

// --- keyboard focus without a click -----------------------------------------

use slint::platform::{Key, WindowEvent};

fn key(app: &WriterApp, text: impl Into<slint::SharedString>) {
    let text = text.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn ctrl(app: &WriterApp, letter: &str) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key(app, letter);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
}

fn blank_app() -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], [None]));
    let (app, state) = test_state(text_document(""), dialogs);
    wire_writer_shared_callbacks(&app, &state, None);
    let _ =
        loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    focus_page_at_launch(&app);
    (app, state)
}

fn typed(state: &GuiState) -> String {
    state.current.borrow().plain_text()
}

#[test]
fn typing_reaches_the_page_at_launch_without_any_click() {
    let (app, state) = blank_app();
    key(&app, "h");
    key(&app, "i");
    assert_eq!(typed(&state), "hi");
}

#[test]
fn ctrl_f_opens_find_at_launch_and_escape_returns_focus_to_the_page() {
    let (app, state) = blank_app();
    ctrl(&app, "f");
    assert!(
        app.global::<FindBar>().get_open(),
        "Ctrl+F reaches the window"
    );
    key(&app, slint::SharedString::from(Key::Escape));
    assert!(!app.global::<FindBar>().get_open());
    key(&app, "z");
    assert_eq!(typed(&state), "z", "typing lands in the page after Escape");
}

#[test]
fn closing_the_palette_returns_focus_to_the_page() {
    let (app, state) = blank_app();
    app.invoke_open_palette();
    assert!(app.get_palette_open());
    // A real window lays the palette out before the next key arrives.
    let _ = loom_test_support::capture::snapshot_component(&app, 1280.0, 800.0, 1.0);
    app.invoke_open_palette();
    key(&app, slint::SharedString::from(Key::Escape));
    assert!(!app.get_palette_open());
    key(&app, "q");
    assert_eq!(typed(&state), "q");
}
