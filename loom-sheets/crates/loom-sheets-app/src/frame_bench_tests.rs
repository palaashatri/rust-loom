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

/// An image-heavy worksheet: sixteen embedded 1600x1200 images anchored well
/// apart, so a 1280x800 window shows only two or three at a time. Every full
/// projection used to decode all sixteen and copy their RGBA buffers.
const IMAGE_BENCH_IMAGES: u32 = 16;
const IMAGE_BENCH_WIDTH: u32 = 1_600;
const IMAGE_BENCH_HEIGHT: u32 = 1_200;

/// Deterministic, non-trivial pixels: structured enough to encode quickly,
/// varied enough that decoding and copying cost real work.
fn benchmark_png(width: u32, height: u32, seed: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let value = x
                .wrapping_mul(2_654_435_761)
                .wrapping_add(y.wrapping_mul(2_246_822_519))
                .wrapping_add(seed.wrapping_mul(3_266_489_917))
                ^ x.wrapping_mul(y).rotate_left(7);
            pixels.extend_from_slice(&[
                (value & 0xff) as u8,
                ((value >> 8) & 0xff) as u8,
                ((value >> 16) & 0xff) as u8,
                0xff,
            ]);
        }
    }
    let image = image::RgbaImage::from_raw(width, height, pixels).expect("pixel count matches");
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("encode benchmark image");
    bytes
}

fn image_heavy_sheet() -> Sheet {
    let mut sheet = Sheet::new("Image bench");
    for row in 0..240u32 {
        for col in 0..20u32 {
            sheet.set_raw(
                CellRef { row, col },
                ((row * 31 + col * 7) % 997).to_string(),
            );
        }
    }
    for index in 0..IMAGE_BENCH_IMAGES {
        let mut object = loom_sheets_core::SheetObject::image(
            CellRef {
                row: index * 14 + 1,
                col: 2,
            },
            format!("bench-{index}.png"),
        )
        .expect("benchmark image needs a path");
        object.width = 420;
        object.height = 300;
        object.embedded = Some(benchmark_png(IMAGE_BENCH_WIDTH, IMAGE_BENCH_HEIGHT, index).into());
        sheet.objects.push(object);
    }
    sheet
}

#[test]
fn image_heavy_sheet_input_and_scroll_stay_within_the_frame_budget() {
    let Ok(mode) = std::env::var("LOOM_FRAME_BENCH") else {
        return;
    };
    set_platform();
    object_images::reset_cache_for_tests(32 * 1024 * 1024);
    let app = SheetsApp::new().expect("create window");
    app.set_local_menu_visible(true);
    let (w, h) = (1280u32, 800u32);
    app.window().set_size(PhysicalSize::new(w, h));
    apply_layout_breakpoints(&app, w);
    apply_headless_viewport_size(&app, w, h);
    let sheet = image_heavy_sheet();
    let encoded_image_bytes = sheet
        .objects
        .iter()
        .filter_map(|object| object.embedded.as_ref())
        .map(|bytes| bytes.len())
        .sum::<usize>();
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

    // Moving the selection is the input that forces a full projection.
    let mut projection = Vec::new();
    let mut render = Vec::new();
    for frame in 0..40u32 {
        app.set_selected_row(((frame * 7) % 220) as i32 + 1);
        let started = Instant::now();
        project_current(&app, &state);
        projection.push(started.elapsed().as_secs_f64() * 1e3);
        let started = Instant::now();
        snapshot_component(&app, w as f32, h as f32, 1.0).expect("frame");
        render.push(started.elapsed().as_secs_f64() * 1e3);
    }

    // Wheel-sized scroll steps cross the anchored images.
    let mut scroll = Vec::new();
    for frame in 0..40u32 {
        let dy = if frame % 5 == 4 { -240.0 } else { -96.0 };
        app.set_grid_scroll_y(app.get_grid_scroll_y() + dy);
        let started = Instant::now();
        scroll_projection::project_scroll(&app, &state);
        scroll.push(started.elapsed().as_secs_f64() * 1e3);
    }

    for samples in [&mut projection, &mut render, &mut scroll] {
        samples.sort_by(f64::total_cmp);
    }
    let (projection_p95, render_p95, scroll_p95) = (
        percentile(&projection, 0.95),
        percentile(&render, 0.95),
        percentile(&scroll, 0.95),
    );
    eprintln!(
        "IMAGE_FRAME_BENCH os={} images={} encoded_image_bytes={} decoded_rgba_bytes={} \
         input_projection_ms median={:.2} p95={:.2} max={:.2}; \
         software_render_ms median={:.2} p95={:.2} max={:.2}; \
         scroll_callback_ms median={:.2} p95={:.2} max={:.2}; \
         cache_entries={} cache_bytes={}",
        std::env::consts::OS,
        IMAGE_BENCH_IMAGES,
        encoded_image_bytes,
        u64::from(IMAGE_BENCH_IMAGES)
            * u64::from(IMAGE_BENCH_WIDTH)
            * u64::from(IMAGE_BENCH_HEIGHT)
            * 4,
        percentile(&projection, 0.5),
        projection_p95,
        projection[projection.len() - 1],
        percentile(&render, 0.5),
        render_p95,
        render[render.len() - 1],
        percentile(&scroll, 0.5),
        scroll_p95,
        scroll[scroll.len() - 1],
        object_images::cached_entries_for_tests(),
        object_images::cached_bytes_for_tests(),
    );
    if mode == "enforce" {
        assert!(
            projection_p95 < CALLBACK_BUDGET_MS,
            "image-heavy input projection p95 {projection_p95:.2} ms exceeds {CALLBACK_BUDGET_MS} ms"
        );
        assert!(
            scroll_p95 < CALLBACK_BUDGET_MS,
            "image-heavy scroll callback p95 {scroll_p95:.2} ms exceeds {CALLBACK_BUDGET_MS} ms"
        );
        assert!(
            render_p95 < RENDER_BUDGET_MS,
            "image-heavy software render p95 {render_p95:.2} ms exceeds {RENDER_BUDGET_MS} ms"
        );
    }
}
