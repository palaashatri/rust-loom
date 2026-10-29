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
    use super::smoke_window::{
        DirectionalPropertyProbeWindow, SmokeWindow, Theme, ToolbarCaptionProbeWindow,
        ToolbarStateInsetProbeWindow,
    };
    use crate::{CommandPalette, CommandPaletteItem};
    use i_slint_backend_testing::{AccessibleRole, ElementHandle};
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
    fn system_appearance_follows_the_platform_and_explicit_choices_do_not() {
        loom_test_support::capture::set_platform();
        let window = SmokeWindow::new().expect("create Loom UI reference window");
        let theme = Theme::get(&window);
        let canvas_hex = |theme: &Theme| theme.get_tokens().palette.canvas;
        slint::platform::update_timers_and_animations();
        assert_eq!(
            theme.get_system_scheme(),
            "light",
            "a platform that reports no preference is light"
        );

        // The default binding reads the platform palette, not a stored value.
        use super::smoke_window::WidgetPalette;
        let platform = WidgetPalette::get(&window);
        platform.set_color_scheme(slint::private_unstable_api::re_exports::ColorScheme::Dark);
        theme.set_appearance("system".into());
        slint::platform::update_timers_and_animations();
        assert_eq!(
            theme.get_system_scheme(),
            "dark",
            "the OS preference reaches Theme"
        );
        assert_eq!(theme.get_active_theme(), "dark");
        platform.set_color_scheme(slint::private_unstable_api::re_exports::ColorScheme::Light);
        slint::platform::update_timers_and_animations();
        assert_eq!(
            theme.get_active_theme(),
            "light",
            "and so does a change back"
        );
        theme.set_appearance("system".into());
        theme.set_system_scheme("dark".into());
        slint::platform::update_timers_and_animations();
        assert_eq!(theme.get_resolved_theme(), "dark");
        assert_eq!(theme.get_active_theme(), "dark", "System follows a dark OS");
        assert_eq!(
            canvas_hex(&theme),
            slint::Color::from_argb_encoded(0xff1c1c1e)
        );

        theme.set_system_scheme("light".into());
        slint::platform::update_timers_and_animations();
        assert_eq!(theme.get_active_theme(), "light", "and follows it back");
        assert_eq!(
            canvas_hex(&theme),
            slint::Color::from_argb_encoded(0xfff5f5f7)
        );

        theme.set_appearance("high-contrast".into());
        slint::platform::update_timers_and_animations();
        theme.set_system_scheme("dark".into());
        slint::platform::update_timers_and_animations();
        assert_eq!(
            theme.get_active_theme(),
            "high-contrast",
            "an explicit choice ignores the OS"
        );
        theme.set_appearance("light".into());
        slint::platform::update_timers_and_animations();
        assert_eq!(theme.get_active_theme(), "light");
    }

    #[test]
    fn application_accents_preserve_document_and_accessibility_palettes() {
        loom_test_support::capture::set_platform();
        let window = SmokeWindow::new().expect("create theme probe");
        let theme = Theme::get(&window);
        for (mode, apps) in [
            (
                "light",
                [
                    ("writer", 0xff6241ad),
                    ("sheets", 0xff087565),
                    ("present", 0xffad432e),
                ],
            ),
            (
                "dark",
                [
                    ("writer", 0xffbba7f4),
                    ("sheets", 0xff73d8ba),
                    ("present", 0xffffaf92),
                ],
            ),
        ] {
            theme.set_application("".into());
            slint::platform::update_timers_and_animations();
            theme.set_active_theme(mode.into());
            slint::platform::update_timers_and_animations();
            let original = theme.get_tokens().palette;
            for (application, accent) in apps {
                theme.set_application(application.into());
                slint::platform::update_timers_and_animations();
                let branded = theme.get_tokens().palette;
                assert_eq!(branded.accent, slint::Color::from_argb_encoded(accent));
                assert_eq!(branded.canvas, original.canvas);
                assert_eq!(branded.chrome, original.chrome);
                assert_eq!(branded.paper, original.paper);
                assert_eq!(branded.paper_ink, original.paper_ink);
                assert_eq!(branded.paper_muted, original.paper_muted);
                assert_eq!(branded.content_accent, original.content_accent);
                assert_eq!(branded.content_accent_soft, original.content_accent_soft);
            }
            theme.set_application("".into());
            slint::platform::update_timers_and_animations();
            assert_eq!(theme.get_tokens().palette, original);
        }
        theme.set_active_theme("high-contrast".into());
        slint::platform::update_timers_and_animations();
        let accessible = theme.get_tokens().palette;
        for application in ["writer", "sheets", "present"] {
            theme.set_application(application.into());
            slint::platform::update_timers_and_animations();
            assert_eq!(theme.get_tokens().palette, accessible);
        }
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
            slint::Color::from_argb_encoded(0xfff5f5f7)
        );
        assert!(!light.reduced_motion);
        assert_eq!(light.motion.standard_ms, 180);

        theme.set_active_theme("dark".into());
        slint::platform::update_timers_and_animations();
        let dark = theme.get_tokens();
        assert_eq!(
            dark.palette.canvas,
            slint::Color::from_argb_encoded(0xff1c1c1e)
        );
        assert_eq!(
            dark.palette.ink_disabled,
            slint::Color::from_argb_encoded(0xff9a9aa2)
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
    fn long_toolbar_captions_remain_complete_at_responsive_boundaries_and_scaled_sizes() {
        loom_test_support::capture::set_platform();
        let window = ToolbarCaptionProbeWindow::new().expect("create toolbar caption probe");
        let theme = Theme::get(&window);
        let label = "Export all currently selected worksheets as a complete accessible spreadsheet";
        window.set_probe_label(label.into());

        for scale in [1.25f32, 1.5] {
            theme.set_text_scale(scale);
            for base_width in [1179.0f32, 1180.0, 1279.0, 1280.0, 1319.0, 1320.0] {
                let width = base_width;
                window.set_probe_width(width);
                let required_width = window.get_required_caption_width();
                let required_line_height = window.get_required_caption_line_height();

                let caption = ElementHandle::find_by_element_id(
                    &window,
                    "IconOverLabelToolbarItem::label-text",
                )
                .next()
                .expect("toolbar caption text");
                let caption_size = caption.size();
                assert!(
                    caption_size.width + 0.1 >= required_width,
                    "caption width {} must fit its measured text {required_width} at width {base_width}, scale {scale}",
                    caption_size.width
                );
                assert!(
                    caption_size.height + 0.1 >= required_line_height,
                    "caption line box {} must meet {required_line_height} at width {base_width}, scale {scale}",
                    caption_size.height
                );

                let action = ElementHandle::find_by_accessible_label(&window, label)
                    .find(|item| item.accessible_role() == Some(AccessibleRole::Button))
                    .expect("toolbar action retains its full accessible label");
                let action_left = action.absolute_position().x;
                let action_right = action_left + action.size().width;
                let caption_left = caption.absolute_position().x;
                assert!(
                    action.size().width + 0.1 >= required_width + 16.0,
                    "toolbar intrinsic width {} must preserve the caption and 16px horizontal padding ({required_width}) at width {base_width}, scale {scale}",
                    action.size().width
                );
                assert!(
                    caption_left + 0.1 >= action_left
                        && caption_left + caption_size.width <= action_right + 0.1,
                    "caption bounds [{caption_left}, {}] must remain inside action bounds [{action_left}, {action_right}] at width {base_width}, scale {scale}",
                    caption_left + caption_size.width
                );
                assert!(
                    action_right <= width + 0.1,
                    "full caption action ends at {action_right} beyond viewport {width} at scale {scale}"
                );
            }
        }
    }

    #[test]
    fn toolbar_state_outlines_are_inset_without_shrinking_targets_at_one_and_two_x() {
        loom_test_support::capture::set_platform();
        let window = ToolbarStateInsetProbeWindow::new().expect("create toolbar inset probe");
        let theme = Theme::get(&window);
        theme.set_text_scale(1.0);

        let find_button = |label: &str| {
            ElementHandle::find_by_accessible_label(&window, label)
                .find(|element| element.accessible_role() == Some(AccessibleRole::Button))
                .unwrap_or_else(|| panic!("toolbar button {label:?}"))
        };
        let assert_inset = |label: &str, outline_id: &str, expected_height: f32| {
            let button = find_button(label);
            let button_position = button.absolute_position();
            let button_size = button.size();
            assert!(
                (button_size.height - expected_height).abs() < 0.1,
                "{label} target height {} must remain {expected_height}",
                button_size.height
            );

            let outline = ElementHandle::find_by_element_id(&window, outline_id)
                .find(|element| {
                    let position = element.absolute_position();
                    (position.x - button_position.x - 2.0).abs() < 0.1
                        && (position.y - button_position.y - 2.0).abs() < 0.1
                })
                .unwrap_or_else(|| panic!("{label} outline has the 2px contract inset"));
            let outline_size = outline.size();
            assert!(
                (outline_size.width - (button_size.width - 4.0)).abs() < 0.1
                    && (outline_size.height - (button_size.height - 4.0)).abs() < 0.1,
                "{label} outline must preserve a 2px inset on every edge"
            );
            (button, button_position)
        };
        let assert_visible_top_stroke = |image: &image::RgbaImage,
                                         button: &ElementHandle,
                                         button_x: f32,
                                         button_y: f32,
                                         scale_factor: f32,
                                         label: &str| {
            let x = ((button_x + button.size().width / 2.0) * scale_factor).round() as u32;
            let gap_y = ((button_y + 1.0) * scale_factor).round() as u32;
            let ring_y = ((button_y + 2.0) * scale_factor).round() as u32;
            assert_ne!(
                image.get_pixel(x, gap_y),
                image.get_pixel(x, ring_y),
                "{label} must show its focus/selection stroke after the 2px inset at {scale_factor}x"
            );
        };

        for scale_factor in [1.0f32, 2.0] {
            let initial =
                loom_test_support::capture::snapshot_component(&window, 300.0, 48.0, scale_factor)
                    .expect("render 48px toolbar inset probe");
            assert_eq!(initial.width(), (300.0 * scale_factor) as u32);
            assert_eq!(initial.height(), (48.0 * scale_factor) as u32);

            let toolbar =
                ElementHandle::find_by_element_id(&window, "ToolbarStateInsetProbeWindow::toolbar")
                    .next()
                    .expect("48px labeled toolbar");
            assert!((toolbar.size().height - 48.0).abs() < 0.1);

            let (format, format_position) =
                assert_inset("Format", "IconOverLabelToolbarItem::state-outline", 48.0);
            let (image_button, image_position) =
                assert_inset("Image", "IconOverLabelToolbarItem::state-outline", 48.0);
            let (edit, _) = assert_inset("Edit", "IconOnlyToolbarItem::state-outline", 28.0);
            let (toolkit_overflow, _) =
                assert_inset("More toolkit", "Overflow::state-outline", 28.0);
            let (loom_overflow, _) =
                assert_inset("More Loom", "LoomIconButton::state-outline", 28.0);

            window.focus_image();
            let focused_image =
                loom_test_support::capture::snapshot_component(&window, 300.0, 48.0, scale_factor)
                    .expect("render focused labeled toolbar item");
            assert_visible_top_stroke(
                &focused_image,
                &format,
                format_position.x,
                format_position.y,
                scale_factor,
                "Format selected state",
            );
            assert_visible_top_stroke(
                &focused_image,
                &image_button,
                image_position.x,
                image_position.y,
                scale_factor,
                "Image focus state",
            );
            assert_visible_top_stroke(
                &focused_image,
                &edit,
                edit.absolute_position().x,
                edit.absolute_position().y,
                scale_factor,
                "Edit selected state",
            );

            window.focus_toolkit_overflow();
            let focused_toolkit_overflow =
                loom_test_support::capture::snapshot_component(&window, 300.0, 48.0, scale_factor)
                    .expect("render focused toolkit overflow action");
            assert_visible_top_stroke(
                &focused_toolkit_overflow,
                &toolkit_overflow,
                toolkit_overflow.absolute_position().x,
                toolkit_overflow.absolute_position().y,
                scale_factor,
                "toolkit overflow focus state",
            );

            window.focus_overflow();
            let focused_loom_overflow =
                loom_test_support::capture::snapshot_component(&window, 300.0, 48.0, scale_factor)
                    .expect("render focused Loom overflow action");
            assert_visible_top_stroke(
                &focused_loom_overflow,
                &loom_overflow,
                loom_overflow.absolute_position().x,
                loom_overflow.absolute_position().y,
                scale_factor,
                "Loom overflow focus state",
            );
        }
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
