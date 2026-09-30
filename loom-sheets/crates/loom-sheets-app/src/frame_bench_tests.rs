//! Opt-in scroll cost measurement on a large sheet: the real scroll callback
//! plus a full window render through Slint's software renderer. Unlike the
//! projection benchmark in `perf_tests`, this includes the work a wheel event
//! actually triggers and the layout and painting that follow it.
//!
//! `LOOM_FRAME_BENCH=1` prints the numbers; `LOOM_FRAME_BENCH=enforce` also
//! fails when a budget is exceeded (used by CI on every desktop OS).

use super::*;
use std::time::Instant;

/// One 60 Hz frame. A scroll callback that costs a large share of this leaves
/// no room to paint, so it gets the whole budget as a ceiling.
const CALLBACK_BUDGET_MS: f64 = 16.7;
/// Software rendering of a full 1280x800 window on a shared CI runner.
const RENDER_BUDGET_MS: f64 = 45.0;

fn percentile(sorted: &[f64], fraction: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * fraction).round() as usize]
}

#[test]
fn scrolling_a_large_sheet_stays_within_the_frame_budget() {
    let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") else {
        return;
    };
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
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new([], []));
    let state = Rc::new(GuiState::new(
        sheet,
        None,
        dialogs,
        FileFilter::new("Workbook", ["loomtable"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("CSV", ["csv"]).expect("filter"),
        FileFilter::new("Excel", ["xlsx"]).expect("filter"),
    ));
    project_current(&app, &state);
    snapshot_component(&app, w as f32, h as f32, 1.0).expect("first frame");

    // Alternate vertical and horizontal wheel-sized steps, as a user does.
    let mut callback = Vec::new();
    let mut render = Vec::new();
    for frame in 0..90u32 {
        let (dx, dy) = if frame % 3 == 2 {
            (-40.0, 0.0)
        } else {
            (0.0, -72.0)
        };
        app.set_grid_scroll_x(app.get_grid_scroll_x() + dx);
        app.set_grid_scroll_y(app.get_grid_scroll_y() + dy);
        let started = Instant::now();
        scroll_projection::project_scroll(&app, &state);
        callback.push(started.elapsed().as_secs_f64() * 1e3);
        let started = Instant::now();
        snapshot_component(&app, w as f32, h as f32, 1.0).expect("frame");
        render.push(started.elapsed().as_secs_f64() * 1e3);
    }
    callback.sort_by(f64::total_cmp);
    render.sort_by(f64::total_cmp);
    let (callback_p95, render_p95) = (percentile(&callback, 0.95), percentile(&render, 0.95));
    eprintln!(
        "FRAME_BENCH os={} cells=300000 scroll_callback_ms median={:.2} p95={:.2} max={:.2}; \
         software_render_ms median={:.2} p95={:.2} max={:.2}",
        std::env::consts::OS,
        percentile(&callback, 0.5),
        callback_p95,
        callback[callback.len() - 1],
        percentile(&render, 0.5),
        render_p95,
        render[render.len() - 1],
    );
    if mode == "enforce" {
        assert!(
            callback_p95 < CALLBACK_BUDGET_MS,
            "scroll callback p95 {callback_p95:.2} ms exceeds {CALLBACK_BUDGET_MS} ms"
        );
        assert!(
            render_p95 < RENDER_BUDGET_MS,
            "software render p95 {render_p95:.2} ms exceeds {RENDER_BUDGET_MS} ms"
        );
    }
}
