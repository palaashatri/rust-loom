from pathlib import Path

replacements = {
    "loom-photo/crates/loom-photo-core/src/lib.rs": [
        (
            """            for channel in 0..3 {\n                let value =\n                    pixel[channel] as f32 * alpha + background[channel] as f32 * (1.0 - alpha);\n                output.push(value.round().clamp(0.0, 255.0) as u8);\n            }""",
            """            for (source, backdrop) in pixel.iter().take(3).zip(background.iter()) {\n                let value = *source as f32 * alpha + *backdrop as f32 * (1.0 - alpha);\n                output.push(value.round().clamp(0.0, 255.0) as u8);\n            }""",
        ),
    ],
    "loom-video/crates/loom-video-core/src/lib.rs": [
        (
            "#[derive(Debug, Clone, PartialEq, Eq)]\npub struct TimelineExportPlan",
            "#[derive(Debug, Clone, PartialEq)]\npub struct TimelineExportPlan",
        ),
    ],
    "loom-studio/crates/loom-studio-core/src/lib.rs": [
        (
            ".add(path.clone(), bytes.clone())",
            ".add(path.as_str(), bytes.clone())",
        ),
    ],
}

for filename, edits in replacements.items():
    path = Path(filename)
    text = path.read_text(encoding="utf-8")
    for old, new in edits:
        count = text.count(old)
        if count != 1:
            raise SystemExit(f"expected one match in {filename}, found {count}: {old[:80]!r}")
        text = text.replace(old, new)
    path.write_text(text, encoding="utf-8")

core = Path("loom-core/crates/loom-ui/src/lib.rs")
text = core.read_text(encoding="utf-8")
old = '''    #[test]
    fn smoke_window_is_deterministic_and_uses_theme_tokens() {
        let render = |dark: bool| {
            std::env::set_var("SLINT_BACKEND", "testing");
            let window = SmokeWindow::new().expect("window");
            window.set_dark_mode(dark);
            Theme::get(&window).set_mode(if dark { 1 } else { 0 });
            loom_test_support::capture_slint_software_rgba(&window, 620, 420, 1.0)
                .expect("render")
        };

        let light_a = render(false);
        let light_b = render(false);
        assert_eq!(
            light_a.as_raw(),
            light_b.as_raw(),
            "the software-rendered component gallery must be deterministic"
        );

        let dark = render(true);
        let different = light_a
            .pixels()
            .zip(dark.pixels())
            .filter(|(left, right)| left != right)
            .count();
        let ratio = different as f32 / (light_a.width() as f32 * light_a.height() as f32);
        assert!(
            ratio > 0.20,
            "light and dark themes should materially differ; ratio={ratio:.3}"
        );

        let palette = Theme::get(&SmokeWindow::new().expect("window")).get_palette();
        assert_ne!(palette.surface.to_argb_encoded(), 0);
        assert_ne!(palette.accent.to_argb_encoded(), palette.surface.to_argb_encoded());
    }
'''
new = '''    #[test]
    fn smoke_window_is_deterministic_and_uses_theme_tokens() {
        let render = |dark: bool| {
            let window = SmokeWindow::new().expect("window");
            window.set_dark_mode(dark);
            Theme::get(&window).set_mode(i32::from(dark));
            loom_test_support::capture_slint_software_rgba(&window, 620, 420, 1.0)
                .expect("render")
        };

        let light_a = render(false);
        let light_b = render(false);
        assert_eq!(
            light_a.as_raw(),
            light_b.as_raw(),
            "the software-rendered component gallery must be deterministic"
        );

        let dark = render(true);
        let different = light_a
            .pixels()
            .zip(dark.pixels())
            .filter(|(left, right)| left != right)
            .count();
        let pixel_count = u64::from(light_a.width()) * u64::from(light_a.height());
        let ratio = different as f64 / pixel_count as f64;
        assert!(
            ratio > 0.20,
            "light and dark themes should materially differ; ratio={ratio:.3}"
        );

        let palette_window = SmokeWindow::new().expect("window");
        let palette = Theme::get(&palette_window).get_palette();
        assert_ne!(palette.surface.to_argb_encoded(), 0);
        assert_ne!(palette.accent.to_argb_encoded(), palette.surface.to_argb_encoded());
    }
'''
count = text.count(old)
if count != 1:
    raise SystemExit(f"expected one core visual test, found {count}")
core.write_text(text.replace(old, new), encoding="utf-8")
