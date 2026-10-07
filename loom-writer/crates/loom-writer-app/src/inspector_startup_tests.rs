//! Startup's docked inspector must not become an unsolicited compact drawer.

use super::actions_tests::{test_state, text_document};
use super::*;

fn launched(width: u32) -> (WriterApp, Rc<GuiState>) {
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(text_document("Hello"), dialogs);
    wire_writer_inspector_toggle(&app, &state, None);
    wire_responsive_layout(&app);
    apply_layout_breakpoints(&app, width);
    toolbar_commands::start_with_inspector_open(&app, width);
    (app, state)
}

#[test]
fn auto_opened_inspector_closes_when_actual_width_is_compact() {
    let (app, _state) = launched(1280);
    assert!(app.get_show_inspector());
    assert!(!app.get_compact_inspector_layout());

    app.invoke_window_resized(1018.0);

    assert!(app.get_compact_inspector_layout());
    assert!(!app.get_show_inspector(), "startup must not cover the page");
}

#[test]
fn user_opened_inspector_survives_compact_resize() {
    for width in [1280, 1024] {
        let (app, _state) = launched(width);
        if app.get_show_inspector() {
            app.invoke_toggle_inspector();
            assert!(!app.get_show_inspector());
        }
        assert!(toolbar_commands::dispatch(&app, "view.inspector"));
        assert!(app.get_show_inspector());

        app.invoke_window_resized(1018.0);

        assert!(app.get_compact_inspector_layout());
        assert!(app.get_show_inspector(), "preserve the user's open drawer");
    }
}

#[test]
fn widening_does_not_reopen_the_startup_inspector() {
    let (app, _state) = launched(1280);
    app.invoke_window_resized(1018.0);
    app.invoke_window_resized(1280.0);

    assert!(!app.get_compact_inspector_layout());
    assert!(!app.get_show_inspector(), "widening must not reopen it");
}
