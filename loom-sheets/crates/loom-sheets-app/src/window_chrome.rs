//! Frameless-window behaviour for Windows and Linux.
//!
//! The Slint side draws the title bar, menu, caption buttons, and resize
//! edges (`LoomTitleBar`, `LoomWindowResizeEdges`); this module carries out the
//! window operations they ask for. macOS keeps its native frame and menu bar,
//! so nothing here is installed there.

use super::*;

#[cfg(any(target_os = "windows", target_os = "linux"))]
use i_slint_backend_winit::{winit::window::ResizeDirection, WinitWindowAccessor};

/// Whether this platform draws Loom's own title bar.
pub(crate) const USES_CUSTOM_CHROME: bool = cfg!(any(target_os = "windows", target_os = "linux"));

/// Turn on custom chrome and connect its callbacks. Safe to call in headless
/// renders: window operations then find no winit window and do nothing.
pub(crate) fn install(app: &SheetsApp) {
    app.set_custom_chrome(USES_CUSTOM_CHROME);
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    wire(app);
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn wire(app: &SheetsApp) {
    let weak = app.as_weak();
    app.on_window_drag({
        let weak = weak.clone();
        move || {
            if let Some(app) = weak.upgrade() {
                app.window().with_winit_window(|window| {
                    let _ = window.drag_window();
                });
            }
        }
    });
    // Route through the same close request the native button raises, so the
    // unsaved-changes decision and drain-before-close logic still run.
    app.on_window_close({
        let weak = weak.clone();
        move || {
            if let Some(app) = weak.upgrade() {
                app.window()
                    .dispatch_event(slint::platform::WindowEvent::CloseRequested);
            }
        }
    });
    app.on_window_resize(move |direction| {
        let Some(app) = weak.upgrade() else { return };
        let direction = match direction {
            0 => ResizeDirection::North,
            1 => ResizeDirection::South,
            2 => ResizeDirection::East,
            3 => ResizeDirection::West,
            4 => ResizeDirection::NorthEast,
            5 => ResizeDirection::NorthWest,
            6 => ResizeDirection::SouthEast,
            _ => ResizeDirection::SouthWest,
        };
        app.window().with_winit_window(|window| {
            let _ = window.drag_resize_window(direction);
        });
    });
}
