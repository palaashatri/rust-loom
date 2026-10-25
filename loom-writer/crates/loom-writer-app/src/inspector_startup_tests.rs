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

#[test]
fn compact_inspector_drawer_does_not_dim_the_page_outside_the_panel() {
    let (app, _state) = launched(1024);
    assert!(app.get_compact_inspector_layout());
    assert!(!app.get_show_inspector());
    let closed_frame = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render closed");
    let closed_pixel = closed_frame.get_pixel(250, 300).0;

    app.invoke_toggle_inspector();
    assert!(app.get_show_inspector());
    let open_frame = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render open");
    let open_pixel = open_frame.get_pixel(250, 300).0;
    assert_eq!(
        open_pixel, closed_pixel,
        "opening the compact inspector drawer must not dim the page outside the panel"
    );

    let pos = slint::LogicalPosition::new(250.0, 300.0);
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerPressed {
            position: pos,
            button: slint::platform::PointerEventButton::Left,
        });
    app.window()
        .dispatch_event(slint::platform::WindowEvent::PointerReleased {
            position: pos,
            button: slint::platform::PointerEventButton::Left,
        });
    assert!(
        !app.get_show_inspector(),
        "clicking outside the compact inspector drawer still closes it"
    );
}

#[test]
fn focus_page_at_launch_focuses_editor_synchronously_without_mutating_focus_during_tree_instantiation(
) {
    use slint::private_unstable_api::re_exports::WindowInner;
    let (app, _state) = launched(1280);
    // Simulate X11 app.show() initial draw instantiating the tree before focus_page_at_launch.
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("initial show draw");

    focus_page_at_launch(&app);
    let window_inner = WindowInner::from_pub(app.window());
    let before = window_inner
        .focus_item
        .borrow()
        .upgrade()
        .expect("focused item immediately after focus_page_at_launch");

    // AccessKitAdapter::build_new_tree calls ensure_tree_instantiated() while holding
    // accesskit_adapter.borrow_mut(). If ensure_tree_instantiated() changes focus_item,
    // WinitWindowAdapter::handle_focus_change re-borrows accesskit_adapter mutably and panics.
    window_inner.ensure_tree_instantiated();
    let after = window_inner
        .focus_item
        .borrow()
        .upgrade()
        .expect("focused item after ensure_tree_instantiated");

    assert_eq!(
        before, after,
        "ensure_tree_instantiated must not mutate focus_item after focus_page_at_launch"
    );
}

#[test]
fn narrowing_after_launch_closes_the_inspector_without_moving_focus_during_tree_build() {
    use slint::private_unstable_api::re_exports::WindowInner;
    // Launch docked (1296 px) with the inspector open and draw once as the
    // window first shows; the page owns keyboard focus.
    let (app, _state) = launched(1296);
    let _ = snapshot_component(&app, 1296.0, 800.0, 1.0).expect("initial show draw");
    let window_inner = WindowInner::from_pub(app.window());
    window_inner.ensure_tree_instantiated();
    let page = window_inner
        .focus_item
        .borrow()
        .upgrade()
        .expect("the page owns focus after launch");

    // Narrow to 1060 px right after launch: the auto-opened inspector closes.
    app.invoke_window_resized(1060.0);
    assert!(
        !app.get_show_inspector(),
        "the auto-opened inspector closes"
    );

    // AccessKit's build_new_tree runs pending change handlers while it holds
    // its adapter borrow. Closing must not move focus from inside them: a
    // focus change there re-borrows the adapter and panics on Windows and Linux.
    window_inner.ensure_tree_instantiated();
    let after = window_inner
        .focus_item
        .borrow()
        .upgrade()
        .expect("focused item after the tree build");
    assert_eq!(
        page, after,
        "closing the auto-opened inspector moved focus during tree instantiation"
    );
}

/// Whether the keyboard focus sits inside any element with this accessible
/// label, judged by window geometry (the focused item's centre inside it).
fn focus_is_on_element_labelled(app: &WriterApp, label: &str) -> bool {
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
    ElementHandle::find_by_accessible_label(app, label).any(|element| {
        let (p, s) = (element.absolute_position(), element.size());
        cx >= p.x && cx <= p.x + s.width && cy >= p.y && cy <= p.y + s.height
    })
}

#[test]
fn closing_the_docked_inspector_returns_focus_to_its_toolbar_tab() {
    // A user closes the inspector (the same callback its close button, its
    // Format tab and the View menu use). Focus goes back to the Format tab.
    let (app, _state) = launched(1280);
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render open");
    assert!(app.get_show_inspector());

    app.invoke_toggle_inspector();
    assert!(!app.get_show_inspector());
    let _ = snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render closed");

    assert!(
        focus_is_on_element_labelled(&app, "Format inspector"),
        "closing the inspector must return focus to its Format tab"
    );
}
