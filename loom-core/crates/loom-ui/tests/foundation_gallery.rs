include!(concat!(env!("OUT_DIR"), "/gallery.rs"));

use slint::Global;
use std::path::PathBuf;

const VIEWPORTS: &[(u32, u32)] = &[(1024, 720), (1280, 800), (1440, 900), (1920, 1200)];
const THEMES: &[&str] = &["light", "dark", "high-contrast"];

#[test]
fn capture_foundation_review_matrix() {
    let Ok(output) = std::env::var("LOOM_FOUNDATION_EVIDENCE") else {
        return;
    };

    loom_test_support::capture::set_platform();
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output).expect("create foundation evidence directory");

    let window = FoundationGallery::new().expect("create foundation gallery");
    for theme_name in THEMES {
        Theme::get(&window).set_active_theme((*theme_name).into());
        for &(width, height) in VIEWPORTS {
            let image = loom_test_support::capture::snapshot_component(
                &window,
                width as f32,
                height as f32,
                1.0,
            )
            .expect("capture foundation gallery");
            let path = output.join(format!("foundation-{theme_name}-{width}x{height}.png"));
            image
                .save(&path)
                .expect("write foundation gallery screenshot");
        }
    }
}
