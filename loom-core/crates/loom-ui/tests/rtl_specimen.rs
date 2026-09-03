//! RTL direction evidence: mirrored primitives rendered for review.
include!(concat!(env!("OUT_DIR"), "/rtl-specimen.rs"));

#[test]
fn capture_rtl_specimen() {
    let Ok(output) = std::env::var("LOOM_FOUNDATION_EVIDENCE") else {
        return;
    };
    loom_test_support::capture::set_platform();
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).expect("create foundation evidence directory");

    for theme_name in ["light", "dark"] {
        let window = RtlSpecimenWindow::new().expect("create RTL specimen");
        // Direction is engaged by the window itself on init; select the
        // color theme through the window property (drives Theme global).
        window.set_theme_name(theme_name.into());
        let image = loom_test_support::capture::snapshot_component(&window, 1100.0, 900.0, 1.0)
            .expect("capture RTL specimen");
        assert_eq!(image.dimensions(), (1100, 900));
        let canvas = if theme_name == "light" {
            image::Rgba([244, 244, 246, 255])
        } else {
            image::Rgba([18, 18, 20, 255])
        };
        let painted = image.pixels().filter(|pixel| **pixel != canvas).count();
        assert!(
            painted > 20_000,
            "RTL specimen looks blank for {theme_name}: {painted} painted pixels"
        );
        image
            .save(output.join(format!("rtl-specimen-{theme_name}.png")))
            .expect("write RTL specimen");
    }
}
