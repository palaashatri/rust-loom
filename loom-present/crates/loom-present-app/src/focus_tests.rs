//! The slide editor owns keyboard focus from launch: arrows, Delete and
//! Ctrl+Z work with no prior click, and focus returns after overlays close.

use super::*;
use slint::platform::{Key, WindowEvent};

fn launched() -> (PresentApp, Rc<GuiState>) {
    set_platform();
    let app = PresentApp::new().expect("create PresentApp");
    let session = sample_session();
    let state = Rc::new(GuiState {
        last_saved: RefCell::new(session.document.clone()),
        last_saved_transitions: RefCell::default(),
        session: RefCell::new(session),
        pending_replacement: Cell::new(None),
        selected_element: Cell::new(0),
        inspector_available: Cell::new(true),
        save_path: RefCell::new(None),
        dialogs: Rc::new(loom_desktop::ScriptedFileDialogs::default()),
        deck_filter: FileFilter::new("Loom Present deck", ["loomdeck"]).expect("filter"),
        pdf_filter: FileFilter::new("PDF document", ["pdf"]).expect("filter"),
        menu_service: None,
        drag_state: RefCell::new(DragState::default()),
    });
    wire_app_callbacks(&app, &state);
    wire_palette(&app);
    state
        .session
        .borrow_mut()
        .select_element("cover-title", false);
    refresh(&app, &state);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    focus_editor_at_launch(&app);
    (app, state)
}

fn key(app: &PresentApp, text: impl Into<SharedString>) {
    let text = text.into();
    app.window()
        .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
    app.window()
        .dispatch_event(WindowEvent::KeyReleased { text });
}

fn ctrl(app: &PresentApp, letter: &str) {
    app.window().dispatch_event(WindowEvent::KeyPressed {
        text: Key::Control.into(),
    });
    key(app, letter);
    app.window().dispatch_event(WindowEvent::KeyReleased {
        text: Key::Control.into(),
    });
}

fn element_count(app: &PresentApp) -> usize {
    app.get_element_contents().row_count()
}

#[test]
fn delete_and_ctrl_z_reach_the_slide_at_launch_without_a_click() {
    let (app, _state) = launched();
    assert_eq!(element_count(&app), 2);
    key(&app, SharedString::from(Key::Delete));
    assert_eq!(element_count(&app), 1, "Delete removes the selection");
    ctrl(&app, "z");
    assert_eq!(element_count(&app), 2, "Ctrl+Z restores it");
}

#[test]
fn arrow_keys_nudge_the_selection_at_launch_without_a_click() {
    let (app, state) = launched();
    let x = |s: &GuiState| s.session.borrow().document.slides[0].elements[0].x;
    let before = x(&state);
    key(&app, SharedString::from(Key::RightArrow));
    assert_ne!(x(&state), before, "the arrow nudged the selected element");
}

#[test]
fn closing_the_palette_returns_focus_to_the_slide_editor() {
    let (app, _state) = launched();
    app.invoke_open_palette();
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0);
    app.invoke_open_palette();
    assert!(app.get_palette_open());
    key(&app, SharedString::from(Key::Escape));
    assert!(!app.get_palette_open());
    key(&app, SharedString::from(Key::Delete));
    assert_eq!(element_count(&app), 1, "Delete reaches the slide again");
}
