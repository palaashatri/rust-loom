//! Frameless-window behaviour for Sheets on Windows and Linux. The Slint side
//! draws the title bar, menu, caption buttons, and resize edges; the shared
//! bindings carry out the window operations they ask for.

use crate::SheetsApp;

loom_desktop::window_chrome_bindings!(SheetsApp);
