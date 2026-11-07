use std::io::Cursor;

use loom_sheets_core::SheetObject;

use super::*;
use crate::object_images;

/// A small deterministic PNG. The content is what the cache keys on, so each
/// seed produces a different identity.
fn png(width: u32, height: u32, seed: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let value = x
                .wrapping_mul(31)
                .wrapping_add(y.wrapping_mul(17))
                .wrapping_add(seed.wrapping_mul(37));
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
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("encode test image");
    bytes
}

fn image_object(row: u32, col: u32, name: &str, bytes: Vec<u8>) -> SheetObject {
    let mut object = SheetObject::image(CellRef { row, col }, name).expect("image path is valid");
    object.embedded = Some(bytes.into());
    object
}

fn bench_app() -> SheetsApp {
    set_platform();
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    apply_layout_breakpoints(&app, 1280);
    apply_headless_viewport_size(&app, 1280, 800);
    app.set_grid_viewport_width(600.0);
    app.set_grid_viewport_height(300.0);
    app
}

#[test]
fn offscreen_images_are_not_decoded_until_they_intersect_the_viewport() {
    object_images::reset_cache_for_tests(32 * 1024 * 1024);
    let app = bench_app();
    let mut sheet = Sheet::new("culled images");
    sheet
        .objects
        .push(image_object(0, 0, "near.png", png(32, 32, 1)));
    sheet
        .objects
        .push(image_object(100, 0, "far.png", png(32, 32, 2)));

    project_sheet_without_reveal(&app, &sheet);

    assert_eq!(
        object_images::decode_calls_for_tests(),
        1,
        "only the visible image may be decoded"
    );
    let views = app.get_object_views();
    assert!(views.row_data(0).expect("near object").visible);
    let far = views.row_data(1).expect("far object");
    assert!(!far.visible);
    assert_eq!(
        far.image.size().width,
        0,
        "an offscreen object must not carry a decoded image"
    );

    app.set_grid_scroll_y(-(100.0 * GRID_ROW_HEIGHT));
    project_sheet_without_reveal(&app, &sheet);

    assert_eq!(
        object_images::decode_calls_for_tests(),
        2,
        "scrolling the far image into view decodes exactly that image"
    );
    let views = app.get_object_views();
    assert!(!views.row_data(0).expect("near object").visible);
    let far = views.row_data(1).expect("far object");
    assert!(far.visible);
    assert_eq!(far.image.size().width, 32);
    assert_eq!(far.image.size().height, 32);
}

#[test]
fn repeated_projections_reuse_the_decoded_image_and_keep_identical_pixels() {
    object_images::reset_cache_for_tests(32 * 1024 * 1024);
    let app = bench_app();
    let mut sheet = Sheet::new("reused image");
    sheet
        .objects
        .push(image_object(0, 0, "hero.png", png(64, 48, 7)));

    project_sheet_without_reveal(&app, &sheet);
    let first = app
        .get_object_views()
        .row_data(0)
        .expect("projected image")
        .image;
    let first_pixels = first
        .to_rgba8()
        .expect("decoded raster")
        .as_bytes()
        .to_vec();

    // A cell edit forces a full projection; the image itself did not change.
    sheet.set_raw(CellRef { row: 2, col: 2 }, "42");
    project_sheet_without_reveal(&app, &sheet);
    let second = app
        .get_object_views()
        .row_data(0)
        .expect("re-projected image")
        .image;
    let second_pixels = second
        .to_rgba8()
        .expect("decoded raster")
        .as_bytes()
        .to_vec();

    assert_eq!(
        object_images::decode_calls_for_tests(),
        1,
        "the second projection must reuse the cached decode"
    );
    assert_eq!(first.size(), second.size());
    assert_eq!(first_pixels, second_pixels);
}

#[test]
fn decoded_image_cache_is_bounded_and_evicts_old_entries() {
    const BUDGET: usize = 64 * 1024;
    object_images::reset_cache_for_tests(BUDGET);
    let app = bench_app();
    let mut sheet = Sheet::new("bounded cache");
    for index in 0..6u32 {
        sheet.objects.push(image_object(
            index * 20,
            0,
            &format!("image-{index}.png"),
            png(64, 64, index + 1),
        ));
    }

    for index in 0..6u32 {
        app.set_grid_scroll_y(-(index as f32 * 20.0 * GRID_ROW_HEIGHT));
        project_sheet_without_reveal(&app, &sheet);
        assert!(
            object_images::cached_bytes_for_tests() <= BUDGET,
            "cache bytes {} exceeded the budget {BUDGET}",
            object_images::cached_bytes_for_tests()
        );
        assert!(
            object_images::cached_entries_for_tests() <= 4,
            "a 64x64 image is 16 KiB, so the 64 KiB budget holds four entries"
        );
    }

    let decoded = object_images::decode_calls_for_tests();
    assert!(decoded >= 6, "each distinct image decodes at least once");
    assert_eq!(
        object_images::cached_entries_for_tests(),
        4,
        "the cache must stop growing at its byte budget"
    );

    // The first image was evicted by the four later ones and must decode again.
    app.set_grid_scroll_y(0.0);
    project_sheet_without_reveal(&app, &sheet);
    assert_eq!(
        object_images::decode_calls_for_tests(),
        decoded + 1,
        "returning to an evicted image decodes it again"
    );
}
