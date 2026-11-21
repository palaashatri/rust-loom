//! The icon-over-label toolbar and its menus send command ids; this module
//! runs them through the same callbacks as the menu bar, the palette and the
//! inspector, and decides how the window starts (inspector open on Format).

use std::cell::RefCell;

use slint::{ComponentHandle, Global};

use crate::{command_dispatch, ResponsivePolicy, Theme, WriterApp};

/// Keep the same breathing room that the page canvas reserves around a sheet.
const PAGE_FIT_GUTTER: f32 = 48.0;

/// One zoom step, as a fraction of actual size.
const ZOOM_STEP: f32 = 0.25;
/// Narrower than this (after text scaling) the inspector is a drawer over the
/// page, so a fresh window starts with it closed.
pub(crate) const DOCKED_INSPECTOR_MIN_WIDTH: f32 = 1180.0;

/// Step zoom in by one preset increment.
pub(crate) fn zoom_in(app: &WriterApp) {
    app.invoke_page_zoom_changed(app.get_page_zoom() + ZOOM_STEP);
}

/// Step zoom out by one preset increment.
pub(crate) fn zoom_out(app: &WriterApp) {
    app.invoke_page_zoom_changed(app.get_page_zoom() - ZOOM_STEP);
}

/// Reset zoom to 100%.
pub(crate) fn zoom_actual(app: &WriterApp) {
    app.invoke_page_zoom_changed(1.0);
}

/// Fit the page width into the available canvas viewport.
pub(crate) fn fit_width(app: &WriterApp) {
    fit_page(app, FitMode::Width);
}

/// Fit the full page into the available canvas viewport.
pub(crate) fn fit_page_view(app: &WriterApp) {
    fit_page(app, FitMode::Page);
}

/// Runs a toolbar or toolbar-menu command; false for an unknown id.
pub(crate) fn dispatch(app: &WriterApp, id: &str) -> bool {
    command_dispatch::dispatch_command(app, id)
}

#[derive(Clone, Copy)]
enum FitMode {
    Width,
    Page,
}

fn fit_page(app: &WriterApp, mode: FitMode) {
    let width = fit_scale(app.get_view_width(), app.get_page_width_pt());
    let height = match mode {
        FitMode::Width => None,
        FitMode::Page => fit_scale(app.get_page_view_height(), app.get_page_height_pt()),
    };
    let Some(scale) = width.and_then(|width| match (mode, height) {
        (FitMode::Width, _) => Some(width),
        (FitMode::Page, Some(height)) => Some(width.min(height)),
        (FitMode::Page, None) => None,
    }) else {
        app.set_status_right("Fit unavailable: page viewport geometry is not ready".into());
        return;
    };
    app.invoke_page_zoom_changed(crate::normalize_page_zoom(scale, app.get_page_zoom()));
}

fn fit_scale(viewport_extent: f32, page_extent: f32) -> Option<f32> {
    let available = viewport_extent - PAGE_FIT_GUTTER;
    if !available.is_finite() || available <= 0.0 || !page_extent.is_finite() || page_extent <= 0.0
    {
        return None;
    }
    let scale = available / page_extent;
    scale.is_finite().then_some(scale)
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
        AUTO_OPENED_INSPECTORS.with(|windows| windows.borrow_mut().push(app.as_weak()));
    }
    // A narrow window keeps the inspector closed, and typing must still land in
    // the page rather than on the Format toolbar button that would otherwise
    // hold the first focus.
    app.invoke_focus_page();
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
        // Nobody asked for focus to move here, so the page takes it on the next
        // turn of the event loop and typing keeps landing in it. The focus
        // request runs outside any change handler (see `focus-inspector-tab`).
        let weak = app.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(30), move || {
            if let Some(app) = weak.upgrade() {
                app.invoke_focus_page();
            }
        });
    }
}
