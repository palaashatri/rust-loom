//! Opt-in frame cost measurement: renders the real window through Slint's
//! software renderer while scrolling a large sheet. Unlike the projection
//! benchmark in `perf_tests`, this includes layout and painting.

use super::*;
use std::time::Instant;

#[test]
fn scrolling_a_large_sheet_renders_within_a_frame() {
    if std::env::var_os("LOOM_FRAME_BENCH").is_none() {
        return;
    }
    set_platform();
    let app = SheetsApp::new().expect("create window");
    app.set_local_menu_visible(true);
    let (w, h) = (1280u32, 800u32);
    app.window().set_size(PhysicalSize::new(w, h));
    apply_layout_breakpoints(&app, w);
    apply_headless_viewport_size(&app, w, h);
    let mut sheet = Sheet::new("Bench");
    for row in 0..1000u32 {
        for col in 0..300u32 {
            sheet.set_raw(
                CellRef { row, col },
                ((row * 31 + col * 7) % 99991).to_string(),
            );
        }
    }
    project_sheet(&app, &sheet);
    snapshot_component(&app, w as f32, h as f32, 1.0).expect("first frame");

    let frames = 60;
    let mut render = Vec::new();
    let step: f32 = std::env::var("LOOM_FRAME_STEP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8.0);
    let mut project = Vec::new();
    for frame in 0..frames {
        app.set_grid_scroll_y(-(frame as f32) * step);
        let started = Instant::now();
        project_sheet_without_reveal(&app, &sheet);
        project.push(started.elapsed().as_secs_f64() * 1e3);
        let started = Instant::now();
        snapshot_component(&app, w as f32, h as f32, 1.0).expect("frame");
        render.push(started.elapsed().as_secs_f64() * 1e3);
    }
    project.sort_by(f64::total_cmp);
    eprintln!(
        "FRAME_BENCH project ms: median {:.2} max {:.2}",
        project[project.len() / 2],
        project[project.len() - 1]
    );
    render.sort_by(f64::total_cmp);
    eprintln!(
        "FRAME_BENCH software render ms: median {:.2} p95 {:.2} max {:.2}",
        render[render.len() / 2],
        render[render.len() * 95 / 100],
        render[render.len() - 1]
    );
}
