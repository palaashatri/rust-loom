//! The icon-over-label toolbar and its menus send command ids; this module
//! runs them through the same callbacks as the menu bar, the palette and the
//! inspector, and decides how the window starts (inspector open on Format).

use std::cell::RefCell;

use slint::{ComponentHandle, Global};

use crate::{dispatch_command, FindBar, ResponsivePolicy, Theme, WriterApp};

/// One zoom step, as a fraction of actual size.
const ZOOM_STEP: f32 = 0.25;
/// Narrower than this (after text scaling) the inspector is a drawer over the
/// page, so a fresh window starts with it closed.
pub(crate) const DOCKED_INSPECTOR_MIN_WIDTH: f32 = 1180.0;

/// Runs a toolbar or toolbar-menu command; false for an unknown id.
pub(crate) fn dispatch(app: &WriterApp, id: &str) -> bool {
    match id {
        "format.strikethrough" => app.invoke_toggle_strikethrough(),
        "format.align_left" => app.invoke_select_alignment(0),
        "format.align_center" => app.invoke_select_alignment(1),
        "format.align_right" => app.invoke_select_alignment(2),
        "paragraph.body" => app.invoke_select_heading(0),
        "paragraph.h1" => app.invoke_select_heading(1),
        "paragraph.h2" => app.invoke_select_heading(2),
        "paragraph.h3" => app.invoke_select_heading(3),
        "list.none" => app.invoke_select_list_style(0),
        "list.bulleted" => app.invoke_select_list_style(1),
        "list.numbered" => app.invoke_select_list_style(2),
        "view.zoom_in" => app.invoke_page_zoom_changed(app.get_page_zoom() + ZOOM_STEP),
        "view.zoom_out" => app.invoke_page_zoom_changed(app.get_page_zoom() - ZOOM_STEP),
        "view.zoom_actual" => app.invoke_page_zoom_changed(1.0),
        "insert.comment" => app.invoke_open_comment_composer(),
        "edit.find" => app.global::<FindBar>().invoke_open_requested(false),
        _ => return dispatch_command(app, id),
    }
    true
}

/// Routes toolbar and menu commands.
pub(crate) fn wire(app: &WriterApp) {
    let app_ref = app.as_weak();
    app.on_toolbar_command(move |id| {
        if let Some(app) = app_ref.upgrade() {
            if !dispatch(&app, id.as_str()) {
                app.set_status_right(format!("Unsupported command: {id}").into());
            }
        }
    });
}

/// A fresh window shows the inspector on the Format tab. A window too narrow
/// to dock it keeps it closed, as the drawer would cover the page.
pub(crate) fn start_with_inspector_open(app: &WriterApp, width: u32) {
    clear_auto_opened_inspector(app);
    app.set_inspector_tab(0);
    let docked =
        width as f32 / Theme::get(app).get_text_scale().max(1.0) >= DOCKED_INSPECTOR_MIN_WIDTH;
    if docked {
        app.set_show_inspector(true);
        // The inspector's focus grab is for user-opened panels; at launch the page keeps focus.
        app.set_inspector_quiet_open(true);
        app.invoke_focus_page();
        AUTO_OPENED_INSPECTORS.with(|windows| windows.borrow_mut().push(app.as_weak()));
    }
}

std::thread_local! {
    // Weak handles keep startup state per window without retaining closed windows.
    static AUTO_OPENED_INSPECTORS: RefCell<Vec<slint::Weak<WriterApp>>> = const { RefCell::new(Vec::new()) };
}

/// Consumes startup ownership on compact resize or an explicit user toggle.
pub(crate) fn clear_auto_opened_inspector(app: &WriterApp) -> bool {
    AUTO_OPENED_INSPECTORS.with(|windows| {
        let mut cleared = false;
        windows.borrow_mut().retain(|weak| {
            let Some(window) = weak.upgrade() else {
                return false;
            };
            if std::ptr::eq(window.window(), app.window()) {
                cleared = true;
                false
            } else {
                true
            }
        });
        cleared
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResponsiveToolbarState {
    pub(crate) icon_only: bool,
    pub(crate) overflow: bool,
    pub(crate) labeled: bool,
}

pub(crate) fn layout_breakpoints(app: &WriterApp, width: u32) -> ResponsiveToolbarState {
    let policy = ResponsivePolicy::get(app);
    let width = width as f32 / Theme::get(app).get_text_scale().max(1.0);
    ResponsiveToolbarState {
        icon_only: width < policy.get_priority_1_icon_only_below(),
        overflow: width < policy.get_priority_2_overflow_below(),
        labeled: width >= policy.get_priority_2_overflow_below(),
    }
}

pub(crate) fn apply_layout_breakpoints(app: &WriterApp, width: u32) {
    let state = layout_breakpoints(app, width);
    app.set_icon_only_toolbar(state.icon_only);
    app.set_labeled_toolbar(state.labeled);
    app.set_wide_toolbar(state.labeled);
    app.set_labeled_export(state.labeled);
    if !state.overflow && app.get_toolbar_overflow_open() {
        app.invoke_close_toolbar_overflow();
    }
    app.set_overflow_toolbar(state.overflow);
    if !state.overflow {
        app.set_toolbar_overflow_open(false);
    }
    // Keep the same Format action available at every width. The shell layout
    // mode is sent through this input property instead of binding it back to
    // root.width from inside the Window's own layout tree.
    app.set_inspector_available(true);
    let compact =
        width as f32 / Theme::get(app).get_text_scale().max(1.0) < DOCKED_INSPECTOR_MIN_WIDTH;
    app.set_compact_inspector_layout(compact);
    if compact && clear_auto_opened_inspector(app) {
        app.set_show_inspector(false);
    }
}
