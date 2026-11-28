#[cfg(target_os = "macos")]
use slint::ComponentHandle;

#[cfg(target_os = "macos")]
const APP_ICON_PNG: &[u8] =
    include_bytes!("../../../../loom-core/crates/loom-ui/ui/app-icons/loom-sheets.512.png");

pub(crate) fn init_app_icon() {
    #[cfg(target_os = "macos")]
    loom_desktop::macos_window::set_app_icon(APP_ICON_PNG);
}

pub(crate) fn sync_window_state(
    app: &crate::SheetsApp,
    dirty: bool,
    path: Option<&std::path::Path>,
) {
    #[cfg(target_os = "macos")]
    {
        let handle = app.window().window_handle();
        loom_desktop::macos_window::set_document_edited(&handle, dirty);
        loom_desktop::macos_window::set_represented_path(&handle, path);
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, dirty, path);
    }
}

/// Apply native titlebar appearance, app icon, dirty dot, and proxy icon once
/// the event loop has created the underlying AppKit window.
pub(crate) fn attach_window(app: &crate::SheetsApp, state: &std::rc::Rc<crate::GuiState>) {
    #[cfg(target_os = "macos")]
    {
        let weak = app.as_weak();
        let dirty = state.is_dirty();
        let path = state.save_path.borrow().clone();
        slint::invoke_from_event_loop(move || {
            if let Some(app) = weak.upgrade() {
                init_app_icon();
                let handle = app.window().window_handle();
                loom_desktop::macos_window::apply(
                    &handle,
                    app.global::<crate::Theme>().get_active_theme().as_str(),
                );
                loom_desktop::macos_window::set_document_edited(&handle, dirty);
                loom_desktop::macos_window::set_represented_path(&handle, path.as_deref());
            }
        })
        .ok();
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, state);
    }
}
