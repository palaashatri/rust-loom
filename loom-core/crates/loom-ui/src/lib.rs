//! Loom UI — the shared Slint component library and design-token set used
//! by every Loom application.
//!
//! Normative geometry and responsive behavior live in
//! `loom-design-bible/contracts/desktop-ui.toml`; primitive/semantic values
//! live in `loom-design-bible/tokens/loom.toml`. The Slint sources are the
//! runtime implementation of those contracts, not an independent design
//! authority.
//!
//! The `.slint` sources live in the `ui/` directory:
//!
//! * `ui/theme.slint` — light, dark, and high-contrast runtime tokens.
//! * `ui/icons.slint` — the original Loom line-icon family.
//! * `ui/components.slint` — shared controls/chrome/editor primitives.
//! * `ui/smoke.slint` — deterministic component reference surface.
//!
//! Applications import these components directly. Standard controls and
//! application chrome must not be forked or restyled in application code.

slint::include_modules!();

#[cfg(test)]
mod smoke_window {
    include!(concat!(env!("OUT_DIR"), "/smoke.rs"));
}

/// Icon names available on the shared [`Icon`] component, kept sorted so
/// tooling and command metadata can validate icon references deterministically.
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

    const VIEWPORTS: &[(f32, f32)] = &[
        (1024.0, 720.0),
        (1280.0, 800.0),
        (1440.0, 900.0),
        (1920.0, 1200.0),
    ];

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

    fn non_canvas_pixels(image: &image::RgbaImage) -> usize {
        // Contract light canvas #F2F2F4. This is a coarse blank-window guard,
        // not a design-quality score.
        let canvas = image::Rgba([242, 242, 244, 255]);
        image.pixels().filter(|pixel| **pixel != canvas).count()
    }

    #[test]
    fn reference_surface_is_deterministic_at_contract_viewports() {
        loom_test_support::capture::set_platform();
        let window = SmokeWindow::new().expect("create Loom UI reference window");
        Theme::get(&window).set_active_theme("light".into());

        for &(width, height) in VIEWPORTS {
            let first = loom_test_support::capture::snapshot_component(&window, width, height, 1.0)
                .expect("capture reference surface");
            let second =
                loom_test_support::capture::snapshot_component(&window, width, height, 1.0)
                    .expect("repeat reference surface");
            assert_eq!(
                (first.width(), first.height()),
                (width as u32, height as u32),
                "capture dimensions must equal contract viewport"
            );
            assert_eq!(
                first.as_raw(),
                second.as_raw(),
                "software render must be deterministic at {width}x{height}"
            );
            assert!(
                non_canvas_pixels(&first) > 10_000,
                "reference surface looks blank at {width}x{height}"
            );
        }
    }

    #[test]
    fn reference_theme_variants_are_materially_distinct() {
        loom_test_support::capture::set_platform();
        let window = SmokeWindow::new().expect("create Loom UI reference window");

        Theme::get(&window).set_active_theme("light".into());
        let light = loom_test_support::capture::snapshot_component(&window, 1280.0, 800.0, 1.0)
            .expect("capture light reference");

        Theme::get(&window).set_active_theme("dark".into());
        let dark = loom_test_support::capture::snapshot_component(&window, 1280.0, 800.0, 1.0)
            .expect("capture dark reference");

        Theme::get(&window).set_active_theme("high-contrast".into());
        let high_contrast =
            loom_test_support::capture::snapshot_component(&window, 1280.0, 800.0, 1.0)
                .expect("capture high-contrast reference");

        assert!(
            diff_ratio(&light, &dark) > 0.20,
            "light and dark themes are not visually distinct"
        );
        assert!(
            diff_ratio(&dark, &high_contrast) > 0.08,
            "dark and high-contrast themes are not visually distinct"
        );
    }
}
