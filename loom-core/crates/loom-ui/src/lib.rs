//! Loom UI — shared Slint infrastructure for the Loom desktop suite.
//!
//! `ui/foundation.slint` is the canonical vNext component surface. It is
//! intentionally blocked from application consumption until the foundation
//! acceptance gate is approved. `ui/toolkit.slint` and `ui/components.slint`
//! remain legacy compatibility surfaces during migration.

slint::include_modules!();

/// Rust-side compatibility handle for the embeddable legacy command palette.
pub type CommandPalette = CommandPaletteTestWindow;

#[cfg(test)]
mod smoke_window {
    include!(concat!(env!("OUT_DIR"), "/smoke.rs"));
}

#[cfg(test)]
mod foundation_gallery {
    include!(concat!(env!("OUT_DIR"), "/gallery.rs"));
}

/// Icon names available on the shared [`Icon`] component, kept sorted so
/// tooling and command metadata can validate icon references deterministically.
pub const ICON_NAMES: &[&str] = &[
    "align-center",
    "align-justify",
    "align-left",
    "align-right",
    "audio",
    "bold",
    "brush",
    "camera",
    "chart",
    "check",
    "chevron-down",
    "chevron-left",
    "chevron-right",
    "chevron-up",
    "clock",
    "close",
    "comment",
    "crop",
    "cursor",
    "document",
    "export",
    "eye",
    "eye-off",
    "fader",
    "film",
    "folder",
    "format",
    "grid",
    "image",
    "import",
    "indent",
    "info",
    "italic",
    "keyboard",
    "layers",
    "link",
    "list",
    "lock",
    "mask",
    "mic",
    "minus",
    "more",
    "music",
    "mute",
    "new",
    "open",
    "organize",
    "outdent",
    "package",
    "pause",
    "play",
    "plus",
    "plus-circle",
    "record",
    "redo",
    "refresh",
    "rotate",
    "save",
    "save-as",
    "scale",
    "scissors",
    "search",
    "settings",
    "shape",
    "share",
    "slide",
    "slider",
    "sparkle",
    "stop",
    "strikethrough",
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
    "view",
    "volume",
    "wand",
    "waveform",
    "waves",
    "zoom-fit",
    "zoom-in",
    "zoom-out",
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
    use super::foundation_gallery::{FoundationGallery, Theme as FoundationTheme};
    use super::smoke_window::{DirectionalPropertyProbeWindow, SmokeWindow, Theme};
    use crate::{CommandPalette, CommandPaletteItem};
    use slint::Global;

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
        let canvas = image::Rgba([244, 244, 246, 255]);
        image.pixels().filter(|pixel| **pixel != canvas).count()
    }

    #[test]
    fn foundation_gallery_is_deterministic_at_contract_viewports() {
        loom_test_support::capture::set_platform();
        let window = FoundationGallery::new().expect("create Loom foundation gallery");
        FoundationTheme::get(&window).set_active_theme("light".into());

        for &(width, height) in VIEWPORTS {
            let first = loom_test_support::capture::snapshot_component(&window, width, height, 1.0)
                .expect("capture foundation gallery");
            let second =
                loom_test_support::capture::snapshot_component(&window, width, height, 1.0)
                    .expect("repeat foundation gallery capture");
            assert_eq!(first.as_raw(), second.as_raw());
            assert!(
                non_canvas_pixels(&first) > 20_000,
                "foundation gallery looks blank at {width}x{height}"
            );
        }
    }

    #[test]
    fn foundation_gallery_exposes_all_required_themes() {
        loom_test_support::capture::set_platform();
        let window = FoundationGallery::new().expect("create Loom foundation gallery");
        let theme = FoundationTheme::get(&window);

        theme.set_active_theme("light".into());
        let light = loom_test_support::capture::snapshot_component(&window, 1280.0, 800.0, 1.0)
            .expect("capture light foundation");
        theme.set_active_theme("dark".into());
        let dark = loom_test_support::capture::snapshot_component(&window, 1280.0, 800.0, 1.0)
            .expect("capture dark foundation");
        theme.set_active_theme("high-contrast".into());
        let high = loom_test_support::capture::snapshot_component(&window, 1280.0, 800.0, 1.0)
            .expect("capture high contrast foundation");

        assert!(diff_ratio(&light, &dark) > 0.20);
        assert!(diff_ratio(&dark, &high) > 0.08);
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

    #[test]
    fn active_theme_switches_the_complete_token_bundle() {
        loom_test_support::capture::set_platform();
        let window = SmokeWindow::new().expect("create Loom UI reference window");
        let theme = Theme::get(&window);

        theme.set_active_theme("light".into());
        slint::platform::update_timers_and_animations();
        let light = theme.get_tokens();
        assert_eq!(
            light.palette.canvas,
            slint::Color::from_argb_encoded(0xfff4f4f6)
        );
        assert!(!light.reduced_motion);
        assert_eq!(light.motion.standard_ms, 180);

        theme.set_active_theme("dark".into());
        slint::platform::update_timers_and_animations();
        let dark = theme.get_tokens();
        assert_eq!(
            dark.palette.canvas,
            slint::Color::from_argb_encoded(0xff121214)
        );
        assert_eq!(
            dark.palette.ink_disabled,
            slint::Color::from_argb_encoded(0xff8c8c94)
        );
        assert!(!dark.reduced_motion);
        assert_eq!(dark.motion.standard_ms, 180);

        theme.set_active_theme("high-contrast".into());
        slint::platform::update_timers_and_animations();
        let high_contrast = theme.get_tokens();
        assert_eq!(
            high_contrast.palette.canvas,
            slint::Color::from_argb_encoded(0xff000000)
        );
        assert!(high_contrast.reduced_motion);
        assert_eq!(high_contrast.motion.standard_ms, 0);
        assert_eq!(high_contrast.metrics.toolbar_height, 40.0);
    }

    #[test]
    fn direction_and_long_property_labels_are_observable_and_safe() {
        loom_test_support::capture::set_platform();
        let window = DirectionalPropertyProbeWindow::new().expect("create direction probe");

        window.set_rtl(false);
        assert!(
            window.get_property_row_stacked(),
            "long localized PropertyRow labels must switch to stacked layout"
        );
        assert!(
            window.get_property_row_label_height() > 18.0,
            "stacked PropertyRow labels must retain wrapped line height (height={})",
            window.get_property_row_label_height()
        );
        assert!(
            window.get_property_row_control_y() > 0.0,
            "stacked PropertyRow controls must move below the wrapped label"
        );
        assert!(
            window.get_first_x() < window.get_second_x(),
            "LTR direction must preserve declaration order"
        );
        let ltr = loom_test_support::capture::snapshot_component(&window, 420.0, 180.0, 1.0)
            .expect("capture LTR probe");
        window.set_rtl(true);
        assert!(
            window.get_first_x() > window.get_second_x(),
            "RTL direction must reverse horizontal child order (first={}, second={})",
            window.get_first_x(),
            window.get_second_x()
        );
        let rtl = loom_test_support::capture::snapshot_component(&window, 420.0, 180.0, 1.0)
            .expect("capture RTL probe");
        assert!(
            diff_ratio(&ltr, &rtl) > 0.001,
            "RTL direction must change row geometry"
        );

        let scaled = loom_test_support::capture::snapshot_component(&window, 420.0, 180.0, 1.5)
            .expect("capture 150% property-row probe");
        assert_eq!(scaled.width(), 630);
        assert_eq!(scaled.height(), 270);
        assert!(
            non_canvas_pixels(&scaled) > 1_000,
            "scaled probe must remain rendered"
        );
    }

    #[test]
    fn command_palette_clips_long_lists_to_modal_surface() {
        use std::rc::Rc;

        loom_test_support::capture::set_platform();

        let empty = CommandPalette::new().expect("create empty command palette");
        empty.set_open(true);
        let empty_image =
            loom_test_support::capture::snapshot_component(&empty, 1280.0, 800.0, 1.0)
                .expect("capture empty command palette");

        let many = CommandPalette::new().expect("create long command palette");
        many.set_open(true);
        many.set_commands(
            Rc::new(slint::VecModel::from(
                (0..24)
                    .map(|index| CommandPaletteItem {
                        id: format!("command-{index}").into(),
                        label: format!("Command {index}").into(),
                        shortcut: "Ctrl+K".into(),
                        enabled: true,
                    })
                    .collect::<Vec<_>>(),
            ))
            .into(),
        );
        let many_image = loom_test_support::capture::snapshot_component(&many, 1280.0, 800.0, 1.0)
            .expect("capture long command palette");

        let outside_bottom = 592;
        let leaked_pixels = (outside_bottom..800)
            .flat_map(|y| (0..1280).map(move |x| (x, y)))
            .filter(|&(x, y)| empty_image.get_pixel(x, y) != many_image.get_pixel(x, y))
            .count();
        assert_eq!(
            leaked_pixels, 0,
            "long command rows must be clipped inside the palette surface"
        );
    }
}
