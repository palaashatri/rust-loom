//! The icon-over-label toolbar and its menus send command ids; this module
//! runs them through the same callbacks as the menu bar, the palette and the
//! inspector, and decides how the window starts (inspector open on Format).

use slint::{ComponentHandle, Global};

use crate::{dispatch_command, FindBar, Theme, WriterApp};

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
    app.set_inspector_tab(0);
    let docked =
        width as f32 / Theme::get(app).get_text_scale().max(1.0) >= DOCKED_INSPECTOR_MIN_WIDTH;
    if docked {
        app.set_show_inspector(true);
    }
}
