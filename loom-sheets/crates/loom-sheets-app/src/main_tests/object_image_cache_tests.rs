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

fn png_declaring(width: u32, height: u32) -> Vec<u8> {
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    let empty_stream = [
        0x78, 0x01, 0x01, 0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x01,
    ];
    let chunks: [(&[u8; 4], &[u8]); 3] =
        [(b"IHDR", &ihdr), (b"IDAT", &empty_stream), (b"IEND", &[])];
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    for (kind, data) in chunks {
        bytes.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let mut body = kind.to_vec();
        body.extend_from_slice(data);
        bytes.extend_from_slice(&body);
        bytes.extend_from_slice(&crc32(&body).to_be_bytes());
    }
    bytes
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = u32::MAX;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

struct TempImageFile {
    path: PathBuf,
}

impl TempImageFile {
    fn write(name: &str, bytes: &[u8]) -> Self {
        let path = std::env::temp_dir().join(format!(
            "loom-sheets-cache-test-{}-{name}",
            std::process::id()
        ));
        std::fs::write(&path, bytes).expect("write test image file");
        Self { path }
    }
}

impl Drop for TempImageFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[test]
fn refused_or_unreadable_images_project_an_unavailable_notice_and_render_fallback() {
    use i_slint_backend_testing::ElementHandle;

    object_images::reset_cache_for_tests(32 * 1024 * 1024);
    let oversized_file = TempImageFile::write("oversized.png", &png_declaring(4097, 4097));
    let app = bench_app();
    let mut sheet = Sheet::new("refused images");
    sheet.objects.push(
        SheetObject::image(
            CellRef { row: 0, col: 0 },
            oversized_file.path.to_string_lossy().into_owned(),
        )
        .expect("path-backed oversized image"),
    );
    sheet.objects.push(image_object(
        4,
        0,
        "embedded-oversized.png",
        png_declaring(5000, 5000),
    ));
    sheet.objects.push(image_object(
        100,
        0,
        "offscreen-oversized.png",
        png_declaring(5000, 5000),
    ));

    project_sheet_without_reveal(&app, &sheet);

    let views = app.get_object_views();
    let file_view = views.row_data(0).expect("file-backed image view");
    assert!(file_view.visible);
    assert_eq!(file_view.image.size().width, 0);
    assert!(
        file_view.details.as_str().contains("Image unavailable")
            && file_view.details.as_str().contains("64 MiB"),
        "visible refused file-backed image must expose unavailable notice in details: {}",
        file_view.details
    );

    let embedded_view = views.row_data(1).expect("embedded image view");
    assert!(embedded_view.visible);
    assert_eq!(embedded_view.image.size().width, 0);
    assert!(
        embedded_view.details.as_str().contains("Image unavailable")
            && embedded_view.details.as_str().contains("64 MiB"),
        "visible refused embedded image must expose unavailable notice in details: {}",
        embedded_view.details
    );

    let offscreen_view = views.row_data(2).expect("offscreen image view");
    assert!(!offscreen_view.visible);
    assert!(
        !offscreen_view
            .details
            .as_str()
            .contains("Image unavailable"),
        "offscreen culled image must not prematurely carry an unavailable notice: {}",
        offscreen_view.details
    );

    let rendered =
        snapshot_component(&app, 1280.0, 800.0, 1.0).expect("render sheet with refused images");
    let expected_label = format!("Image 1: {}", file_view.label);
    let first_item = ElementHandle::find_by_accessible_label(&app, expected_label.as_str())
        .next()
        .expect("accessible image item");
    let desc = first_item
        .accessible_description()
        .expect("image accessible description")
        .to_string();
    assert!(desc.contains("Image unavailable") && desc.contains("64 MiB"));

    // Verify the centered fallback text is visibly drawn inside the image box
    // rather than leaving a solid blank rectangle.
    let pos = first_item.absolute_position();
    let size = first_item.size();
    let x0 = (pos.x.max(0.0) as u32).saturating_add(4);
    let y0 = (pos.y.max(0.0) as u32).saturating_add(4);
    let x1 = ((pos.x + size.width) as u32)
        .saturating_sub(4)
        .min(rendered.width());
    let y1 = ((pos.y + size.height) as u32)
        .saturating_sub(4)
        .min(rendered.height());
    let bg = *rendered.get_pixel(x0, y0);
    let has_text_pixels = (y0..y1).any(|y| (x0..x1).any(|x| *rendered.get_pixel(x, y) != bg));
    assert!(
        has_text_pixels,
        "refused image rectangle ({x0}..{x1}, {y0}..{y1}) must render visible fallback notice pixels"
    );
}

#[test]
fn insert_image_action_rejects_oversized_image_with_64_mib_status_and_no_object() {
    set_platform();
    let oversized_file = TempImageFile::write("insert-too-big.png", &png_declaring(4097, 4097));
    let valid_file = TempImageFile::write("insert-valid.png", &png(16, 16, 3));
    let app = SheetsApp::new().expect("create SheetsApp");
    app.window().set_size(PhysicalSize::new(1280, 800));
    let dialogs = Rc::new(loom_desktop::ScriptedFileDialogs::new(
        [
            Some(oversized_file.path.clone()),
            Some(valid_file.path.clone()),
        ],
        [],
    ));
    let state = cross_sheet_state_with_dialogs(dialogs);
    let menu_service = std::sync::Arc::new(NativeMenuBar::new());
    register_sheet_actions(&app, &state, &menu_service);
    apply_sheet(&app, &state);

    app.invoke_insert_image();

    assert!(
        state.current.borrow().objects.is_empty(),
        "oversized image must not be inserted into the worksheet"
    );
    assert!(
        state.undo_stack.borrow().is_empty(),
        "rejected insert must not push an undo snapshot"
    );
    assert_eq!(app.get_object_views().row_count(), 0);
    let status = app.get_status_left().to_string();
    assert!(
        status.starts_with("Insert image failed:") && status.contains("64 MiB"),
        "expected Insert image failed status mentioning 64 MiB, got {status:?}"
    );

    // Valid image still inserts cleanly afterwards.
    app.invoke_insert_image();
    assert_eq!(state.current.borrow().objects.len(), 1);
    assert_eq!(state.undo_stack.borrow().len(), 1);
    let view = app
        .get_object_views()
        .row_data(0)
        .expect("inserted valid image view");
    assert_eq!(view.image.size().width, 16);
    assert!(!view.details.as_str().contains("Image unavailable"));
}
