//! Frameless-window behaviour for Writer on Windows and Linux. The Slint side
//! draws the title bar, menu, caption buttons, and resize edges; the shared
//! bindings carry out the window operations they ask for.

use crate::WriterApp;

loom_desktop::window_chrome_bindings!(WriterApp);
