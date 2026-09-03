include!(concat!(env!("OUT_DIR"), "/gallery.rs"));

use slint::{ComponentHandle, Global};
use std::path::PathBuf;

const VIEWPORTS: &[(u32, u32)] = &[(1024, 720), (1280, 800), (1440, 900), (1920, 1200)];
const THEMES: &[&str] = &["light", "dark", "high-contrast"];

fn capture_section<W>(output: &std::path::Path, name: &str, theme_name: &str, window: &W)
where
    W: ComponentHandle,
    for<'a> Theme<'a>: Global<'a, W>,
{
    window
        .global::<Theme<'_>>()
        .set_active_theme(theme_name.into());
    let image = loom_test_support::capture::snapshot_component(window, 1100.0, 700.0, 1.0)
        .unwrap_or_else(|error| {
            panic!("capture foundation section {name} in {theme_name}: {error}")
        });
    let path = output.join(format!("section-{name}-{theme_name}.png"));
    image
        .save(&path)
        .unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

#[test]
fn capture_section_evidence() {
    let Ok(output) = std::env::var("LOOM_FOUNDATION_EVIDENCE") else {
        return;
    };

    loom_test_support::capture::set_platform();
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output).expect("create foundation section evidence directory");

    for theme_name in ["light", "dark"] {
        capture_section(
            &output,
            "buttons",
            theme_name,
            &ButtonsSectionWindow::new().expect("create buttons section window"),
        );
        capture_section(
            &output,
            "fields",
            theme_name,
            &FieldsSectionWindow::new().expect("create fields section window"),
        );
        capture_section(
            &output,
            "choice",
            theme_name,
            &ChoiceSectionWindow::new().expect("create choice section window"),
        );
        capture_section(
            &output,
            "navigation",
            theme_name,
            &NavigationSectionWindow::new().expect("create navigation section window"),
        );
        capture_section(
            &output,
            "rows",
            theme_name,
            &RowsSectionWindow::new().expect("create rows section window"),
        );
        capture_section(
            &output,
            "overlays",
            theme_name,
            &OverlaysSectionWindow::new().expect("create overlays section window"),
        );
        capture_section(
            &output,
            "data",
            theme_name,
            &DataSectionWindow::new().expect("create data section window"),
        );
        capture_section(
            &output,
            "canvas",
            theme_name,
            &CanvasSectionWindow::new().expect("create canvas section window"),
        );
    }
}
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
