//! Loom UI — the shared Slint component library and design-token set used
//! by every Loom application.
//!
//! The `.slint` sources live in the `ui/` directory of this crate:
//!
//! * `ui/theme.slint` — semantic design tokens (light, dark, high-contrast
//!   palettes; typography, spacing, motion), exposed through the `Theme`,
//!   `ThemeDark` and `ThemeHighContrast` globals.
//! * `ui/icons.slint` — the original Loom line-icon family (`Icon`
//!   component; see `Icon::icon` for the icon name table).
//! * `ui/components.slint` — `LoomWindow` application chrome,
//!   `StatusBar`, `ToolButton`, `IconButton`, `PrimaryButton`,
//!   `SearchField`, `Field`, `SegmentedControl`, `EmptyState`, `Panel`,
//!   `InspectorSection`.
//!
//! Applications import from these files directly, for example:
//!
//! ```slint
//! import { LoomWindow, ToolButton } from "@loom/ui/ui/components.slint";
//! ```
//!
//! The library itself is backend-agnostic. Desktop applications enable the
//! `default` features for the winit + femtovg backends; tests and headless
//! visual QA use `default-features = false` together with
//! `loom-test-support`'s capture platform.
//!
//! ## Design rules enforced here
//!
//! * Every interactive component has an `accessible-role` and
//!   `accessible-label`, a `FocusScope`, and keyboard activation
//!   (Space/Enter) built in.
//! * Components never animate by default; hosts add motion explicitly and
//!   must honor `Theme.tokens.motion.reduced-motion`.
//! * All colors come from the token palettes — no hard-coded colors inside
//!   app `.slint` files.

slint::include_modules!();

/// The smoke fixture (`ui/smoke.slint`) is only compiled into test builds;
/// it exercises every component in one window and is captured against a
/// committed golden baseline.
#[cfg(test)]
mod smoke_window {
    include!(concat!(env!("OUT_DIR"), "/smoke.rs"));
}

/// The icon names available on the [`Icon`] component, in alphabetical
/// order. Used by tooling and documentation; the authoritative drawing of
/// each icon lives in `ui/icons.slint`.
pub const ICON_NAMES: &[&str] = &[
    "align-center",
    "align-left",
    "align-right",
    "audio",
    "bold",
    "brush",
    "camera",
    "check",
    "chevron-left",
    "chevron-right",
    "clock",
    "close",
    "cursor",
    "document",
    "export",
    "eye",
    "eye-off",
    "film",
    "folder",
    "grid",
    "image",
    "import",
    "italic",
    "keyboard",
    "layers",
    "link",
    "lock",
    "mask",
    "minus",
    "mute",
    "new",
    "open",
    "package",
    "pause",
    "play",
    "plus",
    "plus-circle",
    "record",
    "redo",
    "save",
    "scale",
    "scissors",
    "search",
    "settings",
    "slide",
    "slider",
    "stop",
    "table",
    "terminal",
    "text",
    "timeline",
    "trash",
    "trash-2",
    "underline",
    "undo",
    "unlock",
    "video",
    "volume",
    "wand",
    "waveform",
    "waves",
    "zoom-in",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_names_are_sorted_and_unique() {
        for window in ICON_NAMES.windows(2) {
            assert!(
                window[0] < window[1],
                "ICON_NAMES should be sorted alphabetically: {} >= {}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn icon_names_contains_all_requested_icons() {
        let expected = [
            "bold",
            "italic",
            "underline",
            "align-left",
            "align-center",
            "align-right",
            "eye-off",
            "lock",
            "unlock",
            "volume",
            "mute",
            "record",
            "slider",
            "plus-circle",
            "trash-2",
            "scissors",
        ];
        for icon in expected {
            assert!(
                ICON_NAMES.contains(&icon),
                "ICON_NAMES must contain icon '{}'",
                icon
            );
        }
    }
}

#[cfg(test)]
mod visual_tests {
    use super::smoke_window::*;

    fn diff_ratio(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {
        assert_eq!(a.dimensions(), b.dimensions());
        let different = a
            .pixels()
            .zip(b.pixels())
            .filter(|(left, right)| left != right)
            .count();
        let pixel_count = u64::from(a.width()) * u64::from(a.height());
        different as f64 / pixel_count as f64
    }

    #[test]
    fn smoke_window_is_deterministic_non_blank_and_theme_distinct() {
        loom_test_support::capture::set_platform();
        let window = SmokeWindow::new().expect("create smoke window");

        Theme::get(&window).set_active_theme("light".into());
        let light_a = loom_test_support::capture::snapshot_component(&window, 900.0, 600.0, 1.0)
            .expect("capture light fixture");
        let light_b = loom_test_support::capture::snapshot_component(&window, 900.0, 600.0, 1.0)
            .expect("repeat light fixture");
        assert_eq!((light_a.width(), light_a.height()), (900, 600));
        assert_eq!(
            light_a.as_raw(),
            light_b.as_raw(),
            "software render must be deterministic"
        );

        let canvas = image::Rgba([242, 242, 240, 255]);
        let non_canvas = light_a.pixels().filter(|pixel| **pixel != canvas).count();
        assert!(
            non_canvas > 2_000,
            "window looks blank: only {non_canvas} non-canvas pixels"
        );

        Theme::get(&window).set_active_theme("dark".into());
        let dark = loom_test_support::capture::snapshot_component(&window, 900.0, 600.0, 1.0)
            .expect("capture dark fixture");
        Theme::get(&window).set_active_theme("high-contrast".into());
        let high_contrast =
            loom_test_support::capture::snapshot_component(&window, 900.0, 600.0, 1.0)
                .expect("capture high-contrast fixture");

        assert!(
            diff_ratio(&light_a, &dark) > 0.20,
            "light and dark themes are not visually distinct"
        );
        assert!(
            diff_ratio(&dark, &high_contrast) > 0.08,
            "dark and high-contrast themes are not visually distinct"
        );
    }
}
