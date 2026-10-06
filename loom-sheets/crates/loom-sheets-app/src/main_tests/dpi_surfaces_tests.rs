//! Every surface the scale tests open, rendered at real device scale factors
//! (1.25, 1.5, 2.0). A scale factor changes pixel density only: the logical
//! layout must match the 1.0 layout and the image must be `logical * factor`
//! pixels with at least as much drawn detail and no vanished 1px lines.
use super::scale_surfaces_tests::{editor, open_surface, rects};
use super::*;
use loom_test_support::capture::snapshot_component;
use loom_test_support::dpi::{layout_difference, pixel_problem};

const SURFACES: [&str; 13] = [
    "none",
    "save-changes",
    "save-changes-closing",
    "xlsx-warning",
    "templates",
    "palette",
    "inspector",
    "view-menu",
    "zoom-menu",
    "table-menu",
    "text-menu",
    "export-menu",
    "overflow",
];

#[test]
fn every_surface_keeps_its_logical_layout_at_real_scale_factors() {
    for surface in SURFACES {
        for (width, height) in [(1024.0f32, 720.0f32), (1280.0, 800.0)] {
            let app = editor(width, height, 1.0);
            open_surface(&app, surface);
            let base_image = snapshot_component(&app, width, height, 1.0).expect("render 1.0");
            let base = rects(&app);
            if let Ok(dir) = std::env::var("LOOM_DPI_DUMP") {
                let _ = base_image.save(format!("{dir}/sheets-{surface}-{width}-1.0.png"));
            }
            for factor in [1.25f32, 1.5, 2.0] {
                let image = snapshot_component(&app, width, height, factor).expect("render");
                if let Ok(dir) = std::env::var("LOOM_DPI_DUMP") {
                    let _ = image.save(format!("{dir}/sheets-{surface}-{width}-{factor}.png"));
                }
                if let Some(problem) = layout_difference(&base, &rects(&app)) {
                    panic!("{surface} {width}x{height} at {factor}: layout moved: {problem}");
                }
                if let Some(problem) = pixel_problem(&base_image, &image, (width, height), factor) {
                    panic!("{surface} {width}x{height} at {factor}: {problem}");
                }
            }
        }
    }
}

#[test]
fn scale_factor_is_independent_of_text_scale() {
    let parse = |flags: &[&str]| parse_args_from(flags.iter().copied()).expect("parse");
    assert_eq!(parse(&[]).scale_factor, 1.0);
    let both = parse(&["--scale-factor", "1.5", "--text-scale", "1.25"]);
    assert_eq!((both.scale_factor, both.text_scale), (1.5, 1.25));
    assert!(parse_args_from(["--scale-factor", "0.5"]).is_err());
    assert!(parse_args_from(["--scale-factor", "4.5"]).is_err());
    let app = editor(1024.0, 720.0, 1.5);
    let one = snapshot_component(&app, 1024.0, 720.0, 1.0).expect("render");
    let two = snapshot_component(&app, 1024.0, 720.0, 2.0).expect("render");
    assert_eq!(
        one.dimensions(),
        (1024, 720),
        "text scale keeps the pixel size"
    );
    assert_eq!(two.dimensions(), (2048, 1440));
}
