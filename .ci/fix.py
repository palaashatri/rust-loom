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
    "loom-core/crates/loom-ui/src/lib.rs": [
        (
            """    fn diff_ratio(a: &image::RgbaImage, b: &image::RgbaImage) -> f32 {\n        assert_eq!(a.dimensions(), b.dimensions());\n        let different = a\n            .pixels()\n            .zip(b.pixels())\n            .filter(|(left, right)| left != right)\n            .count();\n        different as f32 / (a.width() as f32 * a.height() as f32)\n    }""",
            """    fn diff_ratio(a: &image::RgbaImage, b: &image::RgbaImage) -> f64 {\n        assert_eq!(a.dimensions(), b.dimensions());\n        let different = a\n            .pixels()\n            .zip(b.pixels())\n            .filter(|(left, right)| left != right)\n            .count();\n        let pixel_count = u64::from(a.width()) * u64::from(a.height());\n        different as f64 / pixel_count as f64\n    }""",
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
