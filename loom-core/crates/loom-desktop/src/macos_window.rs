//! Native macOS window appearance integration.
//!
//! Loom draws its own toolbar and document chrome, but the macOS titlebar is
//! owned by AppKit. Setting the window appearance and background to the
//! active Loom theme keeps the native titlebar, traffic lights and the
//! in-window toolbar in one visual surface. The titlebar keeps AppKit's own
//! material; the audit's Winit root-view constraint rules out reparenting the
//! content view behind a custom `NSVisualEffectView`.

#[cfg(not(target_os = "macos"))]
/// Apply the active Loom theme to a native window where the platform has one.
pub fn apply<W: ?Sized>(_window: &W, _theme: &str) {}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod platform {
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceName,
        NSAppearanceNameAccessibilityHighContrastDarkAqua, NSAppearanceNameAqua,
        NSAppearanceNameDarkAqua, NSColor, NSTitlebarSeparatorStyle, NSView,
    };
    use objc2_core_foundation::CGFloat;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    /// One native titlebar palette derived from the active Loom theme.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct Chrome {
        appearance: &'static NSAppearanceName,
        red: CGFloat,
        green: CGFloat,
        blue: CGFloat,
    }

    fn chrome(theme: &str) -> Chrome {
        match theme {
            "dark" => Chrome {
                appearance: unsafe { NSAppearanceNameDarkAqua },
                red: 0.149,
                green: 0.149,
                blue: 0.161,
            },
            "high-contrast" => Chrome {
                appearance: unsafe { NSAppearanceNameAccessibilityHighContrastDarkAqua },
                red: 0.0,
                green: 0.0,
                blue: 0.0,
            },
            _ => Chrome {
                appearance: unsafe { NSAppearanceNameAqua },
                red: 0.976,
                green: 0.976,
                blue: 0.984,
            },
        }
    }

    /// Apply the active Loom theme to the AppKit window that owns `window`.
    pub fn apply<W: HasWindowHandle + ?Sized>(window: &W, theme: &str) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return;
        };

        // SAFETY: Slint/Winit owns the NSView and the AppKit window. We only
        // read the existing view's window and set appearance/background and
        // titlebar properties on the main thread; we do not replace or
        // reparent the Winit view.
        unsafe {
            let view = &*appkit.ns_view.as_ptr().cast::<NSView>();
            let Some(window) = view.window() else {
                return;
            };
            let chrome = chrome(theme);
            let appearance = NSAppearance::appearanceNamed(chrome.appearance);
            window.setAppearance(appearance.as_deref());
            // Keep AppKit's opaque titlebar material. A transparent titlebar
            // reveals whatever window sits behind Loom; a dark-theme titlebar
            // must stay opaque even over a light desktop.
            window.setTitlebarAppearsTransparent(false);
            window.setTitlebarSeparatorStyle(NSTitlebarSeparatorStyle::None);
            window.setOpaque(true);
            let color = NSColor::colorWithSRGBRed_green_blue_alpha(
                chrome.red,
                chrome.green,
                chrome.blue,
                1.0,
            );
            window.setBackgroundColor(Some(&color));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::chrome;

        #[test]
        fn each_theme_has_a_distinct_native_titlebar_surface() {
            let light = chrome("light");
            let dark = chrome("dark");
            let contrast = chrome("high-contrast");
            assert_ne!(
                (light.red, light.green, light.blue),
                (dark.red, dark.green, dark.blue)
            );
            assert_ne!(
                (light.red, light.green, light.blue),
                (contrast.red, contrast.green, contrast.blue)
            );
            assert_eq!(
                (contrast.red, contrast.green, contrast.blue),
                (0.0, 0.0, 0.0)
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub use platform::apply;
