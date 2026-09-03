//! Icon specimen evidence: every registered icon rendered labeled at 20px.
include!(concat!(env!("OUT_DIR"), "/icon-grid.rs"));

#[test]
fn capture_icon_specimen() {
    let Ok(output) = std::env::var("LOOM_FOUNDATION_EVIDENCE") else {
        return;
    };
    loom_test_support::capture::set_platform();
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).expect("create foundation evidence directory");

    for theme_name in ["light", "dark"] {
        let window = IconGridWindow::new().expect("create icon specimen");
        let names: Vec<slint::SharedString> = loom_ui::ICON_NAMES
            .iter()
            .map(|name| slint::SharedString::from(*name))
            .collect();
        window.set_caption("Loom icon set".into());
        window.set_names(slint::VecModel::from_slice(&names));
        window.set_theme_name(theme_name.into());
        let image = loom_test_support::capture::snapshot_component(&window, 1100.0, 900.0, 1.0)
            .expect("capture icon specimen");
        assert_eq!(image.dimensions(), (1100, 900));
        let canvas = if theme_name == "light" {
            image::Rgba([244, 244, 246, 255])
        } else {
            image::Rgba([18, 18, 20, 255])
        };
        let painted = image.pixels().filter(|pixel| **pixel != canvas).count();
        assert!(
            painted > 100_000,
            "icon specimen looks blank for {theme_name}: {painted} painted pixels"
        );
        image
            .save(output.join(format!("icon-specimen-{theme_name}.png")))
            .expect("write icon specimen");
    }
}
