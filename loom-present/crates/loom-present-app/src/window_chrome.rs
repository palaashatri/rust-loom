//! Window frame for Present: the platform's own title bar, or Loom's title bar
//! (View > Window frame). The Slint side draws Loom's title bar, caption
//! buttons, and resize edges; the shared bindings carry out the window
//! operations they ask for and remember the choice.

use crate::PresentApp;

loom_desktop::window_chrome_bindings!(PresentApp, set_status_left);
