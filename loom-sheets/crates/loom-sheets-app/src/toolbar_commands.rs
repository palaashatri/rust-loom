//! The icon-over-label toolbar and its menus send command ids; this module
//! runs them through the same dispatcher as the menu bar and the palette, and
//! decides how the window starts (inspector open, Format tab showing).

use slint::ComponentHandle;

use crate::command_dispatch::dispatch_command;
use crate::SheetsApp;

/// The Format tab's index in the inspector tab strip (Organize is 0).
pub(crate) const FORMAT_TAB: i32 = 1;

/// Routes toolbar and menu commands to the shared dispatcher.
pub(crate) fn wire(app: &SheetsApp) {
    let app_ref = app.as_weak();
    app.on_toolbar_command(move |id| {
        if let Some(app) = app_ref.upgrade() {
            dispatch_command(&app, id.as_str());
        }
    });
}

/// A fresh window shows the inspector on the Format tab. A window too narrow
/// to dock it keeps it closed, as the compact drawer would cover the sheet.
pub(crate) fn start_with_inspector_open(app: &SheetsApp) {
    app.set_inspector_tab(FORMAT_TAB);
    if !app.get_icon_only_toolbar() {
        app.set_inspector_preference(true);
        app.set_show_inspector(true);
    }
}
