//! Frameless-window behaviour for Present on Windows and Linux. The Slint side
//! draws the title bar, menu, caption buttons, and resize edges; the shared
//! bindings carry out the window operations they ask for.

use crate::PresentApp;

loom_desktop::window_chrome_bindings!(PresentApp);
