//! Frameless-window behaviour for Writer on Windows and Linux. The Slint side
//! draws the title bar, menu, caption buttons, and resize edges; the shared
//! bindings carry out the window operations they ask for.

use crate::WriterApp;

loom_desktop::window_chrome_bindings!(WriterApp);

#[cfg(test)]
mod tests {
    use super::custom_chrome_requested;

    #[test]
    fn custom_chrome_requires_supported_callbacks_and_explicit_opt_in() {
        assert!(custom_chrome_requested(true, Some("1")));
        assert!(!custom_chrome_requested(false, Some("1")));
        assert!(!custom_chrome_requested(true, None));
        assert!(!custom_chrome_requested(true, Some("true")));
    }
}
