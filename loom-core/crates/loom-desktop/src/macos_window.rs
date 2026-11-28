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

#[cfg(not(target_os = "macos"))]
/// Set the application Dock/taskbar icon from PNG bytes.
pub fn set_app_icon(_png_bytes: &[u8]) {}

#[cfg(not(target_os = "macos"))]
/// Mark the native window's document as having unsaved changes.
pub fn set_document_edited<W: ?Sized>(_window: &W, _edited: bool) {}

#[cfg(not(target_os = "macos"))]
/// Associate a filesystem path with the native window (proxy icon on macOS).
pub fn set_represented_path<W: ?Sized>(_window: &W, _path: Option<&std::path::Path>) {}

/// Format a window title for the active platform.
///
/// On macOS, native `setDocumentEdited:` and `setRepresentedURL:` convey dirty
/// state and app identity, so the title stays clean (`document_name`).
/// On other platforms, appends `*` when dirty and ` — {app_name}`.
pub fn format_window_title(document_name: &str, dirty: bool, app_name: &str) -> String {
    if cfg!(target_os = "macos") {
        document_name.to_string()
    } else {
        let dirty_suffix = if dirty { "*" } else { "" };
        format!("{document_name}{dirty_suffix} — {app_name}")
    }
}

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod platform {
    use objc2::{AnyThread, MainThreadMarker};
    use objc2_app_kit::{
        NSAppearance, NSAppearanceCustomization, NSAppearanceName,
        NSAppearanceNameAccessibilityHighContrastAqua,
        NSAppearanceNameAccessibilityHighContrastDarkAqua, NSAppearanceNameAqua,
        NSAppearanceNameDarkAqua, NSApplication, NSColor, NSImage, NSTitlebarSeparatorStyle,
        NSView,
    };
    use objc2_core_foundation::CGFloat;
    use objc2_foundation::{NSData, NSString, NSURL};
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
                red: 0.067,
                green: 0.078,
                blue: 0.106,
            },
            "high-contrast-light" => Chrome {
                appearance: unsafe { NSAppearanceNameAccessibilityHighContrastAqua },
                red: 0.973,
                green: 0.980,
                blue: 0.988,
            },
            "monokai-dark" => Chrome {
                appearance: unsafe { NSAppearanceNameDarkAqua },
                red: 0.165,
                green: 0.169,
                blue: 0.141,
            },
            "monokai-light" => Chrome {
                appearance: unsafe { NSAppearanceNameAqua },
                red: 0.961,
                green: 0.961,
                blue: 0.953,
            },
            "solarized-dark" => Chrome {
                appearance: unsafe { NSAppearanceNameDarkAqua },
                red: 0.016,
                green: 0.188,
                blue: 0.231,
            },
            "solarized-light" => Chrome {
                appearance: unsafe { NSAppearanceNameAqua },
                red: 0.969,
                green: 0.941,
                blue: 0.863,
            },
            "nord" => Chrome {
                appearance: unsafe { NSAppearanceNameDarkAqua },
                red: 0.200,
                green: 0.224,
                blue: 0.278,
            },
            "catppuccin-mocha" => Chrome {
                appearance: unsafe { NSAppearanceNameDarkAqua },
                red: 0.129,
                green: 0.129,
                blue: 0.200,
            },
            "catppuccin-latte" => Chrome {
                appearance: unsafe { NSAppearanceNameAqua },
                red: 0.957,
                green: 0.961,
                blue: 0.973,
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

    /// Set the application Dock/Cmd+Tab icon from PNG bytes.
    pub fn set_app_icon(png_bytes: &[u8]) {
        if png_bytes.is_empty() {
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let data = NSData::with_bytes(png_bytes);
        let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) else {
            return;
        };
        let app = NSApplication::sharedApplication(mtm);
        unsafe {
            app.setApplicationIconImage(Some(&image));
        }
    }

    /// Mark the AppKit window's close button with a native dirty dot.
    pub fn set_document_edited<W: HasWindowHandle + ?Sized>(window: &W, edited: bool) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return;
        };
        unsafe {
            let view = &*appkit.ns_view.as_ptr().cast::<NSView>();
            let Some(window) = view.window() else {
                return;
            };
            window.setDocumentEdited(edited);
        }
    }

    /// Set or clear the AppKit window's represented file URL (proxy icon).
    pub fn set_represented_path<W: HasWindowHandle + ?Sized>(
        window: &W,
        path: Option<&std::path::Path>,
    ) {
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return;
        };
        unsafe {
            let view = &*appkit.ns_view.as_ptr().cast::<NSView>();
            let Some(window) = view.window() else {
                return;
            };
            match path.and_then(|p| p.to_str()) {
                Some(path_str) => {
                    let ns_path = NSString::from_str(path_str);
                    let url = NSURL::fileURLWithPath(&ns_path);
                    window.setRepresentedURL(Some(&url));
                }
                None => {
                    window.setRepresentedURL(None);
                }
            }
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
                (0.067, 0.078, 0.106)
            );
        }
    }
}

#[cfg(target_os = "macos")]
pub use platform::{apply, set_app_icon, set_document_edited, set_represented_path};

#[cfg(test)]
mod title_tests {
    use super::format_window_title;

    #[test]
    fn format_window_title_respects_platform_conventions() {
        let clean = format_window_title("Untitled.loom", false, "Loom Writer");
        let dirty = format_window_title("Untitled.loom", true, "Loom Writer");
        if cfg!(target_os = "macos") {
            assert_eq!(clean, "Untitled.loom");
            assert_eq!(dirty, "Untitled.loom");
        } else {
            assert_eq!(clean, "Untitled.loom — Loom Writer");
            assert_eq!(dirty, "Untitled.loom* — Loom Writer");
        }
    }
}
