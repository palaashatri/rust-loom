use crate::{
    ResponsivePolicy, SheetToolbarLayout, SheetsApp, DEFAULT_VISIBLE_ROWS,
    GRID_COLUMN_HEADER_HEIGHT, GRID_COL_WIDTH, GRID_ROW_HEIGHT,
};
use i_slint_core::item_tree::ParentItemTraversalMode;
use slint::private_unstable_api::re_exports::{AccessibleStringProperty, WindowInner};
use slint::{ComponentHandle, Global};

/// Effective (text-scaled) width below which the labeled toolbar row cannot fit
/// its insert commands. They move into More actions instead of being clipped.
const COMPACT_TOOLBAR_BELOW: f32 = 600.0;

pub(crate) const INSPECTOR_WIDTH: f32 = 320.0;
pub(crate) const TABLE_HORIZONTAL_MARGIN: f32 = 0.0;
const SHELL_VERTICAL_CHROME: f32 = 220.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResponsiveToolbarState {
    pub(crate) icon_only: bool,
    pub(crate) overflow: bool,
    pub(crate) labeled: bool,
    pub(crate) compact: bool,
}

pub(crate) fn layout_breakpoints(app: &SheetsApp, width: u32) -> ResponsiveToolbarState {
    let policy = ResponsivePolicy::get(app);
    let width = width as f32 / app.get_template_text_scale().max(1.0);
    ResponsiveToolbarState {
        icon_only: width < policy.get_priority_1_icon_only_below(),
        overflow: width < policy.get_priority_2_overflow_below(),
        labeled: width >= policy.get_priority_2_overflow_below(),
        compact: width < COMPACT_TOOLBAR_BELOW,
    }
}

pub(crate) fn apply_layout_breakpoints(app: &SheetsApp, width: u32) {
    let was_icon_only = app.get_icon_only_toolbar();
    let state = layout_breakpoints(app, width);
    let entering_compact = state.icon_only && !was_icon_only;
    let leaving_compact = !state.icon_only && was_icon_only;
    let restore_grid_focus = entering_compact
        && app.get_show_inspector()
        && focus_recovery_is_unclaimed(app)
        && focused_item_is_within_inspector(app);
    if entering_compact && app.get_show_inspector() {
        // Keep the desired docked state, but remove the full-window compact
        // backdrop before it can intercept the next grid click.
        app.set_show_inspector(false);
        app.set_inspector_quiet_open(false);
    }
    app.global::<SheetToolbarLayout>()
        .set_compact(state.compact);
    app.set_icon_only_toolbar(state.icon_only);
    app.set_labeled_toolbar(state.labeled);
    app.set_show_quick_formulas(state.labeled);
    app.set_wide_toolbar(state.labeled);
    app.set_labeled_export(state.labeled);
    app.set_overflow_toolbar(state.overflow);
    // Keep Format available at every width. The UI moves it into a drawer
    // when the window is compact instead of disabling the action.
    app.set_inspector_available(true);
    if leaving_compact && app.get_inspector_preference() && !app.get_show_inspector() {
        // Reopen the preferred inspector without moving focus away from the
        // grid, formula bar, or name box that remained active while compact.
        app.set_inspector_quiet_open(true);
        app.set_show_inspector(true);
    }
    if restore_grid_focus {
        // The focused inspector child disappears with the panel. Move focus
        // synchronously so a later grid click reaches the cell on its first try.
        app.invoke_focus_grid();
    }
}

/// Slint has no public focus-within signal. Keep its version-specific item
/// inspection here and identify the inspector by its semantic accessibility ID,
/// so every descendant control follows the same resize behavior. The direct
/// i-slint-core dependency is pinned to the app's Slint 1.17.1 runtime for the
/// popup-aware parent traversal API.
pub(crate) fn focused_item_is_within_inspector(app: &SheetsApp) -> bool {
    let window = WindowInner::from_pub(app.window());
    let Some(mut focused) = window.focus_item.borrow().upgrade() else {
        return false;
    };

    loop {
        if focused
            .accessible_string_property(AccessibleStringProperty::Id)
            .is_some_and(|id| id.as_str() == "loom-sheets-inspector-focus-root")
        {
            return true;
        }
        let Some(parent) = focused.parent_item(ParentItemTraversalMode::StopAtPopups) else {
            return false;
        };
        focused = parent;
    }
}

fn focus_recovery_is_unclaimed(app: &SheetsApp) -> bool {
    let window = WindowInner::from_pub(app.window());
    window.active_popups.borrow().is_empty()
        && !app.get_close_draining()
        && !app.get_save_changes_open()
        && !app.get_xlsx_import_warning_open()
        && !app.get_template_chooser_open()
        && !app.get_palette_open()
        && !app.get_toolbar_overflow_open()
        && app.get_toolbar_menu() == 0
}

/// Size the headless projection like the native canvas. Snapshot rendering
/// skips the native resize event, so compute the viewport before projecting.
pub(crate) fn apply_headless_viewport_size(app: &SheetsApp, width: u32, height: u32) {
    let policy = ResponsivePolicy::get(app);
    let effective_width = width as f32 / app.get_template_text_scale().max(1.0);
    let inspector_width =
        if app.get_show_inspector() && effective_width >= policy.get_priority_1_icon_only_below() {
            INSPECTOR_WIDTH
        } else {
            0.0
        };
    app.set_grid_viewport_width(
        (width as f32 - inspector_width - TABLE_HORIZONTAL_MARGIN).max(GRID_COL_WIDTH),
    );
    app.set_grid_viewport_height(
        (height as f32 - SHELL_VERTICAL_CHROME)
            .max(GRID_ROW_HEIGHT * DEFAULT_VISIBLE_ROWS as f32 + GRID_COLUMN_HEADER_HEIGHT),
    );
}

#[cfg(test)]
#[allow(dead_code)] // exercised by headless breakpoint/focus regression tests
pub(crate) fn wire_responsive_layout(app: &SheetsApp) {
    let app_ref = app.as_weak();
    app.on_window_resized(move |width| {
        if let Some(app) = app_ref.upgrade() {
            apply_layout_breakpoints(&app, width.max(0.0) as u32);
        }
    });
}
