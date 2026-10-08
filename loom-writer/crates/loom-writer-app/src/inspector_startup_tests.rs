//! Startup's docked inspector must not become an unsolicited compact drawer.

use super::actions_tests::{test_state, text_document};
use super::*;
use i_slint_backend_testing::ElementHandle;
use loom_test_support::capture::snapshot_component;

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

/// Whether the keyboard focus sits inside the inspector's Format/Document tab
/// strip, judged by window geometry (the focused item's centre inside the strip).
fn focus_is_on_inspector_tabs(app: &WriterApp) -> bool {
    use slint::private_unstable_api::re_exports::WindowInner;
    let Some(item) = WindowInner::from_pub(app.window())
        .focus_item
        .borrow()
        .upgrade()
    else {
        return false;
    };
    let geometry = item.geometry();
    let origin = item.map_to_window(geometry.origin);
    let (cx, cy) = (
        origin.x + geometry.size.width / 2.0,
        origin.y + geometry.size.height / 2.0,
    );
    let tabs = ElementHandle::find_by_accessible_label(app, "Inspector tab")
        .next()
        .expect("the inspector tab strip exists");
    let (p, s) = (tabs.absolute_position(), tabs.size());
    cx >= p.x && cx <= p.x + s.width && cy >= p.y && cy <= p.y + s.height
}

#[test]
fn startup_does_not_put_keyboard_focus_on_the_inspector_tabs() {
    // Startup as the window first renders it: the inspector opens and nothing
    // has been pressed or clicked, so no inspector tab may draw keyboard focus.
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let (app, state) = test_state(text_document("Hello"), dialogs);
    wire_writer_inspector_toggle(&app, &state, None);
    wire_responsive_layout(&app);
    apply_layout_breakpoints(&app, 1280);
    toolbar_commands::start_with_inspector_open(&app, 1280);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render");
    assert!(app.get_show_inspector());
    assert!(
        !focus_is_on_inspector_tabs(&app),
        "the inspector tab strip drew focus at startup"
    );
}
